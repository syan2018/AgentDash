use std::sync::Arc;

use agentdash_application::session::SessionRuntimeService;
use agentdash_application::task::execution::TaskExecutionError;
use agentdash_application::task::gateway::map_connector_error;
use agentdash_application::task::service::TurnDispatcher;
use async_trait::async_trait;

/// API 层取消适配器。
pub struct AppStateTurnDispatcher {
    pub(crate) session_runtime: SessionRuntimeService,
}

impl AppStateTurnDispatcher {
    pub fn new(session_runtime: SessionRuntimeService) -> Arc<Self> {
        Arc::new(Self { session_runtime })
    }
}

#[async_trait]
impl TurnDispatcher for AppStateTurnDispatcher {
    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError> {
        self.session_runtime
            .cancel(session_id)
            .await
            .map_err(map_connector_error)
    }
}
