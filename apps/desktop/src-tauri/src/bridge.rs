use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tauri::async_runtime::JoinHandle;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::coordination;

pub const COORDINATION_CHANGED: &str = "coordination-changed";
const MCP_PATH: &str = "/mcp";
const FALLBACK_PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_PROTOCOL_VERSION_CHARS: usize = 32;
const MAX_CONTEXT_CHARS: usize = 12_000;

/// Aviso sin payload hacia la GUI. Se inyecta desde el ensamblado de Tauri para
/// que el puente no dependa del runtime de ventanas (y no arrastre la pila GUI
/// a las pruebas unitarias).
pub type Notify = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct Bridge {
    pub base_url: String,
    token: String,
}

impl Bridge {
    pub fn mcp_url(&self) -> String {
        format!("{}{MCP_PATH}", self.base_url)
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

struct RunningBridge {
    bridge: Bridge,
    task: JoinHandle<()>,
}

#[derive(Default)]
pub struct BridgeManager {
    running: Mutex<Option<RunningBridge>>,
    last_error: Mutex<Option<String>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeInfo {
    pub ready: bool,
    pub url: Option<String>,
    pub message: Option<String>,
}

impl BridgeManager {
    /// Inicia el servidor MCP local en un puerto efímero de loopback con un token
    /// de capacidad por ejecución. Solo acepta peticiones autenticadas y rechaza
    /// las que traigan cabecera `Origin` (páginas web).
    pub async fn start(&self, pool: SqlitePool, notify: Notify) -> Result<Bridge, String> {
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", 0)).await {
            Ok(listener) => listener,
            Err(_) => {
                let message = "No se pudo abrir el puente local de herramientas".to_string();
                *self.last_error.lock().await = Some(message.clone());
                return Err(message);
            }
        };
        let port = listener
            .local_addr()
            .map_err(|_| "No se pudo obtener el puerto del puente local".to_string())?
            .port();
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let bridge = Bridge {
            base_url: format!("http://127.0.0.1:{port}"),
            token: token.clone(),
        };

        let state = Arc::new(ServerState {
            pool,
            token,
            notify,
        });
        let router = Router::new()
            .route(
                MCP_PATH,
                post(handle_mcp)
                    .get(method_not_allowed)
                    .delete(close_session),
            )
            .layer(DefaultBodyLimit::max(512 * 1024))
            .with_state(state);

        let task = tauri::async_runtime::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let mut running = self.running.lock().await;
        if let Some(previous) = running.take() {
            previous.task.abort();
        }
        *running = Some(RunningBridge {
            bridge: bridge.clone(),
            task,
        });
        *self.last_error.lock().await = None;
        Ok(bridge)
    }

    pub async fn info(&self) -> BridgeInfo {
        let running = self.running.lock().await;
        match running.as_ref() {
            Some(running) => BridgeInfo {
                ready: true,
                url: Some(running.bridge.base_url.clone()),
                message: None,
            },
            None => BridgeInfo {
                ready: false,
                url: None,
                message: self.last_error.lock().await.clone(),
            },
        }
    }

    /// Copia del puente activo para que la coordinación escriba la configuración
    /// por proyecto. El token nunca cruza el IPC.
    pub async fn current(&self) -> Option<Bridge> {
        self.running
            .lock()
            .await
            .as_ref()
            .map(|running| running.bridge.clone())
    }

    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        if let Some(running) = running.take() {
            running.task.abort();
        }
    }
}

#[cfg(test)]
pub(crate) fn test_bridge(base_url: &str, token: &str) -> Bridge {
    Bridge {
        base_url: base_url.to_string(),
        token: token.to_string(),
    }
}

struct ServerState {
    pool: SqlitePool,
    token: String,
    notify: Notify,
}

impl ServerState {
    fn notify_change(&self) {
        (self.notify)();
    }
}

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

async fn method_not_allowed() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED
}

/// Terminación de sesión MCP (DELETE). No hay estado que liberar además del token.
async fn close_session(State(state): State<Arc<ServerState>>, headers: HeaderMap) -> StatusCode {
    match authorize(&state, &headers) {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(status) => status,
    }
}

