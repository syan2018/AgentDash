use std::sync::Arc;

use agentdash_agent::dash::{DashCoreError, DashToolCall, DashToolCallbacks, DashToolResult};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentEffectIdentity, AgentHostCallbackMeta,
    AgentHostCallbacks, AgentIdempotencyKey, AgentItemId, AgentSourceCoordinate,
    AgentToolInvocation, AgentToolName, AgentToolResult, AgentTurnId,
};
use async_trait::async_trait;

pub struct DashAgentCoreToolCallbacks {
    callbacks: Arc<dyn AgentHostCallbacks>,
    route_id: AgentCallbackRouteId,
    binding_generation: AgentBindingGeneration,
    source: AgentSourceCoordinate,
    deadline_at_ms: u64,
}

impl DashAgentCoreToolCallbacks {
    pub fn new(
        callbacks: Arc<dyn AgentHostCallbacks>,
        route_id: AgentCallbackRouteId,
        binding_generation: AgentBindingGeneration,
        source: AgentSourceCoordinate,
        deadline_at_ms: u64,
    ) -> Self {
        Self {
            callbacks,
            route_id,
            binding_generation,
            source,
            deadline_at_ms,
        }
    }
}

#[async_trait]
impl DashToolCallbacks for DashAgentCoreToolCallbacks {
    async fn invoke(
        &self,
        turn_id: &agentdash_agent::dash::AgentTurnId,
        call: DashToolCall,
    ) -> Result<DashToolResult, DashCoreError> {
        let item_id =
            AgentItemId::new(call.call_id.clone()).map_err(|error| DashCoreError::Tool {
                message: error.to_string(),
                retryable: false,
            })?;
        let invocation = AgentToolInvocation {
            meta: AgentHostCallbackMeta {
                route_id: self.route_id.clone(),
                binding_generation: self.binding_generation,
                source: self.source.clone(),
                turn_id: AgentTurnId::new(turn_id.0.clone()).map_err(|error| {
                    DashCoreError::Tool {
                        message: error.to_string(),
                        retryable: false,
                    }
                })?,
                item_id: Some(item_id),
                interaction_id: None,
                effect_id: AgentEffectIdentity::new(format!("tool:{}", call.call_id)).map_err(
                    |error| DashCoreError::Tool {
                        message: error.to_string(),
                        retryable: false,
                    },
                )?,
                idempotency_key: AgentIdempotencyKey::new(format!("tool:{}", call.call_id))
                    .map_err(|error| DashCoreError::Tool {
                        message: error.to_string(),
                        retryable: false,
                    })?,
                deadline_at_ms: self.deadline_at_ms,
            },
            tool: AgentToolName::new(call.name).map_err(|error| DashCoreError::Tool {
                message: error.to_string(),
                retryable: false,
            })?,
            arguments: call.arguments,
        };
        let result = self
            .callbacks
            .invoke_tool(invocation)
            .await
            .map_err(|error| DashCoreError::Tool {
                message: error.to_string(),
                retryable: false,
            })?;
        match result {
            AgentToolResult::Completed { output } => Ok(DashToolResult {
                call_id: call.call_id,
                content: output.to_string(),
                is_error: false,
            }),
            AgentToolResult::Rejected { code, message }
            | AgentToolResult::Failed { code, message } => Ok(DashToolResult {
                call_id: call.call_id,
                content: serde_json::json!({"code": code, "message": message}).to_string(),
                is_error: true,
            }),
        }
    }
}
