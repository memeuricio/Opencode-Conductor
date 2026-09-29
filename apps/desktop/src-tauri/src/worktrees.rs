use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub(crate) id: i64,
    pub(crate) project_id: i64,
    pub(crate) label: String,
    pub(crate) branch_name: String,
    pub(crate) directory: String,
    base_commit: String,
    pub(crate) status: String,
    last_error: Option<String>,
    created_at: String,
    pub(crate) opencode_session_id: Option<String>,
    pub(crate) opencode_agent_id: Option<String>,
    pub(crate) opencode_provider_id: Option<String>,
    pub(crate) opencode_model_id: Option<String>,
    pub(crate) opencode_location_directory: Option<String>,
    pub(crate) opencode_location_matches: Option<bool>,
}

#[derive(Debug, FromRow)]
struct WorktreeRow {
    id: i64,
    project_id: i64,
    label: String,
    branch_name: String,
    directory: String,
    base_commit: String,
    status: String,
    last_error: Option<String>,
    created_at: String,
    opencode_session_id: Option<String>,
    opencode_agent_id: Option<String>,
    opencode_provider_id: Option<String>,
    opencode_model_id: Option<String>,
    opencode_location_directory: Option<String>,
    opencode_location_matches: Option<bool>,
}

impl From<WorktreeRow> for Worktree {
    fn from(row: WorktreeRow) -> Self {
        Self {
            id: row.id,
            project_id: row.project_id,
            label: row.label,
            branch_name: row.branch_name,
            directory: row.directory,
            base_commit: row.base_commit,
            status: row.status,
            last_error: row.last_error,
            created_at: row.created_at,
            opencode_session_id: row.opencode_session_id,
            opencode_agent_id: row.opencode_agent_id,
            opencode_provider_id: row.opencode_provider_id,
            opencode_model_id: row.opencode_model_id,
            opencode_location_directory: row.opencode_location_directory,
            opencode_location_matches: row.opencode_location_matches,
        }
    }
}

const WORKTREE_COLUMNS: &str = "id, project_id, label, branch_name, directory, base_commit, status, last_error, created_at, opencode_session_id, opencode_agent_id, opencode_provider_id, opencode_model_id, opencode_location_directory, opencode_location_matches";

pub async fn list(pool: &SqlitePool, project_id: i64) -> Result<Vec<Worktree>, String> {
    let query = format!(
        "SELECT {WORKTREE_COLUMNS} FROM worktrees WHERE project_id = ? ORDER BY created_at DESC, id DESC"
    );
    sqlx::query_as::<_, WorktreeRow>(&query)
        .bind(project_id)
        .fetch_all(pool)
        .await
        .map(|rows| rows.into_iter().map(Worktree::from).collect())
        .map_err(|_| "No se pudieron cargar los entornos aislados".to_string())
}

