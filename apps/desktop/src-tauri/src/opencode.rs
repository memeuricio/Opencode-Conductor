use std::{collections::HashMap, time::Duration};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use url::{Host, Url};

#[derive(Debug, Deserialize)]
struct OpenCodeInfo {
    version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeHealth {
    pub(crate) healthy: bool,
    pub(crate) version: String,
}

#[derive(Debug, Deserialize)]
struct OpenCodeList<T> {
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenCodeAgent {
    id: String,
    name: String,
    mode: Option<String>,
    description: Option<String>,
    hidden: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenCodeModel {
    id: String,
    #[serde(rename = "modelID")]
    model_id: Option<String>,
    #[serde(rename = "providerID")]
    provider_id: Option<String>,
    name: String,
    enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSummary {
    id: String,
    name: String,
    mode: Option<String>,
    description: Option<String>,
    hidden: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    id: String,
    model_id: Option<String>,
    provider_id: Option<String>,
    name: String,
    enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeCatalog {
    agents: Option<Vec<AgentSummary>>,
    models: Option<Vec<ModelSummary>>,
    warnings: Vec<String>,
}

pub(crate) fn validate_local_base_url(base_url: &str) -> Result<Url, String> {
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

pub(crate) fn local_server_port(base_url: &str) -> Result<u16, String> {
    let url = validate_local_base_url(base_url)?;
    url.port()
        .filter(|port| *port != 0)
        .ok_or_else(|| "Especifica un puerto local válido para iniciar OpenCode".to_string())
}

fn create_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|_| "No se pudo preparar la conexión con OpenCode".to_string())
}

async fn request_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: Url,
    username: &str,
    password: &str,
    resource: &str,
) -> Result<T, String> {
    let response = client
        .get(url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(map_request_error)?;

    parse_json_response(response, resource).await
}

async fn post_json<TBody: Serialize, TResponse: DeserializeOwned>(
    client: &reqwest::Client,
    url: Url,
    username: &str,
    password: &str,
    body: &TBody,
    resource: &str,
) -> Result<TResponse, String> {
    let response = client
        .post(url)
        .basic_auth(username, Some(password))
        .json(body)
        .send()
        .await
        .map_err(map_request_error)?;

    parse_json_response(response, resource).await
}

async fn post_success<TBody: Serialize>(
    client: &reqwest::Client,
    url: Url,
    username: &str,
    password: &str,
    body: &TBody,
    resource: &str,
) -> Result<(), String> {
    let response = client
        .post(url)
        .basic_auth(username, Some(password))
        .json(body)
        .send()
        .await
        .map_err(map_request_error)?;

    if !response.status().is_success() {
        return Err(response_error(response.status(), resource));
    }
    if response.status() != reqwest::StatusCode::NO_CONTENT {
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !content_type.contains("application/json") {
            return Err(format!("La respuesta para {resource} no es JSON"));
        }
    }
    Ok(())
}

fn map_request_error(error: reqwest::Error) -> String {
    if error.is_connect() {
        "No hay un servidor OpenCode escuchando en esa dirección".to_string()
    } else if error.is_timeout() {
        "OpenCode tardó demasiado en responder".to_string()
    } else {
        "No se pudo completar la solicitud a OpenCode".to_string()
    }
}

async fn parse_json_response<T: DeserializeOwned>(
    response: reqwest::Response,
    resource: &str,
) -> Result<T, String> {
    let status = response.status();
    if !status.is_success() {
        return Err(response_error(status, resource));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !content_type.contains("application/json") {
        return Err(format!(
            "La respuesta para {resource} no es JSON; puede ser una página web o una ruta incompatible"
        ));
    }

    response
        .json()
        .await
        .map_err(|_| format!("OpenCode devolvió un formato no compatible para {resource}"))
}

fn response_error(status: reqwest::StatusCode, resource: &str) -> String {
    match status {
        reqwest::StatusCode::UNAUTHORIZED => {
            "OpenCode rechazó las credenciales del servidor".to_string()
        }
        reqwest::StatusCode::NOT_FOUND => {
            format!("OpenCode no reconoce el endpoint para {resource}; comprueba la versión")
        }
        reqwest::StatusCode::CONFLICT => {
            format!("OpenCode no pudo admitir {resource} porque la sesión está ocupada")
        }
        reqwest::StatusCode::BAD_REQUEST => {
            format!("OpenCode rechazó {resource}; comprueba la sesión y la versión")
        }
        _ => format!("OpenCode respondió con un error HTTP ({})", status.as_u16()),
    }
}

fn validate_session_id(session_id: &str) -> Result<(), String> {
    if !session_id.starts_with("ses")
        || session_id.len() > 256
        || session_id.contains('/')
        || session_id.contains('\\')
    {
        return Err("El identificador de sesión de OpenCode no es válido".to_string());
    }
    Ok(())
}

fn session_url(base_url: &str, session_id: &str, suffix: &[&str]) -> Result<Url, String> {
    validate_session_id(session_id)?;
    let mut url = validate_local_base_url(base_url)?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "No se pudo preparar la ruta de la API de OpenCode".to_string())?;
        segments.pop_if_empty();
        segments.push("api").push("session").push(session_id);
        for segment in suffix {
            segments.push(segment);
        }
    }
    Ok(url)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateSessionRequest<'a> {
    title: &'a str,
    agent: &'a str,
    model: ModelReference<'a>,
    location: SessionLocation<'a>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelReference<'a> {
    id: &'a str,
    #[serde(rename = "providerID")]
    provider_id: &'a str,
}

#[derive(Debug, Serialize)]
struct SessionLocation<'a> {
    directory: &'a str,
}

#[derive(Debug, Deserialize)]
struct CreateSessionResponse {
    data: CreatedSessionInfo,
}

#[derive(Debug, Deserialize)]
struct CreatedSessionInfo {
    id: String,
    location: SessionLocationResponse,
}

#[derive(Debug, Deserialize)]
struct SessionLocationResponse {
    directory: String,
}

#[derive(Debug)]
pub struct CreatedOpenCodeSession {
    pub session_id: String,
    pub location_directory: String,
    pub location_matches: bool,
}

#[derive(Serialize)]
struct PromptRequest<'a> {
    text: &'a str,
    delivery: &'static str,
}

#[derive(Debug, Serialize)]
struct PermissionReplyRequest {
    decision: &'static str,
}

#[derive(Debug, Deserialize)]
struct SessionLocationEnvelope {
    data: SessionInfoForLocation,
}

#[derive(Debug, Deserialize)]
struct SessionInfoForLocation {
    location: SessionLocationResponse,
}

#[derive(Debug, Deserialize)]
struct ActiveSessionsResponse {
    data: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PermissionRequestsResponse {
    data: Vec<ApiPermissionRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiPermissionRequest {
    id: String,
    #[serde(rename = "sessionID")]
    session_id: String,
    action: String,
    resources: Vec<String>,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPermission {
    pub id: String,
    pub action: String,
    pub resources: Vec<String>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub active: bool,
    pub latest_response: Option<String>,
    pub response_completed: bool,
    pub permissions: Vec<PendingPermission>,
    pub permission_warning: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssistantMessagesResponse {
    data: Vec<AssistantMessage>,
}

#[derive(Debug, Deserialize)]
struct AssistantMessage {
    #[serde(rename = "type")]
    message_type: String,
    content: Vec<AssistantContent>,
    time: Option<AssistantMessageTime>,
    finish: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssistantContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssistantMessageTime {
    completed: Option<f64>,
}

#[allow(clippy::too_many_arguments)]
pub async fn create_session(
    base_url: &str,
    username: &str,
    password: &str,
    directory: &str,
    title: &str,
    agent_id: &str,
    provider_id: &str,
    model_id: &str,
) -> Result<CreatedOpenCodeSession, String> {
    let mut url = validate_local_base_url(base_url)?;
    url.set_path("/api/session");
    let client = create_http_client()?;
    let body = CreateSessionRequest {
        title,
        agent: agent_id,
        model: ModelReference {
            id: model_id,
            provider_id,
        },
        location: SessionLocation { directory },
    };
    let response: CreateSessionResponse = post_json(
        &client,
        url,
        username,
        password,
        &body,
        "la creación de la sesión",
    )
    .await?;

    let location_matches = directories_match(directory, &response.data.location.directory);
    Ok(CreatedOpenCodeSession {
        session_id: response.data.id,
        location_directory: response.data.location.directory,
        location_matches,
    })
}

pub async fn verify_session_directory(
    base_url: &str,
    username: &str,
    password: &str,
    session_id: &str,
    expected_directory: &str,
) -> Result<(), String> {
    let url = session_url(base_url, session_id, &[])?;
    let response: SessionLocationEnvelope = request_json(
        &create_http_client()?,
        url,
        username,
        password,
        "la sesión de OpenCode",
    )
    .await?;

    if directories_match(expected_directory, &response.data.location.directory) {
        Ok(())
    } else {
        Err("La sesión ya no apunta al worktree asignado; no se enviará el mensaje".to_string())
    }
}

pub async fn send_session_prompt(
    base_url: &str,
    username: &str,
    password: &str,
    session_id: &str,
    text: &str,
) -> Result<(), String> {
    let url = session_url(base_url, session_id, &["prompt"])?;
    let request = PromptRequest {
        text,
        delivery: "queue",
    };
    post_success(
        &create_http_client()?,
        url,
        username,
        password,
        &request,
        "el mensaje",
    )
    .await
}

pub async fn session_snapshot(
    base_url: &str,
    username: &str,
    password: &str,
    session_id: &str,
    directory: &str,
) -> Result<SessionSnapshot, String> {
    verify_session_directory(base_url, username, password, session_id, directory).await?;
    let client = create_http_client()?;

    let mut active_url = validate_local_base_url(base_url)?;
    active_url.set_path("/api/session/active");
    let active: ActiveSessionsResponse = request_json(
        &client,
        active_url,
        username,
        password,
        "el estado de las sesiones",
    )
    .await?;

    let mut messages_url = session_url(base_url, session_id, &["message"])?;
    messages_url
        .query_pairs_mut()
        .append_pair("limit", "5")
        .append_pair("order", "desc")
        .append_pair("type", "assistant");
    let messages: AssistantMessagesResponse = request_json(
        &client,
        messages_url,
        username,
        password,
        "las respuestas de la sesión",
    )
    .await?;

    let latest = messages.data.into_iter().find_map(|message| {
        if message.message_type != "assistant" {
            return None;
        }
        let text = message
            .content
            .into_iter()
            .filter(|part| part.content_type == "text")
            .filter_map(|part| part.text)
            .collect::<Vec<_>>()
            .join("");
        if text.trim().is_empty() {
            return None;
        }
        Some((
            truncate_response(text),
            message.time.and_then(|time| time.completed).is_some() || message.finish.is_some(),
        ))
    });

    let mut permissions_url = validate_local_base_url(base_url)?;
    permissions_url.set_path("/api/permission/request");
    permissions_url
        .query_pairs_mut()
        .append_pair("location[directory]", directory);
    let (permissions, permission_warning) = match request_json::<PermissionRequestsResponse>(
        &client,
        permissions_url,
        username,
        password,
        "las aprobaciones pendientes",
    )
    .await
    {
        Ok(response) => (
            response
                .data
                .into_iter()
                .filter(|request| request.session_id == session_id)
                .map(|request| PendingPermission {
                    id: request.id,
                    action: request.action,
                    resources: request.resources,
                    message: request.message,
                })
                .collect(),
            None,
        ),
        Err(error) => (Vec::new(), Some(error)),
    };

    Ok(SessionSnapshot {
        active: active.data.contains_key(session_id),
        latest_response: latest.as_ref().map(|(text, _)| text.clone()),
        response_completed: latest.map(|(_, completed)| completed).unwrap_or(false),
        permissions,
        permission_warning,
    })
}

fn truncate_response(text: String) -> String {
    const MAX_CHARS: usize = 30_000;
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated}\n\n[Respuesta recortada en la vista; OpenCode conserva el mensaje completo]")
    } else {
        truncated
    }
}

pub async fn reply_to_permission(
    base_url: &str,
    username: &str,
    password: &str,
    session_id: &str,
    permission_id: &str,
    allow_once: bool,
) -> Result<(), String> {
    if !permission_id.starts_with("per")
        || permission_id.len() > 256
        || permission_id.contains('/')
        || permission_id.contains('\\')
    {
        return Err("El identificador de aprobación no es válido".to_string());
    }
    let url = session_url(
        base_url,
        session_id,
        &["permission", permission_id, "reply"],
    )?;
    let request = PermissionReplyRequest {
        decision: if allow_once { "once" } else { "reject" },
    };
    post_success(
        &create_http_client()?,
        url,
        username,
        password,
        &request,
        "la decisión de permiso",
    )
    .await
}

fn directories_match(expected: &str, actual: &str) -> bool {
    let (Ok(expected), Ok(actual)) = (
        std::fs::canonicalize(expected),
        std::fs::canonicalize(actual),
    ) else {
        return false;
    };

    #[cfg(windows)]
    {
        expected
            .to_string_lossy()
            .eq_ignore_ascii_case(&actual.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        expected == actual
    }
}

pub async fn check_connection(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<OpenCodeHealth, String> {
    let mut url = validate_local_base_url(base_url)?;
    url.set_path("/api/info");

    let info: OpenCodeInfo = request_json(
        &create_http_client()?,
        url,
        username,
        password,
        "la información del servidor",
    )
    .await?;

    Ok(OpenCodeHealth {
        healthy: true,
        version: info.version,
    })
}

pub async fn discover_catalog(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<OpenCodeCatalog, String> {
    let mut base_url = validate_local_base_url(base_url)?;
    let client = create_http_client()?;

    base_url.set_path("/api/agent");
    let agents = request_json::<OpenCodeList<OpenCodeAgent>>(
        &client,
        base_url.clone(),
        username,
        password,
        "los perfiles de agente",
    )
    .await;

    base_url.set_path("/api/model");
    let models = request_json::<OpenCodeList<OpenCodeModel>>(
        &client,
        base_url,
        username,
        password,
        "los modelos",
    )
    .await;

    let mut warnings = Vec::new();
    let agents = match agents {
        Ok(response) => Some(
            response
                .data
                .into_iter()
                .map(|agent| AgentSummary {
                    id: agent.id,
                    name: agent.name,
                    mode: agent.mode,
                    description: agent.description,
                    hidden: agent.hidden,
                })
                .collect(),
        ),
        Err(error) => {
            warnings.push(format!("Perfiles: {error}"));
            None
        }
    };
    let models = match models {
        Ok(response) => Some(
            response
                .data
                .into_iter()
                .map(|model| ModelSummary {
                    id: model.id,
                    model_id: model.model_id,
                    provider_id: model.provider_id,
                    name: model.name,
                    enabled: model.enabled,
                })
                .collect(),
        ),
        Err(error) => {
            warnings.push(format!("Modelos: {error}"));
            None
        }
    };

    if agents.is_none() && models.is_none() {
        return Err(format!(
            "No se pudo cargar el catálogo de OpenCode. {}",
            warnings.join(". ")
        ));
    }

    Ok(OpenCodeCatalog {
        agents,
        models,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        directories_match, validate_local_base_url, CreateSessionRequest, CreateSessionResponse,
        ModelReference, OpenCodeAgent, OpenCodeList, OpenCodeModel, SessionLocation,
    };

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
        assert!(validate_local_base_url("https://localhost:4096").is_err());
        assert!(validate_local_base_url("http://user:pass@localhost:4096").is_err());
    }

    #[test]
    fn reads_only_public_agent_and_model_catalog_fields() {
        let agents = serde_json::from_str::<OpenCodeList<OpenCodeAgent>>(
            r#"{"location":{},"data":[{"id":"build","name":"Build","mode":"subagent","hidden":false,"request":{},"permissions":[],"system":"private instructions"}]}"#,
        )
        .expect("agent list should match the supported API shape");
        assert_eq!(agents.data[0].id, "build");

        let models = serde_json::from_str::<OpenCodeList<OpenCodeModel>>(
            r#"{"location":{},"data":[{"id":"provider/model","modelID":"model","providerID":"provider","name":"Model","enabled":true,"settings":{"secret":"must not be deserialized"}}]}"#,
        )
        .expect("model list should match the supported API shape");
        assert_eq!(models.data[0].name, "Model");
        assert_eq!(models.data[0].model_id.as_deref(), Some("model"));
        assert_eq!(models.data[0].provider_id.as_deref(), Some("provider"));
        assert_eq!(models.data[0].enabled, Some(true));
    }

    #[test]
    fn creates_session_payload_with_exact_worktree_location_and_model_reference() {
        let request = CreateSessionRequest {
            title: "Backend",
            agent: "build",
            model: ModelReference {
                id: "gpt-5.2",
                provider_id: "openai",
            },
            location: SessionLocation {
                directory: r"C:\worktrees\backend",
            },
        };
        let json = serde_json::to_value(request).expect("session body should serialize");

        assert_eq!(json["title"], "Backend");
        assert_eq!(json["agent"], "build");
        assert_eq!(json["model"]["id"], "gpt-5.2");
        assert_eq!(json["model"]["providerID"], "openai");
        assert_eq!(json["location"]["directory"], r"C:\worktrees\backend");
    }

    #[test]
    fn reads_session_location_from_the_v2_create_response() {
        let directory = tempfile::tempdir().expect("worktree directory should be created");
        let response = serde_json::json!({
            "data": {
                "id": "ses_example",
                "projectID": "project_example",
                "cost": 0,
                "tokens": {},
                "time": {"created": 1, "updated": 1},
                "location": {"directory": directory.path().to_string_lossy()},
            }
        });
        let response = serde_json::from_value::<CreateSessionResponse>(response)
            .expect("session response should match the API contract");

        assert!(directories_match(
            &directory.path().to_string_lossy(),
            &response.data.location.directory
        ));
        assert_eq!(response.data.id, "ses_example");
    }

    #[test]
    fn prompt_is_queued_and_sent_only_as_request_text() {
        let request = super::PromptRequest {
            text: "Implement the isolated task",
            delivery: "queue",
        };
        let json = serde_json::to_value(request).expect("prompt request should serialize");

        assert_eq!(json["text"], "Implement the isolated task");
        assert_eq!(json["delivery"], "queue");
        assert_eq!(json.as_object().map(|fields| fields.len()), Some(2));
    }
}
