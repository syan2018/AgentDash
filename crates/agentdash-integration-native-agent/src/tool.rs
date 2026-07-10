use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    DriverItemId, DriverThreadId, DriverTurnId, RuntimeBindingId, RuntimeDriverGeneration,
    ToolSetRevision,
};
use agentdash_agent_types::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
};
use agentdash_integration_api::{
    AgentRuntimeToolCallback, DriverToolDefinition, DriverToolInvocation, DriverToolOutcome,
};
use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

pub(crate) struct NativeRuntimeTool {
    definition: DriverToolDefinition,
    binding_id: RuntimeBindingId,
    generation: RuntimeDriverGeneration,
    source_thread_id: DriverThreadId,
    active_turn: Arc<RwLock<Option<DriverTurnId>>>,
    tool_set_revision: ToolSetRevision,
    callback: Arc<dyn AgentRuntimeToolCallback>,
}

impl NativeRuntimeTool {
    pub(crate) fn new(
        definition: DriverToolDefinition,
        binding_id: RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        source_thread_id: DriverThreadId,
        active_turn: Arc<RwLock<Option<DriverTurnId>>>,
        tool_set_revision: ToolSetRevision,
        callback: Arc<dyn AgentRuntimeToolCallback>,
    ) -> Self {
        Self {
            definition,
            binding_id,
            generation,
            source_thread_id,
            active_turn,
            tool_set_revision,
            callback,
        }
    }
}

#[async_trait]
impl AgentTool for NativeRuntimeTool {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.definition.parameters_schema.clone()
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let source_turn_id = self.active_turn.read().await.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed("native tool invoked without an active turn".into())
        })?;
        let source_item_id = tool_call_id.parse::<DriverItemId>().map_err(|error| {
            AgentToolError::ExecutionFailed(format!("invalid native tool call identity: {error}"))
        })?;
        let callback = self.callback.invoke(DriverToolInvocation {
            binding_id: self.binding_id.clone(),
            generation: self.generation,
            source_thread_id: self.source_thread_id.clone(),
            source_turn_id,
            source_item_id,
            tool_set_revision: self.tool_set_revision,
            tool_name: self.definition.name.clone(),
            arguments: args,
            timeout_ms: 120_000,
        });
        let outcome = tokio::select! {
            _ = cancel.cancelled() => return Err(AgentToolError::ExecutionFailed("tool call cancelled".into())),
            outcome = callback => outcome.map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?,
        };
        match outcome {
            DriverToolOutcome::Completed { output, is_error } => Ok(AgentToolResult {
                content: vec![ContentPart::text(output.to_string())],
                is_error,
                details: Some(output),
            }),
            DriverToolOutcome::InteractionRequired { reason, .. } => {
                Err(AgentToolError::ExecutionFailed(format!(
                    "tool interaction must be resolved before callback completion: {reason}"
                )))
            }
            DriverToolOutcome::Denied { reason } => Err(AgentToolError::ExecutionFailed(reason)),
        }
    }
}
