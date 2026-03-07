pub mod acp_sessions;
pub mod backends;
pub mod discovered_options;
pub mod discovery;
pub mod health;
pub mod projects;
pub mod settings;
pub mod stories;
pub mod story_sessions;
pub mod task_execution;
pub mod workspace_files;
pub mod workspaces;

use std::sync::Arc;

use agentdash_mcp::{services::McpServices, transport::McpRouterBuilder};
use axum::{
    Router,
    routing::{delete, get, patch, post},
};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::app_state::AppState;
use crate::stream;

pub fn create_router(state: Arc<AppState>) -> Router {
    let mcp_services = Arc::new(McpServices {
        project_repo: state.project_repo.clone(),
        story_repo: state.story_repo.clone(),
        task_repo: state.task_repo.clone(),
        workspace_repo: state.workspace_repo.clone(),
    });
    let mcp = McpRouterBuilder::new(mcp_services).build();

    let api = Router::new()
        .route("/health", get(health::health_check))
        // Project CRUD
        .route(
            "/projects",
            get(projects::list_projects).post(projects::create_project),
        )
        .route(
            "/projects/{id}",
            get(projects::get_project)
                .put(projects::update_project)
                .delete(projects::delete_project),
        )
        // Workspace（嵌套在 Project 下创建/列表，独立路由操作）
        .route(
            "/projects/{project_id}/workspaces",
            get(workspaces::list_workspaces).post(workspaces::create_workspace),
        )
        .route(
            "/workspaces/pick-directory",
            post(workspaces::pick_directory),
        )
        .route("/workspaces/detect-git", post(workspaces::detect_git))
        .route(
            "/workspaces/{id}",
            get(workspaces::get_workspace)
                .put(workspaces::update_workspace)
                .delete(workspaces::delete_workspace),
        )
        .route(
            "/workspaces/{id}/status",
            patch(workspaces::update_workspace_status),
        )
        // Story（支持 project_id 或 backend_id 查询）
        .route(
            "/stories",
            get(stories::list_stories).post(stories::create_story),
        )
        .route(
            "/stories/{id}",
            get(stories::get_story)
                .put(stories::update_story)
                .delete(stories::delete_story),
        )
        .route(
            "/stories/{id}/sessions",
            get(story_sessions::list_story_sessions).post(story_sessions::create_story_session),
        )
        .route(
            "/stories/{id}/sessions/{binding_id}",
            axum::routing::delete(story_sessions::unbind_story_session),
        )
        .route(
            "/stories/{id}/tasks",
            get(stories::list_tasks).post(stories::create_task),
        )
        .route(
            "/tasks/{id}",
            get(stories::get_task)
                .put(stories::update_task)
                .delete(stories::delete_task),
        )
        .route("/tasks/{id}/start", post(task_execution::start_task))
        .route("/tasks/{id}/continue", post(task_execution::continue_task))
        .route("/tasks/{id}/cancel", post(task_execution::cancel_task))
        .route("/tasks/{id}/session", get(task_execution::get_task_session))
        // Backend
        .route(
            "/backends",
            get(backends::list_backends).post(backends::add_backend),
        )
        .route(
            "/backends/{id}",
            get(backends::get_backend).delete(backends::remove_backend),
        )
        // Settings
        .route(
            "/settings",
            get(settings::list_settings).put(settings::update_settings),
        )
        .route("/settings/{key}", delete(settings::delete_setting))
        // ACP Sessions — CRUD
        .route(
            "/sessions",
            get(acp_sessions::list_sessions).post(acp_sessions::create_session),
        )
        .route(
            "/sessions/{id}",
            get(acp_sessions::get_session).delete(acp_sessions::delete_session),
        )
        .route(
            "/sessions/{id}/bindings",
            get(acp_sessions::get_session_bindings),
        )
        // ACP Sessions — Execution
        .route("/sessions/{id}/prompt", post(acp_sessions::prompt_session))
        .route("/sessions/{id}/cancel", post(acp_sessions::cancel_session))
        .route(
            "/acp/sessions/{id}/stream",
            get(acp_sessions::acp_session_stream_sse),
        )
        .route(
            "/acp/sessions/{id}/stream/ndjson",
            get(acp_sessions::acp_session_stream_ndjson),
        )
        // Events
        .route("/events/stream", get(stream::event_stream))
        .route("/events/stream/ndjson", get(stream::event_stream_ndjson))
        .route("/events/since/{since_id}", get(stream::get_events_since))
        // Workspace Files（内部 API，用于 @ 文件引用选择器）
        .route("/workspace-files", get(workspace_files::list_files))
        .route("/workspace-files/read", post(workspace_files::read_file))
        .route(
            "/workspace-files/batch-read",
            post(workspace_files::batch_read_files),
        )
        // Agent Discovery
        .route("/agents/discovery", get(discovery::get_discovery))
        .route(
            "/agents/discovered-options/stream",
            get(discovered_options::discovered_options_stream),
        )
        .with_state(state);

    Router::new()
        .merge(mcp)
        .nest("/api", api)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
