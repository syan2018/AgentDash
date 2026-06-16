use std::sync::Arc;

use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::platform::tool_capability::CAP_COLLABORATION;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext, ToolCluster};
use async_trait::async_trait;

use crate::companion::tool_context::CompanionToolContext;
use crate::companion::tools::{CompanionRequestTool, CompanionRespondTool};
use crate::runtime_tools::provider::SharedSessionToolServicesHandle;

#[derive(Clone)]
pub struct CollaborationRuntimeToolProvider {
    repos: crate::repository_set::RepositorySet,
    session_services_handle: SharedSessionToolServicesHandle,
}

impl CollaborationRuntimeToolProvider {
    pub fn new(
        repos: crate::repository_set::RepositorySet,
        session_services_handle: SharedSessionToolServicesHandle,
    ) -> Self {
        Self {
            repos,
            session_services_handle,
        }
    }
}

#[async_trait]
impl RuntimeToolProvider for CollaborationRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let flow = &context.turn.capability_state;
        if !flow
            .tool
            .enabled_clusters
            .contains(&ToolCluster::Collaboration)
        {
            return Ok(Vec::new());
        }

        let mut tools: Vec<DynAgentTool> = Vec::new();
        let companion_tool_context = CompanionToolContext::from_execution_context(context);
        if flow.is_capability_tool_enabled(
            CAP_COLLABORATION,
            "companion_request",
            Some(ToolCluster::Collaboration),
        ) {
            tools.push(Arc::new(CompanionRequestTool::new(
                self.repos.project_agent_repo.clone(),
                self.repos.clone(),
                self.session_services_handle.clone(),
                companion_tool_context.clone(),
                context.session.executor_config.clone(),
            )));
        }
        if flow.is_capability_tool_enabled(
            CAP_COLLABORATION,
            "companion_respond",
            Some(ToolCluster::Collaboration),
        ) {
            tools.push(Arc::new(CompanionRespondTool::new(
                self.repos.clone(),
                self.session_services_handle.clone(),
                companion_tool_context,
            )));
        }
        Ok(tools)
    }
}
