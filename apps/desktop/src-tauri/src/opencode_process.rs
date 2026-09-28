use std::{
    env,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::{Child, Command},
    sync::Mutex,
    time::{sleep, timeout},
};

use crate::opencode;

const SERVER_USERNAME: &str = "opencode";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Default)]
pub struct OpenCodeProcessManager {
    server: Mutex<Option<ManagedServer>>,
}

struct ManagedServer {
    child: Child,
    base_url: String,
    version: String,
}

/// This response crosses Tauri IPC so the UI can use the existing authenticated API calls.
/// The password is only returned at startup and is never persisted or logged.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedOpenCodeServer {
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeProcessStatus {
    pub running: bool,
    pub base_url: Option<String>,
    pub version: Option<String>,
}

impl OpenCodeProcessManager {
    pub async fn start(&self, requested_base_url: &str) -> Result<StartedOpenCodeServer, String> {
        let port = opencode::local_server_port(requested_base_url)?;
        let base_url = format!("http://127.0.0.1:{port}");
        let mut managed = self.server.lock().await;

        if let Some(current) = managed.as_mut() {
            let is_running = current
                .child
                .try_wait()
                .map_err(|_| "No se pudo comprobar el proceso OpenCode administrado".to_string())?
                .is_none();
            if is_running {
                return Err("Stade Studio ya inició un servidor OpenCode".to_string());
            }
        }
        *managed = None;
        ensure_loopback_port_available(port)?;

        let executable = resolve_opencode_executable()?;
        let mut command = Command::new(executable);
        command
            .args(["serve", "--hostname", "127.0.0.1", "--port"])
            .arg(port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|_| "No se pudo iniciar OpenCode. Comprueba que esté instalado".to_string())?;

        let Some(stdout) = child.stdout.take() else {
            let _ = terminate_child(&mut child).await;
            return Err(
                "OpenCode no expuso su salida de inicio; no se pudo capturar la credencial"
                    .to_string(),
            );
        };
        if let Some(stderr) = child.stderr.take() {
            // Drain stderr to avoid blocking the child, but never forward its contents to logs or UI.
            tauri::async_runtime::spawn(discard_lines(stderr));
        }
        let mut stdout_lines = BufReader::new(stdout).lines();

        let startup = timeout(STARTUP_TIMEOUT, async {
            let mut password = None;
            let mut listening = false;

            while password.is_none() || !listening {
                tokio::select! {
                    line = stdout_lines.next_line() => {
                        let line = line
                            .map_err(|_| "No se pudo leer la salida de inicio de OpenCode".to_string())?
                            .ok_or_else(|| format!("OpenCode terminó antes de quedar listo; el puerto local {port} podría estar ocupado"))?;
                        if let Some(captured) = parse_server_password(&line) {
                            password = Some(captured);
                        }
                        if is_expected_listening_line(&line, port) {
                            listening = true;
                        }
                    }
                    _ = sleep(Duration::from_millis(100)) => {
                        match child.try_wait() {
                            Ok(Some(_)) => return Err(format!("OpenCode terminó antes de quedar listo; el puerto local {port} podría estar ocupado")),
                            Ok(None) => {},
                            Err(_) => return Err("No se pudo comprobar el inicio de OpenCode".to_string()),
                        }
                    }
                }
            }

            password.ok_or_else(|| "OpenCode no anunció la contraseña temporal; se canceló el inicio".to_string())
        })
        .await;

        let password = match startup {
            Ok(Ok(password)) => password,
            Ok(Err(error)) => {
                let _ = terminate_child(&mut child).await;
                return Err(error);
            }
            Err(_) => {
                let _ = terminate_child(&mut child).await;
                return Err("OpenCode tardó demasiado en anunciar que estaba listo".to_string());
            }
        };

        // Continue consuming stdout after startup so later server output cannot fill the pipe.
        tauri::async_runtime::spawn(discard_lines_from_lines(stdout_lines));

        let health = match opencode::check_connection(&base_url, SERVER_USERNAME, &password).await {
            Ok(health) => health,
            Err(error) => {
                let _ = terminate_child(&mut child).await;
                return Err(format!(
                    "El servidor inició, pero no se pudo validar la conexión: {error}"
                ));
            }
        };

        if child
            .try_wait()
            .map_err(|_| "No se pudo comprobar el proceso OpenCode".to_string())?
            .is_some()
        {
            return Err("OpenCode se detuvo justo después de iniciar".to_string());
        }

        let result = StartedOpenCodeServer {
            base_url: base_url.clone(),
            username: SERVER_USERNAME.to_string(),
            password,
            version: health.version.clone(),
        };
        *managed = Some(ManagedServer {
            child,
            base_url,
            version: health.version,
        });

        Ok(result)
    }

