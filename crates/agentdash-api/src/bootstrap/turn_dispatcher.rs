use std::sync::Arc;

use agentdash_application::session::SessionHub;
use agentdash_application::task::execution::TaskExecutionError;
use agentdash_application::task::gateway::map_connector_error;
use agentdash_application::task::service::TurnDispatcher;
use async_trait::async_trait;

/// API 层取消适配器。
pub struct AppStateTurnDispatcher {
    pub(crate) session_hub: SessionHub,
}

impl AppStateTurnDispatcher {
    pub fn new(session_hub: SessionHub) -> Arc<Self> {
        Arc::new(Self { session_hub })
    }
}

#[async_trait]
impl TurnDispatcher for AppStateTurnDispatcher {
    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError> {
        self.session_hub
            .cancel(session_id)
            .await
            .map_err(map_connector_error)
    }
}
