use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::{Host, Url};

#[derive(Debug, Deserialize)]
struct OpenCodeInfo {
    version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenCodeHealth {
    healthy: bool,
    version: String,
}

fn validate_local_base_url(base_url: &str) -> Result<Url, String> {
    let mut url = Url::parse(base_url.trim()).map_err(|_| "La URL no es válida".to_string())?;

    if url.scheme() != "http" {
        return Err("Por seguridad, la conexión local debe usar HTTP".to_string());
    }

    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("La URL no debe incluir credenciales, query ni fragmento".to_string());
    }

    let is_loopback = match url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };

    if !is_loopback {
        return Err(
            "Solo se permiten servidores OpenCode locales (localhost/loopback)".to_string(),
        );
    }

    if url.path() != "/" && !url.path().is_empty() {
        return Err("Introduce la URL base, sin una ruta adicional".to_string());
    }

    url.set_path("");
    Ok(url)
}

#[tauri::command]
async fn check_opencode_connection(
    base_url: String,
    username: String,
    password: String,
) -> Result<OpenCodeHealth, String> {
    let mut url = validate_local_base_url(&base_url)?;
    url.set_path("/api/info");

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|error| format!("No se pudo preparar la conexión: {error}"))?;

    let response = client
        .get(url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(|error| {
            if error.is_connect() {
                "No hay un servidor OpenCode escuchando en esa dirección".to_string()
            } else if error.is_timeout() {
                "OpenCode tardó demasiado en responder".to_string()
            } else {
                format!("No se pudo conectar con OpenCode: {error}")
            }
        })?
        .error_for_status()
        .map_err(|error| {
            if error.status() == Some(reqwest::StatusCode::UNAUTHORIZED) {
                "OpenCode rechazó las credenciales del servidor. Revisa el usuario y la contraseña"
                    .to_string()
            } else if error.status() == Some(reqwest::StatusCode::NOT_FOUND) {
                "No se encontró la API v2 de OpenCode en esa dirección".to_string()
            } else {
                format!("OpenCode respondió con un error HTTP: {error}")
            }
        })?;

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !content_type.contains("application/json") {
        return Err("La dirección respondió contenido web, no la API de OpenCode".to_string());
    }

    let info: OpenCodeInfo = response.json().await.map_err(|error| {
        format!("La respuesta de OpenCode no tiene el formato esperado: {error}")
    })?;

    Ok(OpenCodeHealth {
        healthy: true,
        version: info.version,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![check_opencode_connection])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::validate_local_base_url;

    #[test]
    fn accepts_localhost_and_loopback_addresses() {
        assert!(validate_local_base_url("http://localhost:4096").is_ok());
        assert!(validate_local_base_url("http://127.0.0.1:4096").is_ok());
        assert!(validate_local_base_url("http://[::1]:4096").is_ok());
    }

    #[test]
    fn rejects_remote_hosts_and_paths() {
        assert!(validate_local_base_url("http://example.com:4096").is_err());
        assert!(validate_local_base_url("http://192.168.1.5:4096").is_err());
        assert!(validate_local_base_url("http://localhost:4096/api").is_err());
    }
}