pub async fn create(
    pool: &SqlitePool,
    worktrees_root: &Path,
    project_id: i64,
    label: String,
) -> Result<Worktree, String> {
    let label = validate_label(&label)?;
    let project = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT root_path, archived_at FROM projects WHERE id = ?",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "No se pudo consultar el proyecto".to_string())?
    .ok_or_else(|| "El proyecto ya no existe".to_string())?;

    if project.1.is_some() {
        return Err("Restaura el proyecto antes de crear un entorno".to_string());
    }

    let project_root = PathBuf::from(project.0);
    if !project_root.join(".git").exists() {
        return Err("La carpeta del proyecto debe ser la raíz de un repositorio Git".to_string());
    }

    let project_root_for_check = project_root.clone();
    let base_commit = tauri::async_runtime::spawn_blocking(move || {
        verify_clean_repository(&project_root_for_check)
    })
    .await
    .map_err(|_| "No se pudo comprobar el repositorio Git".to_string())??;

    let environment_id = Uuid::new_v4().simple().to_string();
    let branch_name = format!("conductor/{project_id}/{environment_id}");
    let directory = worktrees_root
        .join(format!("project-{project_id}"))
        .join(&environment_id);
    let directory_string = directory.to_string_lossy().into_owned();
    let insert_query = format!(
        "INSERT INTO worktrees (project_id, label, branch_name, directory, base_commit, status) \
         VALUES (?, ?, ?, ?, ?, 'creating') \
         RETURNING {WORKTREE_COLUMNS}"
    );
    let record = sqlx::query_as::<_, WorktreeRow>(&insert_query)
        .bind(project_id)
        .bind(label)
        .bind(&branch_name)
        .bind(&directory_string)
        .bind(&base_commit)
        .fetch_one(pool)
        .await
        .map_err(|_| "No se pudo registrar el nuevo entorno".to_string())?;

    if let Some(parent) = directory.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            let _ = set_status(
                pool,
                record.id,
                "failed",
                Some("No se pudo preparar la carpeta local"),
            )
            .await;
            return Err(format!(
                "No se pudo preparar la carpeta del entorno: {error}"
            ));
        }
    }

    let project_root_for_create = project_root.clone();
    let directory_for_create = directory.clone();
    let branch_for_create = branch_name.clone();
    let commit_for_create = base_commit.clone();
    let created = tauri::async_runtime::spawn_blocking(move || {
        create_git_worktree(
            &project_root_for_create,
            &branch_for_create,
            &directory_for_create,
            &commit_for_create,
        )
    })
    .await
    .map_err(|_| "No se pudo iniciar Git para crear el entorno".to_string())?;

    if let Err(error) = created {
        let safe_error =
            "Git no pudo crear el entorno. Revisa el estado del repositorio y vuelve a intentarlo.";
        let _ = set_status(pool, record.id, "failed", Some(safe_error)).await;
        return Err(error);
    }

    set_status(pool, record.id, "ready", None).await?;
    get(pool, record.id).await
}

async fn get(pool: &SqlitePool, worktree_id: i64) -> Result<Worktree, String> {
    let query = format!("SELECT {WORKTREE_COLUMNS} FROM worktrees WHERE id = ?");
    sqlx::query_as::<_, WorktreeRow>(&query)
        .bind(worktree_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo cargar el entorno recién creado".to_string())?
        .map(Worktree::from)
        .ok_or_else(|| "El entorno recién creado no aparece en la base local".to_string())
}

pub async fn get_by_id(pool: &SqlitePool, worktree_id: i64) -> Result<Worktree, String> {
    get(pool, worktree_id).await
}

#[allow(clippy::too_many_arguments)]
pub async fn attach_opencode_session(
    pool: &SqlitePool,
    worktree_id: i64,
    session_id: &str,
    agent_id: &str,
    provider_id: &str,
    model_id: &str,
    session_directory: &str,
    location_matches: bool,
) -> Result<Worktree, String> {
    let query = format!(
        "UPDATE worktrees \
         SET opencode_session_id = ?, opencode_agent_id = ?, opencode_provider_id = ?, \
             opencode_model_id = ?, opencode_location_directory = ?, opencode_location_matches = ? \
         WHERE id = ? AND status = 'ready' AND opencode_session_id IS NULL \
         RETURNING {WORKTREE_COLUMNS}"
    );

    sqlx::query_as::<_, WorktreeRow>(&query)
        .bind(session_id)
        .bind(agent_id)
        .bind(provider_id)
        .bind(model_id)
        .bind(session_directory)
        .bind(location_matches)
        .bind(worktree_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo asociar la sesión con el entorno".to_string())?
        .map(Worktree::from)
        .ok_or_else(|| "El entorno ya tiene una sesión o no está listo".to_string())
}

pub fn validate_managed_directory(worktrees_root: &Path, directory: &str) -> Result<(), String> {
    let root = std::fs::canonicalize(worktrees_root)
        .map_err(|_| "No se encontró el directorio local de worktrees".to_string())?;
    let worktree = std::fs::canonicalize(directory)
        .map_err(|_| "El directorio del entorno ya no existe".to_string())?;
    if worktree == root || !worktree.starts_with(&root) {
        return Err(
            "La ruta del entorno está fuera del directorio administrado por Stade Studio"
                .to_string(),
        );
    }
    Ok(())
}

async fn set_status(
    pool: &SqlitePool,
    worktree_id: i64,
    status: &str,
    last_error: Option<&str>,
) -> Result<(), String> {
    sqlx::query("UPDATE worktrees SET status = ?, last_error = ? WHERE id = ?")
        .bind(status)
        .bind(last_error)
        .bind(worktree_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|_| "No se pudo actualizar el estado del entorno".to_string())
}

fn validate_label(label: &str) -> Result<String, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("Escribe un nombre para el entorno".to_string());
    }
    if label.chars().count() > 80 {
        return Err("El nombre del entorno no puede superar 80 caracteres".to_string());
    }
    Ok(label.to_string())
}

