use serde::Serialize;
use sqlx::SqlitePool;

use crate::{opencode, worktrees};

const MAX_PROMPT_CHARS: usize = 20_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptReceipt {
    accepted: bool,
}

pub async fn create_for_worktree(
    pool: &SqlitePool,
    worktrees_root: &std::path::Path,
    base_url: &str,
    username: &str,
    password: &str,
    worktree_id: i64,
    agent_id: &str,
    provider_id: &str,
    model_id: &str,
) -> Result<worktrees::Worktree, String> {
    let worktree = worktrees::get_by_id(pool, worktree_id).await?;
    if worktree.status != "ready" {
        return Err("El entorno no está listo para abrir una sesión".to_string());
    }
    if worktree.opencode_session_id.is_some() {
        return Err("Este entorno ya tiene una sesión de OpenCode asociada".to_string());
    }
    if agent_id.trim().is_empty() || agent_id.chars().count() > 200 {
        return Err("Selecciona un perfil de agente válido".to_string());
    }
    if provider_id.trim().is_empty()
        || provider_id.chars().count() > 200
        || model_id.trim().is_empty()
        || model_id.chars().count() > 300
    {
        return Err("Selecciona un modelo válido".to_string());
    }

    worktrees::validate_managed_directory(worktrees_root, &worktree.directory)?;

    let created = opencode::create_session(
        base_url,
        username,
        password,
        &worktree.directory,
        &worktree.label,
        agent_id,
        provider_id,
        model_id,
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
        worktree_id,
        &created.session_id,
        agent_id,
        provider_id,
        model_id,
        &created.location_directory,
        created.location_matches,
    )
    .await
}

pub async fn send_to_worktree(
    pool: &SqlitePool,
    worktrees_root: &std::path::Path,
    base_url: &str,
    username: &str,
    password: &str,
    worktree_id: i64,
    text: &str,
) -> Result<PromptReceipt, String> {
    validate_prompt(text)?;
    let worktree = checked_worktree(
        pool,
        worktrees_root,
        base_url,
        username,
        password,
        worktree_id,
    )
    .await?;
    let session_id = worktree
        .opencode_session_id
        .as_deref()
        .ok_or_else(|| "Este entorno todavía no tiene una sesión OpenCode".to_string())?;

    opencode::send_session_prompt(base_url, username, password, session_id, text).await?;
    Ok(PromptReceipt { accepted: true })
}

pub async fn refresh_worktree_session(
    pool: &SqlitePool,
    worktrees_root: &std::path::Path,
    base_url: &str,
    username: &str,
    password: &str,
    worktree_id: i64,
) -> Result<opencode::SessionSnapshot, String> {
    let worktree = checked_worktree(
        pool,
        worktrees_root,
        base_url,
        username,
        password,
        worktree_id,
    )
    .await?;
    let session_id = worktree
        .opencode_session_id
        .as_deref()
        .ok_or_else(|| "Este entorno todavía no tiene una sesión OpenCode".to_string())?;

    opencode::session_snapshot(
        base_url,
        username,
        password,
        session_id,
        &worktree.directory,
    )
    .await
}

pub async fn decide_permission(
    pool: &SqlitePool,
    worktrees_root: &std::path::Path,
    base_url: &str,
    username: &str,
    password: &str,
    worktree_id: i64,
    permission_id: &str,
    allow_once: bool,
) -> Result<(), String> {
    let worktree = checked_worktree(
        pool,
        worktrees_root,
        base_url,
        username,
        password,
        worktree_id,
    )
    .await?;
    let session_id = worktree
        .opencode_session_id
        .as_deref()
        .ok_or_else(|| "Este entorno todavía no tiene una sesión OpenCode".to_string())?;

    opencode::reply_to_permission(
        base_url,
        username,
        password,
        session_id,
        permission_id,
        allow_once,
    )
    .await
}

async fn checked_worktree(
    pool: &SqlitePool,
    worktrees_root: &std::path::Path,
    base_url: &str,
    username: &str,
    password: &str,
    worktree_id: i64,
) -> Result<worktrees::Worktree, String> {
    let worktree = worktrees::get_by_id(pool, worktree_id).await?;
    if worktree.status != "ready" {
        return Err("El entorno de trabajo no está listo".to_string());
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
        .ok_or_else(|| "Este entorno todavía no tiene una sesión OpenCode".to_string())?;
    opencode::verify_session_directory(
        base_url,
        username,
        password,
        session_id,
        &worktree.directory,
    )
    .await?;
    Ok(worktree)
}

fn validate_prompt(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("Escribe la tarea que quieres enviar al agente".to_string());
    }
    if text.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!(
            "La tarea no puede superar {MAX_PROMPT_CHARS} caracteres"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_prompt;

    #[test]
    fn rejects_empty_and_oversized_prompts() {
        assert!(validate_prompt("  \n ").is_err());
        assert!(validate_prompt(&"x".repeat(20_001)).is_err());
        assert!(validate_prompt("Implementa la tarea y ejecuta las pruebas").is_ok());
    }
}
