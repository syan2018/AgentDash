use std::sync::Arc;

use agentdash_api::{app_state::AppState, routes};
use axum::Router;

#[test]
fn product_interaction_routes_are_mounted_on_the_production_router() {
    let _: fn(Arc<AppState>) -> Router = routes::create_router;
    let routes = format!("{:?}", routes::lifecycle_agents::router());
    for path in [
        "/agent-runs/{run_id}/agents/{agent_id}/composer-submit",
        "/agent-runs/{run_id}/agents/{agent_id}/fork",
        "/agent-runs/{run_id}/agents/{agent_id}/fork-submit",
        "/agent-runs/{run_id}/agents/{agent_id}/cancel",
    ] {
        assert!(
            routes.contains(path),
            "production router is missing {path}: {routes}"
        );
    }
}
