use sqlx::SqlitePool;

/// Reads instructions from pre-0.2 task roles so tasks already stored in an
/// existing local database keep their context after the UI moves to the single
/// Builder profile.
pub async fn legacy_instructions(
    pool: &SqlitePool,
    role_id: i64,
) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT instructions FROM roles WHERE id = ?")
        .bind(role_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "No se pudieron cargar las instrucciones heredadas de la tarea".to_string())
        .map(Option::flatten)
}
