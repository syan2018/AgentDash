//! Product control-plane route composition evidence.
//!
//! These assertions inspect the actual Axum routers used by `create_router`. They require no
//! database fixture because route reachability is a composition property; repository and
//! projection behavior remains covered by the owning Product modules.

use std::sync::Arc;

use agentdash_api::{app_state::AppState, routes};
use axum::Router;

fn assert_routes(router: Router<Arc<AppState>>, expected: &[&str]) {
    let routes = format!("{router:?}");
    for path in expected {
        assert!(
            routes.contains(path),
            "production router is missing {path}: {routes}"
        );
    }
}

#[test]
fn product_control_plane_routes_expose_runtime_backed_capabilities() {
    let _: fn(Arc<AppState>) -> Router = routes::create_router;

    assert_routes(
        routes::interactions::router(),
        &[
            "/projects/{project_id}/interaction-definitions/canvas",
            "/interaction-definitions/{definition_id}",
            "/interaction-definitions/{definition_id}/promote-extension",
            "/interaction-definitions/{definition_id}/instances",
            "/projects/{project_id}/interaction-instances",
            "/interaction-instances/{instance_id}",
            "/interaction-instances/{instance_id}/commands",
            "/interaction-instances/{instance_id}/presentation",
        ],
    );
    assert_routes(
        routes::workspace_module::router(),
        &[
            "/projects/{project_id}/workspace-modules",
            "/projects/{project_id}/workspace-modules/present",
        ],
    );
    assert_routes(
        routes::lifecycle_agents::router(),
        &[
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/view",
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/context/projection",
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/updates",
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/commands",
            "/agent-runs/{run_id}/agents/{agent_id}/workspace",
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/snapshot",
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/changes",
        ],
    );
    assert_routes(
        routes::terminals::router(),
        &[
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals",
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/{id}/input",
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/{id}/resize",
            "/agent-runs/{run_id}/agents/{agent_id}/runtime/terminals/{id}",
        ],
    );
    assert_routes(routes::vfs::router(), &["/vfs", "/vfs/{space_id}/entries"]);
    assert_routes(
        routes::vfs_surfaces::router(),
        &[
            "/vfs-surfaces/resolve",
            "/vfs-surfaces/{surface_ref}",
            "/vfs-surfaces/{surface_ref}/mounts/{mount_id}/entries",
            "/vfs-surfaces/read-file",
        ],
    );
}