fn run_git(repo: &Path, args: &[&str]) -> Result<Output, String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).args(args);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
        .output()
        .map_err(|_| "No se encontró Git o no se pudo iniciar el proceso".to_string())
}

fn verify_clean_repository(repo: &Path) -> Result<String, String> {
    let status = run_git(repo, &["status", "--porcelain", "--untracked-files=normal"])?;
    if !status.status.success() {
        return Err("Git no pudo leer el estado del repositorio".to_string());
    }
    if !status.stdout.is_empty() {
        return Err(
            "El proyecto tiene cambios o archivos sin seguimiento. Guarda esos cambios en Git antes de crear el entorno aislado."
                .to_string(),
        );
    }

    let head = run_git(repo, &["rev-parse", "--verify", "HEAD"])?;
    if !head.status.success() {
        return Err("El repositorio todavía no tiene un commit inicial".to_string());
    }
    String::from_utf8(head.stdout)
        .map(|commit| commit.trim().to_string())
        .map_err(|_| "Git devolvió un identificador de commit no válido".to_string())
}

fn create_git_worktree(
    repo: &Path,
    branch: &str,
    directory: &Path,
    base_commit: &str,
) -> Result<(), String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo)
        .args(["worktree", "add", "-b", branch])
        .arg(directory)
        .arg(base_commit);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .output()
        .map_err(|_| "No se encontró Git o no se pudo iniciar el proceso".to_string())?;
    if !output.status.success() {
        return Err("Git no pudo crear el entorno aislado".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{path::Path, process::Command};

    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

    use super::{create, list};

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
                .expect("worktree migration should apply");
            pool
        })
    }

    fn init_clean_repository(path: &Path) {
        let run = |args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(path)
                .args(args)
                .output()
                .expect("git should be installed for worktree tests");
            assert!(
                output.status.success(),
                "git test setup command should succeed"
            );
        };
        std::fs::write(path.join("README.md"), "test repo\n")
            .expect("test repository file should be written");
        run(&["init"]);
        run(&["config", "user.name", "Stade Studio Test"]);
        run(&["config", "user.email", "conductor-test@example.invalid"]);
        run(&["add", "README.md"]);
        run(&["commit", "-m", "initial commit"]);
    }

    #[test]
    fn creates_distinct_worktrees_and_records_them() {
        let repository = tempfile::tempdir().expect("test repository should be created");
        let worktrees_root = tempfile::tempdir().expect("worktree directory should be created");
        init_clean_repository(repository.path());
        let pool = test_pool();

        tauri::async_runtime::block_on(async {
            let project = crate::projects::create(
                &pool,
                "Worktree test".to_string(),
                repository.path().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("git project should be registered");
            assert!(project.is_git_repository);

            let first = create(
                &pool,
                worktrees_root.path(),
                project.id,
                "Backend".to_string(),
            )
            .await
            .expect("first isolated worktree should be created");
            let second = create(
                &pool,
                worktrees_root.path(),
                project.id,
                "Frontend".to_string(),
            )
            .await
            .expect("second isolated worktree should be created");

            assert_eq!(first.status, "ready");
            assert_eq!(second.status, "ready");
            assert_ne!(first.directory, second.directory);
            assert_ne!(first.branch_name, second.branch_name);
            assert!(Path::new(&first.directory).join("README.md").exists());
            let linked = super::attach_opencode_session(
                &pool,
                first.id,
                "ses_test",
                "build",
                "openai",
                "gpt-5.2",
                &first.directory,
                true,
            )
            .await
            .expect("OpenCode session should attach to its worktree");
            assert_eq!(linked.opencode_session_id.as_deref(), Some("ses_test"));
            assert_eq!(linked.opencode_location_matches, Some(true));
            assert_eq!(
                list(&pool, project.id)
                    .await
                    .expect("worktree list should load")
                    .len(),
                2
            );
        });
    }
}
