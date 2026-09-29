use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_READY: &str = "ready";
pub const STATUS_WORKING: &str = "working";
pub const STATUS_BLOCKED: &str = "blocked";
pub const STATUS_REVIEW: &str = "review";
pub const STATUS_COMPLETED: &str = "completed";
pub const STATUS_FAILED: &str = "failed";

const ACTIVE_STATUSES: &str = "'working', 'blocked', 'review'";
const MAX_TITLE_CHARS: usize = 120;
const MAX_OBJECTIVE_CHARS: usize = 8_000;
const MAX_SUMMARY_CHARS: usize = 8_000;
const MAX_INSTRUCTIONS_CHARS: usize = 4_000;
const MAX_QUESTION_CHARS: usize = 2_000;
const MAX_CONTEXT_CHARS: usize = 4_000;
const MAX_SCOPE_CHARS: usize = 4_000;
const MAX_LIST_ITEMS: usize = 50;
const MAX_LIST_ITEM_CHARS: usize = 500;
const ACTIVITY_KEEP_PER_TASK: i64 = 200;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub project_id: i64,
    pub worktree_id: Option<i64>,
    #[serde(skip_serializing)]
    pub role_id: Option<i64>,
    pub title: String,
    pub objective: String,
    pub status: String,
    pub origin: String,
    pub agent_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub file_scope: String,
    pub blocker_reason: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

