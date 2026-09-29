use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

pub const BUILDER: &str = "builder";
const MAX_INSTRUCTIONS_CHARS: usize = 12_000;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub role: String,
    pub name: String,
    pub instructions: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAgent {
    pub project_id: i64,
    pub role: String,
    pub agent_id: String,
    pub provider_id: String,
    pub model_id: String,
}

pub async fn list_profiles(pool: &SqlitePool) -> Result<Vec<AgentProfile>, String> {
    sqlx::query_as::<_, AgentProfile>(
        "SELECT role, 'Builder' AS name, instructions, updated_at \
         FROM agent_profiles WHERE role = 'builder'",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudieron cargar las instrucciones base del Builder".to_string())
}

pub async fn update_profile(
    pool: &SqlitePool,
    role: &str,
    instructions: &str,
) -> Result<AgentProfile, String> {
    validate_builder_role(role)?;
    let instructions = instructions.trim();
    if instructions.chars().count() > MAX_INSTRUCTIONS_CHARS {
        return Err(format!(
            "Las instrucciones no pueden superar {MAX_INSTRUCTIONS_CHARS} caracteres"
        ));
    }
    sqlx::query_as::<_, AgentProfile>(
        "UPDATE agent_profiles SET instructions = ?, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE role = 'builder' RETURNING role, 'Builder' AS name, instructions, updated_at",
    )
    .bind(instructions)
    .fetch_optional(pool)
    .await
    .map_err(|_| "No se pudieron guardar las instrucciones base".to_string())?
    .ok_or_else(|| "No se encontró el perfil Builder".to_string())
}

pub async fn list_project_agents(
    pool: &SqlitePool,
    project_id: i64,
) -> Result<Vec<ProjectAgent>, String> {
    sqlx::query_as::<_, ProjectAgent>(
        "SELECT project_id, role, agent_id, provider_id, model_id \
         FROM project_agents WHERE project_id = ? AND role = 'builder'",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudo cargar el Builder del proyecto".to_string())
}

pub async fn get_project_agent(
    pool: &SqlitePool,
    project_id: i64,
    role: &str,
) -> Result<ProjectAgent, String> {
    validate_builder_role(role)?;
    sqlx::query_as::<_, ProjectAgent>(
        "SELECT project_id, role, agent_id, provider_id, model_id \
         FROM project_agents WHERE project_id = ? AND role = 'builder'",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "No se pudo consultar el Builder del proyecto".to_string())?
    .ok_or_else(|| "Asigna un Builder al proyecto antes de crear tareas".to_string())
}

pub async fn save_project_agent(
    pool: &SqlitePool,
    project_id: i64,
    role: &str,
    agent_id: &str,
    provider_id: &str,
    model_id: &str,
) -> Result<ProjectAgent, String> {
    validate_builder_role(role)?;
    let agent_id = validate_text(agent_id, 200, "El perfil de OpenCode")?;
    let provider_id = validate_text(provider_id, 200, "El proveedor del modelo")?;
    let model_id = validate_text(model_id, 300, "El modelo")?;

    let project_exists = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM projects WHERE id = ? AND archived_at IS NULL",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "No se pudo comprobar el proyecto".to_string())?;
    if project_exists.is_none() {
        return Err("El proyecto no existe o está archivado".to_string());
    }

    sqlx::query(
        "INSERT INTO project_agents (project_id, role, agent_id, provider_id, model_id) \
         VALUES (?, 'builder', ?, ?, ?) \
         ON CONFLICT(project_id, role) DO UPDATE SET \
           agent_id = excluded.agent_id, provider_id = excluded.provider_id, \
           model_id = excluded.model_id, \
           updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
    )
    .bind(project_id)
    .bind(agent_id)
    .bind(provider_id)
    .bind(model_id)
    .execute(pool)
    .await
    .map_err(|_| "No se pudo guardar el Builder del proyecto".to_string())?;
    get_project_agent(pool, project_id, BUILDER).await
}

pub async fn profile_instructions(pool: &SqlitePool, role: &str) -> Result<String, String> {
    validate_builder_role(role)?;
    sqlx::query_scalar::<_, String>(
        "SELECT instructions FROM agent_profiles WHERE role = 'builder'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| "No se pudieron cargar las instrucciones del Builder".to_string())?
    .ok_or_else(|| "No se encontró el perfil Builder".to_string())
}

fn validate_builder_role(role: &str) -> Result<(), String> {
    if role == BUILDER {
        Ok(())
    } else {
        Err("Solo se configura un Builder por proyecto".to_string())
    }
}

fn validate_text(value: &str, max: usize, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{label} no puede estar vacío"));
    }
    if value.chars().count() > max {
        return Err(format!("{label} es demasiado largo"));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

    use super::{list_profiles, update_profile, BUILDER};

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
                .expect("agent migrations should apply");
            pool
        })
    }

    #[test]
    fn builder_base_instructions_are_editable() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let profiles = list_profiles(&pool)
                .await
                .expect("builder profile should exist");
            assert_eq!(profiles.len(), 1);
            assert_eq!(profiles[0].role, BUILDER);

            let updated = update_profile(&pool, BUILDER, "  Sigue el contrato aprobado.  ")
                .await
                .expect("builder instructions should save");
            assert_eq!(updated.instructions, "Sigue el contrato aprobado.");
            assert!(update_profile(&pool, "planner", "no").await.is_err());
        });
    }
}
