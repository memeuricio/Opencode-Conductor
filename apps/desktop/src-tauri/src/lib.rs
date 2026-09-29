mod bridge;
mod coordination;
mod dispatch;
mod events;
mod integration;
mod opencode;
mod opencode_process;
mod projects;
mod sessions;
mod storage;
mod usage;
mod workspaces;
mod worktrees;

use tauri::{Emitter, Manager, State};

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
async fn start_opencode_event_stream(
    app: tauri::AppHandle,
    base_url: String,
    username: String,
    password: String,
    manager: State<'_, events::OpenCodeEventManager>,
) -> Result<(), String> {
    manager.start(app, &base_url, &username, &password).await
}

#[tauri::command]
async fn stop_opencode_event_stream(
    app: tauri::AppHandle,
    manager: State<'_, events::OpenCodeEventManager>,
) -> Result<(), String> {
    manager.stop(&app).await;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn get_opencode_usage(
    base_url: String,
    username: String,
    password: String,
    utc_offset_minutes: i64,
    timezone: String,
    local_day_start_ms: i64,
    local_month_start_ms: i64,
    days: i64,
) -> Result<usage::UsageOverview, String> {
    usage::fetch_overview(
        &base_url,
        &username,
        &password,
        utc_offset_minutes,
        &timezone,
        local_day_start_ms,
        local_month_start_ms,
        days,
    )
    .await
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
async fn preview_worktree_integration(
    worktree_id: i64,
    database: State<'_, storage::Database>,
) -> Result<integration::IntegrationPreview, String> {
    integration::preview(&database.pool, &database.worktrees_root, worktree_id).await
}

#[tauri::command]
async fn integrate_worktree(
    worktree_id: i64,
    database: State<'_, storage::Database>,
) -> Result<integration::IntegrationOutcome, String> {
    integration::integrate(&database.pool, &database.worktrees_root, worktree_id).await
}

#[allow(clippy::too_many_arguments)]
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

#[tauri::command]
async fn bridge_status(
    bridge: State<'_, bridge::BridgeManager>,
) -> Result<bridge::BridgeInfo, String> {
    Ok(bridge.info().await)
}

#[tauri::command]
async fn list_project_tasks(
    project_id: i64,
    database: State<'_, storage::Database>,
) -> Result<Vec<coordination::TaskDetail>, String> {
    coordination::list_project(&database.pool, project_id).await
}

#[tauri::command]
async fn list_workspaces(
    include_archived: bool,
    database: State<'_, storage::Database>,
) -> Result<Vec<workspaces::Workspace>, String> {
    workspaces::list(&database.pool, include_archived).await
}

#[tauri::command]
async fn create_workspace(
    name: String,
    description: Option<String>,
    database: State<'_, storage::Database>,
) -> Result<workspaces::Workspace, String> {
    workspaces::create(&database.pool, name, description).await
}

#[tauri::command]
async fn update_workspace(
    workspace_id: i64,
    name: String,
    description: Option<String>,
    database: State<'_, storage::Database>,
) -> Result<workspaces::Workspace, String> {
    workspaces::update(&database.pool, workspace_id, name, description).await
}

#[tauri::command]
async fn set_workspace_archived(
    workspace_id: i64,
    archived: bool,
    database: State<'_, storage::Database>,
) -> Result<workspaces::Workspace, String> {
    workspaces::set_archived(&database.pool, workspace_id, archived).await
}

#[tauri::command]
async fn delete_workspace(
    workspace_id: i64,
    database: State<'_, storage::Database>,
) -> Result<(), String> {
    workspaces::delete(&database.pool, workspace_id).await
}

#[tauri::command]
async fn add_workspace_project(
    workspace_id: i64,
    project_id: i64,
    database: State<'_, storage::Database>,
) -> Result<workspaces::Workspace, String> {
    workspaces::add_project(&database.pool, workspace_id, project_id).await
}

#[tauri::command]
async fn remove_workspace_project(
    workspace_id: i64,
    project_id: i64,
    database: State<'_, storage::Database>,
) -> Result<workspaces::Workspace, String> {
    workspaces::remove_project(&database.pool, workspace_id, project_id).await
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn start_ready_workspace_tasks(
    base_url: String,
    username: String,
    password: String,
    workspace_id: i64,
    database: State<'_, storage::Database>,
    bridge: State<'_, bridge::BridgeManager>,
) -> Result<Vec<dispatch::DispatchOutcome>, String> {
    let endpoint = bridge
        .current()
        .await
        .ok_or_else(|| "El puente local de herramientas no está activo".to_string())?;
    // Las tareas y worktrees pertenecen a cada proyecto; el espacio solo
    // agrupa. Se reutiliza el lanzamiento por proyecto y se agregan resultados.
    let project_ids = workspaces::active_project_ids(&database.pool, workspace_id).await?;
    let mut outcomes = Vec::new();
    for project_id in project_ids {
        outcomes.extend(
            dispatch::start_ready_tasks(
                &database.pool,
                &database.worktrees_root,
                &base_url,
                &username,
                &password,
                &endpoint,
                project_id,
            )
            .await?,
        );
    }
    Ok(outcomes)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn create_task(
    project_id: i64,
    title: String,
    objective: String,
    agent_id: String,
    provider_id: String,
    model_id: String,
    file_scope: String,
    depends_on_ids: Vec<i64>,
    database: State<'_, storage::Database>,
) -> Result<coordination::Task, String> {
    coordination::create(
        &database.pool,
        project_id,
        &title,
        &objective,
        &agent_id,
        &provider_id,
        &model_id,
        &file_scope,
        &depends_on_ids,
    )
    .await
}

#[tauri::command]
async fn update_task_definition(
    task_id: i64,
    title: String,
    objective: String,
    file_scope: String,
    database: State<'_, storage::Database>,
) -> Result<coordination::Task, String> {
    coordination::update_definition(&database.pool, task_id, &title, &objective, &file_scope).await
}

#[tauri::command]
async fn start_task(
    base_url: String,
    username: String,
    password: String,
    task_id: i64,
    allow_scope_conflicts: bool,
    database: State<'_, storage::Database>,
    bridge: State<'_, bridge::BridgeManager>,
) -> Result<coordination::Task, String> {
    let endpoint = bridge
        .current()
        .await
        .ok_or_else(|| "El puente local de herramientas no está activo".to_string())?;
    dispatch::start_task(
        &database.pool,
        &database.worktrees_root,
        &base_url,
        &username,
        &password,
        &endpoint,
        task_id,
        allow_scope_conflicts,
    )
    .await
}

#[tauri::command]
async fn start_ready_project_tasks(
    base_url: String,
    username: String,
    password: String,
    project_id: i64,
    database: State<'_, storage::Database>,
    bridge: State<'_, bridge::BridgeManager>,
) -> Result<Vec<dispatch::DispatchOutcome>, String> {
    let endpoint = bridge
        .current()
        .await
        .ok_or_else(|| "El puente local de herramientas no está activo".to_string())?;
    dispatch::start_ready_tasks(
        &database.pool,
        &database.worktrees_root,
        &base_url,
        &username,
        &password,
        &endpoint,
        project_id,
    )
    .await
}

#[tauri::command]
async fn accept_task_handoff(
    handoff_id: i64,
    database: State<'_, storage::Database>,
) -> Result<coordination::Task, String> {
    coordination::accept_handoff(&database.pool, handoff_id).await
}

#[tauri::command]
async fn return_task_handoff(
    base_url: String,
    username: String,
    password: String,
    handoff_id: i64,
    note: String,
    database: State<'_, storage::Database>,
) -> Result<coordination::Task, String> {
    dispatch::return_handoff_with_note(
        &database.pool,
        &database.worktrees_root,
        &base_url,
        &username,
        &password,
        handoff_id,
        &note,
    )
    .await
}

#[tauri::command]
async fn answer_task_decision(
    base_url: String,
    username: String,
    password: String,
    decision_id: i64,
    answer: String,
    database: State<'_, storage::Database>,
) -> Result<coordination::Task, String> {
    dispatch::answer_decision_with_prompt(
        &database.pool,
        &database.worktrees_root,
        &base_url,
        &username,
        &password,
        decision_id,
        &answer,
    )
    .await
}

#[tauri::command]
async fn complete_task_manually(
    task_id: i64,
    summary: String,
    database: State<'_, storage::Database>,
) -> Result<coordination::Task, String> {
    coordination::complete_manually(&database.pool, task_id, &summary).await
}

#[tauri::command]
async fn fail_task(
    task_id: i64,
    note: String,
    database: State<'_, storage::Database>,
) -> Result<coordination::Task, String> {
    coordination::mark_failed(&database.pool, task_id, &note).await
}

#[tauri::command]
async fn reopen_task(
    task_id: i64,
    database: State<'_, storage::Database>,
) -> Result<coordination::Task, String> {
    coordination::reopen(&database.pool, task_id).await
}

#[tauri::command]
async fn refresh_project_bridge_config(
    project_id: i64,
    database: State<'_, storage::Database>,
    bridge: State<'_, bridge::BridgeManager>,
) -> Result<(), String> {
    let endpoint = bridge
        .current()
        .await
        .ok_or_else(|| "El puente local de herramientas no está activo".to_string())?;
    dispatch::write_project_bridge_config(&database.worktrees_root, project_id, &endpoint)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let database = tauri::async_runtime::block_on(storage::connect(app_data_dir))
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let pool = database.pool.clone();
            app.manage(database);

            let bridge_manager = bridge::BridgeManager::default();
            let app_handle = app.handle().clone();
            let notify: bridge::Notify = std::sync::Arc::new(move || {
                let _ = app_handle.emit(bridge::COORDINATION_CHANGED, ());
            });
            // El puente puede fallar (por ejemplo, si el sistema bloquea el puerto);
            // la app sigue funcionando y la GUI muestra el motivo.
            let _ = tauri::async_runtime::block_on(bridge_manager.start(pool, notify));
            app.manage(bridge_manager);

            app.manage(opencode_process::OpenCodeProcessManager::default());
            app.manage(events::OpenCodeEventManager::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_opencode_connection,
            discover_opencode_catalog,
            start_managed_opencode_server,
            stop_managed_opencode_server,
            managed_opencode_server_status,
            start_opencode_event_stream,
            stop_opencode_event_stream,
            get_opencode_usage,
            list_projects,
            create_project,
            update_project,
            set_project_archived,
            list_project_worktrees,
            create_project_worktree,
            preview_worktree_integration,
            integrate_worktree,
            create_worktree_session,
            send_worktree_prompt,
            refresh_worktree_session,
            reply_to_worktree_permission,
            bridge_status,
            list_workspaces,
            create_workspace,
            update_workspace,
            set_workspace_archived,
            delete_workspace,
            add_workspace_project,
            remove_workspace_project,
            start_ready_workspace_tasks,
            list_project_tasks,
            create_task,
            update_task_definition,
            start_task,
            start_ready_project_tasks,
            accept_task_handoff,
            return_task_handoff,
            answer_task_decision,
            complete_task_manually,
            fail_task,
            reopen_task,
            refresh_project_bridge_config
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            let event_manager = app_handle.state::<events::OpenCodeEventManager>();
            tauri::async_runtime::block_on(event_manager.stop(app_handle));
            let bridge_manager = app_handle.state::<bridge::BridgeManager>();
            tauri::async_runtime::block_on(bridge_manager.stop());
            let process_manager = app_handle.state::<opencode_process::OpenCodeProcessManager>();
            // Tauri exits the process directly after RunEvent::Exit, so explicitly stop
            // the child here instead of relying on async Child drop during runtime teardown.
            tauri::async_runtime::block_on(process_manager.shutdown());
        }
    });
}
