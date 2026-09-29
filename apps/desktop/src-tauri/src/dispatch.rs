use std::path::Path;

use serde::Serialize;

use crate::{bridge::Bridge, coordination, opencode, worktrees};

pub const MAX_PROMPT_CHARS: usize = 20_000;
const MAX_HANDOFF_SUMMARY_IN_PROMPT: usize = 2_000;
const HANDOFF_CONTEXT_BUDGET: usize = 14_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchOutcome {
    pub task_id: i64,
    pub ok: bool,
    pub error: Option<String>,
}

/// Escribe la configuración generada que registra el puente MCP en el
/// directorio administrado de worktrees del proyecto. Nunca toca el repositorio
/// del usuario: vive junto a los worktrees y OpenCode la hereda desde el
/// directorio de la sesión.
pub fn write_project_bridge_config(
    worktrees_root: &Path,
    project_id: i64,
    bridge: &Bridge,
) -> Result<(), String> {
    let directory = worktrees_root.join(format!("project-{project_id}"));
    std::fs::create_dir_all(&directory)
        .map_err(|_| "No se pudo preparar la configuración del puente local".to_string())?;
    let config = serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "mcp": {
            "servers": {
                "stade": {
                    "type": "remote",
                    "url": bridge.mcp_url(),
                    "headers": {
                        "Authorization": format!("Bearer {}", bridge.token()),
                    },
                    "codemode": false,
                }
            }
        }
    });
    let serialized = serde_json::to_string_pretty(&config)
        .map_err(|_| "No se pudo serializar la configuración del puente".to_string())?;
    std::fs::write(directory.join("opencode.json"), serialized)
        .map_err(|_| "No se pudo guardar la configuración del puente local".to_string())
}

#[allow(clippy::too_many_arguments)]
pub async fn start_task(
    pool: &sqlx::SqlitePool,
    worktrees_root: &Path,
    base_url: &str,
    username: &str,
    password: &str,
    bridge: &Bridge,
    task_id: i64,
    allow_scope_conflicts: bool,
    use_fallback: bool,
) -> Result<coordination::Task, String> {
    // El modelo alternativo solo se aplica por decisión explícita del usuario,
    // nunca automáticamente. Solo afecta a la creación de la sesión.
    if use_fallback {
        coordination::apply_role_fallback(pool, task_id).await?;
    }
    let task = coordination::get(pool, task_id).await?;
    match task.status.as_str() {
        coordination::STATUS_READY => {}
        coordination::STATUS_PENDING => {
            return Err(
                "La tarea aún espera a que sus dependencias se completen y se acepten".to_string(),
            );
        }
        coordination::STATUS_WORKING => return Err("La tarea ya está en curso".to_string()),
        coordination::STATUS_REVIEW => {
            return Err("La tarea está en revisión; acepta o devuelve su entrega".to_string());
        }
        coordination::STATUS_BLOCKED => {
            return Err(
                "La tarea está bloqueada; responde la pregunta pendiente o reactívala".to_string(),
            );
        }
        coordination::STATUS_COMPLETED => return Err("La tarea ya está completada".to_string()),
        coordination::STATUS_FAILED => {
            return Err("Reabre la tarea antes de volver a lanzarla".to_string());
        }
        _ => return Err("La tarea no se puede lanzar en su estado actual".to_string()),
    }

    let project = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT name, root_path, archived_at FROM projects WHERE id = ?",
    )
    .bind(task.project_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "No se pudo consultar el proyecto".to_string())?
    .ok_or_else(|| "El proyecto ya no existe".to_string())?;
    if project.2.is_some() {
        return Err("Restaura el proyecto antes de lanzar tareas".to_string());
    }

    if !allow_scope_conflicts {
        let detail = coordination::get_detail(pool, task_id).await?;
        if !detail.scope_conflicts.is_empty() {
            let titles = detail
                .scope_conflicts
                .iter()
                .map(|conflict| format!("#{} «{}»", conflict.id, conflict.title))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "El ámbito de archivos se solapa con tareas activas: {titles}. Confirma el solapamiento para continuar"
            ));
        }
    }

    // 1. Worktree propio de la tarea (se reutiliza si ya existe).
    let worktree = match task.worktree_id {
        Some(worktree_id) => {
            let worktree = worktrees::get_by_id(pool, worktree_id).await?;
            if worktree.status != "ready" {
                return Err("El entorno aislado de la tarea no está listo".to_string());
            }
            worktrees::validate_managed_directory(worktrees_root, &worktree.directory)?;
            worktree
        }
        None => {
            if !Path::new(&project.1).join(".git").exists() {
                return Err(
                    "El proyecto debe ser un repositorio Git para aislar la tarea en un worktree"
                        .to_string(),
                );
            }
            let label: String = task.title.chars().take(80).collect();
            let worktree = worktrees::create(pool, worktrees_root, task.project_id, label).await?;
            sqlx::query(
                "UPDATE tasks SET worktree_id = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
            )
            .bind(worktree.id)
            .bind(task.id)
            .execute(pool)
            .await
            .map_err(|_| "No se pudo asociar el entorno a la tarea".to_string())?;
            worktree
        }
    };

    // 2. Configuración del puente de herramientas para este proyecto.
    write_project_bridge_config(worktrees_root, task.project_id, bridge)?;

    // 3. Sesión OpenCode en la ruta exacta del worktree.
    let session_id = match worktree.opencode_session_id.as_deref() {
        Some(session_id) => {
            opencode::verify_session_directory(
                base_url,
                username,
                password,
                session_id,
                &worktree.directory,
            )
            .await?;
            session_id.to_string()
        }
        None => {
            let created = opencode::create_session(
                base_url,
                username,
                password,
                &worktree.directory,
                &worktree.label,
                &task.agent_id,
                &task.provider_id,
                &task.model_id,
            )
            .await?;
            if !created.session_id.starts_with("ses") || created.session_id.len() > 256 {
                return Err(
                    "OpenCode devolvió un identificador de sesión no válido; no se asoció al entorno"
                        .to_string(),
                );
            }
            worktrees::attach_opencode_session(
                pool,
                worktree.id,
                &created.session_id,
                &task.agent_id,
                &task.provider_id,
                &task.model_id,
                &created.location_directory,
                created.location_matches,
            )
            .await?;
            created.session_id
        }
    };

    // 4. Prompt con el contexto durable de las dependencias.
    let context = coordination::accepted_dependency_context(pool, task_id).await?;
    let role_instructions = match task.role_id {
        Some(role_id) => crate::roles::get(pool, role_id)
            .await
            .ok()
            .and_then(|role| role.instructions.clone()),
        None => None,
    };
    let prompt = build_task_prompt(&task, &context, role_instructions.as_deref())?;

    if let Err(error) =
        opencode::send_session_prompt(base_url, username, password, &session_id, &prompt).await
    {
        coordination::record_note(
            pool,
            task_id,
            "dispatch_failed",
            "No se pudo enviar el prompt",
        )
        .await?;
        return Err(format!("No se pudo enviar la tarea a OpenCode: {error}"));
    }

    coordination::record_dispatch_note(pool, task_id, &format!("Enviada a la sesión {session_id}"))
        .await?;
    coordination::mark_working(pool, task_id, None).await
}

