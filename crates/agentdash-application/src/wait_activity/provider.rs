use std::sync::Arc;

use agentdash_application_runtime_session::session::terminal_cache::SessionTerminalCache;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext};
use async_trait::async_trait;

use super::service::WaitActivityService;
use super::tool::WaitTool;
use super::types::WaitToolContext;
use crate::repository_set::RepositorySet;
use crate::runtime_tools::provider::runtime_session_id_from_context;

#[derive(Clone)]
pub struct WaitRuntimeToolProvider {
    service: WaitActivityService,
}

impl WaitRuntimeToolProvider {
    pub fn new(repos: RepositorySet, terminal_cache: Arc<SessionTerminalCache>) -> Self {
        Self {
            service: WaitActivityService::from_repository_set(repos, terminal_cache),
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
        let delivery_runtime_session_id = context
            .turn
            .hook_runtime
            .as_ref()
            .map(|runtime| runtime.session_id().to_string())
            .or_else(|| Some(runtime_session_id_from_context(context)));
        Ok(vec![Arc::new(WaitTool::new(
            self.service.clone(),
            WaitToolContext {
                delivery_runtime_session_id,
                turn_id: context.session.turn_id.clone(),
            },
        ))])
    }
}
