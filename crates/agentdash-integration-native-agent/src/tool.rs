use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    DriverEventEnvelope, DriverEventSink, DriverItemId, DriverThreadId, DriverTurnId,
    RuntimeBindingId, RuntimeDriverGeneration, RuntimeEvent, RuntimeItemContent, RuntimeItemId,
    RuntimeItemTerminal, RuntimeThreadId, RuntimeTurnId, ToolSetRevision,
};
use agentdash_agent_types::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
};
use agentdash_integration_api::{
    AgentRuntimeToolCallback, AuthIdentity, DriverToolDefinition, DriverToolInvocation,
    DriverToolOutcome,
};
use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::context::{NativeBindingContext, NativeToolCallContext};

pub(crate) struct NativeRuntimeTool {
    definition: DriverToolDefinition,
    binding_id: RuntimeBindingId,
    generation: RuntimeDriverGeneration,
    source_thread_id: DriverThreadId,
    runtime_thread_id: RuntimeThreadId,
    active_turn: Arc<RwLock<Option<DriverTurnId>>>,
    active_runtime_turn: Arc<RwLock<Option<RuntimeTurnId>>>,
    tool_set_revision: ToolSetRevision,
    callback: Arc<dyn AgentRuntimeToolCallback>,
    authorization_identity: Option<AuthIdentity>,
    events: Arc<RwLock<Option<NativeToolEventContext>>>,
}

#[derive(Clone)]
pub(crate) struct NativeToolEventContext {
    pub sink: Arc<dyn DriverEventSink>,
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
}

impl NativeRuntimeTool {
    pub(crate) fn new(
        definition: DriverToolDefinition,
        binding: NativeBindingContext,
        call: NativeToolCallContext,
        callback: Arc<dyn AgentRuntimeToolCallback>,
    ) -> Self {
        Self {
            definition,
            binding_id: binding.binding_id,
            generation: binding.generation,
            source_thread_id: binding.source_thread_id,
            runtime_thread_id: binding.runtime_thread_id,
            active_turn: call.active_turn,
            active_runtime_turn: call.active_runtime_turn,
            tool_set_revision: call.tool_set_revision,
            callback,
            authorization_identity: binding.authorization_identity,
            events: call.events,
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
        let turn_id = self
            .active_runtime_turn
            .read()
            .await
            .clone()
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "native tool invoked without a canonical Runtime turn".into(),
                )
            })?;
        let item_id = RuntimeItemId::new(source_item_id.to_string()).map_err(|error| {
            AgentToolError::ExecutionFailed(format!("invalid canonical item identity: {error}"))
        })?;
        let event_context = self.events.read().await.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "native tool invoked without an authoritative driver event sink".into(),
            )
        })?;
        event_context
            .sink
            .emit(DriverEventEnvelope {
                binding_id: event_context.binding_id.clone(),
                generation: event_context.generation,
                source_thread_id: event_context.source_thread_id.clone(),
                source_turn_id: Some(source_turn_id.clone()),
                source_item_id: Some(source_item_id.clone()),
                event: RuntimeEvent::ItemStarted {
                    turn_id: turn_id.clone(),
                    item_id: item_id.clone(),
                    initial_content: RuntimeItemContent::temporary_dynamic_tool_call(
                        item_id.as_str(),
                        self.definition.name.clone(),
                        args.clone(),
                    ),
                },
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        let callback = self.callback.invoke(DriverToolInvocation {
            thread_id: self.runtime_thread_id.clone(),
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
            binding_id: self.binding_id.clone(),
            generation: self.generation,
            source_thread_id: self.source_thread_id.clone(),
            source_turn_id: source_turn_id.clone(),
            source_item_id: source_item_id.clone(),
            tool_set_revision: self.tool_set_revision,
            tool_name: self.definition.name.clone(),
            arguments: args,
            timeout_ms: 120_000,
            authorization_identity: self.authorization_identity.clone(),
        });
        let outcome = tokio::select! {
            _ = cancel.cancelled() => Err("tool call cancelled".to_string()),
            outcome = callback => outcome.map_err(|error| error.to_string()),
        };
        if let Err(message) = &outcome {
            // A successful callback owns canonical terminal convergence through ToolBroker.
            // Only callback transport/cancellation failures remain Driver-owned fallbacks.
            let terminal = if cancel.is_cancelled() {
                RuntimeItemTerminal::Cancelled {
                    message: Some(message.clone()),
                }
            } else {
                RuntimeItemTerminal::Failed {
                    message: Some(message.clone()),
                }
            };
            event_context
                .sink
                .emit(DriverEventEnvelope {
                    binding_id: event_context.binding_id,
                    generation: event_context.generation,
                    source_thread_id: event_context.source_thread_id,
                    source_turn_id: Some(source_turn_id),
                    source_item_id: Some(source_item_id),
                    event: RuntimeEvent::ItemTerminal {
                        turn_id,
                        item_id,
                        terminal,
                    },
                })
                .await
                .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        }
        let outcome = outcome.map_err(AgentToolError::ExecutionFailed)?;
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