#[allow(clippy::too_many_arguments)]
pub async fn start_ready_tasks(
    pool: &sqlx::SqlitePool,
    worktrees_root: &Path,
    base_url: &str,
    username: &str,
    password: &str,
    bridge: &Bridge,
    project_id: i64,
) -> Result<Vec<DispatchOutcome>, String> {
    coordination::refresh_project(pool, project_id).await?;
    let details = coordination::list_project(pool, project_id).await?;
    let ready: Vec<i64> = details
        .into_iter()
        .filter(|detail| detail.task.status == coordination::STATUS_READY)
        .map(|detail| detail.task.id)
        .collect();

    let mut outcomes = Vec::with_capacity(ready.len());
    for task_id in ready {
        let result = start_task(
            pool,
            worktrees_root,
            base_url,
            username,
            password,
            bridge,
            task_id,
            false,
            false,
        )
        .await;
        outcomes.push(match result {
            Ok(_) => DispatchOutcome {
                task_id,
                ok: true,
                error: None,
            },
            Err(error) => DispatchOutcome {
                task_id,
                ok: false,
                error: Some(error),
            },
        });
    }
    Ok(outcomes)
}

pub async fn return_handoff_with_note(
    pool: &sqlx::SqlitePool,
    worktrees_root: &Path,
    base_url: &str,
    username: &str,
    password: &str,
    handoff_id: i64,
    note: &str,
) -> Result<coordination::Task, String> {
    let handoff = coordination::handoff(pool, handoff_id).await?;
    let task = coordination::get(pool, handoff.task_id).await?;
    let prompt = format!(
        "El usuario devolvió tu entrega de la tarea #{} «{}» con esta nota:\n\n{}\n\nRetoma la tarea y ajusta lo necesario. Cuando termines, registra una nueva entrega con submit_handoff.",
        task.id, task.title, note.trim()
    );
    send_to_task_session(
        pool,
        worktrees_root,
        base_url,
        username,
        password,
        &task,
        &prompt,
    )
    .await?;
    coordination::return_handoff(pool, handoff_id, note).await
}

