use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub(crate) id: i64,
    name: String,
    description: Option<String>,
    root_path: String,
    pub(crate) is_git_repository: bool,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
}

#[derive(Debug, FromRow)]
struct ProjectRow {
    id: i64,
    name: String,
    description: Option<String>,
    root_path: String,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
}

impl From<ProjectRow> for Project {
    fn from(row: ProjectRow) -> Self {
        let is_git_repository = Path::new(&row.root_path).join(".git").exists();
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            root_path: row.root_path,
            is_git_repository,
            created_at: row.created_at,
            updated_at: row.updated_at,
            archived_at: row.archived_at,
        }
    }
}

const PROJECT_COLUMNS: &str =
    "id, name, description, root_path, created_at, updated_at, archived_at";

pub async fn list(pool: &SqlitePool, include_archived: bool) -> Result<Vec<Project>, String> {
    let query = if include_archived {
        format!(
            "SELECT {PROJECT_COLUMNS} FROM projects ORDER BY archived_at IS NOT NULL, name COLLATE NOCASE"
        )
    } else {
        format!(
            "SELECT {PROJECT_COLUMNS} FROM projects WHERE archived_at IS NULL ORDER BY name COLLATE NOCASE"
        )
    };

    sqlx::query_as::<_, ProjectRow>(&query)
        .fetch_all(pool)
        .await
        .map(|rows| rows.into_iter().map(Project::from).collect())
        .map_err(|_| "No se pudieron cargar los proyectos locales".to_string())
}

pub async fn create(
    pool: &SqlitePool,
    name: String,
    root_path: String,
    description: Option<String>,
) -> Result<Project, String> {
    let name = validate_name(&name)?;
    let description = validate_description(description)?;
    let root_path = validate_root_path(&root_path)?;
    let root_path = root_path.to_string_lossy().into_owned();
    let root_path_key = root_path.to_lowercase();
    let query = format!(
        "INSERT INTO projects (name, description, root_path, root_path_key) \
         VALUES (?, ?, ?, ?) \
         RETURNING {PROJECT_COLUMNS}"
    );

    sqlx::query_as::<_, ProjectRow>(&query)
        .bind(name)
        .bind(description)
        .bind(root_path)
        .bind(root_path_key)
        .fetch_one(pool)
        .await
        .map(Project::from)
        .map_err(map_write_error)
}

pub async fn update(
    pool: &SqlitePool,
    project_id: i64,
    name: String,
    description: Option<String>,
) -> Result<Project, String> {
    let name = validate_name(&name)?;
    let description = validate_description(description)?;
    let query = format!(
        "UPDATE projects \
         SET name = ?, description = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ? AND archived_at IS NULL \
         RETURNING {PROJECT_COLUMNS}"
    );

    sqlx::query_as::<_, ProjectRow>(&query)
        .bind(name)
        .bind(description)
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo actualizar el proyecto local".to_string())?
        .map(Project::from)
        .ok_or_else(|| "El proyecto no existe o está archivado".to_string())
}

pub async fn set_archived(
    pool: &SqlitePool,
    project_id: i64,
    archived: bool,
) -> Result<Project, String> {
    let query = format!(
        "UPDATE projects \
         SET archived_at = CASE WHEN ? THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now') ELSE NULL END, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ? \
         RETURNING {PROJECT_COLUMNS}"
    );

    sqlx::query_as::<_, ProjectRow>(&query)
        .bind(archived)
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo cambiar el estado del proyecto".to_string())?
        .map(Project::from)
        .ok_or_else(|| "El proyecto ya no existe".to_string())
}