    pub async fn stop(&self) -> Result<(), String> {
        let mut managed = self.server.lock().await;
        if let Some(server) = managed.as_mut() {
            terminate_child(&mut server.child).await?;
        }
        *managed = None;
        Ok(())
    }

    pub async fn shutdown(&self) {
        let mut managed = self.server.lock().await;
        if let Some(server) = managed.as_mut() {
            let _ = server.child.start_kill();
            let _ = timeout(Duration::from_secs(5), server.child.wait()).await;
        }
        *managed = None;
    }

    pub async fn status(&self) -> Result<OpenCodeProcessStatus, String> {
        let mut managed = self.server.lock().await;
        let Some(server) = managed.as_mut() else {
            return Ok(stopped_status());
        };

        match server.child.try_wait() {
            Ok(Some(_)) => {
                *managed = None;
                Ok(stopped_status())
            }
            Ok(None) => Ok(OpenCodeProcessStatus {
                running: true,
                base_url: Some(server.base_url.clone()),
                version: Some(server.version.clone()),
            }),
            Err(_) => Err("No se pudo comprobar el proceso OpenCode administrado".to_string()),
        }
    }
}

fn stopped_status() -> OpenCodeProcessStatus {
    OpenCodeProcessStatus {
        running: false,
        base_url: None,
        version: None,
    }
}

fn ensure_loopback_port_available(port: u16) -> Result<(), String> {
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            return Err(format!(
                "El puerto local {port} ya está en uso. Cierra el proceso que lo ocupa o elige otro puerto"
            ));
        }
        Err(_) => {
            return Err(format!(
                "No se pudo comprobar si el puerto local {port} está disponible"
            ));
        }
    }
    Ok(())
}

fn parse_server_password(line: &str) -> Option<String> {
    let line = line.trim();
    let prefix = "server password ";
    let remainder = line
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .and_then(|_| line.get(prefix.len()..))?
        .trim();

    if remainder.is_empty() || remainder.len() > 512 || remainder.chars().any(char::is_control) {
        return None;
    }

    Some(remainder.to_string())
}

fn is_expected_listening_line(line: &str, port: u16) -> bool {
    line.trim()
        .eq_ignore_ascii_case(&format!("server listening on http://127.0.0.1:{port}"))
}

async fn discard_lines<R>(reader: R)
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while matches!(lines.next_line().await, Ok(Some(_))) {}
}

async fn discard_lines_from_lines<R>(mut lines: tokio::io::Lines<BufReader<R>>)
where
    R: AsyncRead + Unpin,
{
    while matches!(lines.next_line().await, Ok(Some(_))) {}
}

async fn terminate_child(child: &mut Child) -> Result<(), String> {
    if child
        .try_wait()
        .map_err(|_| "No se pudo comprobar el proceso OpenCode antes de detenerlo".to_string())?
        .is_none()
    {
        if child.start_kill().is_err() && child.try_wait().ok().flatten().is_none() {
            return Err(
                "No se pudo detener el proceso OpenCode iniciado por Stade Studio".to_string(),
            );
        }
    }
    timeout(Duration::from_secs(5), child.wait())
        .await
        .map_err(|_| "OpenCode no terminó después de solicitar su cierre".to_string())?
        .map(|_| ())
        .map_err(|_| "No se pudo confirmar el cierre del proceso OpenCode".to_string())
}

