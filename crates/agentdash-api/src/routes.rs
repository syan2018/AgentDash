pub mod acp_sessions;
pub mod backends;
pub mod discovered_options;
pub mod discovery;
pub mod health;
pub mod projects;
pub mod stories;
pub mod workspaces;

use std::sync::Arc;

use axum::{
    Router,
    routing::{get, patch, post},
};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::app_state::AppState;
use crate::stream;

pub fn create_router(state: Arc<AppState>) -> Router {
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
        .route("/workspaces/pick-directory", post(workspaces::pick_directory))
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
            "/stories/{id}/tasks",
            get(stories::list_tasks).post(stories::create_task),
        )
        .route(
            "/tasks/{id}",
            get(stories::get_task)
                .put(stories::update_task)
                .delete(stories::delete_task),
        )
        // Backend
        .route(
            "/backends",
            get(backends::list_backends).post(backends::add_backend),
        )
        .route(
            "/backends/{id}",
            get(backends::get_backend).delete(backends::remove_backend),
        )
        // ACP Sessions
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
        .route("/acp/sessions/{id}/ws", get(acp_sessions::acp_session_ws))
        // Events
        .route("/events/stream", get(stream::event_stream))
        .route("/events/stream/ndjson", get(stream::event_stream_ndjson))
        .route("/events/since/{since_id}", get(stream::get_events_since))
        // Agent Discovery
        .route("/agents/discovery", get(discovery::get_discovery))
        .route(
            "/agents/discovered-options/ws",
            get(discovered_options::discovered_options_ws),
        );

    Router::new()
        .nest("/api", api)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