pub async fn answer_decision_with_prompt(
    pool: &sqlx::SqlitePool,
    worktrees_root: &Path,
    base_url: &str,
    username: &str,
    password: &str,
    decision_id: i64,
    answer: &str,
) -> Result<coordination::Task, String> {
    let decision = coordination::decision(pool, decision_id).await?;
    let task = coordination::get(pool, decision.task_id).await?;
    let prompt = format!(
        "El usuario respondió tu pregunta pendiente en la tarea #{} «{}»:\n\nPregunta: {}\nRespuesta: {}\n\nContinúa con la tarea según esta decisión. Puedes consultar el contexto con get_task_context.",
        task.id,
        task.title,
        decision.question,
        answer.trim()
    );
    send_to_task_session(
        pool,
        worktrees_root,
        base_url,
        username,
        password,
        &task,
        &prompt,
    )
    .await?;
    coordination::answer_decision(pool, decision_id, answer).await
}

async fn send_to_task_session(
    pool: &sqlx::SqlitePool,
    worktrees_root: &Path,
    base_url: &str,
    username: &str,
    password: &str,
    task: &coordination::Task,
    prompt: &str,
) -> Result<(), String> {
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err("El mensaje para el agente es demasiado largo".to_string());
    }
    let worktree_id = task
        .worktree_id
        .ok_or_else(|| "La tarea no tiene un entorno aislado asignado".to_string())?;
    let worktree = worktrees::get_by_id(pool, worktree_id).await?;
    if worktree.status != "ready" {
        return Err("El entorno aislado de la tarea no está listo".to_string());
    }
    if worktree.opencode_location_matches != Some(true) {
        return Err(
            "La ubicación de la sesión no está verificada; no se enviará trabajo".to_string(),
        );
    }
    worktrees::validate_managed_directory(worktrees_root, &worktree.directory)?;
    let session_id = worktree
        .opencode_session_id
        .as_deref()
        .ok_or_else(|| "La tarea todavía no tiene una sesión OpenCode".to_string())?;
    opencode::verify_session_directory(
        base_url,
        username,
        password,
        session_id,
        &worktree.directory,
    )
    .await?;
    opencode::send_session_prompt(base_url, username, password, session_id, prompt).await
}

