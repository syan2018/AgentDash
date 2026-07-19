use std::sync::Arc;

use agentdash_platform_spi::RuntimeToolProvider;
use agentdash_platform_spi::platform::tool_capability::CAP_TASK;
use agentdash_platform_spi::{PlatformRuntimeError, DynAgentTool, ExecutionContext, ToolCluster};
use async_trait::async_trait;

use crate::task::tools::{TaskReadTool, TaskWriteTool};

#[derive(Clone)]
pub struct TaskRuntimeToolProvider {
    repos: crate::repository_set::RepositorySet,
}

impl TaskRuntimeToolProvider {
    pub fn new(repos: crate::repository_set::RepositorySet) -> Self {
        Self { repos }
    }
}

#[async_trait]
impl RuntimeToolProvider for TaskRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, PlatformRuntimeError> {
        let flow = &context.turn.capability_state;
        if !flow.tool.enabled_clusters.contains(&ToolCluster::Task) {
            return Ok(Vec::new());
        }

        let mut tools: Vec<DynAgentTool> = Vec::new();
        if flow.is_capability_tool_enabled(CAP_TASK, "task_read", Some(ToolCluster::Task)) {
            tools.push(Arc::new(TaskReadTool::new(self.repos.clone(), context)));
        }
        if flow.is_capability_tool_enabled(CAP_TASK, "task_write", Some(ToolCluster::Task)) {
            tools.push(Arc::new(TaskWriteTool::new(self.repos.clone(), context)));
        }
        Ok(tools)
    }
}
