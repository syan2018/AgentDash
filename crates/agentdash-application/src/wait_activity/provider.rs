use std::sync::Arc;

use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext};
use async_trait::async_trait;

use super::service::{WaitActivityDeps, WaitActivityService};
use super::tool::WaitTool;
use super::types::WaitToolContext;
use crate::runtime_tools::provider::runtime_session_id_from_context;

#[derive(Clone)]
pub struct WaitRuntimeToolProvider {
    service: WaitActivityService,
}

impl WaitRuntimeToolProvider {
    pub fn new(deps: WaitActivityDeps) -> Self {
        Self {
            service: WaitActivityService::new(deps),
        }
    }

    pub fn from_service(service: WaitActivityService) -> Self {
        Self { service }
    }
}

#[async_trait]
impl RuntimeToolProvider for WaitRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let runtime_thread_id = context
            .turn
            .hook_runtime
            .as_ref()
            .map(|runtime| runtime.session_id().to_string())
            .or_else(|| Some(runtime_session_id_from_context(context)))
            .and_then(|value| agentdash_agent_runtime_contract::RuntimeThreadId::new(value).ok());
        Ok(vec![Arc::new(WaitTool::new(
            self.service.clone(),
            WaitToolContext {
                runtime_thread_id,
                turn_id: context.session.turn_id.clone(),
            },
        ))])
    }
}
