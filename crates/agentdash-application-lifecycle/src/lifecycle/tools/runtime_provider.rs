use std::sync::Arc;

use agentdash_platform_spi::RuntimeToolProvider;
use agentdash_platform_spi::platform::tool_capability::CAP_WORKFLOW;
use agentdash_platform_spi::{DynAgentTool, ExecutionContext, PlatformRuntimeError, ToolCluster};
use async_trait::async_trait;

use crate::lifecycle::LifecycleOrchestratorDeps;
use crate::lifecycle::tools::advance_node::{
    CompleteLifecycleNodeTool, SharedRuntimeThreadToolServicesHandle,
};

#[derive(Clone)]
pub struct WorkflowRuntimeToolProvider {
    orchestrator_deps: LifecycleOrchestratorDeps,
    runtime_thread_services_handle: SharedRuntimeThreadToolServicesHandle,
}

impl WorkflowRuntimeToolProvider {
    pub fn new(
        orchestrator_deps: LifecycleOrchestratorDeps,
        runtime_thread_services_handle: SharedRuntimeThreadToolServicesHandle,
    ) -> Self {
        Self {
            orchestrator_deps,
            runtime_thread_services_handle,
        }
    }
}

#[async_trait]
impl RuntimeToolProvider for WorkflowRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, PlatformRuntimeError> {
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
            self.runtime_thread_services_handle.clone(),
            context,
        ))])
    }
}
