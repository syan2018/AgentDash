use std::sync::Arc;

use crate::frame_launch_envelope::TerminalHookEffectBinding;
pub use agentdash_agent_protocol::RuntimeTerminalDiagnostic;
use agentdash_agent_protocol::{BackboneEnvelope, SourceInfo};
use agentdash_spi::hooks::{HookControlTarget, HookEffect, HookRuntimeAccess, SharedHookRuntime};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait AgentRunPostTurnHandler: Send + Sync + 'static {
    async fn on_event(&self, session_id: &str, envelope: &BackboneEnvelope);

    async fn execute_effects(
        &self,
        session_id: &str,
        turn_id: &str,
        effects: &[HookEffect],
    ) -> Result<(), String>;

    fn supported_effect_kinds(&self) -> &[&str];

    fn durable_effect_handler(&self) -> Option<serde_json::Value> {
        None
    }
}

pub type DynAgentRunPostTurnHandler = Arc<dyn AgentRunPostTurnHandler>;

#[async_trait]
pub trait AgentRunHookEffectHandlerRegistry: Send + Sync + 'static {
    async fn handler_for(
        &self,
        delivery_runtime_session_id: &str,
        payload: &serde_json::Value,
    ) -> Result<Option<DynAgentRunPostTurnHandler>, String>;
}

pub type DynAgentRunHookEffectHandlerRegistry = Arc<dyn AgentRunHookEffectHandlerRegistry>;

#[derive(Debug, Default)]
pub struct EmptyAgentRunHookEffectHandlerRegistry;

#[async_trait]
impl AgentRunHookEffectHandlerRegistry for EmptyAgentRunHookEffectHandlerRegistry {
    async fn handler_for(
        &self,
        _delivery_runtime_session_id: &str,
        payload: &serde_json::Value,
    ) -> Result<Option<DynAgentRunPostTurnHandler>, String> {
        match payload.get("handler") {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(handler) => Err(format!(
                "未注册 durable AgentRun hook effect handler: {handler}"
            )),
        }
    }
}

#[derive(Clone)]
pub struct AgentRunTerminalHookEffects {
    pub control_target: Option<HookControlTarget>,
    pub effects: Vec<HookEffect>,
    pub handler: Option<DynAgentRunPostTurnHandler>,
    pub durable_binding: Option<TerminalHookEffectBinding>,
}

#[derive(Clone)]
pub struct AgentRunTerminalHookContext {
    pub hook_runtime: SharedHookRuntime,
    pub post_turn_handler: Option<DynAgentRunPostTurnHandler>,
    pub source: SourceInfo,
}

#[derive(Clone)]
pub struct AgentRunTerminalControlInput {
    pub delivery_runtime_session_id: String,
    pub turn_id: String,
    pub terminal_event_seq: u64,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub terminal_diagnostic: Option<RuntimeTerminalDiagnostic>,
    pub terminal_hook_context: Option<AgentRunTerminalHookContext>,
}

pub struct AgentRunTerminalHookTriggerInput {
    pub delivery_runtime_session_id: String,
    pub turn_id: String,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub terminal_diagnostic: Option<RuntimeTerminalDiagnostic>,
    pub source: SourceInfo,
}

#[async_trait]
pub trait AgentRunTerminalHookTriggerPort: Send + Sync {
    async fn emit_agent_run_terminal_hook_trigger(
        &self,
        hook_runtime: &dyn HookRuntimeAccess,
        input: AgentRunTerminalHookTriggerInput,
    ) -> Vec<HookEffect>;
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunWaitProducerTerminalEvent {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub terminal_diagnostic: Option<RuntimeTerminalDiagnostic>,
    pub producer_last_message: Option<ProducerLastMessageEvidence>,
    pub source_turn_id: Option<String>,
    pub delivery_trace_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProducerLastMessageEvidence {
    pub summary: String,
    pub message_path: String,
    pub journal_session_id: String,
    pub source_event_seq: u64,
}

#[async_trait]
pub trait AgentRunWaitProducerTerminalConvergencePort: Send + Sync {
    async fn observe_agent_run_wait_producer_terminal(
        &self,
        event: AgentRunWaitProducerTerminalEvent,
    ) -> Result<(), String>;
}

#[async_trait]
pub trait AgentRunLifecycleTerminalConvergencePort: Send + Sync {
    async fn observe_lifecycle_terminal(
        &self,
        delivery_runtime_session_id: &str,
        terminal_state: &str,
    ) -> Result<(), String>;
}

#[async_trait]
pub trait AgentRunControlEffectPort: Send + Sync {
    async fn observe_runtime_terminal(
        &self,
        input: AgentRunTerminalControlInput,
    ) -> Result<(), String>;
}

#[async_trait]
pub trait AgentRunControlEffectReplayPort: Send + Sync {
    async fn replay_control_effect_outbox(&self, limit: u32) -> Result<usize, String>;
}
