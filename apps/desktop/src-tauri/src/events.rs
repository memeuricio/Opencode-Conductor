use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{async_runtime::JoinHandle, AppHandle, Emitter};
use tokio::{
    sync::{watch, Mutex},
    time::sleep,
};
use url::Url;

use crate::opencode;

const EVENT_STREAM_PATH: &str = "/api/event";
const EVENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const EVENT_SIGNAL_INTERVAL: Duration = Duration::from_millis(350);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);
const EVENT_STREAM_STATUS: &str = "opencode-event-stream-status";
const OPENCODE_STATE_CHANGED: &str = "opencode-state-changed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventStreamStatus {
    pub state: String,
    pub retry_in_ms: Option<u64>,
    pub message: Option<String>,
}

struct ActiveStream {
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

#[derive(Default)]
pub struct OpenCodeEventManager {
    active: Mutex<Option<ActiveStream>>,
}

impl OpenCodeEventManager {
    pub async fn start(
        &self,
        app: AppHandle,
        base_url: &str,
        username: &str,
        password: &str,
    ) -> Result<(), String> {
        let mut url = opencode::validate_local_base_url(base_url)?;
        url.set_path(EVENT_STREAM_PATH);

        let client = reqwest::Client::builder()
            .connect_timeout(EVENT_CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "No se pudo preparar el canal de eventos de OpenCode".to_string())?;

        let (stop, stop_rx) = watch::channel(false);
        let username = username.to_string();
        let password = password.to_string();

        let mut active = self.active.lock().await;
        if let Some(previous) = active.take() {
            let _ = previous.stop.send(true);
            previous.task.abort();
        }

        let task = tauri::async_runtime::spawn(run_event_stream(
            app, client, url, username, password, stop_rx,
        ));
        *active = Some(ActiveStream { stop, task });
        Ok(())
    }

    pub async fn stop(&self, app: &AppHandle) {
        let active = self.active.lock().await.take();
        if let Some(active) = active {
            let _ = active.stop.send(true);
            active.task.abort();
        }
        emit_status(app, "stopped", None, None);
    }
}

fn emit_status(app: &AppHandle, state: &str, retry_in_ms: Option<u64>, message: Option<&str>) {
    let status = EventStreamStatus {
        state: state.to_string(),
        retry_in_ms,
        message: message.map(str::to_string),
    };
    let _ = app.emit(EVENT_STREAM_STATUS, status);
}

fn emit_state_changed(app: &AppHandle, last_signal: &mut Option<Instant>) {
    let now = Instant::now();
    let should_emit = last_signal
        .map(|last| now.duration_since(last) >= EVENT_SIGNAL_INTERVAL)
        .unwrap_or(true);
    if should_emit {
        let _ = app.emit(OPENCODE_STATE_CHANGED, ());
        *last_signal = Some(now);
    }
}

fn retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.min(5);
    let seconds = 1_u64 << exponent;
    Duration::from_secs(seconds).min(MAX_RECONNECT_DELAY)
}

fn retry_status(app: &AppHandle, attempt: u32) -> Duration {
    let delay = retry_delay(attempt);
    emit_status(
        app,
        "reconnecting",
        Some(delay.as_millis() as u64),
        Some("El flujo se interrumpió; se volverá a intentar."),
    );
    delay
}

async fn wait_to_retry(delay: Duration, stop: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = sleep(delay) => !*stop.borrow(),
        changed = stop.changed() => changed.is_ok() && !*stop.borrow(),
    }
}

async fn run_event_stream(
    app: AppHandle,
    client: reqwest::Client,
    url: Url,
    username: String,
    password: String,
    mut stop: watch::Receiver<bool>,
) {
    let mut attempt = 0_u32;
    let mut last_signal = None;

    loop {
        if *stop.borrow() {
            break;
        }

        if attempt == 0 {
            emit_status(&app, "connecting", None, None);
        }
        let response = tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
                continue;
            }
            response = client
                .get(url.clone())
                .basic_auth(&username, Some(&password))
                .header(reqwest::header::ACCEPT, "text/event-stream")
                .send() => response,
        };

        let response = match response {
            Ok(response) => response,
            Err(_) => {
                attempt = attempt.saturating_add(1);
                let delay = retry_status(&app, attempt);
                if !wait_to_retry(delay, &mut stop).await {
                    break;
                }
                continue;
            }
        };

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            emit_status(
                &app,
                "error",
                None,
                Some("OpenCode rechazó las credenciales del canal de eventos."),
            );
            return;
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            emit_status(
                &app,
                "error",
                None,
                Some("Esta versión de OpenCode no expone el canal de eventos esperado."),
            );
            return;
        }
        if !status.is_success() {
            attempt = attempt.saturating_add(1);
            let delay = retry_status(&app, attempt);
            if !wait_to_retry(delay, &mut stop).await {
                break;
            }
            continue;
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !content_type
            .to_ascii_lowercase()
            .contains("text/event-stream")
        {
            emit_status(
                &app,
                "error",
                None,
                Some("OpenCode no devolvió un flujo SSE; comprueba la versión instalada."),
            );
            return;
        }

        emit_status(&app, "connected", None, None);
        // Fuerza una reconciliación al abrir o recuperar el stream. Los eventos SSE
        // son volátiles y no se reproducen después de una desconexión.
        emit_state_changed(&app, &mut last_signal);
        let connected_at = Instant::now();
        let ended = consume_stream(response, &app, &mut stop, &mut last_signal).await;
        if ended == StreamEnd::Stopped || *stop.borrow() {
            break;
        }

        if connected_at.elapsed() >= Duration::from_secs(60) {
            attempt = 0;
        }
        attempt = attempt.saturating_add(1);
        let delay = retry_status(&app, attempt);
        if !wait_to_retry(delay, &mut stop).await {
            break;
        }
    }

    emit_status(&app, "stopped", None, None);
}