async fn handle_mcp(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(status) = authorize(&state, &headers) {
        return status.into_response();
    }

    let request = match serde_json::from_slice::<JsonRpcRequest>(&body) {
        Ok(request) => request,
        Err(_) => {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32700, "message": "Cuerpo JSON-RPC no válido" }
            }))
            .into_response();
        }
    };

    let id = request.id.clone();
    let outcome = dispatch(&state, &request).await;

    // Notificación (sin id): la respuesta HTTP no lleva cuerpo JSON-RPC.
    let Some(id) = id else {
        return StatusCode::ACCEPTED.into_response();
    };

    match outcome {
        Ok(result) => {
            let mut response = Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            }))
            .into_response();
            if request.method == "initialize" {
                if let Ok(value) = Uuid::new_v4().simple().to_string().parse() {
                    response.headers_mut().insert("mcp-session-id", value);
                }
            }
            response
        }
        Err(error) => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": error.code, "message": error.message },
        }))
        .into_response(),
    }
}

struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    fn not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Método no soportado: {method}"),
        }
    }
}

async fn dispatch(state: &ServerState, request: &JsonRpcRequest) -> Result<Value, RpcError> {
    match request.method.as_str() {
        "initialize" => {
            let requested = request
                .params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .filter(|version| {
                    !version.is_empty()
                        && version.len() <= MAX_PROTOCOL_VERSION_CHARS
                        && version
                            .chars()
                            .all(|character| character.is_ascii_digit() || character == '-')
                })
                .unwrap_or(FALLBACK_PROTOCOL_VERSION);
            Ok(json!({
                "protocolVersion": requested,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": {
                    "name": "stade-studio",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }))
        }
        "notifications/initialized" | "notifications/cancelled" => Ok(Value::Null),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(state, &request.params).await,
        other => Err(RpcError::not_found(other)),
    }
}

fn authorize(state: &ServerState, headers: &HeaderMap) -> Result<(), StatusCode> {
    if headers.contains_key(header::ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if tokens_match(presented, &state.token) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn tokens_match(presented: &str, expected: &str) -> bool {
    let presented = presented.as_bytes();
    let expected = expected.as_bytes();
    if presented.len() != expected.len() {
        return false;
    }
    presented
        .iter()
        .zip(expected.iter())
        .fold(0_u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "get_task_context",
            "description": "Devuelve el contexto durable de la tarea asignada a esta sesión: objetivo, ámbito de archivos, estado y entregas aceptadas de sus dependencias. Úsala al comenzar o cuando dudes del alcance.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "submit_handoff",
            "description": "Entrega el trabajo terminado al coordinador con un resumen, artefactos e instrucciones para el siguiente rol. La tarea queda en revisión del usuario y no debes continuar trabajando en ella.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "summary": { "type": "string", "description": "Resumen de lo realizado y su estado" },
                    "artifacts": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Archivos, commits o rutas relevantes"
                    },
                    "next_instructions": { "type": "string", "description": "Instrucciones para el siguiente rol" },
                    "open_questions": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Preguntas o riesgos que quedan abiertos"
                    }
                },
                "required": ["summary"],
                "additionalProperties": false
            }
        },
        {
            "name": "complete_task",
            "description": "Marca la tarea como terminada cuando no dejas trabajo para otro rol. Queda también en revisión del usuario.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "summary": { "type": "string", "description": "Resumen final del resultado" }
                },
                "required": ["summary"],
                "additionalProperties": false
            }
        },
        {
            "name": "report_blocker",
            "description": "Registra un bloqueo técnico que impide continuar y pausa la tarea. Si lo que necesitas es una decisión del usuario, usa request_user_input.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "reason": { "type": "string", "description": "Motivo concreto del bloqueo" },
                    "details": { "type": "string", "description": "Contexto técnico adicional" }
                },
                "required": ["reason"],
                "additionalProperties": false
            }
        },
        {
            "name": "request_user_input",
            "description": "Pide una decisión al usuario y bloquea la tarea hasta que responda. La respuesta llegará como un mensaje nuevo en esta sesión.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "Pregunta concreta" },
                    "context": { "type": "string", "description": "Contexto necesario para decidir" },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Opciones sugeridas (opcional)"
                    }
                },
                "required": ["question"],
                "additionalProperties": false
            }
        }
    ])
}

