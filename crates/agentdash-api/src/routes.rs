pub mod acp_sessions;
pub mod auth_routes;
pub mod backend_access;
pub mod backends;
pub mod canvases;
pub mod discovered_options;
pub mod discovery;
pub mod extension_package_artifacts;
pub mod extension_runtime;
pub mod file_picker;
pub mod health;
pub mod identity_directory;
pub mod llm_providers;
pub mod mcp_presets;
pub mod me;
pub mod project_agents;
pub mod project_extensions;
pub mod project_sessions;
pub mod project_vfs_mounts;
pub mod projects;
pub mod routines;
pub mod settings;
pub mod shared_library;
pub mod skill_assets;
pub mod stories;
pub mod story_sessions;
pub mod task_execution;
pub mod terminals;
pub mod vfs;
pub mod vfs_surfaces;
pub mod workflows;
pub mod workspaces;

use std::sync::Arc;

use agentdash_mcp::{services::McpServices, transport::McpRouterBuilder};
use axum::{Router, middleware, routing::get};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::app_state::AppState;
use crate::relay;
use crate::stream;

pub fn create_router(state: Arc<AppState>) -> Router {
    let mcp_services = Arc::new(McpServices {
        project_repo: state.repos.project_repo.clone(),
        story_repo: state.repos.story_repo.clone(),
        workspace_repo: state.repos.workspace_repo.clone(),
        workflow_definition_repo: state.repos.workflow_definition_repo.clone(),
        activity_lifecycle_definition_repo: state.repos.activity_lifecycle_definition_repo.clone(),
        state_change_repo: state.repos.state_change_repo.clone(),
    });
    let mcp = McpRouterBuilder::new(mcp_services)
        .build()
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::authenticate_request,
        ));

    let secured_api = Router::new()
        .merge(me::router())
        .merge(auth_routes::router())
        .merge(identity_directory::router())
        .merge(projects::router())
        .merge(project_vfs_mounts::router())
        .merge(llm_providers::router())
        .merge(project_agents::router())
        .merge(routines::router())
        .merge(project_sessions::router())
        .merge(canvases::router())
        .merge(mcp_presets::router())
        .merge(skill_assets::router())
        .merge(workspaces::router())
        .merge(backend_access::router())
        .merge(stories::router())
        .merge(story_sessions::router())
        .merge(task_execution::router())
        .merge(workflows::router())
        .merge(backends::router())
        .merge(settings::router())
        .merge(shared_library::router())
        .merge(extension_runtime::router())
        .merge(project_extensions::router())
        .merge(extension_package_artifacts::router())
        .merge(acp_sessions::router())
        .route("/events/stream/ndjson", get(stream::event_stream_ndjson))
        .merge(vfs::router())
        .merge(vfs_surfaces::router())
        .merge(terminals::router())
        .merge(file_picker::router())
        .merge(discovery::router())
        .merge(discovered_options::router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::authenticate_request,
        ));

    let api = Router::new()
        .merge(health::router())
        .merge(auth_routes::public_router())
        .merge(routines::public_router())
        .merge(extension_package_artifacts::public_router())
        .merge(secured_api)
        .with_state(state.clone());

    Router::new()
        .merge(mcp)
        .nest("/api", api)
        .route(
            "/ws/backend",
            get(relay::ws_handler::ws_backend_handler).with_state(state),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
