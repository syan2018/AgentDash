use std::sync::Arc;

use agentdash_platform_spi::RuntimeToolProvider;
use agentdash_platform_spi::{DynAgentTool, ExecutionContext, PlatformRuntimeError};
use async_trait::async_trait;

use super::service::{WaitActivityDeps, WaitActivityService};
use super::tool::WaitTool;
use super::types::{WaitActivityOwnerScope, WaitToolContext};

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
    ) -> Result<Vec<DynAgentTool>, PlatformRuntimeError> {
        let owner = context
            .turn
            .platform_tool_execution
            .as_ref()
            .ok_or_else(|| {
                PlatformRuntimeError::InvalidConfig(
                    "缺少 Platform Tool typed owner context，无法构建 wait scope".to_string(),
                )
            })?;
        let runtime_thread_id = Some(owner.runtime_thread_id.clone());
        Ok(vec![Arc::new(WaitTool::new(
            self.service.clone(),
            WaitToolContext {
                runtime_thread_id,
                turn_id: context.session.turn_id.clone(),
                owner: Some(WaitActivityOwnerScope {
                    run_id: owner.run_id,
                    agent_id: owner.agent_id,
                    frame_id: owner.current_surface_frame_id,
                }),
            },
        ))])
    }
}
