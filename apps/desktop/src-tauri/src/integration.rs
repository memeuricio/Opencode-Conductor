use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::SqlitePool;

use crate::worktrees;

/// Límite de caracteres del diff devuelto a la GUI. El diff completo vive en
/// Git; aquí solo viaja un extracto para revisión.
const MAX_DIFF_CHARS: usize = 120_000;
const MAX_FILES: usize = 500;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationFile {
    status: String,
    path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationPreview {
    worktree_id: i64,
    label: String,
    branch: String,
    target_branch: String,
    ahead: u32,
    behind: u32,
    worktree_clean: bool,
    main_clean: bool,
    files: Vec<IntegrationFile>,
    files_truncated: bool,
    diff: String,
    diff_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationOutcome {
    worktree_id: i64,
    branch: String,
    target_branch: String,
    commit: String,
    files: Vec<IntegrationFile>,
}

pub async fn preview(
    pool: &SqlitePool,
    worktrees_root: &Path,
    worktree_id: i64,
) -> Result<IntegrationPreview, String> {
    let worktree = worktrees::get_by_id(pool, worktree_id).await?;
    if worktree.status != "ready" {
        return Err("El entorno todavía no está listo para revisar".to_string());
    }
    let project_root = project_root(pool, worktree.project_id).await?;
    worktrees::validate_managed_directory(worktrees_root, &worktree.directory)?;

    let worktree_dir = PathBuf::from(worktree.directory.clone());
    let branch = worktree.branch_name.clone();
    tauri::async_runtime::spawn_blocking(move || {
        preview_blocking(&project_root, &worktree_dir, &branch)
    })
    .await
    .map_err(|_| "No se pudo completar la revisión de cambios".to_string())?
    .map(|mut preview| {
        preview.worktree_id = worktree_id;
        preview.label = worktree.label.clone();
        preview
    })
}

pub async fn integrate(
    pool: &SqlitePool,
    worktrees_root: &Path,
    worktree_id: i64,
) -> Result<IntegrationOutcome, String> {
    let worktree = worktrees::get_by_id(pool, worktree_id).await?;
    if worktree.status != "ready" {
        return Err("El entorno todavía no está listo para integrar".to_string());
    }
    let project_root = project_root(pool, worktree.project_id).await?;
    worktrees::validate_managed_directory(worktrees_root, &worktree.directory)?;

    let worktree_dir = PathBuf::from(worktree.directory.clone());
    let branch = worktree.branch_name.clone();
    let label = worktree.label.clone();
    tauri::async_runtime::spawn_blocking(move || {
        integrate_blocking(&project_root, &worktree_dir, &branch, &label)
    })
    .await
    .map_err(|_| "No se pudo completar la integración".to_string())?
    .map(|mut outcome| {
        outcome.worktree_id = worktree_id;
        outcome
    })
}

async fn project_root(pool: &SqlitePool, project_id: i64) -> Result<PathBuf, String> {
    let root: Option<String> =
        sqlx::query_scalar("SELECT root_path FROM projects WHERE id = ? AND archived_at IS NULL")
            .bind(project_id)
            .fetch_optional(pool)
            .await
            .map_err(|_| "No se pudo consultar el proyecto".to_string())?
            .flatten();
    root.map(PathBuf::from)
        .ok_or_else(|| "El proyecto ya no existe o está archivado".to_string())
}

fn preview_blocking(
    repo: &Path,
    worktree_dir: &Path,
    branch: &str,
) -> Result<IntegrationPreview, String> {
    let target_branch = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if target_branch == "HEAD" {
        return Err("El repositorio principal no tiene una rama activa".to_string());
    }
    // La rama existe si el worktree sigue registrado en este repositorio.
    run_git(repo, &["rev-parse", "--verify", branch])?;

    let base = run_git(repo, &["merge-base", "HEAD", branch])?;
    let ahead = run_git(repo, &["rev-list", "--count", &format!("{base}..{branch}")])?
        .parse::<u32>()
        .map_err(|_| "Git devolvió un conteo no válido".to_string())?;
    let behind = run_git(repo, &["rev-list", "--count", &format!("{branch}..HEAD")])?
        .parse::<u32>()
        .map_err(|_| "Git devolvió un conteo no válido".to_string())?;

    let worktree_clean = is_clean(worktree_dir)?;
    let main_clean = is_clean(repo)?;

    let name_status = run_git(repo, &["diff", "--name-status", &base, branch])?;
    let mut files = Vec::new();
    for line in name_status.lines() {
        if files.len() >= MAX_FILES {
            break;
        }
        let mut parts = line.split('\t');
        let status = parts.next().unwrap_or("").to_string();
        // Los renombrados traen dos rutas; se muestra el destino.
        let path = parts.last().unwrap_or("").to_string();
        if status.is_empty() || path.is_empty() {
            continue;
        }
        files.push(IntegrationFile { status, path });
    }
    let files_truncated = name_status.lines().count() > files.len();

    let raw_diff = run_git(repo, &["diff", &base, branch, "--"])?;
    let (diff, diff_truncated) = truncate(&raw_diff, MAX_DIFF_CHARS);

    Ok(IntegrationPreview {
        worktree_id: 0,
        label: String::new(),
        branch: branch.to_string(),
        target_branch,
        ahead,
        behind,
        worktree_clean,
        main_clean,
        files,
        files_truncated,
        diff,
        diff_truncated,
    })
}

fn integrate_blocking(
    repo: &Path,
    worktree_dir: &Path,
    branch: &str,
    label: &str,
) -> Result<IntegrationOutcome, String> {
    let target_branch = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if target_branch == "HEAD" {
        return Err("El repositorio principal no tiene una rama activa".to_string());
    }

    // Proteger cambios locales: ambos lados deben estar limpios antes de tocar nada.
    if !is_clean(worktree_dir)? {
        return Err(
            "El entorno tiene cambios sin guardar en Git. Guárdalos antes de integrar".to_string(),
        );
    }
    if !is_clean(repo)? {
        return Err(
            "El repositorio principal tiene cambios sin guardar. Guárdalos antes de integrar"
                .to_string(),
        );
    }

    let base = run_git(repo, &["merge-base", "HEAD", branch])?;
    let name_status = run_git(repo, &["diff", "--name-status", &base, branch])?;
    if name_status.trim().is_empty() {
        return Err("No hay cambios que integrar en esta rama".to_string());
    }
    let files = parse_files(&name_status);

    // Merge explícito sin commit: si hay conflictos se aborta y el árbol
    // principal queda intacto (partíamos de un árbol limpio).
    let merge = run_git_status(repo, &["merge", "--no-commit", "--no-ff", branch])?;
    if !merge.success {
        let conflicted = run_git(repo, &["diff", "--name-only", "--diff-filter=U"])?;
        let _ = run_git_status(repo, &["merge", "--abort"]);
        let files = conflicted.trim().replace('\n', ", ");
        return Err(if files.is_empty() {
            "La integración tiene conflictos y se abortó sin tocar tu rama".to_string()
        } else {
            format!("La integración tiene conflictos en {files} y se abortó sin tocar tu rama")
        });
    }

    run_git(
        repo,
        &[
            "commit",
            "-m",
            &format!("Integra entorno «{label}» ({branch})"),
        ],
    )?;
    let commit = run_git(repo, &["rev-parse", "HEAD"])?;

    Ok(IntegrationOutcome {
        worktree_id: 0,
        branch: branch.to_string(),
        target_branch,
        commit,
        files,
    })
}

fn parse_files(name_status: &str) -> Vec<IntegrationFile> {
    name_status
        .lines()
        .take(MAX_FILES)
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let status = parts.next()?.to_string();
            let path = parts.last()?.to_string();
            if status.is_empty() || path.is_empty() {
                return None;
            }
            Some(IntegrationFile { status, path })
        })
        .collect()
}

