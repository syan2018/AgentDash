use std::sync::Arc;

use agentdash_agent_runtime::{
    PlatformToolBroker, RuntimeToolBrokerError, RuntimeToolResolvedContext,
};
use agentdash_agent_service_api::{AgentHostCallbackError, AgentToolResult};
use async_trait::async_trait;

use crate::{CompleteAgentToolHandler, ResolvedCompleteAgentToolCallback};

pub struct RuntimePlatformToolHandler {
    broker: Arc<PlatformToolBroker>,
}

impl RuntimePlatformToolHandler {
    pub fn new(broker: Arc<PlatformToolBroker>) -> Self {
        Self { broker }
    }
}

#[async_trait]
impl CompleteAgentToolHandler for RuntimePlatformToolHandler {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentToolCallback,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        let context = callback.context;
        match self
            .broker
            .invoke(
                RuntimeToolResolvedContext {
                    runtime_thread_id: context.runtime_thread_id,
                    binding_generation: context.binding_generation,
                    source: context.source,
                    service_instance_id: context.service_instance_id,
                    profile_digest: context.profile_digest,
                    bound_surface_revision: context.bound_surface_revision,
                    bound_surface_digest: context.bound_surface_digest,
                    applied_surface_revision: context.applied_surface_revision,
                    applied_surface_digest: context.applied_surface_digest,
                },
                callback.invocation.tool,
                callback.invocation.arguments,
            )
            .await
        {
            Ok(result) => Ok(result),
            Err(error) => Ok(rejected_result(error)),
        }
    }
}

fn rejected_result(error: RuntimeToolBrokerError) -> AgentToolResult {
    let code = match error {
        RuntimeToolBrokerError::EmptyCatalog => "empty_runtime_tool_catalog",
        RuntimeToolBrokerError::UnknownTool(_) => "unknown_runtime_tool",
        RuntimeToolBrokerError::DuplicateTool(_) => "duplicate_runtime_tool",
        RuntimeToolBrokerError::PermissionDenied { .. } => "runtime_tool_permission_denied",
        RuntimeToolBrokerError::EffectMismatch { .. } => "runtime_tool_effect_mismatch",
        RuntimeToolBrokerError::AuthorizationDenied { .. } => "runtime_tool_authorization_denied",
    };
    AgentToolResult::Rejected {
        code: code.to_owned(),
        message: error.to_string(),
    }
}
