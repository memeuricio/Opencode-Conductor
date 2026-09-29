use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

const RETENTION_KEY: &str = "activity_retention_days";
const DEFAULT_RETENTION_DAYS: i64 = 30;
const MAX_RETENTION_DAYS: i64 = 365;
const MAX_ENTRIES: i64 = 500;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    id: i64,
    task_id: i64,
    task_title: String,
    task_status: String,
    kind: String,
    detail: Option<String>,
    created_at: String,
}

/// Timeline de orquestación de un proyecto: actividad durable de todas sus
/// tareas, sin prompts ni respuestas (nunca se guardan). Los eventos SSE de
/// OpenCode no se persisten por diseño (sus payloads pueden traer contenido
/// sensible); la GUI combina esta timeline con el estado en vivo del stream.
pub async fn project_timeline(
    pool: &SqlitePool,
    project_id: i64,
    limit: i64,
) -> Result<Vec<TimelineEntry>, String> {
    prune_old_entries(pool).await?;
    let limit = limit.clamp(1, MAX_ENTRIES);
    sqlx::query_as::<_, TimelineEntry>(
        "SELECT a.id, a.task_id AS task_id, t.title AS task_title, t.status AS task_status, \
         a.kind, a.detail, a.created_at AS created_at \
         FROM task_activity a JOIN tasks t ON t.id = a.task_id \
         WHERE t.project_id = ? \
         ORDER BY a.created_at DESC, a.id DESC LIMIT ?",
    )
    .bind(project_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudo cargar la actividad del proyecto".to_string())
}

pub async fn retention_days(pool: &SqlitePool) -> Result<i64, String> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(RETENTION_KEY)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo cargar la configuración".to_string())?
        .flatten();
    Ok(raw
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|days| (1..=MAX_RETENTION_DAYS).contains(days))
        .unwrap_or(DEFAULT_RETENTION_DAYS))
}

pub async fn set_retention_days(pool: &SqlitePool, days: i64) -> Result<i64, String> {
    if !(1..=MAX_RETENTION_DAYS).contains(&days) {
        return Err(format!(
            "La retención debe estar entre 1 y {MAX_RETENTION_DAYS} días"
        ));
    }
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(RETENTION_KEY)
    .bind(days.to_string())
    .execute(pool)
    .await
    .map_err(|_| "No se pudo guardar la configuración".to_string())?;
    prune_old_entries(pool).await?;
    Ok(days)
}

async fn prune_old_entries(pool: &SqlitePool) -> Result<(), String> {
    let days = retention_days_raw(pool).await?;
    sqlx::query("DELETE FROM task_activity WHERE julianday('now') - julianday(created_at) > ?")
        .bind(days as f64)
        .execute(pool)
        .await
        .map_err(|_| "No se pudo aplicar la retención de actividad".to_string())?;
    Ok(())
}

async fn retention_days_raw(pool: &SqlitePool) -> Result<i64, String> {
    // Sin podar aquí dentro para no recursar: lee el valor tal cual.
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(RETENTION_KEY)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo cargar la configuración".to_string())?
        .flatten();
    Ok(raw
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|days| (1..=MAX_RETENTION_DAYS).contains(days))
        .unwrap_or(DEFAULT_RETENTION_DAYS))
}

#[cfg(test)]
mod tests {
    use super::{project_timeline, retention_days, set_retention_days};
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

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
                .expect("activity migration should apply");
            pool
        })
    }

    #[test]
    fn timeline_combines_task_activity_without_sensitive_content() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            assert_eq!(
                retention_days(&pool).await.expect("retention should load"),
                30
            );
            let directory = tempfile::tempdir().expect("temp dir should exist");
            let project = crate::projects::create(
                &pool,
                "Actividad".to_string(),
                directory.keep().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("project should be created");
            let task = crate::coordination::create(
                &pool,
                project.id,
                "Tarea visible",
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

            let entries = project_timeline(&pool, project.id, 100)
                .await
                .expect("timeline should load");
            assert!(!entries.is_empty());
            assert!(entries.iter().all(|entry| entry.task_id == task.id));
            assert!(entries
                .iter()
                .all(|entry| entry.task_title == "Tarea visible"));
        });
    }

    #[test]
    fn retention_is_configurable_and_prunes_old_entries() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            assert!(set_retention_days(&pool, 0).await.is_err());
            assert!(set_retention_days(&pool, 400).await.is_err());
            assert_eq!(
                set_retention_days(&pool, 7)
                    .await
                    .expect("retention should save"),
                7
            );

            let directory = tempfile::tempdir().expect("temp dir should exist");
            let project = crate::projects::create(
                &pool,
                "Retención".to_string(),
                directory.keep().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("project should be created");
            let task = crate::coordination::create(
                &pool,
                project.id,
                "Tarea vieja",
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
            sqlx::query(
                "UPDATE task_activity SET created_at = '2020-01-01T00:00:00.000Z' WHERE task_id = ?",
            )
            .bind(task.id)
            .execute(&pool)
            .await
            .expect("entry should be aged");

            let entries = project_timeline(&pool, project.id, 100)
                .await
                .expect("timeline should load");
            assert!(entries.is_empty());
        });
    }
}
