use std::sync::Arc;

use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::platform::tool_capability::CAP_WORKFLOW;
use agentdash_spi::{ConnectorError, DynAgentTool, ExecutionContext, ToolCluster};
use async_trait::async_trait;

use crate::SharedPlatformConfig;
use crate::lifecycle::LifecycleOrchestratorDeps;
use crate::lifecycle::tools::advance_node::{
    CompleteLifecycleNodeTool, SharedSessionToolServicesHandle,
};

#[derive(Clone)]
pub struct WorkflowRuntimeToolProvider {
    orchestrator_deps: LifecycleOrchestratorDeps,
    session_services_handle: SharedSessionToolServicesHandle,
    platform_config: SharedPlatformConfig,
    function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
}

impl WorkflowRuntimeToolProvider {
    pub fn new(
        orchestrator_deps: LifecycleOrchestratorDeps,
        session_services_handle: SharedSessionToolServicesHandle,
        platform_config: SharedPlatformConfig,
        function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    ) -> Self {
        Self {
            orchestrator_deps,
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
            self.orchestrator_deps.clone(),
            self.session_services_handle.clone(),
            Some(self.function_runner.clone()),
            self.platform_config.clone(),
            context,
        ))])
    }
}