#[cfg(windows)]
fn resolve_opencode_executable() -> Result<PathBuf, String> {
    let path = env::var_os("PATH").ok_or_else(opencode_not_found)?;
    let directories = env::split_paths(&path).collect::<Vec<_>>();

    for directory in &directories {
        let executable = directory.join("opencode.exe");
        if executable.is_file() {
            return Ok(executable);
        }
    }

    // npm's generated Windows shim points at the official standalone CLI binary.
    for directory in &directories {
        let shim = directory.join("opencode.cmd");
        if !shim.is_file() || !is_opencode_npm_shim(&shim) {
            continue;
        }
        let executable = directory
            .join("node_modules")
            .join("@opencode")
            .join("cli")
            .join("bin")
            .join("opencode.exe");
        if executable.is_file() {
            return Ok(executable);
        }
    }

    Err(opencode_not_found())
}

#[cfg(not(windows))]
fn resolve_opencode_executable() -> Result<PathBuf, String> {
    Ok(PathBuf::from("opencode"))
}

#[cfg(windows)]
fn is_opencode_npm_shim(shim: &Path) -> bool {
    std::fs::read_to_string(shim)
        .map(|contents| {
            contents
                .to_ascii_lowercase()
                .contains("node_modules\\@opencode\\cli\\bin\\opencode.exe")
        })
        .unwrap_or(false)
}

fn opencode_not_found() -> String {
    "No se encontró OpenCode. Instálalo o comprueba que esté disponible en PATH".to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_loopback_port_available, is_expected_listening_line, parse_server_password,
        OpenCodeProcessManager,
    };
    use std::time::Duration;

    #[test]
    fn captures_only_the_temporary_password_announcement() {
        assert_eq!(
            parse_server_password("server password a-generated-secret"),
            Some("a-generated-secret".to_string())
        );
        assert_eq!(
            parse_server_password("SERVER PASSWORD another-secret  "),
            Some("another-secret".to_string())
        );
        assert_eq!(
            parse_server_password("server listening on http://127.0.0.1:4096"),
            None
        );
        assert_eq!(parse_server_password("server password "), None);
    }

    #[test]
    fn accepts_only_the_expected_loopback_ready_message() {
        assert!(is_expected_listening_line(
            "server listening on http://127.0.0.1:4096",
            4096
        ));
        assert!(!is_expected_listening_line(
            "server listening on http://0.0.0.0:4096",
            4096
        ));
        assert!(!is_expected_listening_line(
            "server listening on http://127.0.0.1:4097",
            4096
        ));
    }

    #[test]
    fn reports_when_the_requested_port_is_already_in_use() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("an unused local port should be available");
        let port = listener
            .local_addr()
            .expect("the listener address should be available")
            .port();

        let error = ensure_loopback_port_available(port)
            .expect_err("a listening socket should reserve its port");
        assert!(error.contains(&port.to_string()));
        assert!(error.contains("ya está en uso"));
    }

    #[test]
    #[ignore = "smoke test against the locally installed OpenCode CLI"]
    fn starts_and_stops_only_its_managed_server() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("an unused local port should be available");
        let port = listener
            .local_addr()
            .expect("the listener address should be available")
            .port();
        drop(listener);

        tauri::async_runtime::block_on(async {
            let manager = OpenCodeProcessManager::default();
            let started = manager
                .start(&format!("http://127.0.0.1:{port}"))
                .await
                .expect("the installed OpenCode server should start and authenticate");
            assert_eq!(started.base_url, format!("http://127.0.0.1:{port}"));
            assert_eq!(started.username, "opencode");
            assert!(!started.password.is_empty());
            assert!(!started.version.is_empty());
            assert!(manager.status().await.expect("status should load").running);

            manager
                .stop()
                .await
                .expect("the managed process should stop");
            assert!(
                !manager
                    .status()
                    .await
                    .expect("stopped status should load")
                    .running
            );

            let manager = OpenCodeProcessManager::default();
            manager
                .start(&format!("http://127.0.0.1:{port}"))
                .await
                .expect("the server should be restartable after stopping");
            manager.shutdown().await;
            std::thread::sleep(Duration::from_millis(250));
            let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            assert!(
                std::net::TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_err()
            );
        });
    }
}