#[derive(Debug, PartialEq, Eq)]
enum StreamEnd {
    Closed,
    Stopped,
}

async fn consume_stream(
    mut response: reqwest::Response,
    app: &AppHandle,
    stop: &mut watch::Receiver<bool>,
    last_signal: &mut Option<Instant>,
) -> StreamEnd {
    let mut parser = SseSignalParser::default();

    loop {
        let chunk = tokio::select! {
            changed = stop.changed() => {
                return if changed.is_err() || *stop.borrow() {
                    StreamEnd::Stopped
                } else {
                    StreamEnd::Closed
                };
            }
            chunk = response.chunk() => chunk,
        };

        match chunk {
            Ok(Some(bytes)) => {
                if parser.push(&bytes) > 0 {
                    emit_state_changed(app, last_signal);
                }
            }
            Ok(None) | Err(_) => return StreamEnd::Closed,
        }
    }
}

/// Reduce each SSE frame to a no-payload invalidation signal. Event names and
/// message bodies may include prompts, responses, or tool contents, so they
/// never cross Tauri IPC and are not logged or persisted here.
#[derive(Default)]
struct SseSignalParser {
    line_prefix: [u8; 5],
    prefix_len: usize,
    line_len: usize,
    event_has_data: bool,
    skip_lf: bool,
}

impl SseSignalParser {
    fn push(&mut self, bytes: &[u8]) -> usize {
        let mut completed_events = 0;
        for byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if *byte == b'\n' {
                    continue;
                }
            }

            match *byte {
                b'\r' | b'\n' => {
                    if self.line_len == 0 {
                        if self.event_has_data {
                            completed_events += 1;
                        }
                        self.event_has_data = false;
                    } else if self.prefix_len == self.line_prefix.len()
                        && self.line_prefix == *b"data:"
                        && self.line_len > self.line_prefix.len()
                    {
                        self.event_has_data = true;
                    }

                    self.line_len = 0;
                    self.prefix_len = 0;
                    self.line_prefix = [0; 5];
                    if *byte == b'\r' {
                        self.skip_lf = true;
                    }
                }
                value => {
                    if self.prefix_len < self.line_prefix.len() {
                        self.line_prefix[self.prefix_len] = value;
                        self.prefix_len += 1;
                    }
                    self.line_len = self.line_len.saturating_add(1);
                }
            }
        }
        completed_events
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{SseSignalParser, EVENT_CONNECT_TIMEOUT, EVENT_STREAM_PATH};

    #[test]
    fn signals_data_frames_without_retaining_event_bodies() {
        let mut parser = SseSignalParser::default();
        assert_eq!(parser.push(b": heartbeat\r\n\r\nevent: message\r\ndata: {\"type\":\"session.updated\"}\r\n\r\n"), 1);
    }

    #[test]
    fn parses_split_multiline_frames_and_ignores_comments() {
        let mut parser = SseSignalParser::default();
        assert_eq!(
            parser.push(b": ping\n\nevent: test\ndata: one\ndata: two\n"),
            0
        );
        assert_eq!(parser.push(b"\n"), 1);
    }

    #[test]
    #[ignore = "smoke test against the locally installed OpenCode CLI"]
    fn installed_opencode_exposes_an_authenticated_event_stream() {
        tauri::async_runtime::block_on(async {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
                .expect("an unused local port should be available");
            let port = listener
                .local_addr()
                .expect("the listener address should be available")
                .port();
            drop(listener);

            let manager = crate::opencode_process::OpenCodeProcessManager::default();
            let started = manager
                .start(&format!("http://127.0.0.1:{port}"))
                .await
                .expect("the installed OpenCode server should start and authenticate");

            let mut url =
                url::Url::parse(&started.base_url).expect("the server URL should be valid");
            url.set_path(EVENT_STREAM_PATH);
            let client = reqwest::Client::builder()
                .connect_timeout(EVENT_CONNECT_TIMEOUT)
                .build()
                .expect("the event client should build");
            let response = client
                .get(url)
                .basic_auth(&started.username, Some(&started.password))
                .header(reqwest::header::ACCEPT, "text/event-stream")
                .send()
                .await
                .expect("the event endpoint should accept the local connection");

            assert!(response.status().is_success());
            assert!(response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .contains("text/event-stream"));

            let has_data_frame = tokio::time::timeout(Duration::from_secs(5), async {
                let mut response = response;
                let mut buffer = Vec::new();
                loop {
                    let Some(chunk) = response
                        .chunk()
                        .await
                        .expect("the event stream should read")
                    else {
                        return false;
                    };
                    buffer.extend_from_slice(&chunk);
                    if buffer.windows(5).any(|window| window == b"data:") {
                        return true;
                    }
                }
            })
            .await
            .expect("OpenCode should emit an initial event");
            assert!(has_data_frame, "the SSE stream should include a data frame");

            manager
                .stop()
                .await
                .expect("the managed OpenCode server should stop");
        });
    }
}
