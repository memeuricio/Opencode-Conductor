use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

const MAX_KEYWORDS: usize = 20;
const MAX_SCOPE_LINES: usize = 50;
const MAX_RECOMMENDATIONS: usize = 3;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub agent_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub fallback_provider_id: Option<String>,
    pub fallback_model_id: Option<String>,
    pub file_scope: String,
    pub match_keywords: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

const ROLE_COLUMNS: &str = "id, name, description, instructions, agent_id, provider_id, model_id, \
     fallback_provider_id, fallback_model_id, file_scope, match_keywords, created_at, updated_at, \
     archived_at";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleRecommendation {
    pub role: Role,
    pub reasons: Vec<String>,
}

pub async fn list(pool: &SqlitePool, include_archived: bool) -> Result<Vec<Role>, String> {
    let query = if include_archived {
        format!(
            "SELECT {ROLE_COLUMNS} FROM roles \
             ORDER BY archived_at IS NOT NULL, name COLLATE NOCASE"
        )
    } else {
        format!(
            "SELECT {ROLE_COLUMNS} FROM roles WHERE archived_at IS NULL ORDER BY name COLLATE NOCASE"
        )
    };
    sqlx::query_as::<_, Role>(&query)
        .fetch_all(pool)
        .await
        .map_err(|_| "No se pudieron cargar los roles locales".to_string())
}

