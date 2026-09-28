use std::path::PathBuf;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

#[derive(Clone)]
pub struct Database {
    pub pool: SqlitePool,
    pub worktrees_root: PathBuf,
}

pub async fn connect(
    app_data_dir: PathBuf,
) -> Result<Database, Box<dyn std::error::Error + Send + Sync>> {
    std::fs::create_dir_all(&app_data_dir)?;
    let worktrees_root = app_data_dir.join("worktrees");
    std::fs::create_dir_all(&worktrees_root)?;

    let database_path = app_data_dir.join("conductor.sqlite3");
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(Database {
        pool,
        worktrees_root,
    })
}