pub fn build_task_prompt(
    task: &coordination::Task,
    context: &[(coordination::Task, coordination::HandoffRecord)],
    role_instructions: Option<&str>,
) -> Result<String, String> {
    let mut prompt = String::new();
    prompt.push_str("Tarea coordinada por Stade Studio.\n\n");
    prompt.push_str(&format!("Tarea #{} — «{}»\n", task.id, task.title));
    prompt.push_str(&format!("Objetivo:\n{}\n", task.objective.trim()));
    if let Some(instructions) = role_instructions
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        prompt.push_str(&format!("\nInstrucciones de tu rol:\n{instructions}\n"));
    }
    prompt.push_str(&format!(
        "\nÁmbito de archivos declarado: {}\n",
        if task.file_scope.trim().is_empty() {
            "sin restricción declarada"
        } else {
            task.file_scope.trim()
        }
    ));
    prompt.push_str(
        "\nTrabaja únicamente dentro de este worktree. No modifiques otros entornos ni el checkout principal.\n",
    );

    if !context.is_empty() {
        prompt.push_str("\nContexto de dependencias completadas y aceptadas:\n");
        let mut budget = HANDOFF_CONTEXT_BUDGET;
        for (dependency, handoff) in context {
            let summary: String = handoff
                .summary
                .trim()
                .chars()
                .take(MAX_HANDOFF_SUMMARY_IN_PROMPT)
                .collect();
            let mut section = format!(
                "\n### Tarea #{} — «{}»\n{}\n",
                dependency.id, dependency.title, summary
            );
            if !handoff.artifacts.is_empty() {
                section.push_str(&format!("Artefactos: {}\n", handoff.artifacts.join(", ")));
            }
            if let Some(instructions) = handoff.next_instructions.as_deref() {
                section.push_str(&format!(
                    "Instrucciones para esta tarea: {}\n",
                    instructions.trim()
                ));
            }
            if !handoff.open_questions.is_empty() {
                section.push_str(&format!(
                    "Preguntas abiertas: {}\n",
                    handoff.open_questions.join("; ")
                ));
            }
            if section.chars().count() > budget {
                prompt.push_str(
                    "\n[Contexto de dependencias recortado; usa get_task_context para consultarlo completo]\n",
                );
                break;
            }
            budget -= section.chars().count();
            prompt.push_str(&section);
        }
    }

    prompt.push_str(
        "\nHerramientas de coordinación disponibles (servidor MCP «stade»):\n\
         - get_task_context: consulta el estado y el contexto de esta tarea.\n\
         - submit_handoff: entrega el trabajo con resumen, artefactos e instrucciones para el siguiente rol.\n\
         - complete_task: marca la tarea como terminada si no dejas trabajo para otro rol.\n\
         - report_blocker: registra un bloqueo técnico.\n\
         - request_user_input: pide una decisión al usuario y espera su respuesta.\n\
         \nAl terminar, usa la herramienta de entrega correspondiente y detén el trabajo: el usuario revisa la entrega antes de continuar.\n",
    );

    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(
            "El contexto de la tarea es demasiado largo para enviarse; recorta las entregas o el objetivo"
                .to_string(),
        );
    }
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordination;

    fn sample_task() -> coordination::Task {
        coordination::Task {
            id: 7,
            project_id: 1,
            worktree_id: None,
            role_id: None,
            title: "Construir la API".to_string(),
            objective: "Exponer el contrato aprobado".to_string(),
            status: coordination::STATUS_READY.to_string(),
            agent_id: "build".to_string(),
            provider_id: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            file_scope: "apps/desktop/src-tauri/src/api.rs".to_string(),
            blocker_reason: None,
            last_error: None,
            created_at: "2026-09-29T00:00:00.000Z".to_string(),
            updated_at: "2026-09-29T00:00:00.000Z".to_string(),
            started_at: None,
            completed_at: None,
        }
    }

    fn sample_handoff() -> coordination::HandoffRecord {
        coordination::HandoffRecord {
            id: 3,
            task_id: 1,
            kind: "handoff".to_string(),
            summary: "Esquema migrado".to_string(),
            artifacts: vec!["migrations/0001.sql".to_string()],
            next_instructions: Some("Usa las tablas existentes".to_string()),
            open_questions: vec![],
            created_at: "2026-09-29T00:00:00.000Z".to_string(),
            reviewed_at: Some("2026-09-29T00:00:00.000Z".to_string()),
            review_decision: Some("accepted".to_string()),
            review_note: None,
        }
    }

    #[test]
    fn prompt_includes_objective_scope_and_dependency_handoffs() {
        let task = sample_task();
        let dependency = coordination::Task {
            id: 1,
            ..sample_task()
        };
        let prompt = build_task_prompt(&task, &[(dependency, sample_handoff())], None)
            .expect("prompt should build");

        assert!(prompt.contains("Tarea #7 — «Construir la API»"));
        assert!(prompt.contains("Exponer el contrato aprobado"));
        assert!(prompt.contains("apps/desktop/src-tauri/src/api.rs"));
        assert!(prompt.contains("Esquema migrado"));
        assert!(prompt.contains("migrations/0001.sql"));
        assert!(prompt.contains("submit_handoff"));
        assert!(prompt.contains("request_user_input"));
    }

    #[test]
    fn prompt_includes_role_instructions_when_present() {
        let task = sample_task();
        let prompt = build_task_prompt(&task, &[], Some("Sigue el contrato aprobado"))
            .expect("prompt should build");
        assert!(prompt.contains("Instrucciones de tu rol"));
        assert!(prompt.contains("Sigue el contrato aprobado"));
    }

    #[test]
    fn prompt_rejects_oversized_objectives() {
        let mut task = sample_task();
        task.objective = "x".repeat(MAX_PROMPT_CHARS);
        let error = build_task_prompt(&task, &[], None).expect_err("prompt should not fit");
        assert!(error.contains("demasiado largo"));
    }

    #[test]
    fn bridge_config_lands_next_to_the_managed_worktrees() {
        let root = tempfile::tempdir().expect("temp root should exist");
        let bridge = crate::bridge::test_bridge("http://127.0.0.1:45999", "token-de-prueba");
        write_project_bridge_config(root.path(), 4, &bridge).expect("config should be written");
        let raw = std::fs::read_to_string(root.path().join("project-4").join("opencode.json"))
            .expect("config file should exist");
        let parsed: serde_json::Value =
            serde_json::from_str(&raw).expect("config should be valid JSON");
        assert_eq!(parsed["mcp"]["servers"]["stade"]["type"], "remote");
        assert_eq!(
            parsed["mcp"]["servers"]["stade"]["url"],
            "http://127.0.0.1:45999/mcp"
        );
        assert_eq!(
            parsed["mcp"]["servers"]["stade"]["headers"]["Authorization"],
            "Bearer token-de-prueba"
        );
        assert_eq!(parsed["mcp"]["servers"]["stade"]["codemode"], false);
    }
}
