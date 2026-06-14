use std::sync::Arc;

use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::platform::tool_capability::CAP_WORKFLOW;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext, ToolCluster};
use async_trait::async_trait;

use crate::platform_config::SharedPlatformConfig;
use crate::vfs::tools::provider::SharedSessionToolServicesHandle;
use crate::workflow::tools::advance_node::CompleteLifecycleNodeTool;

#[derive(Clone)]
pub struct WorkflowRuntimeToolProvider {
    repos: crate::repository_set::RepositorySet,
    session_services_handle: SharedSessionToolServicesHandle,
    platform_config: SharedPlatformConfig,
    function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
}

impl WorkflowRuntimeToolProvider {
    pub fn new(
        repos: crate::repository_set::RepositorySet,
        session_services_handle: SharedSessionToolServicesHandle,
        platform_config: SharedPlatformConfig,
        function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    ) -> Self {
        Self {
            repos,
            session_services_handle,
            platform_config,
            function_runner,
        }
    }
}

#[async_trait]
impl RuntimeToolProvider for WorkflowRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let flow = &context.turn.capability_state;
        if !flow.tool.enabled_clusters.contains(&ToolCluster::Workflow)
            || !flow.is_capability_tool_enabled(
                CAP_WORKFLOW,
                "complete_lifecycle_node",
                Some(ToolCluster::Workflow),
            )
        {
            return Ok(Vec::new());
        }

        Ok(vec![Arc::new(CompleteLifecycleNodeTool::new(
            self.repos.clone(),
            self.session_services_handle.get().await,
            Some(self.function_runner.clone()),
            self.platform_config.clone(),
            context,
        ))])
    }
}