/// Removes the project registration and its local coordination records. The
/// source folder and Git branches are preserved; managed worktrees are removed
/// only when clean. Archiving first prevents new tasks from starting during
/// preflight and cleanup.
pub async fn delete(
    pool: &SqlitePool,
    worktrees_root: &Path,
    project_id: i64,
) -> Result<(), String> {
    let project = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT root_path, archived_at FROM projects WHERE id = ?",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "No se pudo consultar el proyecto".to_string())?
    .ok_or_else(|| "El proyecto ya no existe".to_string())?;
    let was_archived = project.1.is_some();

    sqlx::query(
        "UPDATE projects SET archived_at = COALESCE(archived_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(project_id)
    .execute(pool)
    .await
    .map_err(|_| "No se pudo preparar la eliminación del proyecto".to_string())?;

    let active_tasks = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tasks WHERE project_id = ? AND status IN ('working', 'blocked', 'review')",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(|_| "No se pudo comprobar si el proyecto tiene tareas activas".to_string())?;
    if active_tasks > 0 {
        restore_active_project(pool, project_id, was_archived).await;
        return Err(
            "Resuelve o cierra las tareas activas antes de eliminar el proyecto. Si está archivado, restáuralo para gestionarlas".to_string(),
        );
    }

    if let Err(error) = crate::worktrees::remove_clean_for_project(
        pool,
        worktrees_root,
        Path::new(&project.0),
        project_id,
    )
    .await
    {
        restore_active_project(pool, project_id, was_archived).await;
        return Err(error);
    }

    if let Err(error) = crate::worktrees::remove_project_config(worktrees_root, project_id) {
        restore_active_project(pool, project_id, was_archived).await;
        return Err(error);
    }

    let deletion = async {
        let mut transaction = pool
            .begin()
            .await
            .map_err(|_| "No se pudo iniciar la eliminación del proyecto".to_string())?;

        sqlx::query(
            "DELETE FROM task_dependencies WHERE task_id IN (SELECT id FROM tasks WHERE project_id = ?) \
             OR depends_on_task_id IN (SELECT id FROM tasks WHERE project_id = ?)",
        )
        .bind(project_id)
        .bind(project_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| "No se pudieron eliminar las dependencias del proyecto".to_string())?;
        for table in ["handoffs", "user_decisions", "task_activity"] {
            let query = format!(
                "DELETE FROM {table} WHERE task_id IN (SELECT id FROM tasks WHERE project_id = ?)"
            );
            sqlx::query(&query)
                .bind(project_id)
                .execute(&mut *transaction)
                .await
                .map_err(|_| "No se pudo eliminar el historial de tareas del proyecto".to_string())?;
        }
        sqlx::query("DELETE FROM tasks WHERE project_id = ?")
            .bind(project_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| "No se pudieron eliminar las tareas del proyecto".to_string())?;
        sqlx::query("DELETE FROM worktrees WHERE project_id = ?")
            .bind(project_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| "No se pudieron eliminar los entornos del proyecto".to_string())?;
        sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(project_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| "No se pudo eliminar el proyecto".to_string())?;
        transaction
            .commit()
            .await
            .map_err(|_| "No se pudo confirmar la eliminación del proyecto".to_string())
    }
    .await;

    if deletion.is_err() {
        restore_active_project(pool, project_id, was_archived).await;
    }
    deletion
}

async fn restore_active_project(pool: &SqlitePool, project_id: i64, was_archived: bool) {
    if !was_archived {
        let _ = sqlx::query(
            "UPDATE projects SET archived_at = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(project_id)
        .execute(pool)
        .await;
    }
}

fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Escribe un nombre para el proyecto".to_string());
    }
    if name.chars().count() > 100 {
        return Err("El nombre del proyecto no puede superar 100 caracteres".to_string());
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

fn validate_root_path(root_path: &str) -> Result<PathBuf, String> {
    let root_path = root_path.trim();
    if root_path.is_empty() {
        return Err("Selecciona la carpeta raíz del proyecto".to_string());
    }

    let path = PathBuf::from(root_path);
    if !path.is_absolute() {
        return Err("La ruta del proyecto debe ser absoluta".to_string());
    }

    let path = std::fs::canonicalize(path)
        .map_err(|_| "No se pudo acceder a esa carpeta. Comprueba que exista".to_string())?;
    if !path.is_dir() {
        return Err("La ruta seleccionada no es una carpeta".to_string());
    }

    Ok(path)
}

fn map_write_error(error: sqlx::Error) -> String {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error.is_unique_violation() {
            return "Ya existe un proyecto registrado para esa carpeta".to_string();
        }
    }
    "No se pudo guardar el proyecto en la base de datos local".to_string()
}

#[cfg(test)]
mod tests {
    use super::{create, delete, list, set_archived, update};
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
                .expect("project migration should apply");
            pool
        })
    }

    #[test]
    fn creates_updates_and_archives_projects_without_deleting_them() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let pool = test_pool();

        tauri::async_runtime::block_on(async {
            let project = create(
                &pool,
                "  Stade Studio demo  ".to_string(),
                directory.path().to_string_lossy().into_owned(),
                Some("  local test  ".to_string()),
            )
            .await
            .expect("project should be created");
            assert_eq!(project.name, "Stade Studio demo");
            assert_eq!(project.description.as_deref(), Some("local test"));
            assert!(!project.is_git_repository);

            let updated = update(&pool, project.id, "Demo renombrado".to_string(), None)
                .await
                .expect("project should be updated");
            assert_eq!(updated.name, "Demo renombrado");
            assert_eq!(updated.description, None);

            set_archived(&pool, project.id, true)
                .await
                .expect("project should be archived");
            assert!(list(&pool, false)
                .await
                .expect("active list should load")
                .is_empty());
            assert_eq!(
                list(&pool, true)
                    .await
                    .expect("all projects should load")
                    .len(),
                1
            );

            set_archived(&pool, project.id, false)
                .await
                .expect("project should be restorable");
            assert_eq!(
                list(&pool, false)
                    .await
                    .expect("active list should load")
                    .len(),
                1
            );
        });
    }

    #[test]
    fn deleting_project_removes_local_records_but_preserves_source_folder() {
        let directory = tempfile::tempdir().expect("project folder should be created");
        let worktrees_root = tempfile::tempdir().expect("worktrees folder should be created");
        let pool = test_pool();

        tauri::async_runtime::block_on(async {
            let project = create(
                &pool,
                "Eliminar demo".to_string(),
                directory.path().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("project should be created");
            let workspace = crate::workspaces::create(&pool, "Espacio demo".to_string(), None)
                .await
                .expect("workspace should be created");
            crate::workspaces::add_project(&pool, workspace.id, project.id)
                .await
                .expect("project should join its workspace");
            let task = crate::coordination::create(
                &pool,
                project.id,
                "Tarea demo",
                "Objetivo demo",
                "build",
                "openai",
                "model",
                "",
                &[],
            )
            .await
            .expect("task should be created");

            delete(&pool, worktrees_root.path(), project.id)
                .await
                .expect("project should be deleted");
            assert!(list(&pool, true)
                .await
                .expect("project list should load")
                .is_empty());
            assert_eq!(
                crate::coordination::list_project(&pool, project.id)
                    .await
                    .expect("task list should load")
                    .len(),
                0
            );
            assert!(crate::coordination::get(&pool, task.id).await.is_err());
            assert!(directory.path().exists(), "source folder must be preserved");
            let membership_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM workspace_projects WHERE workspace_id = ? AND project_id = ?",
            )
            .bind(workspace.id)
            .bind(project.id)
            .fetch_one(&pool)
            .await
            .expect("workspace membership should be queryable");
            assert_eq!(
                membership_count, 0,
                "workspace should remain without the project"
            );
        });
    }

    #[test]
    fn active_tasks_block_project_deletion_and_restore_archive_state() {
        let directory = tempfile::tempdir().expect("project folder should be created");
        let worktrees_root = tempfile::tempdir().expect("worktrees folder should be created");
        let pool = test_pool();

        tauri::async_runtime::block_on(async {
            let project = create(
                &pool,
                "Proyecto activo".to_string(),
                directory.path().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("project should be created");
            let task = crate::coordination::create(
                &pool,
                project.id,
                "Tarea activa",
                "Objetivo demo",
                "build",
                "openai",
                "model",
                "",
                &[],
            )
            .await
            .expect("task should be created");
            crate::coordination::mark_working(&pool, task.id, None)
                .await
                .expect("task should become active");

            let error = delete(&pool, worktrees_root.path(), project.id)
                .await
                .expect_err("active tasks should prevent deletion");
            assert!(error.contains("tareas activas"));
            assert_eq!(
                list(&pool, false)
                    .await
                    .expect("projects should load")
                    .len(),
                1
            );
            assert!(directory.path().exists());
        });
    }

    #[test]
    fn rejects_duplicate_roots_and_missing_directories() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let pool = test_pool();

        tauri::async_runtime::block_on(async {
            create(
                &pool,
                "One".to_string(),
                directory.path().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("first project should be created");
            let duplicate = create(
                &pool,
                "Two".to_string(),
                directory.path().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect_err("same root should not be registered twice");
            assert!(duplicate.contains("Ya existe"));

            let missing = create(
                &pool,
                "Missing".to_string(),
                directory
                    .path()
                    .join("missing")
                    .to_string_lossy()
                    .into_owned(),
                None,
            )
            .await
            .expect_err("nonexistent root should be rejected");
            assert!(missing.contains("Comprueba que exista"));
        });
    }
}