fn truncate(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_string(), false);
    }
    let truncated: String = text.chars().take(max_chars).collect();
    (truncated, true)
}

fn is_clean(repo: &Path) -> Result<bool, String> {
    let status = run_git(repo, &["status", "--porcelain", "--untracked-files=normal"])?;
    Ok(status.trim().is_empty())
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = run_git_status(repo, args)?;
    if !output.success {
        return Err("Git no pudo completar la operación de revisión".to_string());
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_string())
        .map_err(|_| "Git devolvió una respuesta no válida".to_string())
}

struct GitOutput {
    success: bool,
    stdout: Vec<u8>,
}

fn run_git_status(repo: &Path, args: &[&str]) -> Result<GitOutput, String> {
    let mut command = std::process::Command::new("git");
    command.arg("-C").arg(repo).args(args);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .output()
        .map_err(|_| "No se encontró Git o no se pudo iniciar el proceso".to_string())?;
    Ok(GitOutput {
        success: output.status.success(),
        stdout: output.stdout,
    })
}

#[cfg(test)]
mod tests {
    use std::{path::Path, process::Command};

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
                .expect("integration migration should apply");
            pool
        })
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("git should be installed for integration tests");
        assert!(
            output.status.success(),
            "git test setup command should succeed"
        );
    }

    fn init_repository(path: &Path) {
        std::fs::write(path.join("app.txt"), "v1\n").expect("test file should be written");
        run_git(path, &["init"]);
        run_git(path, &["config", "user.name", "Stade Studio Test"]);
        run_git(
            path,
            &["config", "user.email", "conductor-test@example.invalid"],
        );
        run_git(path, &["add", "app.txt"]);
        run_git(path, &["commit", "-m", "initial commit"]);
    }

    #[test]
    fn previews_and_integrates_worktree_changes() {
        let repository = tempfile::tempdir().expect("test repository should be created");
        let worktrees_root = tempfile::tempdir().expect("worktree directory should be created");
        init_repository(repository.path());
        let pool = test_pool();

        tauri::async_runtime::block_on(async {
            let project = crate::projects::create(
                &pool,
                "Integration test".to_string(),
                repository.path().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("git project should be registered");
            let worktree = crate::worktrees::create(
                &pool,
                worktrees_root.path(),
                project.id,
                "Backend".to_string(),
            )
            .await
            .expect("isolated worktree should be created");

            // El agente trabaja en el worktree y guarda su cambio en Git.
            let worktree_path = Path::new(&worktree.directory);
            std::fs::write(worktree_path.join("app.txt"), "v2\n")
                .expect("change should be written");
            run_git(worktree_path, &["add", "app.txt"]);
            run_git(worktree_path, &["commit", "-m", "agent change"]);

            let preview = super::preview(&pool, worktrees_root.path(), worktree.id)
                .await
                .expect("preview should load");
            assert_eq!(preview.ahead, 1);
            assert_eq!(preview.behind, 0);
            assert!(preview.worktree_clean);
            assert!(preview.main_clean);
            assert_eq!(preview.files.len(), 1);
            assert_eq!(preview.files[0].path, "app.txt");
            assert!(preview.diff.contains("v2"));

            let outcome = super::integrate(&pool, worktrees_root.path(), worktree.id)
                .await
                .expect("integration should succeed");
            assert!(!outcome.commit.is_empty());

            let merged = std::fs::read_to_string(repository.path().join("app.txt"))
                .expect("file should read");
            assert_eq!(merged.trim(), "v2");
        });
    }

    #[test]
    fn integration_blocks_on_dirty_main_checkout() {
        let repository = tempfile::tempdir().expect("test repository should be created");
        let worktrees_root = tempfile::tempdir().expect("worktree directory should be created");
        init_repository(repository.path());
        let pool = test_pool();

        tauri::async_runtime::block_on(async {
            let project = crate::projects::create(
                &pool,
                "Dirty test".to_string(),
                repository.path().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("git project should be registered");
            let worktree = crate::worktrees::create(
                &pool,
                worktrees_root.path(),
                project.id,
                "Backend".to_string(),
            )
            .await
            .expect("isolated worktree should be created");

            let worktree_path = Path::new(&worktree.directory);
            std::fs::write(worktree_path.join("app.txt"), "v2\n")
                .expect("change should be written");
            run_git(worktree_path, &["add", "app.txt"]);
            run_git(worktree_path, &["commit", "-m", "agent change"]);

            // Cambio local sin guardar en el checkout principal.
            std::fs::write(repository.path().join("notes.txt"), "local\n")
                .expect("local file should be written");

            let error = super::integrate(&pool, worktrees_root.path(), worktree.id)
                .await
                .expect_err("dirty main checkout should block integration");
            assert!(error.contains("principal"));
            let merged = std::fs::read_to_string(repository.path().join("app.txt"))
                .expect("file should read");
            assert_eq!(merged, "v1\n");
        });
    }
}
