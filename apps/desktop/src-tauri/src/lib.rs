mod opencode;
mod opencode_process;
mod projects;
mod sessions;
mod storage;
mod worktrees;

use tauri::{Manager, State};

#[tauri::command]
async fn check_opencode_connection(
    base_url: String,
    username: String,
    password: String,
) -> Result<opencode::OpenCodeHealth, String> {
    opencode::check_connection(&base_url, &username, &password).await
}

#[tauri::command]
async fn discover_opencode_catalog(
    base_url: String,
    username: String,
    password: String,
) -> Result<opencode::OpenCodeCatalog, String> {
    opencode::discover_catalog(&base_url, &username, &password).await
}

#[tauri::command]
async fn start_managed_opencode_server(
    base_url: String,
    manager: State<'_, opencode_process::OpenCodeProcessManager>,
) -> Result<opencode_process::StartedOpenCodeServer, String> {
    manager.start(&base_url).await
}

#[tauri::command]
async fn stop_managed_opencode_server(
    manager: State<'_, opencode_process::OpenCodeProcessManager>,
) -> Result<(), String> {
    manager.stop().await
}

#[tauri::command]
async fn managed_opencode_server_status(
    manager: State<'_, opencode_process::OpenCodeProcessManager>,
) -> Result<opencode_process::OpenCodeProcessStatus, String> {
    manager.status().await
}

#[tauri::command]
async fn list_projects(
    include_archived: bool,
    database: State<'_, storage::Database>,
) -> Result<Vec<projects::Project>, String> {
    projects::list(&database.pool, include_archived).await
}

#[tauri::command]
async fn create_project(
    name: String,
    root_path: String,
    description: Option<String>,
    database: State<'_, storage::Database>,
) -> Result<projects::Project, String> {
    projects::create(&database.pool, name, root_path, description).await
}

#[tauri::command]
async fn update_project(
    project_id: i64,
    name: String,
    description: Option<String>,
    database: State<'_, storage::Database>,
) -> Result<projects::Project, String> {
    projects::update(&database.pool, project_id, name, description).await
}

#[tauri::command]
async fn set_project_archived(
    project_id: i64,
    archived: bool,
    database: State<'_, storage::Database>,
) -> Result<projects::Project, String> {
    projects::set_archived(&database.pool, project_id, archived).await
}

#[tauri::command]
async fn list_project_worktrees(
    project_id: i64,
    database: State<'_, storage::Database>,
) -> Result<Vec<worktrees::Worktree>, String> {
    worktrees::list(&database.pool, project_id).await
}

#[tauri::command]
async fn create_project_worktree(
    project_id: i64,
    label: String,
    database: State<'_, storage::Database>,
) -> Result<worktrees::Worktree, String> {
    worktrees::create(&database.pool, &database.worktrees_root, project_id, label).await
}

#[tauri::command]
async fn create_worktree_session(
    base_url: String,
    username: String,
    password: String,
    worktree_id: i64,
    agent_id: String,
    provider_id: String,
    model_id: String,
    database: State<'_, storage::Database>,
) -> Result<worktrees::Worktree, String> {
    sessions::create_for_worktree(
        &database.pool,
        &database.worktrees_root,
        &base_url,
        &username,
        &password,
        worktree_id,
        &agent_id,
        &provider_id,
        &model_id,
    )
    .await
}

#[tauri::command]
async fn send_worktree_prompt(
    base_url: String,
    username: String,
    password: String,
    worktree_id: i64,
    text: String,
    database: State<'_, storage::Database>,
) -> Result<sessions::PromptReceipt, String> {
    sessions::send_to_worktree(
        &database.pool,
        &database.worktrees_root,
        &base_url,
        &username,
        &password,
        worktree_id,
        &text,
    )
    .await
}

#[tauri::command]
async fn refresh_worktree_session(
    base_url: String,
    username: String,
    password: String,
    worktree_id: i64,
    database: State<'_, storage::Database>,
) -> Result<opencode::SessionSnapshot, String> {
    sessions::refresh_worktree_session(
        &database.pool,
        &database.worktrees_root,
        &base_url,
        &username,
        &password,
        worktree_id,
    )
    .await
}

#[tauri::command]
async fn reply_to_worktree_permission(
    base_url: String,
    username: String,
    password: String,
    worktree_id: i64,
    permission_id: String,
    allow_once: bool,
    database: State<'_, storage::Database>,
) -> Result<(), String> {
    sessions::decide_permission(
        &database.pool,
        &database.worktrees_root,
        &base_url,
        &username,
        &password,
        worktree_id,
        &permission_id,
        allow_once,
    )
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let database = tauri::async_runtime::block_on(storage::connect(app_data_dir))
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            app.manage(database);
            app.manage(opencode_process::OpenCodeProcessManager::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_opencode_connection,
            discover_opencode_catalog,
            start_managed_opencode_server,
            stop_managed_opencode_server,
            managed_opencode_server_status,
            list_projects,
            create_project,
            update_project,
            set_project_archived,
            list_project_worktrees,
            create_project_worktree,
            create_worktree_session,
            send_worktree_prompt,
            refresh_worktree_session,
            reply_to_worktree_permission
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            let process_manager = app_handle.state::<opencode_process::OpenCodeProcessManager>();
            // Tauri exits the process directly after RunEvent::Exit, so explicitly stop
            // the child here instead of relying on async Child drop during runtime teardown.
            tauri::async_runtime::block_on(process_manager.shutdown());
        }
    });
}