async fn call_tool(state: &ServerState, params: &Value) -> Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid("La llamada no incluye el nombre de la herramienta"))?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    let session_id = params
        .get("_meta")
        .and_then(|meta| meta.get("sessionID"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let task = match coordination::task_for_session(&state.pool, &session_id).await {
        Ok(task) => task,
        Err(message) => return Ok(tool_error(message)),
    };

    let outcome = match name {
        "get_task_context" => task_context_text(state, task.id).await,
        "submit_handoff" => {
            let summary = required_string(&arguments, "summary")?;
            let artifacts = optional_string_list(&arguments, "artifacts")?;
            let next_instructions = optional_string(&arguments, "next_instructions")?;
            let open_questions = optional_string_list(&arguments, "open_questions")?;
            match coordination::record_handoff(
                &state.pool,
                task.id,
                "handoff",
                &summary,
                &artifacts,
                next_instructions.as_deref(),
                &open_questions,
            )
            .await
            {
                Ok(_) => Ok(format!(
                    "Entrega registrada para la tarea #{}. La tarea queda en revisión del usuario; detén el trabajo y espera. Si el usuario la devuelve, recibirás una nota en esta sesión.",
                    task.id
                )),
                Err(error) => Ok(error),
            }
        }
        "complete_task" => {
            let summary = required_string(&arguments, "summary")?;
            match coordination::record_handoff(
                &state.pool,
                task.id,
                "completion",
                &summary,
                &[],
                None,
                &[],
            )
            .await
            {
                Ok(_) => Ok(format!(
                    "Tarea #{} marcada como terminada. Queda pendiente de la revisión del usuario.",
                    task.id
                )),
                Err(error) => Ok(error),
            }
        }
        "report_blocker" => {
            let reason = required_string(&arguments, "reason")?;
            let details = optional_string(&arguments, "details")?;
            let reason_text = match details {
                Some(details) => format!("{reason}\n\nDetalles: {details}"),
                None => reason,
            };
            match coordination::record_blocker(&state.pool, task.id, &reason_text).await {
                Ok(_) => Ok(format!(
                    "Bloqueo registrado. La tarea #{} queda bloqueada y visible para el usuario; no continúes hasta que se resuelva.",
                    task.id
                )),
                Err(error) => Ok(error),
            }
        }
        "request_user_input" => {
            let question = required_string(&arguments, "question")?;
            let context = optional_string(&arguments, "context")?;
            let options = optional_string_list(&arguments, "options")?;
            match coordination::record_decision(
                &state.pool,
                task.id,
                &question,
                context.as_deref(),
                &options,
            )
            .await
            {
                Ok(_) => Ok(format!(
                    "Pregunta registrada. La tarea #{} queda bloqueada hasta que el usuario responda; la respuesta llegará como un mensaje nuevo en esta sesión.",
                    task.id
                )),
                Err(error) => Ok(error),
            }
        }
        other => {
            return Err(RpcError::invalid(format!(
                "Herramienta desconocida: {other}"
            )))
        }
    };

    let (text, is_error) = match outcome {
        Ok(text) => (text, false),
        Err(message) => (message, true),
    };
    if !is_error {
        state.notify_change();
    }
    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    }))
}

fn tool_error(message: String) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

async fn task_context_text(state: &ServerState, task_id: i64) -> Result<String, String> {
    let detail = coordination::get_detail(&state.pool, task_id).await?;
    let task = &detail.task;
    let mut text = String::new();
    text.push_str(&format!("Tarea #{} — «{}»\n", task.id, task.title));
    text.push_str(&format!("Estado: {}\n", status_label(&task.status)));
    text.push_str(&format!("Objetivo: {}\n", task.objective));
    text.push_str(&format!(
        "Ámbito de archivos declarado: {}\n",
        if task.file_scope.trim().is_empty() {
            "sin restricción declarada"
        } else {
            task.file_scope.trim()
        }
    ));
    if let Some(worktree_id) = task.worktree_id {
        if let Ok(worktree) = crate::worktrees::get_by_id(&state.pool, worktree_id).await {
            text.push_str(&format!(
                "Worktree: {} (rama {})\n",
                worktree.directory, worktree.branch_name
            ));
        }
    }

    let context = coordination::accepted_dependency_context(&state.pool, task_id).await?;
    if context.is_empty() {
        text.push_str("\nSin dependencias completadas: esta tarea no recibe entregas previas.\n");
    } else {
        text.push_str("\nEntregas aceptadas de dependencias:\n");
        for (dependency, handoff) in context {
            text.push_str(&format!(
                "\n### Tarea #{} — «{}»\n{}\n",
                dependency.id,
                dependency.title,
                handoff.summary.trim()
            ));
            if !handoff.artifacts.is_empty() {
                text.push_str(&format!("Artefactos: {}\n", handoff.artifacts.join(", ")));
            }
            if let Some(instructions) = handoff.next_instructions.as_deref() {
                text.push_str(&format!(
                    "Instrucciones para esta tarea: {}\n",
                    instructions.trim()
                ));
            }
            if !handoff.open_questions.is_empty() {
                text.push_str(&format!(
                    "Preguntas abiertas: {}\n",
                    handoff.open_questions.join("; ")
                ));
            }
        }
    }

    if let Some(decision) = coordination::open_decision_for_task(&state.pool, task_id).await? {
        text.push_str(&format!(
            "\nHay una pregunta pendiente del usuario: «{}»\nNo continúes hasta recibir la respuesta.\n",
            decision.question
        ));
    }

    text.push_str(
        "\nTrabaja solo dentro de este worktree. Al terminar usa submit_handoff o complete_task; si necesitas una decisión del usuario usa request_user_input y espera la respuesta.",
    );

    if text.chars().count() > MAX_CONTEXT_CHARS {
        let truncated: String = text.chars().take(MAX_CONTEXT_CHARS).collect();
        return Ok(format!(
            "{truncated}\n[Contexto recortado; consulta el resto en Stade Studio]"
        ));
    }
    Ok(text)
}