pub async fn get(pool: &SqlitePool, role_id: i64) -> Result<Role, String> {
    let query = format!("SELECT {ROLE_COLUMNS} FROM roles WHERE id = ?");
    sqlx::query_as::<_, Role>(&query)
        .bind(role_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo cargar el rol".to_string())?
        .ok_or_else(|| "El rol ya no existe".to_string())
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    name: String,
    description: Option<String>,
    instructions: Option<String>,
    agent_id: String,
    provider_id: String,
    model_id: String,
    fallback_provider_id: Option<String>,
    fallback_model_id: Option<String>,
    file_scope: String,
    match_keywords: String,
) -> Result<Role, String> {
    let validated = validate_fields(
        &name,
        description,
        instructions,
        &agent_id,
        &provider_id,
        &model_id,
        fallback_provider_id,
        fallback_model_id,
        &file_scope,
        &match_keywords,
    )?;
    let query = format!(
        "INSERT INTO roles (name, name_key, description, instructions, agent_id, provider_id, \
         model_id, fallback_provider_id, fallback_model_id, file_scope, match_keywords) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING {ROLE_COLUMNS}"
    );
    sqlx::query_as::<_, Role>(&query)
        .bind(validated.name)
        .bind(validated.name_key)
        .bind(validated.description)
        .bind(validated.instructions)
        .bind(validated.agent_id)
        .bind(validated.provider_id)
        .bind(validated.model_id)
        .bind(validated.fallback_provider_id)
        .bind(validated.fallback_model_id)
        .bind(validated.file_scope)
        .bind(validated.match_keywords)
        .fetch_one(pool)
        .await
        .map_err(map_write_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &SqlitePool,
    role_id: i64,
    name: String,
    description: Option<String>,
    instructions: Option<String>,
    agent_id: String,
    provider_id: String,
    model_id: String,
    fallback_provider_id: Option<String>,
    fallback_model_id: Option<String>,
    file_scope: String,
    match_keywords: String,
) -> Result<Role, String> {
    let validated = validate_fields(
        &name,
        description,
        instructions,
        &agent_id,
        &provider_id,
        &model_id,
        fallback_provider_id,
        fallback_model_id,
        &file_scope,
        &match_keywords,
    )?;
    let query = format!(
        "UPDATE roles \
         SET name = ?, name_key = ?, description = ?, instructions = ?, agent_id = ?, \
             provider_id = ?, model_id = ?, fallback_provider_id = ?, fallback_model_id = ?, \
             file_scope = ?, match_keywords = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ? AND archived_at IS NULL RETURNING {ROLE_COLUMNS}"
    );
    sqlx::query_as::<_, Role>(&query)
        .bind(validated.name)
        .bind(validated.name_key)
        .bind(validated.description)
        .bind(validated.instructions)
        .bind(validated.agent_id)
        .bind(validated.provider_id)
        .bind(validated.model_id)
        .bind(validated.fallback_provider_id)
        .bind(validated.fallback_model_id)
        .bind(validated.file_scope)
        .bind(validated.match_keywords)
        .bind(role_id)
        .fetch_optional(pool)
        .await
        .map_err(map_write_error)?
        .ok_or_else(|| "El rol no existe o está archivado".to_string())
}

pub async fn set_archived(pool: &SqlitePool, role_id: i64, archived: bool) -> Result<Role, String> {
    let query = format!(
        "UPDATE roles \
         SET archived_at = CASE WHEN ? THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now') ELSE NULL END, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ? RETURNING {ROLE_COLUMNS}"
    );
    sqlx::query_as::<_, Role>(&query)
        .bind(archived)
        .bind(role_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudo cambiar el estado del rol".to_string())?
        .ok_or_else(|| "El rol ya no existe".to_string())
}

/// Borrar un rol no borra tareas: la columna `tasks.role_id` queda en NULL
/// por `ON DELETE SET NULL` y cada tarea conserva sus valores copiados.
pub async fn delete(pool: &SqlitePool, role_id: i64) -> Result<(), String> {
    let affected = sqlx::query("DELETE FROM roles WHERE id = ?")
        .bind(role_id)
        .execute(pool)
        .await
        .map_err(|_| "No se pudo eliminar el rol".to_string())?
        .rows_affected();
    if affected == 0 {
        return Err("El rol ya no existe".to_string());
    }
    Ok(())
}

/// Recomendación transparente y configurable: puntúa por palabras clave en
/// título/objetivo y por solapamiento de ámbito. La comparación ignora
/// mayúsculas y tildes para no fallar con texto en español. No afirma calidad
/// objetiva; devuelve motivos legibles para que el usuario decida y pueda cambiarlo.
pub async fn recommend(
    pool: &SqlitePool,
    title: &str,
    objective: &str,
    file_scope: &str,
) -> Result<Vec<RoleRecommendation>, String> {
    let roles = list(pool, false).await?;
    let title = fold_spanish(title);
    let objective = fold_spanish(objective);
    let scope_lines: Vec<String> = file_scope
        .lines()
        .map(|line| fold_spanish(line.trim()))
        .filter(|line| !line.is_empty())
        .take(MAX_SCOPE_LINES)
        .collect();

    let mut scored: Vec<(i64, RoleRecommendation)> = Vec::new();
    for role in roles {
        let mut score: i64 = 0;
        let mut reasons = Vec::new();
        // Palabras clave explícitas más palabras del nombre y la descripción,
        // para que «Backend» encaje con un objetivo que dice «backend».
        let mut terms = split_keywords(&role.match_keywords);
        terms.extend(name_terms(&role.name, &role.description));
        for keyword in terms {
            if title.contains(&keyword) {
                score += 3;
                reasons.push(format!("«{keyword}» aparece en el título"));
            } else if objective.contains(&keyword) {
                score += 2;
                reasons.push(format!("«{keyword}» aparece en el objetivo"));
            }
        }
        if !scope_lines.is_empty() {
            for role_line in role
                .file_scope
                .lines()
                .map(|line| fold_spanish(line.trim()))
                .filter(|line| !line.is_empty())
                .take(MAX_SCOPE_LINES)
            {
                if scope_lines.iter().any(|task_line| {
                    task_line.contains(&role_line) || role_line.contains(task_line)
                }) {
                    score += 2;
                    reasons.push(format!("cubre «{role_line}» del ámbito"));
                }
            }
        }
        if score > 0 {
            scored.push((score, RoleRecommendation { role, reasons }));
        }
    }
    scored.sort_by(|left, right| right.0.cmp(&left.0));
    scored.truncate(MAX_RECOMMENDATIONS);
    Ok(scored.into_iter().map(|(_, item)| item).collect())
}

fn split_keywords(raw: &str) -> Vec<String> {
    raw.split([',', '\n', ';'])
        .map(|part| fold_spanish(part.trim()))
        .filter(|part| !part.is_empty())
        .take(MAX_KEYWORDS)
        .collect()
}

/// Palabras del nombre y la descripción (de 4+ letras) como términos
/// adicionales, sin duplicar las claves explícitas.
fn name_terms(name: &str, description: &Option<String>) -> Vec<String> {
    let mut terms = Vec::new();
    let mut corpus = name.to_string();
    if let Some(description) = description {
        corpus.push(' ');
        corpus.push_str(description);
    }
    for word in fold_spanish(&corpus).split(|c: char| !c.is_alphanumeric()) {
        if word.chars().count() >= 4 && !terms.contains(&word.to_string()) {
            terms.push(word.to_string());
        }
    }
    terms.into_iter().take(MAX_KEYWORDS).collect()
}

/// Minúsculas sin tildes para comparar texto en español sin dependencias.
fn fold_spanish(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            _ => c,
        })
        .collect()
}

struct ValidatedRole {
    name: String,
    name_key: String,
    description: Option<String>,
    instructions: Option<String>,
    agent_id: String,
    provider_id: String,
    model_id: String,
    fallback_provider_id: Option<String>,
    fallback_model_id: Option<String>,
    file_scope: String,
    match_keywords: String,
}

#[allow(clippy::too_many_arguments)]
fn validate_fields(
    name: &str,
    description: Option<String>,
    instructions: Option<String>,
    agent_id: &str,
    provider_id: &str,
    model_id: &str,
    fallback_provider_id: Option<String>,
    fallback_model_id: Option<String>,
    file_scope: &str,
    match_keywords: &str,
) -> Result<ValidatedRole, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Escribe un nombre para el rol".to_string());
    }
    if name.chars().count() > 100 {
        return Err("El nombre del rol no puede superar 100 caracteres".to_string());
    }
    let agent_id = required_text(agent_id, 200, "El perfil de agente del rol")?;
    let provider_id = required_text(provider_id, 200, "El proveedor del modelo preferido")?;
    let model_id = required_text(model_id, 300, "El modelo preferido")?;
    let fallback_provider_id = optional_text(fallback_provider_id, 200)?;
    let fallback_model_id = optional_text(fallback_model_id, 300)?;
    if fallback_provider_id.is_some() != fallback_model_id.is_some() {
        return Err("El modelo alternativo necesita proveedor y modelo a la vez".to_string());
    }
    Ok(ValidatedRole {
        name: name.to_string(),
        name_key: name.to_lowercase(),
        description: optional_capped(description, 500, "La descripción")?,
        instructions: optional_capped(instructions, 4000, "Las instrucciones")?,
        agent_id,
        provider_id,
        model_id,
        fallback_provider_id,
        fallback_model_id,
        file_scope: capped(file_scope.trim(), 4000, "El ámbito de archivos")?,
        match_keywords: capped(match_keywords.trim(), 500, "Las palabras clave")?,
    })
}