const TASK_COLUMNS: &str = "id, project_id, worktree_id, role_id, title, objective, status, origin, agent_id, provider_id, \
     model_id, file_scope, blocker_reason, last_error, created_at, updated_at, started_at, completed_at";

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TaskRef {
    pub id: i64,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct HandoffRecord {
    pub id: i64,
    pub task_id: i64,
    pub kind: String,
    pub summary: String,
    pub artifacts: Vec<String>,
    pub next_instructions: Option<String>,
    pub open_questions: Vec<String>,
    pub created_at: String,
    pub reviewed_at: Option<String>,
    pub review_decision: Option<String>,
    pub review_note: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Handoff {
    pub id: i64,
    pub task_id: i64,
    pub kind: String,
    pub summary: String,
    pub artifacts: Vec<String>,
    pub next_instructions: Option<String>,
    pub open_questions: Vec<String>,
    pub created_at: String,
    pub reviewed_at: Option<String>,
    pub review_decision: Option<String>,
    pub review_note: Option<String>,
}

impl From<HandoffRecord> for Handoff {
    fn from(record: HandoffRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            kind: record.kind,
            summary: record.summary,
            artifacts: record.artifacts,
            next_instructions: record.next_instructions,
            open_questions: record.open_questions,
            created_at: record.created_at,
            reviewed_at: record.reviewed_at,
            review_decision: record.review_decision,
            review_note: record.review_note,
        }
    }
}

#[derive(Debug, FromRow)]
struct HandoffRow {
    id: i64,
    task_id: i64,
    kind: String,
    summary: String,
    artifacts: String,
    next_instructions: Option<String>,
    open_questions: String,
    created_at: String,
    reviewed_at: Option<String>,
    review_decision: Option<String>,
    review_note: Option<String>,
}

const HANDOFF_COLUMNS: &str =
    "id, task_id, kind, summary, artifacts, next_instructions, open_questions, \
     created_at, reviewed_at, review_decision, review_note";

impl From<HandoffRow> for HandoffRecord {
    fn from(row: HandoffRow) -> Self {
        Self {
            id: row.id,
            task_id: row.task_id,
            kind: row.kind,
            summary: row.summary,
            artifacts: parse_string_list(&row.artifacts),
            next_instructions: row.next_instructions,
            open_questions: parse_string_list(&row.open_questions),
            created_at: row.created_at,
            reviewed_at: row.reviewed_at,
            review_decision: row.review_decision,
            review_note: row.review_note,
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserDecision {
    pub id: i64,
    pub task_id: i64,
    pub question: String,
    pub context: Option<String>,
    pub options: Vec<String>,
    pub status: String,
    pub answer: Option<String>,
    pub created_at: String,
    pub answered_at: Option<String>,
}

#[derive(Debug, FromRow)]
struct UserDecisionRow {
    id: i64,
    task_id: i64,
    question: String,
    context: Option<String>,
    options: String,
    status: String,
    answer: Option<String>,
    created_at: String,
    answered_at: Option<String>,
}

const DECISION_COLUMNS: &str =
    "id, task_id, question, context, options, status, answer, created_at, answered_at";

impl From<UserDecisionRow> for UserDecision {
    fn from(row: UserDecisionRow) -> Self {
        Self {
            id: row.id,
            task_id: row.task_id,
            question: row.question,
            context: row.context,
            options: parse_string_list(&row.options),
            status: row.status,
            answer: row.answer,
            created_at: row.created_at,
            answered_at: row.answered_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: i64,
    pub task_id: i64,
    pub kind: String,
    pub detail: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    pub task: Task,
    pub depends_on: Vec<TaskRef>,
    pub dependents: Vec<TaskRef>,
    pub handoffs: Vec<Handoff>,
    pub decisions: Vec<UserDecision>,
    pub activity: Vec<ActivityEntry>,
    pub scope_conflicts: Vec<TaskRef>,
}

pub async fn list_project(pool: &SqlitePool, project_id: i64) -> Result<Vec<TaskDetail>, String> {
    let query = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE project_id = ? ORDER BY id ASC");
    let tasks = sqlx::query_as::<_, Task>(&query)
        .bind(project_id)
        .fetch_all(pool)
        .await
        .map_err(|_| "No se pudieron cargar las tareas del proyecto".to_string())?;

    let mut details = Vec::with_capacity(tasks.len());
    for task in tasks {
        details.push(detail_for_task(pool, task).await?);
    }
    Ok(details)
}

pub async fn get_detail(pool: &SqlitePool, task_id: i64) -> Result<TaskDetail, String> {
    let task = get(pool, task_id).await?;
    detail_for_task(pool, task).await
}

pub async fn get(pool: &SqlitePool, task_id: i64) -> Result<Task, String> {
    let query = format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?");
    sqlx::query_as::<_, Task>(&query)
        .bind(task_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo consultar la tarea".to_string())?
        .ok_or_else(|| "La tarea ya no existe".to_string())
}

async fn detail_for_task(pool: &SqlitePool, task: Task) -> Result<TaskDetail, String> {
    let depends_on = sqlx::query_as::<_, TaskRef>(
        "SELECT t.id, t.title, t.status FROM task_dependencies d \
         JOIN tasks t ON t.id = d.depends_on_task_id \
         WHERE d.task_id = ? ORDER BY t.id ASC",
    )
    .bind(task.id)
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudieron cargar las dependencias de la tarea".to_string())?;

    let dependents = sqlx::query_as::<_, TaskRef>(
        "SELECT t.id, t.title, t.status FROM task_dependencies d \
         JOIN tasks t ON t.id = d.task_id \
         WHERE d.depends_on_task_id = ? ORDER BY t.id ASC",
    )
    .bind(task.id)
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudieron cargar las tareas dependientes".to_string())?;

    let handoff_query =
        format!("SELECT {HANDOFF_COLUMNS} FROM handoffs WHERE task_id = ? ORDER BY id DESC");
    let handoffs = sqlx::query_as::<_, HandoffRow>(&handoff_query)
        .bind(task.id)
        .fetch_all(pool)
        .await
        .map_err(|_| "No se pudieron cargar las entregas de la tarea".to_string())?
        .into_iter()
        .map(|row| Handoff::from(HandoffRecord::from(row)))
        .collect();

    let decision_query =
        format!("SELECT {DECISION_COLUMNS} FROM user_decisions WHERE task_id = ? ORDER BY id DESC");
    let decisions = sqlx::query_as::<_, UserDecisionRow>(&decision_query)
        .bind(task.id)
        .fetch_all(pool)
        .await
        .map_err(|_| "No se pudieron cargar las decisiones de la tarea".to_string())?
        .into_iter()
        .map(UserDecision::from)
        .collect();

    let activity = sqlx::query_as::<_, ActivityEntry>(
        "SELECT id, task_id, kind, detail, created_at FROM task_activity \
         WHERE task_id = ? ORDER BY id DESC LIMIT ?",
    )
    .bind(task.id)
    .bind(ACTIVITY_KEEP_PER_TASK)
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudo cargar la actividad de la tarea".to_string())?;

    let scope_conflicts = scope_conflicts_for(pool, &task).await?;

    Ok(TaskDetail {
        task,
        depends_on,
        dependents,
        handoffs,
        decisions,
        activity,
        scope_conflicts,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    project_id: i64,
    title: &str,
    objective: &str,
    agent_id: &str,
    provider_id: &str,
    model_id: &str,
    file_scope: &str,
    depends_on: &[i64],
) -> Result<Task, String> {
    let title = validate_text(title, MAX_TITLE_CHARS, "El título de la tarea")?;
    let objective = validate_text(objective, MAX_OBJECTIVE_CHARS, "El objetivo de la tarea")?;
    let agent_id = validate_text(agent_id, 200, "El perfil de agente")?;
    let provider_id = validate_text(provider_id, 200, "El proveedor del modelo")?;
    let model_id = validate_text(model_id, 300, "El modelo")?;
    let file_scope = validate_optional_text(file_scope, MAX_SCOPE_CHARS, "El ámbito de archivos")?
        .unwrap_or_default();
    let mut depends_on = depends_on.to_vec();
    depends_on.sort_unstable();
    depends_on.dedup();

    let project =
        sqlx::query_as::<_, (Option<String>,)>("SELECT archived_at FROM projects WHERE id = ?")
            .bind(project_id)
            .fetch_optional(pool)
            .await
            .map_err(|_| "No se pudo consultar el proyecto".to_string())?
            .ok_or_else(|| "El proyecto ya no existe".to_string())?;
    if project.0.is_some() {
        return Err("Restaura el proyecto antes de planificar tareas".to_string());
    }

    if !depends_on.is_empty() {
        let placeholders = vec!["?"; depends_on.len()].join(", ");
        let query = format!("SELECT id FROM tasks WHERE project_id = ? AND id IN ({placeholders})");
        let mut request = sqlx::query_scalar::<_, i64>(&query).bind(project_id);
        for dependency in &depends_on {
            request = request.bind(dependency);
        }
        let found = request
            .fetch_all(pool)
            .await
            .map_err(|_| "No se pudieron validar las dependencias".to_string())?;
        if found.len() != depends_on.len() {
            return Err("Alguna dependencia no existe o pertenece a otro proyecto".to_string());
        }
    }

    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| "No se pudo iniciar el registro de la tarea".to_string())?;

    let insert_query = format!(
        "INSERT INTO tasks (project_id, title, objective, status, agent_id, provider_id, model_id, file_scope, origin) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'user') RETURNING {TASK_COLUMNS}"
    );
    let task = sqlx::query_as::<_, Task>(&insert_query)
        .bind(project_id)
        .bind(&title)
        .bind(&objective)
        .bind(STATUS_PENDING)
        .bind(&agent_id)
        .bind(&provider_id)
        .bind(&model_id)
        .bind(&file_scope)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| "No se pudo guardar la tarea".to_string())?;

    for dependency in &depends_on {
        sqlx::query("INSERT INTO task_dependencies (task_id, depends_on_task_id) VALUES (?, ?)")
            .bind(task.id)
            .bind(dependency)
            .execute(&mut *transaction)
            .await
            .map_err(|_| "No se pudieron guardar las dependencias".to_string())?;
    }

    insert_activity_tx(&mut transaction, task.id, "created", None).await?;

    transaction
        .commit()
        .await
        .map_err(|_| "No se pudo confirmar el registro de la tarea".to_string())?;

    refresh_project(pool, project_id).await?;
    get(pool, task.id).await
}

pub async fn update_definition(
    pool: &SqlitePool,
    task_id: i64,
    title: &str,
    objective: &str,
    file_scope: &str,
) -> Result<Task, String> {
    let title = validate_text(title, MAX_TITLE_CHARS, "El título de la tarea")?;
    let objective = validate_text(objective, MAX_OBJECTIVE_CHARS, "El objetivo de la tarea")?;
    let file_scope = validate_optional_text(file_scope, MAX_SCOPE_CHARS, "El ámbito de archivos")?
        .unwrap_or_default();

    let query = format!(
        "UPDATE tasks SET title = ?, objective = ?, file_scope = ?, last_error = NULL, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ? AND status IN ('pending', 'ready', 'failed') RETURNING {TASK_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Task>(&query)
        .bind(title)
        .bind(objective)
        .bind(file_scope)
        .bind(task_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo actualizar la tarea".to_string())?
        .ok_or_else(|| "La tarea ya empezó o está en revisión; no se puede editar".to_string())?;

    insert_activity(pool, task_id, "updated", None).await?;
    Ok(updated)
}

/// Recalcula `pending`/`ready` para las tareas que aún no se han lanzado: una tarea
/// está lista solo cuando todas sus dependencias están completadas (y por tanto
/// tienen una entrega aceptada).
pub async fn refresh_project(pool: &SqlitePool, project_id: i64) -> Result<(), String> {
    let candidates = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, status FROM tasks WHERE project_id = ? AND status IN ('pending', 'ready')",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudo recalcular el plan".to_string())?;

    for (task_id, status) in candidates {
        let unmet = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM task_dependencies d \
             JOIN tasks dependency ON dependency.id = d.depends_on_task_id \
             WHERE d.task_id = ? AND dependency.status <> 'completed'",
        )
        .bind(task_id)
        .fetch_one(pool)
        .await
        .map_err(|_| "No se pudo recalcular el plan".to_string())?;

        let next = if unmet == 0 {
            STATUS_READY
        } else {
            STATUS_PENDING
        };
        if next != status {
            sqlx::query(
                "UPDATE tasks SET status = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
                 WHERE id = ?",
            )
            .bind(next)
            .bind(task_id)
            .execute(pool)
            .await
            .map_err(|_| "No se pudo recalcular el plan".to_string())?;
        }
    }
    Ok(())
}

pub async fn mark_working(
    pool: &SqlitePool,
    task_id: i64,
    detail: Option<&str>,
) -> Result<Task, String> {
    let task = get(pool, task_id).await?;
    if !matches!(
        task.status.as_str(),
        STATUS_READY | STATUS_BLOCKED | STATUS_REVIEW
    ) {
        return Err("La tarea no está en un estado desde el que pueda ejecutarse".to_string());
    }

    let query = format!(
        "UPDATE tasks SET status = ?, blocker_reason = NULL, last_error = NULL, \
         started_at = COALESCE(started_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ? RETURNING {TASK_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Task>(&query)
        .bind(STATUS_WORKING)
        .bind(task_id)
        .fetch_one(pool)
        .await
        .map_err(|_| "No se pudo marcar la tarea como en curso".to_string())?;

    insert_activity(pool, task_id, "working", detail).await?;
    Ok(updated)
}

#[allow(clippy::too_many_arguments)]
pub async fn record_handoff(
    pool: &SqlitePool,
    task_id: i64,
    kind: &str,
    summary: &str,
    artifacts: &[String],
    next_instructions: Option<&str>,
    open_questions: &[String],
) -> Result<HandoffRecord, String> {
    if kind != "handoff" && kind != "completion" {
        return Err("Tipo de entrega no válido".to_string());
    }
    let summary = validate_text(summary, MAX_SUMMARY_CHARS, "El resumen de la entrega")?;
    let artifacts = validate_string_list(artifacts, "Los artefactos")?;
    let open_questions = validate_string_list(open_questions, "Las preguntas abiertas")?;
    let next_instructions = validate_optional_text(
        next_instructions.unwrap_or_default(),
        MAX_INSTRUCTIONS_CHARS,
        "Las instrucciones para la siguiente tarea",
    )?;

    let task = get(pool, task_id).await?;
    if !matches!(task.status.as_str(), STATUS_WORKING | STATUS_BLOCKED) {
        return Err("La tarea no está en ejecución; no se puede registrar una entrega".to_string());
    }

    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| "No se pudo registrar la entrega".to_string())?;

    let insert_query = format!(
        "INSERT INTO handoffs (task_id, kind, summary, artifacts, next_instructions, open_questions) \
         VALUES (?, ?, ?, ?, ?, ?) RETURNING {HANDOFF_COLUMNS}"
    );
    let handoff = sqlx::query_as::<_, HandoffRow>(&insert_query)
        .bind(task_id)
        .bind(kind)
        .bind(&summary)
        .bind(serde_json::to_string(&artifacts).unwrap_or_else(|_| "[]".to_string()))
        .bind(&next_instructions)
        .bind(serde_json::to_string(&open_questions).unwrap_or_else(|_| "[]".to_string()))
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| "No se pudo guardar la entrega".to_string())?;

    sqlx::query(
        "UPDATE tasks SET status = ?, blocker_reason = NULL, last_error = NULL, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(STATUS_REVIEW)
    .bind(task_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| "No se pudo actualizar la tarea tras la entrega".to_string())?;

    let detail = if kind == "handoff" {
        "Entrega enviada para revisión"
    } else {
        "Tarea terminada por el agente; pendiente de revisión"
    };
    insert_activity_tx(&mut transaction, task_id, "handoff_submitted", Some(detail)).await?;

    transaction
        .commit()
        .await
        .map_err(|_| "No se pudo confirmar la entrega".to_string())?;

    Ok(HandoffRecord::from(handoff))
}

pub async fn record_blocker(pool: &SqlitePool, task_id: i64, reason: &str) -> Result<Task, String> {
    let reason = validate_text(reason, MAX_SUMMARY_CHARS, "El motivo del bloqueo")?;
    let task = get(pool, task_id).await?;
    if !matches!(task.status.as_str(), STATUS_WORKING | STATUS_BLOCKED) {
        return Err("La tarea no está en ejecución; no se puede registrar un bloqueo".to_string());
    }

    let query = format!(
        "UPDATE tasks SET status = ?, blocker_reason = ?, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? RETURNING {TASK_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Task>(&query)
        .bind(STATUS_BLOCKED)
        .bind(&reason)
        .bind(task_id)
        .fetch_one(pool)
        .await
        .map_err(|_| "No se pudo registrar el bloqueo".to_string())?;

    insert_activity(pool, task_id, "blocked", Some("Bloqueo reportado")).await?;
    Ok(updated)
}

pub async fn record_decision(
    pool: &SqlitePool,
    task_id: i64,
    question: &str,
    context: Option<&str>,
    options: &[String],
) -> Result<UserDecision, String> {
    let question = validate_text(question, MAX_QUESTION_CHARS, "La pregunta")?;
    let context = validate_optional_text(
        context.unwrap_or_default(),
        MAX_CONTEXT_CHARS,
        "El contexto de la pregunta",
    )?;
    let options = validate_string_list(options, "Las opciones")?;

    let task = get(pool, task_id).await?;
    if !matches!(task.status.as_str(), STATUS_WORKING | STATUS_BLOCKED) {
        return Err("La tarea no está en ejecución; no se puede pedir una decisión".to_string());
    }
    let open = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM user_decisions WHERE task_id = ? AND status = 'open'",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await
    .map_err(|_| "No se pudo comprobar la pregunta pendiente".to_string())?;
    if open > 0 {
        return Err("Ya hay una pregunta pendiente de respuesta para esta tarea".to_string());
    }

    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| "No se pudo registrar la pregunta".to_string())?;

    let insert_query = format!(
        "INSERT INTO user_decisions (task_id, question, context, options, status) \
         VALUES (?, ?, ?, ?, 'open') RETURNING {DECISION_COLUMNS}"
    );
    let decision = sqlx::query_as::<_, UserDecisionRow>(&insert_query)
        .bind(task_id)
        .bind(&question)
        .bind(&context)
        .bind(serde_json::to_string(&options).unwrap_or_else(|_| "[]".to_string()))
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| "No se pudo guardar la pregunta".to_string())?;

    sqlx::query(
        "UPDATE tasks SET status = ?, blocker_reason = ?, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(STATUS_BLOCKED)
    .bind(&question)
    .bind(task_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| "No se pudo actualizar la tarea con la pregunta".to_string())?;

    insert_activity_tx(
        &mut transaction,
        task_id,
        "decision_requested",
        Some("Pregunta enviada al usuario"),
    )
    .await?;

    transaction
        .commit()
        .await
        .map_err(|_| "No se pudo confirmar la pregunta".to_string())?;

    Ok(UserDecision::from(decision))
}

pub async fn accept_handoff(pool: &SqlitePool, handoff_id: i64) -> Result<Task, String> {
    let handoff = get_handoff(pool, handoff_id).await?;
    if handoff.reviewed_at.is_some() {
        return Err("Esta entrega ya fue revisada".to_string());
    }
    let task = get(pool, handoff.task_id).await?;
    if task.status != STATUS_REVIEW {
        return Err("La tarea no está en revisión".to_string());
    }

    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| "No se pudo aceptar la entrega".to_string())?;

    sqlx::query(
        "UPDATE handoffs SET reviewed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), \
         review_decision = 'accepted' WHERE id = ?",
    )
    .bind(handoff_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| "No se pudo aceptar la entrega".to_string())?;

    let query = format!(
        "UPDATE tasks SET status = ?, blocker_reason = NULL, completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? RETURNING {TASK_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Task>(&query)
        .bind(STATUS_COMPLETED)
        .bind(task.id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| "No se pudo completar la tarea".to_string())?;

    insert_activity_tx(
        &mut transaction,
        task.id,
        "handoff_accepted",
        Some("Entrega aceptada"),
    )
    .await?;

    transaction
        .commit()
        .await
        .map_err(|_| "No se pudo confirmar la aceptación".to_string())?;

    refresh_project(pool, task.project_id).await?;
    Ok(updated)
}

pub async fn return_handoff(
    pool: &SqlitePool,
    handoff_id: i64,
    note: &str,
) -> Result<Task, String> {
    let note = validate_text(note, MAX_SUMMARY_CHARS, "La nota de devolución")?;
    let handoff = get_handoff(pool, handoff_id).await?;
    if handoff.reviewed_at.is_some() {
        return Err("Esta entrega ya fue revisada".to_string());
    }
    let task = get(pool, handoff.task_id).await?;
    if task.status != STATUS_REVIEW {
        return Err("La tarea no está en revisión".to_string());
    }

    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| "No se pudo devolver la entrega".to_string())?;

    sqlx::query(
        "UPDATE handoffs SET reviewed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), \
         review_decision = 'returned', review_note = ? WHERE id = ?",
    )
    .bind(&note)
    .bind(handoff_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| "No se pudo devolver la entrega".to_string())?;

    let query = format!(
        "UPDATE tasks SET status = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ? RETURNING {TASK_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Task>(&query)
        .bind(STATUS_WORKING)
        .bind(task.id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| "No se pudo reabrir la tarea".to_string())?;

    insert_activity_tx(
        &mut transaction,
        task.id,
        "handoff_returned",
        Some("Entrega devuelta con nota"),
    )
    .await?;

    transaction
        .commit()
        .await
        .map_err(|_| "No se pudo confirmar la devolución".to_string())?;

    Ok(updated)
}

pub async fn answer_decision(
    pool: &SqlitePool,
    decision_id: i64,
    answer: &str,
) -> Result<Task, String> {
    let answer = validate_text(answer, MAX_SUMMARY_CHARS, "La respuesta")?;
    let decision = get_decision(pool, decision_id).await?;
    if decision.status != "open" {
        return Err("Esta pregunta ya fue respondida".to_string());
    }
    let task = get(pool, decision.task_id).await?;
    if task.status != STATUS_BLOCKED {
        return Err("La tarea no está esperando una respuesta".to_string());
    }

    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| "No se pudo registrar la respuesta".to_string())?;

    sqlx::query(
        "UPDATE user_decisions SET status = 'answered', answer = ?, \
         answered_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(&answer)
    .bind(decision_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| "No se pudo guardar la respuesta".to_string())?;

    let query = format!(
        "UPDATE tasks SET status = ?, blocker_reason = NULL, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? RETURNING {TASK_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Task>(&query)
        .bind(STATUS_WORKING)
        .bind(task.id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| "No se pudo reanudar la tarea".to_string())?;

    insert_activity_tx(
        &mut transaction,
        task.id,
        "decision_answered",
        Some("El usuario respondió la pregunta"),
    )
    .await?;

    transaction
        .commit()
        .await
        .map_err(|_| "No se pudo confirmar la respuesta".to_string())?;

    Ok(updated)
}

pub async fn complete_manually(
    pool: &SqlitePool,
    task_id: i64,
    summary: &str,
) -> Result<Task, String> {
    let summary = validate_text(summary, MAX_SUMMARY_CHARS, "El resumen de cierre")?;
    let task = get(pool, task_id).await?;
    if matches!(task.status.as_str(), STATUS_COMPLETED) {
        return Err("La tarea ya está completada".to_string());
    }

    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| "No se pudo cerrar la tarea".to_string())?;

    let insert_query = format!(
        "INSERT INTO handoffs (task_id, kind, summary, artifacts, open_questions, reviewed_at, review_decision) \
         VALUES (?, 'completion', ?, '[]', '[]', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'accepted') \
         RETURNING {HANDOFF_COLUMNS}"
    );
    sqlx::query_as::<_, HandoffRow>(&insert_query)
        .bind(task_id)
        .bind(&summary)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| "No se pudo guardar el cierre".to_string())?;

    let query = format!(
        "UPDATE tasks SET status = ?, blocker_reason = NULL, \
         completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? RETURNING {TASK_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Task>(&query)
        .bind(STATUS_COMPLETED)
        .bind(task_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| "No se pudo completar la tarea".to_string())?;

    insert_activity_tx(
        &mut transaction,
        task_id,
        "completed_manually",
        Some("Cierre registrado por el usuario"),
    )
    .await?;

    transaction
        .commit()
        .await
        .map_err(|_| "No se pudo confirmar el cierre".to_string())?;

    refresh_project(pool, task.project_id).await?;
    Ok(updated)
}

pub async fn mark_failed(pool: &SqlitePool, task_id: i64, note: &str) -> Result<Task, String> {
    let note = validate_text(note, MAX_SUMMARY_CHARS, "La nota")?;
    let task = get(pool, task_id).await?;
    if task.status == STATUS_COMPLETED {
        return Err("Una tarea completada no puede marcarse como fallida".to_string());
    }

    let query = format!(
        "UPDATE tasks SET status = ?, last_error = ?, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? RETURNING {TASK_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Task>(&query)
        .bind(STATUS_FAILED)
        .bind(&note)
        .bind(task_id)
        .fetch_one(pool)
        .await
        .map_err(|_| "No se pudo marcar la tarea como fallida".to_string())?;

    insert_activity(pool, task_id, "failed", Some("Marcada como fallida")).await?;

    refresh_project(pool, task.project_id).await?;
    Ok(updated)
}

pub async fn reopen(pool: &SqlitePool, task_id: i64) -> Result<Task, String> {
    let task = get(pool, task_id).await?;
    if !matches!(task.status.as_str(), STATUS_FAILED | STATUS_COMPLETED) {
        return Err("Solo se pueden reabrir tareas completadas o fallidas".to_string());
    }

    sqlx::query(
        "UPDATE tasks SET status = ?, completed_at = NULL, blocker_reason = NULL, \
         last_error = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(STATUS_PENDING)
    .bind(task_id)
    .execute(pool)
    .await
    .map_err(|_| "No se pudo reabrir la tarea".to_string())?;

    insert_activity(pool, task_id, "reopened", None).await?;
    refresh_project(pool, task.project_id).await?;
    get(pool, task_id).await
}

pub async fn record_dispatch_note(
    pool: &SqlitePool,
    task_id: i64,
    detail: &str,
) -> Result<(), String> {
    insert_activity(pool, task_id, "dispatched", Some(detail)).await
}

pub async fn record_note(
    pool: &SqlitePool,
    task_id: i64,
    kind: &str,
    detail: &str,
) -> Result<(), String> {
    insert_activity(pool, task_id, kind, Some(detail)).await
}

/// Resuelve la tarea activa a partir del `_meta.sessionID` que OpenCode envía en
/// cada llamada MCP. El modelo no aporta identificadores de tarea ni de proyecto.
pub async fn task_for_session(pool: &SqlitePool, session_id: &str) -> Result<Task, String> {
    if session_id.trim().is_empty() || session_id.len() > 256 {
        return Err("La llamada no incluye una sesión de OpenCode válida".to_string());
    }

    let query = format!(
        "SELECT {TASK_COLUMNS} FROM tasks \
         WHERE worktree_id IN (SELECT id FROM worktrees WHERE opencode_session_id = ?) \
         AND status IN ({ACTIVE_STATUSES}) ORDER BY id DESC LIMIT 1"
    );
    let task = sqlx::query_as::<_, Task>(&query)
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo resolver la tarea de esta sesión".to_string())?;

    if let Some(task) = task {
        return Ok(task);
    }

    let known = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM worktrees WHERE opencode_session_id = ?",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .map_err(|_| "No se pudo resolver la sesión".to_string())?;
    if known == 0 {
        return Err(
            "Esta sesión de OpenCode no está registrada en Stade Studio; no hay una tarea que pueda usar estas herramientas"
                .to_string(),
        );
    }

    Err("Esta sesión no tiene una tarea en curso en Stade Studio".to_string())
}

/// Contexto que recibe el agente: dependencias con sus entregas aceptadas y la
/// última devolución, si existe.
pub async fn accepted_dependency_context(
    pool: &SqlitePool,
    task_id: i64,
) -> Result<Vec<(Task, HandoffRecord)>, String> {
    let dependencies = sqlx::query_as::<_, TaskRef>(
        "SELECT t.id, t.title, t.status FROM task_dependencies d \
         JOIN tasks t ON t.id = d.depends_on_task_id \
         WHERE d.task_id = ? ORDER BY t.id ASC",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudieron cargar las dependencias".to_string())?;

    let mut context = Vec::new();
    for dependency in dependencies {
        let task = get(pool, dependency.id).await?;
        let handoff = latest_accepted_handoff(pool, dependency.id).await?;
        if let Some(handoff) = handoff {
            context.push((task, handoff));
        }
    }
    Ok(context)
}

pub async fn latest_accepted_handoff(
    pool: &SqlitePool,
    task_id: i64,
) -> Result<Option<HandoffRecord>, String> {
    let query = format!(
        "SELECT {HANDOFF_COLUMNS} FROM handoffs \
         WHERE task_id = ? AND review_decision = 'accepted' ORDER BY id DESC LIMIT 1"
    );
    sqlx::query_as::<_, HandoffRow>(&query)
        .bind(task_id)
        .fetch_optional(pool)
        .await
        .map(|row| row.map(HandoffRecord::from))
        .map_err(|_| "No se pudo cargar la entrega aceptada".to_string())
}

pub async fn open_decision_for_task(
    pool: &SqlitePool,
    task_id: i64,
) -> Result<Option<UserDecision>, String> {
    let query = format!(
        "SELECT {DECISION_COLUMNS} FROM user_decisions WHERE task_id = ? AND status = 'open' LIMIT 1"
    );
    sqlx::query_as::<_, UserDecisionRow>(&query)
        .bind(task_id)
        .fetch_optional(pool)
        .await
        .map(|row| row.map(UserDecision::from))
        .map_err(|_| "No se pudo cargar la pregunta pendiente".to_string())
}

async fn get_handoff(pool: &SqlitePool, handoff_id: i64) -> Result<HandoffRecord, String> {
    let query = format!("SELECT {HANDOFF_COLUMNS} FROM handoffs WHERE id = ?");
    sqlx::query_as::<_, HandoffRow>(&query)
        .bind(handoff_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo consultar la entrega".to_string())?
        .map(HandoffRecord::from)
        .ok_or_else(|| "La entrega ya no existe".to_string())
}

pub async fn handoff(pool: &SqlitePool, handoff_id: i64) -> Result<HandoffRecord, String> {
    get_handoff(pool, handoff_id).await
}

pub async fn decision(pool: &SqlitePool, decision_id: i64) -> Result<UserDecision, String> {
    get_decision(pool, decision_id).await
}

async fn get_decision(pool: &SqlitePool, decision_id: i64) -> Result<UserDecision, String> {
    let query = format!("SELECT {DECISION_COLUMNS} FROM user_decisions WHERE id = ?");
    sqlx::query_as::<_, UserDecisionRow>(&query)
        .bind(decision_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo consultar la pregunta".to_string())?
        .map(UserDecision::from)
        .ok_or_else(|| "La pregunta ya no existe".to_string())
}

pub async fn scope_conflicts_for(pool: &SqlitePool, task: &Task) -> Result<Vec<TaskRef>, String> {
    if task.file_scope.trim().is_empty() {
        return Ok(Vec::new());
    }
    let active = sqlx::query_as::<_, Task>(&format!(
        "SELECT {TASK_COLUMNS} FROM tasks WHERE project_id = ? AND id <> ? \
             AND status IN ({ACTIVE_STATUSES})"
    ))
    .bind(task.project_id)
    .bind(task.id)
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudo comprobar el solapamiento de archivos".to_string())?;

    let scope = normalize_scope(&task.file_scope);
    let mut conflicts = Vec::new();
    for other in active {
        let other_scope = normalize_scope(&other.file_scope);
        if scopes_overlap(&scope, &other_scope) {
            conflicts.push(TaskRef {
                id: other.id,
                title: other.title,
                status: other.status,
            });
        }
    }
    Ok(conflicts)
}

fn normalize_scope(scope: &str) -> Vec<String> {
    scope
        .split(['\n', ',', ';'])
        .map(|entry| entry.trim().trim_start_matches("./").replace('\\', "/"))
        .map(|entry| entry.trim_end_matches('/').to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
        .collect()
}

fn scopes_overlap(left: &[String], right: &[String]) -> bool {
    for a in left {
        let a_base = base_entry(a);
        for b in right {
            let b_base = base_entry(b);
            if a_base.is_empty() || b_base.is_empty() {
                return true;
            }
            if a_base == b_base
                || a_base.starts_with(&format!("{b_base}/"))
                || b_base.starts_with(&format!("{a_base}/"))
            {
                return true;
            }
        }
    }
    false
}

fn base_entry(entry: &str) -> &str {
    entry
        .split('*')
        .next()
        .unwrap_or(entry)
        .trim_end_matches('/')
}

fn validate_text(value: &str, max_chars: usize, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{label} no puede estar vacío"));
    }
    if value.chars().count() > max_chars {
        return Err(format!("{label} no puede superar {max_chars} caracteres"));
    }
    Ok(value.to_string())
}

fn validate_optional_text(
    value: &str,
    max_chars: usize,
    label: &str,
) -> Result<Option<String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max_chars {
        return Err(format!("{label} no puede superar {max_chars} caracteres"));
    }
    Ok(Some(value.to_string()))
}

fn validate_string_list(values: &[String], label: &str) -> Result<Vec<String>, String> {
    if values.len() > MAX_LIST_ITEMS {
        return Err(format!(
            "{label} no pueden superar {MAX_LIST_ITEMS} entradas"
        ));
    }
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > MAX_LIST_ITEM_CHARS {
            return Err(format!(
                "{label} no pueden superar {MAX_LIST_ITEM_CHARS} caracteres por entrada"
            ));
        }
        result.push(value.to_string());
    }
    Ok(result)
}

fn parse_string_list(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

async fn insert_activity(
    pool: &SqlitePool,
    task_id: i64,
    kind: &str,
    detail: Option<&str>,
) -> Result<(), String> {
    sqlx::query("INSERT INTO task_activity (task_id, kind, detail) VALUES (?, ?, ?)")
        .bind(task_id)
        .bind(kind)
        .bind(detail)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|_| "No se pudo registrar la actividad de la tarea".to_string())
}

async fn insert_activity_tx(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task_id: i64,
    kind: &str,
    detail: Option<&str>,
) -> Result<(), String> {
    sqlx::query("INSERT INTO task_activity (task_id, kind, detail) VALUES (?, ?, ?)")
        .bind(task_id)
        .bind(kind)
        .bind(detail)
        .execute(&mut **transaction)
        .await
        .map(|_| ())
        .map_err(|_| "No se pudo registrar la actividad de la tarea".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

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
                .expect("coordination migrations should apply");
            pool
        })
    }

    async fn create_project(pool: &SqlitePool) -> i64 {
        let project = crate::projects::create(
            pool,
            "Coordinación".to_string(),
            tempfile::tempdir()
                .expect("temp dir should exist")
                .keep()
                .to_string_lossy()
                .into_owned(),
            None,
        )
        .await
        .expect("project should be created");
        project.id
    }

    async fn create_task(pool: &SqlitePool, project_id: i64, title: &str) -> Task {
        create(
            pool,
            project_id,
            title,
            "Objetivo de prueba",
            "build",
            "openai",
            "gpt-test",
            "",
            &[],
        )
        .await
        .expect("task should be created")
    }

    #[test]
    fn new_task_without_dependencies_is_ready() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let project_id = create_project(&pool).await;
            let task = create_task(&pool, project_id, "Primera").await;
            assert_eq!(task.status, STATUS_READY);
        });
    }

    #[test]
    fn dependencies_gate_readiness_until_handoff_is_accepted() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let project_id = create_project(&pool).await;
            let first = create_task(&pool, project_id, "Base de datos").await;
            let second = create(
                &pool,
                project_id,
                "Backend",
                "Consumir el esquema",
                "build",
                "openai",
                "gpt-test",
                "",
                &[first.id],
            )
            .await
            .expect("dependent task should be created");
            assert_eq!(second.status, STATUS_PENDING);

            mark_working(&pool, first.id, None)
                .await
                .expect("task should start");
            let handoff = record_handoff(
                &pool,
                first.id,
                "handoff",
                "Esquema listo",
                &["migrations/0001.sql".to_string()],
                Some("Construye la API"),
                &[],
            )
            .await
            .expect("handoff should be recorded");
            assert_eq!(
                get(&pool, first.id).await.expect("task should load").status,
                STATUS_REVIEW
            );
            assert_eq!(
                get(&pool, second.id)
                    .await
                    .expect("dependent task should load")
                    .status,
                STATUS_PENDING
            );

            accept_handoff(&pool, handoff.id)
                .await
                .expect("handoff should be accepted");
            assert_eq!(
                get(&pool, first.id).await.expect("task should load").status,
                STATUS_COMPLETED
            );
            assert_eq!(
                get(&pool, second.id)
                    .await
                    .expect("dependent task should load")
                    .status,
                STATUS_READY
            );
        });
    }

    #[test]
    fn returning_a_handoff_reopens_the_task_and_blocks_dependents() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let project_id = create_project(&pool).await;
            let first = create_task(&pool, project_id, "Primera").await;
            let second = create(
                &pool,
                project_id,
                "Dependiente",
                "Objetivo",
                "build",
                "openai",
                "gpt-test",
                "",
                &[first.id],
            )
            .await
            .expect("dependent task should be created");
            mark_working(&pool, first.id, None).await.expect("start");
            let handoff = record_handoff(&pool, first.id, "handoff", "Entrega", &[], None, &[])
                .await
                .expect("handoff should be recorded");
            return_handoff(&pool, handoff.id, "Falta una prueba")
                .await
                .expect("handoff should be returned");
            assert_eq!(
                get(&pool, first.id).await.expect("task should load").status,
                STATUS_WORKING
            );
            let twice = accept_handoff(&pool, handoff.id)
                .await
                .expect_err("a returned handoff cannot be accepted");
            assert!(twice.contains("revisada") || twice.contains("revisión"));
            assert_eq!(
                get(&pool, second.id)
                    .await
                    .expect("dependent task should load")
                    .status,
                STATUS_PENDING
            );
        });
    }

    #[test]
    fn decision_blocks_and_answer_resumes_the_task() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let project_id = create_project(&pool).await;
            let task = create_task(&pool, project_id, "Con pregunta").await;
            mark_working(&pool, task.id, None).await.expect("start");

            let decision = record_decision(
                &pool,
                task.id,
                "¿Postgres o SQLite?",
                Some("Afecta migraciones"),
                &["Postgres".to_string(), "SQLite".to_string()],
            )
            .await
            .expect("decision should be recorded");
            let blocked = get(&pool, task.id).await.expect("task should load");
            assert_eq!(blocked.status, STATUS_BLOCKED);
            assert_eq!(
                blocked.blocker_reason.as_deref(),
                Some("¿Postgres o SQLite?")
            );

            let duplicate = record_decision(&pool, task.id, "Otra", None, &[])
                .await
                .expect_err("second open decision should be rejected");
            assert!(duplicate.contains("pendiente"));

            answer_decision(&pool, decision.id, "Postgres")
                .await
                .expect("decision should be answered");
            let resumed = get(&pool, task.id).await.expect("task should load");
            assert_eq!(resumed.status, STATUS_WORKING);
            assert!(resumed.blocker_reason.is_none());
            assert!(open_decision_for_task(&pool, task.id)
                .await
                .expect("open decision should load")
                .is_none());
        });
    }

    fn init_clean_repository(path: &std::path::Path) {
        let run = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(path)
                .args(args)
                .output()
                .expect("git should be installed for coordination tests");
            assert!(output.status.success(), "git test setup should succeed");
        };
        std::fs::write(path.join("README.md"), "test repo\n").expect("file should be written");
        run(&["init"]);
        run(&["config", "user.name", "Stade Studio Test"]);
        run(&["config", "user.email", "conductor-test@example.invalid"]);
        run(&["add", "README.md"]);
        run(&["commit", "-m", "initial commit"]);
    }

    #[test]
    fn session_mapping_rejects_unknown_sessions_and_resolves_active_tasks() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let repository = tempfile::tempdir().expect("repository should exist");
            let worktrees_root = tempfile::tempdir().expect("worktrees root should exist");
            init_clean_repository(repository.path());
            let project = crate::projects::create(
                &pool,
                "Con sesión".to_string(),
                repository.path().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("project should be created");

            let unknown = task_for_session(&pool, "ses_desconocida")
                .await
                .expect_err("unknown session should not resolve");
            assert!(unknown.contains("no está registrada"));

            let worktree = crate::worktrees::create(
                &pool,
                worktrees_root.path(),
                project.id,
                "Entorno de prueba".to_string(),
            )
            .await
            .expect("worktree should be created");
            crate::worktrees::attach_opencode_session(
                &pool,
                worktree.id,
                "ses_test",
                "build",
                "openai",
                "gpt-test",
                &worktree.directory,
                true,
            )
            .await
            .expect("session should attach");

            let task = create_task(&pool, project.id, "Con sesión").await;
            let without_worktree = task_for_session(&pool, "ses_test")
                .await
                .expect_err("an idle session should not resolve to a task");
            assert!(without_worktree.contains("no tiene una tarea en curso"));

            sqlx::query("UPDATE tasks SET worktree_id = ? WHERE id = ?")
                .bind(worktree.id)
                .bind(task.id)
                .execute(&pool)
                .await
                .expect("task should link its worktree");
            mark_working(&pool, task.id, None)
                .await
                .expect("task should start");
            let resolved = task_for_session(&pool, "ses_test")
                .await
                .expect("active task should resolve");
            assert_eq!(resolved.id, task.id);

            record_handoff(&pool, task.id, "handoff", "Entrega", &[], None, &[])
                .await
                .expect("handoff should be recorded");
            let in_review = task_for_session(&pool, "ses_test")
                .await
                .expect("a task in review still belongs to its session");
            assert_eq!(in_review.status, STATUS_REVIEW);

            let handoff = latest_accepted_handoff(&pool, task.id)
                .await
                .expect("query should succeed");
            assert!(handoff.is_none());
        });
    }

    #[test]
    fn scope_conflicts_detect_overlapping_paths() {
        assert!(scopes_overlap(
            &normalize_scope("apps/desktop/src"),
            &normalize_scope("apps/desktop/src/tasks")
        ));
        assert!(scopes_overlap(
            &normalize_scope("src/**/*.rs"),
            &normalize_scope("src/lib.rs")
        ));
        assert!(!scopes_overlap(
            &normalize_scope("apps/desktop/src"),
            &normalize_scope("docs")
        ));
        assert!(!scopes_overlap(
            &normalize_scope(""),
            &normalize_scope("docs")
        ));
    }
}