fn status_label(status: &str) -> &'static str {
    match status {
        "pending" => "pendiente de dependencias",
        "ready" => "lista para ejecutarse",
        "working" => "en curso",
        "blocked" => "bloqueada",
        "review" => "en revisión del usuario",
        "completed" => "completada",
        "failed" => "fallida",
        _ => "desconocida",
    }
}

fn required_string(arguments: &Value, key: &str) -> Result<String, RpcError> {
    optional_string(arguments, key)?
        .ok_or_else(|| RpcError::invalid(format!("Falta el campo obligatorio «{key}»")))
}

fn optional_string(arguments: &Value, key: &str) -> Result<Option<String>, RpcError> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(RpcError::invalid(format!(
            "El campo «{key}» debe ser texto"
        ))),
    }
}

fn optional_string_list(arguments: &Value, key: &str) -> Result<Vec<String>, RpcError> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => {
            let mut result = Vec::with_capacity(values.len());
            for value in values {
                let Some(text) = value.as_str() else {
                    return Err(RpcError::invalid(format!(
                        "El campo «{key}» debe ser una lista de textos"
                    )));
                };
                result.push(text.to_string());
            }
            Ok(result)
        }
        Some(_) => Err(RpcError::invalid(format!(
            "El campo «{key}» debe ser una lista de textos"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request;
    use sqlx::sqlite::SqlitePoolOptions;
    use tower::ServiceExt;

    fn test_pool() -> SqlitePool {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("in-memory SQLite should open");
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .expect("migrations should apply");
            pool
        })
    }

    fn test_router(pool: SqlitePool, token: &str) -> Router {
        let state = Arc::new(ServerState {
            pool,
            token: token.to_string(),
            notify: Arc::new(|| {}),
        });
        Router::new()
            .route(
                MCP_PATH,
                post(handle_mcp)
                    .get(method_not_allowed)
                    .delete(close_session),
            )
            .with_state(state)
    }

    fn request(body: Value, token: Option<&str>) -> Request<axum::body::Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri(MCP_PATH)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        builder
            .body(axum::body::Body::from(body.to_string()))
            .expect("request should build")
    }

    async fn body_json(response: Response) -> Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should be readable");
        serde_json::from_slice(&bytes).expect("body should be JSON")
    }

    #[test]
    fn rejects_requests_without_token_or_with_origin() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let router = test_router(pool, "secret-token");
            let unauthorized = router
                .clone()
                .oneshot(request(
                    json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}),
                    None,
                ))
                .await
                .expect("router should respond");
            assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

            let with_origin = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(MCP_PATH)
                        .header(header::AUTHORIZATION, "Bearer secret-token")
                        .header(header::ORIGIN, "https://example.com")
                        .body(axum::body::Body::from("{}"))
                        .expect("request should build"),
                )
                .await
                .expect("router should respond");
            assert_eq!(with_origin.status(), StatusCode::FORBIDDEN);

            let authorized = router
                .oneshot(request(
                    json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}),
                    Some("secret-token"),
                ))
                .await
                .expect("router should respond");
            assert_eq!(authorized.status(), StatusCode::OK);
        });
    }

    #[test]
    fn initializes_and_lists_the_coordination_tools() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let router = test_router(pool, "secret-token");
            let initialized = router
                .clone()
                .oneshot(request(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "initialize",
                        "params": { "protocolVersion": "2025-06-18" }
                    }),
                    Some("secret-token"),
                ))
                .await
                .expect("router should respond");
            assert_eq!(initialized.status(), StatusCode::OK);
            assert!(initialized.headers().contains_key("mcp-session-id"));
            let payload = body_json(initialized).await;
            assert_eq!(payload["result"]["protocolVersion"], "2025-06-18");
            assert_eq!(payload["result"]["serverInfo"]["name"], "stade-studio");

            let listed = router
                .oneshot(request(
                    json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
                    Some("secret-token"),
                ))
                .await
                .expect("router should respond");
            let payload = body_json(listed).await;
            let tools = payload["result"]["tools"]
                .as_array()
                .expect("tools should be an array");
            assert_eq!(tools.len(), 5);
            let names: Vec<&str> = tools
                .iter()
                .filter_map(|tool| tool["name"].as_str())
                .collect();
            assert!(names.contains(&"submit_handoff"));
            assert!(names.contains(&"request_user_input"));
        });
    }

    #[test]
    fn unknown_sessions_receive_a_clear_tool_error() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let router = test_router(pool, "secret-token");
            let response = router
                .oneshot(request(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 3,
                        "method": "tools/call",
                        "params": {
                            "name": "submit_handoff",
                            "arguments": { "summary": "Listo" },
                            "_meta": { "sessionID": "ses_desconocida" }
                        }
                    }),
                    Some("secret-token"),
                ))
                .await
                .expect("router should respond");
            let payload = body_json(response).await;
            assert_eq!(payload["result"]["isError"], true);
            let text = payload["result"]["content"][0]["text"]
                .as_str()
                .expect("tool text should be present");
            assert!(text.contains("no está registrada"));
        });
    }

    #[test]
    fn bridge_tool_call_moves_the_task_into_review_using_the_session_id() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let router = test_router(pool.clone(), "secret-token");

            let repository = tempfile::tempdir().expect("repository should exist");
            let worktrees_root = tempfile::tempdir().expect("worktrees root should exist");
            let run = |args: &[&str]| {
                let output = std::process::Command::new("git")
                    .arg("-C")
                    .arg(repository.path())
                    .args(args)
                    .output()
                    .expect("git should be installed");
                assert!(output.status.success(), "git setup should succeed");
            };
            std::fs::write(repository.path().join("README.md"), "test\n")
                .expect("file should be written");
            run(&["init"]);
            run(&["config", "user.name", "Stade Studio Test"]);
            run(&["config", "user.email", "conductor-test@example.invalid"]);
            run(&["add", "README.md"]);
            run(&["commit", "-m", "initial commit"]);

            let project = crate::projects::create(
                &pool,
                "Puente".to_string(),
                repository.path().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("project should be created");
            let worktree = crate::worktrees::create(
                &pool,
                worktrees_root.path(),
                project.id,
                "Entorno".to_string(),
            )
            .await
            .expect("worktree should be created");
            crate::worktrees::attach_opencode_session(
                &pool,
                worktree.id,
                "ses_bridge",
                "build",
                "openai",
                "gpt-test",
                &worktree.directory,
                true,
            )
            .await
            .expect("session should attach");
            let task = coordination::create(
                &pool,
                project.id,
                "Tarea con puente",
                "Objetivo",
                "build",
                "openai",
                "gpt-test",
                "",
                &[],
                None,
            )
            .await
            .expect("task should be created");
            sqlx::query("UPDATE tasks SET worktree_id = ? WHERE id = ?")
                .bind(worktree.id)
                .bind(task.id)
                .execute(&pool)
                .await
                .expect("task should link its worktree");
            coordination::mark_working(&pool, task.id, None)
                .await
                .expect("task should start");

            let context_call = router
                .clone()
                .oneshot(request(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 10,
                        "method": "tools/call",
                        "params": {
                            "name": "get_task_context",
                            "arguments": {},
                            "_meta": { "sessionID": "ses_bridge" }
                        }
                    }),
                    Some("secret-token"),
                ))
                .await
                .expect("router should respond");
            let payload = body_json(context_call).await;
            let text = payload["result"]["content"][0]["text"]
                .as_str()
                .expect("context should be text");
            assert!(text.contains("Tarea con puente"));
            assert!(text.contains("Objetivo"));

            let handoff_call = router
                .oneshot(request(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 11,
                        "method": "tools/call",
                        "params": {
                            "name": "submit_handoff",
                            "arguments": {
                                "summary": "Implementado",
                                "artifacts": ["src/api.rs"],
                                "next_instructions": "Revisa el contrato"
                            },
                            "_meta": { "sessionID": "ses_bridge" }
                        }
                    }),
                    Some("secret-token"),
                ))
                .await
                .expect("router should respond");
            let payload = body_json(handoff_call).await;
            assert_eq!(payload["result"]["isError"], false);

            let updated = coordination::get(&pool, task.id)
                .await
                .expect("task should load");
            assert_eq!(updated.status, coordination::STATUS_REVIEW);
            let detail = coordination::get_detail(&pool, task.id)
                .await
                .expect("detail should load");
            assert_eq!(detail.handoffs.len(), 1);
            assert_eq!(detail.handoffs[0].kind, "handoff");
            assert_eq!(detail.handoffs[0].artifacts, vec!["src/api.rs".to_string()]);
        });
    }

    #[test]
    fn serves_the_mcp_endpoint_over_real_http() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let manager = BridgeManager::default();
            let bridge = manager
                .start(pool, Arc::new(|| {}))
                .await
                .expect("bridge should start");
            assert!(manager.info().await.ready);

            let client = reqwest::Client::new();
            let unauthorized = client
                .post(bridge.mcp_url())
                .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}))
                .send()
                .await
                .expect("request should reach the bridge");
            assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);

            let authorized = client
                .post(bridge.mcp_url())
                .bearer_auth(bridge.token())
                .json(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
                .send()
                .await
                .expect("request should reach the bridge");
            assert_eq!(authorized.status(), reqwest::StatusCode::OK);
            let payload: Value = authorized.json().await.expect("payload should be JSON");
            assert!(
                payload["result"]["tools"]
                    .as_array()
                    .map(|tools| tools.len())
                    .unwrap_or(0)
                    >= 5
            );

            manager.stop().await;
            assert!(!manager.info().await.ready);
        });
    }

    #[test]
    #[ignore = "smoke test against the locally installed OpenCode CLI"]
    fn installed_opencode_connects_to_the_coordination_bridge() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
                .expect("an unused local port should be available");
            let port = listener.local_addr().expect("addr should resolve").port();
            drop(listener);

            let root = tempfile::tempdir().expect("worktrees root should exist");
            let manager = BridgeManager::default();
            let bridge = manager
                .start(pool, Arc::new(|| {}))
                .await
                .expect("bridge should start");
            crate::dispatch::write_project_bridge_config(root.path(), 9, &bridge)
                .expect("bridge config should be written");
            let worktree = root.path().join("project-9").join("worktree-1");
            std::fs::create_dir_all(&worktree).expect("worktree directory should exist");

            let process = crate::opencode_process::OpenCodeProcessManager::default();
            let started = process
                .start(&format!("http://127.0.0.1:{port}"))
                .await
                .expect("the installed OpenCode server should start");

            let client = reqwest::Client::new();
            let mut last_status: Option<String> = None;
            let mut last_payload = Value::Null;
            for _ in 0..24 {
                let response = client
                    .get(format!("{}/api/mcp", started.base_url))
                    .basic_auth(&started.username, Some(&started.password))
                    .query(&[("location[directory]", worktree.to_string_lossy().as_ref())])
                    .send()
                    .await
                    .expect("the MCP list endpoint should respond");
                assert!(response.status().is_success());
                let payload: Value = response.json().await.expect("payload should be JSON");
                last_payload = payload.clone();
                let entry = payload["data"]
                    .as_array()
                    .and_then(|servers| servers.iter().find(|server| server["name"] == "stade"));
                last_status = entry
                    .and_then(|entry| {
                        entry["status"]["status"]
                            .as_str()
                            .or_else(|| entry["status"]["type"].as_str())
                    })
                    .map(str::to_string);
                if last_status.as_deref() == Some("connected") {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            assert_eq!(
                last_status.as_deref(),
                Some("connected"),
                "OpenCode debería conectar el puente MCP; última respuesta: {last_payload}"
            );

            process
                .stop()
                .await
                .expect("the managed OpenCode server should stop");
            manager.stop().await;
        });
    }

    #[test]
    fn notifications_do_not_require_a_json_rpc_body() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let router = test_router(pool, "secret-token");
            let response = router
                .oneshot(request(
                    json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
                    Some("secret-token"),
                ))
                .await
                .expect("router should respond");
            assert_eq!(response.status(), StatusCode::ACCEPTED);
        });
    }
}