fn required_text(value: &str, max: usize, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{label} no puede estar vacío"));
    }
    if value.chars().count() > max {
        return Err(format!("{label} es demasiado largo"));
    }
    Ok(value.to_string())
}

fn optional_text(value: Option<String>, max: usize) -> Result<Option<String>, String> {
    let value = value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    if value
        .as_ref()
        .is_some_and(|text| text.chars().count() > max)
    {
        return Err("El modelo alternativo es demasiado largo".to_string());
    }
    Ok(value)
}

fn optional_capped(
    value: Option<String>,
    max: usize,
    label: &str,
) -> Result<Option<String>, String> {
    let value = value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    if value
        .as_ref()
        .is_some_and(|text| text.chars().count() > max)
    {
        return Err(format!("{label} superan el límite de {max} caracteres"));
    }
    Ok(value)
}

fn capped(value: &str, max: usize, label: &str) -> Result<String, String> {
    if value.chars().count() > max {
        return Err(format!("{label} supera el límite de {max} caracteres"));
    }
    Ok(value.to_string())
}

fn map_write_error(error: sqlx::Error) -> String {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error.is_unique_violation() {
            return "Ya existe un rol con ese nombre".to_string();
        }
    }
    "No se pudo guardar el rol en la base de datos local".to_string()
}

