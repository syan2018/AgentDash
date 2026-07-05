mod agent_run_mailbox_contracts;
pub mod auth_routes;
pub mod backend_access;
pub mod backends;
pub mod canvases;
pub mod companion_gates;
pub mod diagnostics;
pub mod discovered_options;
pub mod discovery;
pub mod extension_package_artifacts;
pub mod extension_runtime;
pub mod file_picker;
pub mod health;
pub mod identity_directory;
pub mod lifecycle_agents;
mod lifecycle_contracts;
pub mod lifecycle_views;
pub mod llm_providers;
pub mod marketplace;
pub mod mcp_presets;
pub mod me;
pub mod permission_grants;
pub mod project_agents;
pub mod project_extensions;
pub mod project_vfs_mounts;
pub mod projects;
pub mod release_info;
pub mod routines;
pub mod runner_registration_tokens;
pub mod runtime_traces;
pub mod settings;
pub mod shared_library;
pub mod skill_assets;
pub mod stories;
pub mod story_runs;
pub mod task_plan;
pub mod terminals;
pub mod vfs;
pub mod vfs_surfaces;
pub mod workflows;
pub mod workspace_module;
pub mod workspaces;

use std::{path::PathBuf, sync::Arc};

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_mcp::{services::McpServices, transport::McpRouterBuilder};
use axum::{Router, middleware, routing::get};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::app_state::AppState;
use crate::relay;
use crate::stream;

pub fn create_router(state: Arc<AppState>) -> Router {
    let mcp_services = Arc::new(McpServices {
        project_repo: state.repos.project_repo.clone(),
        story_repo: state.repos.story_repo.clone(),
        workspace_repo: state.repos.workspace_repo.clone(),
        agent_procedure_repo: state.repos.agent_procedure_repo.clone(),
        workflow_graph_repo: state.repos.workflow_graph_repo.clone(),
        lifecycle_run_repo: state.repos.lifecycle_run_repo.clone(),
        lifecycle_subject_association_repo: state.repos.lifecycle_subject_association_repo.clone(),
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
        .merge(permission_grants::router())
        .merge(llm_providers::router())
        .merge(project_agents::router())
        .merge(routines::router())
        .merge(runner_registration_tokens::router())
        .merge(canvases::router())
        .merge(companion_gates::router())
        .merge(mcp_presets::router())
        .merge(skill_assets::router())
        .merge(workspaces::router())
        .merge(backend_access::router())
        .merge(stories::router())
        .merge(story_runs::router())
        .merge(task_plan::router())
        .merge(lifecycle_agents::router())
        .merge(lifecycle_views::router())
        .merge(workflows::router())
        .merge(backends::router())
        .merge(settings::router())
        .merge(shared_library::router())
        .merge(marketplace::router())
        .merge(extension_runtime::router())
        .merge(workspace_module::router())
        .merge(project_extensions::router())
        .merge(extension_package_artifacts::router())
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
        .route("/version", get(release_info::version_info))
        .merge(health::router())
        .merge(auth_routes::public_router())
        .merge(routines::public_router())
        .merge(runner_registration_tokens::public_router())
        .merge(extension_package_artifacts::public_router())
        .merge(diagnostics::router())
        .merge(secured_api)
        .with_state(state.clone());

    let router = Router::new()
        .merge(mcp)
        .nest("/api", api)
        .route(
            "/.well-known/agentdash",
            get(release_info::agentdash_discovery),
        )
        .route(
            "/ws/backend",
            get(relay::ws_handler::ws_backend_handler).with_state(state),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    with_web_static_fallback(router)
}

fn with_web_static_fallback(router: Router) -> Router {
    let Some(web_dist_dir) = std::env::var("AGENTDASH_WEB_DIST_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    else {
        return router;
    };

    if !web_dist_dir.is_dir() {
        diag!(
            Warn,
            Subsystem::Api,
            path = %web_dist_dir.display(),
            "AGENTDASH_WEB_DIST_DIR 不存在，跳过 Web Dashboard 静态托管"
        );
        return router;
    }

    let index_file = web_dist_dir.join("index.html");
    diag!(
        Info,
        Subsystem::Api,
        path = %web_dist_dir.display(),
        "启用 Web Dashboard 静态托管"
    );
    router
        .fallback_service(ServeDir::new(web_dist_dir).not_found_service(ServeFile::new(index_file)))
}
