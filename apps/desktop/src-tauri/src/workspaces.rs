use std::collections::HashMap;

use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub(crate) id: i64,
    name: String,
    description: Option<String>,
    project_ids: Vec<i64>,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
}

#[derive(Debug, FromRow)]
struct WorkspaceRow {
    id: i64,
    name: String,
    description: Option<String>,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
}

const WORKSPACE_COLUMNS: &str = "id, name, description, created_at, updated_at, archived_at";

pub async fn list(pool: &SqlitePool, include_archived: bool) -> Result<Vec<Workspace>, String> {
    let query = if include_archived {
        format!(
            "SELECT {WORKSPACE_COLUMNS} FROM workspaces \
             ORDER BY archived_at IS NOT NULL, name COLLATE NOCASE"
        )
    } else {
        format!(
            "SELECT {WORKSPACE_COLUMNS} FROM workspaces \
             WHERE archived_at IS NULL ORDER BY name COLLATE NOCASE"
        )
    };

    let rows = sqlx::query_as::<_, WorkspaceRow>(&query)
        .fetch_all(pool)
        .await
        .map_err(|_| "No se pudieron cargar los espacios locales".to_string())?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let memberships = membership_map(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| from_row(row, &memberships))
        .collect())
}

/// Proyectos activos (no archivados) de un espacio, ordenados por nombre.
pub async fn active_project_ids(pool: &SqlitePool, workspace_id: i64) -> Result<Vec<i64>, String> {
    sqlx::query_scalar::<_, i64>(
        "SELECT p.id FROM workspace_projects wp \
         JOIN projects p ON p.id = wp.project_id \
         WHERE wp.workspace_id = ? AND p.archived_at IS NULL \
         ORDER BY p.name COLLATE NOCASE",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudieron cargar los proyectos del espacio".to_string())
}

pub async fn create(
    pool: &SqlitePool,
    name: String,
    description: Option<String>,
) -> Result<Workspace, String> {
    let name = validate_name(&name)?;
    let description = validate_description(description)?;
    let name_key = name.to_lowercase();
    let row = sqlx::query_as::<_, WorkspaceRow>(&format!(
        "INSERT INTO workspaces (name, name_key, description) \
         VALUES (?, ?, ?) \
         RETURNING {WORKSPACE_COLUMNS}"
    ))
    .bind(name)
    .bind(name_key)
    .bind(description)
    .fetch_one(pool)
    .await
    .map_err(map_write_error)?;
    Ok(from_row(row, &HashMap::new()))
}

pub async fn update(
    pool: &SqlitePool,
    workspace_id: i64,
    name: String,
    description: Option<String>,
) -> Result<Workspace, String> {
    let name = validate_name(&name)?;
    let description = validate_description(description)?;
    let name_key = name.to_lowercase();
    let affected = sqlx::query(
        "UPDATE workspaces \
         SET name = ?, name_key = ?, description = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ? AND archived_at IS NULL",
    )
    .bind(&name)
    .bind(name_key)
    .bind(&description)
    .bind(workspace_id)
    .execute(pool)
    .await
    .map_err(map_write_error)?
    .rows_affected();
    if affected == 0 {
        return Err("El espacio no existe o está archivado".to_string());
    }
    load_with_memberships(pool, workspace_id).await
}

pub async fn set_archived(
    pool: &SqlitePool,
    workspace_id: i64,
    archived: bool,
) -> Result<Workspace, String> {
    let affected = sqlx::query(
        "UPDATE workspaces \
         SET archived_at = CASE WHEN ? THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now') ELSE NULL END, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ?",
    )
    .bind(archived)
    .bind(workspace_id)
    .execute(pool)
    .await
    .map_err(|_| "No se pudo cambiar el estado del espacio".to_string())?
    .rows_affected();
    if affected == 0 {
        return Err("El espacio ya no existe".to_string());
    }
    load_with_memberships(pool, workspace_id).await
}

/// Borrar un espacio solo elimina la agrupación y sus membresías
/// (por `ON DELETE CASCADE`); los proyectos quedan intactos.
pub async fn delete(pool: &SqlitePool, workspace_id: i64) -> Result<(), String> {
    let affected = sqlx::query("DELETE FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .execute(pool)
        .await
        .map_err(|_| "No se pudo eliminar el espacio".to_string())?
        .rows_affected();
    if affected == 0 {
        return Err("El espacio ya no existe".to_string());
    }
    Ok(())
}

pub async fn add_project(
    pool: &SqlitePool,
    workspace_id: i64,
    project_id: i64,
) -> Result<Workspace, String> {
    ensure_workspace_exists(pool, workspace_id).await?;
    ensure_project_exists(pool, project_id).await?;
    sqlx::query(
        "INSERT INTO workspace_projects (workspace_id, project_id) \
         VALUES (?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(workspace_id)
    .bind(project_id)
    .execute(pool)
    .await
    .map_err(|_| "No se pudo añadir el proyecto al espacio".to_string())?;
    load_with_memberships(pool, workspace_id).await
}

pub async fn remove_project(
    pool: &SqlitePool,
    workspace_id: i64,
    project_id: i64,
) -> Result<Workspace, String> {
    ensure_workspace_exists(pool, workspace_id).await?;
    sqlx::query("DELETE FROM workspace_projects WHERE workspace_id = ? AND project_id = ?")
        .bind(workspace_id)
        .bind(project_id)
        .execute(pool)
        .await
        .map_err(|_| "No se pudo quitar el proyecto del espacio".to_string())?;
    load_with_memberships(pool, workspace_id).await
}

fn from_row(row: WorkspaceRow, memberships: &HashMap<i64, Vec<i64>>) -> Workspace {
    Workspace {
        project_ids: memberships.get(&row.id).cloned().unwrap_or_default(),
        id: row.id,
        name: row.name,
        description: row.description,
        created_at: row.created_at,
        updated_at: row.updated_at,
        archived_at: row.archived_at,
    }
}

async fn load_with_memberships(pool: &SqlitePool, workspace_id: i64) -> Result<Workspace, String> {
    let row = sqlx::query_as::<_, WorkspaceRow>(&format!(
        "SELECT {WORKSPACE_COLUMNS} FROM workspaces WHERE id = ?"
    ))
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "No se pudo cargar el espacio".to_string())?
    .ok_or_else(|| "El espacio ya no existe".to_string())?;
    let memberships = membership_map(pool).await?;
    Ok(from_row(row, &memberships))
}

async fn membership_map(pool: &SqlitePool) -> Result<HashMap<i64, Vec<i64>>, String> {
    let rows = sqlx::query_as::<_, (i64, i64)>(
        "SELECT workspace_id, project_id FROM workspace_projects ORDER BY workspace_id, project_id",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| "No se pudieron cargar los espacios locales".to_string())?;

    let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
    for (workspace_id, project_id) in rows {
        map.entry(workspace_id).or_default().push(project_id);
    }
    Ok(map)
}

async fn ensure_workspace_exists(pool: &SqlitePool, workspace_id: i64) -> Result<(), String> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT id FROM workspaces WHERE id = ?")
        .bind(workspace_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo comprobar el espacio".to_string())?;
    if exists.is_none() {
        return Err("El espacio ya no existe".to_string());
    }
    Ok(())
}

async fn ensure_project_exists(pool: &SqlitePool, project_id: i64) -> Result<(), String> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT id FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo comprobar el proyecto".to_string())?;
    if exists.is_none() {
        return Err("El proyecto ya no existe".to_string());
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Escribe un nombre para el espacio".to_string());
    }
    if name.chars().count() > 100 {
        return Err("El nombre del espacio no puede superar 100 caracteres".to_string());
    }
    Ok(name.to_string())
}

fn validate_description(description: Option<String>) -> Result<Option<String>, String> {
    let description = description
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if description
        .as_ref()
        .is_some_and(|value| value.chars().count() > 500)
    {
        return Err("La descripción no puede superar 500 caracteres".to_string());
    }
    Ok(description)
}

fn map_write_error(error: sqlx::Error) -> String {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error.is_unique_violation() {
            return "Ya existe un espacio con ese nombre".to_string();
        }
    }
    "No se pudo guardar el espacio en la base de datos local".to_string()
}

