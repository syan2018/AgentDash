use std::sync::Arc;

use agentdash_application::session::{PromptSessionRequest, SessionHub, UserPromptInput};
use agentdash_application::task::execution::{StartedTurn, TaskExecutionError};
use agentdash_application::task::gateway::{PreparedTurnContext, map_connector_error};
use agentdash_application::task::service::TurnDispatcher;
use async_trait::async_trait;

use crate::relay::registry::BackendRegistry;
use crate::runtime_bridge::runtime_mcp_servers_to_acp;
use crate::workspace_resolution::resolve_workspace_binding_core;
use agentdash_application::workspace::WorkspaceResolutionError;

/// API 层 `TurnDispatcher` 实现 — 所有执行器统一走 `SessionHub.start_prompt()`。
///
/// relay 与 cloud-native 的差异由 `CompositeConnector` 内部的子连接器处理，
/// dispatcher 只负责 workspace 解析和构建 `PromptSessionRequest`。
pub struct AppStateTurnDispatcher {
    pub(crate) session_hub: SessionHub,
    pub(crate) backend_registry: Arc<BackendRegistry>,
}

impl AppStateTurnDispatcher {
    pub fn new(
        session_hub: SessionHub,
        backend_registry: Arc<BackendRegistry>,
    ) -> Arc<Self> {
        Arc::new(Self {
            session_hub,
            backend_registry,
        })
    }
}

#[async_trait]
impl TurnDispatcher for AppStateTurnDispatcher {
    async fn dispatch_turn(
        &self,
        session_id: &str,
        ctx: PreparedTurnContext,
    ) -> Result<StartedTurn, TaskExecutionError> {
        let workspace_root = if let Some(ws) = ctx.workspace.as_ref() {
            let resolved =
                resolve_workspace_binding_core(self.backend_registry.as_ref(), ws)
                    .await
                    .map_err(|e| match e {
                        WorkspaceResolutionError::NoBindings(msg)
                        | WorkspaceResolutionError::NoAvailable(msg) => {
                            TaskExecutionError::Internal(msg)
                        }
                    })?;
            Some(std::path::PathBuf::from(resolved.root_ref.clone()))
        } else {
            None
        };

        let prompt_req = PromptSessionRequest {
            user_input: UserPromptInput {
                prompt_blocks: Some(ctx.built.prompt_blocks),
                working_dir: ctx.built.working_dir,
                env: Default::default(),
                executor_config: ctx.resolved_config.clone(),
            },
            mcp_servers: runtime_mcp_servers_to_acp(&ctx.built.mcp_servers),
            workspace_root,
            address_space: ctx.address_space.clone(),
            flow_capabilities: Some(agentdash_spi::FlowCapabilities::from_clusters([
                agentdash_spi::ToolCluster::Read,
                agentdash_spi::ToolCluster::Write,
                agentdash_spi::ToolCluster::Execute,
                agentdash_spi::ToolCluster::Workflow,
                agentdash_spi::ToolCluster::Collaboration,
                agentdash_spi::ToolCluster::Canvas,
            ])),
            system_context: ctx.built.system_context.clone(),
            bootstrap_action: agentdash_application::session::SessionBootstrapAction::None,
            identity: ctx.identity,
            post_turn_handler: ctx.post_turn_handler,
        };

        let turn_id = self
            .session_hub
            .start_prompt(session_id, prompt_req)
            .await
            .map_err(map_connector_error)?;

        Ok(StartedTurn {
            turn_id,
            context_sources: ctx.built.source_summary,
        })
    }

    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError> {
        self.session_hub
            .cancel(session_id)
            .await
            .map_err(map_connector_error)
    }
}