#[cfg(test)]
mod tests {
    use super::{create, delete, list, recommend};
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
                .expect("role migration should apply");
            pool
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn sample_role(
        pool: &SqlitePool,
        name: &str,
        keywords: &str,
        scope: &str,
    ) -> super::Role {
        create(
            pool,
            name.to_string(),
            None,
            Some("Sigue el contrato aprobado".to_string()),
            "build".to_string(),
            "openai".to_string(),
            "gpt-test".to_string(),
            Some("openai".to_string()),
            Some("gpt-fallback".to_string()),
            scope.to_string(),
            keywords.to_string(),
        )
        .await
        .expect("role should be created")
    }

    #[test]
    fn recommends_by_keywords_and_scope_with_reasons() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            sample_role(&pool, "Backend", "api, endpoint", "src/api").await;
            sample_role(&pool, "Base de datos", "migración, esquema", "migrations").await;

            let suggestions = recommend(
                &pool,
                "Exponer endpoint de sesiones",
                "Construir la API con pruebas",
                "src/api/sessions.ts",
            )
            .await
            .expect("recommendation should load");
            assert!(!suggestions.is_empty());
            assert_eq!(suggestions[0].role.name, "Backend");
            assert!(suggestions[0]
                .reasons
                .iter()
                .any(|reason| reason.contains("endpoint")));
            assert!(suggestions[0]
                .reasons
                .iter()
                .any(|reason| reason.contains("src/api")));
        });
    }

    #[test]
    fn matching_ignores_accents_and_uses_the_role_name() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            sample_role(&pool, "Base de datos", "esquema", "").await;

            // «migracion» sin tilde encaja con la clave «migración»; «datos»
            // viene del nombre del rol aunque no esté en las claves.
            let suggestions = recommend(&pool, "Migracion inicial", "Crear la base de datos", "")
                .await
                .expect("recommendation should load");
            assert!(!suggestions.is_empty());
            assert_eq!(suggestions[0].role.name, "Base de datos");
        });
    }

    #[test]
    fn empty_draft_gets_no_recommendation() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            sample_role(&pool, "Backend", "api", "src/api").await;
            let suggestions = recommend(&pool, "Varios", "Sin pistas", "")
                .await
                .expect("recommendation should load");
            assert!(suggestions.is_empty());
        });
    }

    #[test]
    fn deleting_a_role_keeps_existing_tasks_intact() {
        let pool = test_pool();
        tauri::async_runtime::block_on(async {
            let role = sample_role(&pool, "Backend", "api", "").await;
            let directory = tempfile::tempdir().expect("temp dir should exist");
            let project = crate::projects::create(
                &pool,
                "Roles".to_string(),
                directory.keep().to_string_lossy().into_owned(),
                None,
            )
            .await
            .expect("project should be created");
            let task = crate::coordination::create(
                &pool,
                project.id,
                "Tarea con rol",
                "Objetivo",
                "build",
                "openai",
                "gpt-test",
                "",
                &[],
                Some(role.id),
            )
            .await
            .expect("task should be created");
            assert_eq!(task.role_id, Some(role.id));

            delete(&pool, role.id)
                .await
                .expect("role should be deleted");
            let reloaded = crate::coordination::get(&pool, task.id)
                .await
                .expect("task should load");
            assert_eq!(reloaded.role_id, None);
            assert_eq!(reloaded.model_id, "gpt-test");
            assert!(list(&pool, false)
                .await
                .expect("roles should load")
                .is_empty());
        });
    }
}