#[cfg(test)]
mod tests {
    use super::{active_project_ids, add_project, create, delete, list, remove_project};
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
                .expect("workspace migration should apply");
            pool
        })
    }

    async fn test_project(pool: &SqlitePool, name: &str, root: &std::path::Path) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO projects (name, root_path, root_path_key) VALUES (?, ?, ?) RETURNING id",
        )
        .bind(name)
        .bind(root.to_string_lossy().into_owned())
        .bind(root.to_string_lossy().to_lowercase())
        .fetch_one(pool)
        .await
        .expect("project should be created")
    }

    #[test]
    fn workspace_groups_projects_many_to_many() {
        let pool = test_pool();
        let backend_dir = tempfile::tempdir().expect("backend directory should be created");
        let frontend_dir = tempfile::tempdir().expect("frontend directory should be created");

        tauri::async_runtime::block_on(async {
            let backend = test_project(&pool, "Backend", backend_dir.path()).await;
            let frontend = test_project(&pool, "Frontend", frontend_dir.path()).await;

            let finanzas = create(&pool, "App finanzas".to_string(), None)
                .await
                .expect("workspace should be created");
            let otro = create(&pool, "Otro".to_string(), None)
                .await
                .expect("second workspace should be created");

            // El backend compartido vive en dos espacios a la vez.
            add_project(&pool, finanzas.id, backend)
                .await
                .expect("backend should join finanzas");
            add_project(&pool, finanzas.id, frontend)
                .await
                .expect("frontend should join finanzas");
            add_project(&pool, otro.id, backend)
                .await
                .expect("backend should join the second workspace");

            let workspaces = list(&pool, false).await.expect("workspaces should load");
            assert_eq!(workspaces.len(), 2);
            let finanzas = workspaces
                .iter()
                .find(|workspace| workspace.id == finanzas.id)
                .expect("finanzas should be listed");
            assert!(finanzas.project_ids.contains(&backend));
            assert!(finanzas.project_ids.contains(&frontend));

            assert_eq!(
                active_project_ids(&pool, otro.id)
                    .await
                    .expect("member projects should load"),
                vec![backend]
            );

            remove_project(&pool, otro.id, backend)
                .await
                .expect("backend should leave the second workspace");
            assert!(active_project_ids(&pool, otro.id)
                .await
                .expect("member projects should load")
                .is_empty());

            // Borrar el espacio no borra los proyectos.
            delete(&pool, otro.id)
                .await
                .expect("workspace should be deleted");
            let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
                .fetch_one(&pool)
                .await
                .expect("projects should be countable");
            assert_eq!(remaining, 2);
        });
    }

    #[test]
    fn workspace_names_are_unique_case_insensitive() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            create(&pool, "App finanzas".to_string(), None)
                .await
                .expect("workspace should be created");
            let duplicate = create(&pool, "APP FINANZAS".to_string(), None)
                .await
                .expect_err("duplicate name should be rejected");
            assert!(duplicate.contains("Ya existe"));
        });
    }
}
