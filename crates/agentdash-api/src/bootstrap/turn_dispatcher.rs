use std::sync::Arc;

use agentdash_application::session::SessionRuntimeService;
use agentdash_application::task::execution::TaskExecutionError;
use agentdash_application::task::gateway::map_connector_error;
use agentdash_application::task::service::TurnDispatcher;
use agentdash_application::workflow::RuntimeCancelDeliveryCommand;
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
    async fn deliver_runtime_cancel(
        &self,
        command: RuntimeCancelDeliveryCommand,
    ) -> Result<(), TaskExecutionError> {
        self.session_runtime
            .cancel(&command.runtime_session_id)
            .await
            .map_err(map_connector_error)
    }
}
