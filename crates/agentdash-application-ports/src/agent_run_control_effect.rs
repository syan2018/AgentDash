use std::sync::Arc;

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_agent_protocol::RuntimeTerminalDiagnostic;
use agentdash_agent_runtime_contract::{
    EventSequence, PresentationThreadId, PresentationTurnId, RuntimeBindingId,
    RuntimeDriverGeneration, RuntimeTerminalHookEffectBinding, RuntimeTerminalHookEffectHandlerRef,
    RuntimeThreadId, RuntimeTurnId, RuntimeTurnTerminal, SurfaceDigest, SurfaceRevision,
};
use agentdash_platform_spi::hooks::HookEffect;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait AgentRunPostTurnHandler: Send + Sync + 'static {
    async fn on_event(&self, session_id: &str, envelope: &BackboneEnvelope);

    async fn execute_effects(
        &self,
        effect_id: &str,
        session_id: &str,
        turn_id: &str,
        effects: &[HookEffect],
    ) -> Result<(), String>;

    fn supported_effect_kinds(&self) -> &[&str];

    /// Stable typed identity persisted with the canonical Runtime surface binding.
    fn durable_effect_handler(&self) -> Option<RuntimeTerminalHookEffectHandlerRef> {
        None
    }
}

pub type DynAgentRunPostTurnHandler = Arc<dyn AgentRunPostTurnHandler>;

#[async_trait]
pub trait AgentRunHookEffectHandlerRegistry: Send + Sync + 'static {
    async fn handler_for(
        &self,
        presentation_thread_id: &PresentationThreadId,
        handler: &RuntimeTerminalHookEffectHandlerRef,
    ) -> Result<Option<DynAgentRunPostTurnHandler>, String>;
}

pub type DynAgentRunHookEffectHandlerRegistry = Arc<dyn AgentRunHookEffectHandlerRegistry>;

#[derive(Debug, Default)]
pub struct EmptyAgentRunHookEffectHandlerRegistry;

#[async_trait]
impl AgentRunHookEffectHandlerRegistry for EmptyAgentRunHookEffectHandlerRegistry {
    async fn handler_for(
        &self,
        _presentation_thread_id: &PresentationThreadId,
        handler: &RuntimeTerminalHookEffectHandlerRef,
    ) -> Result<Option<DynAgentRunPostTurnHandler>, String> {
        Err(format!(
            "未注册 durable AgentRun hook effect handler: {}:{}@{}",
            handler.handler_type, handler.handler_id, handler.revision.0
        ))
    }
}

/// Managed Runtime 已经原子提交的 terminal evidence。
///
/// 该类型只描述 application side effects 的稳定输入；lease、attempt 与 ack 状态属于
/// Runtime durable outbox，不混入业务 evidence。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AgentRunTerminalControlInput {
    pub effect_id: String,
    pub runtime_thread_id: RuntimeThreadId,
    pub presentation_thread_id: PresentationThreadId,
    pub runtime_turn_id: RuntimeTurnId,
    pub presentation_turn_id: PresentationTurnId,
    pub terminal_event_sequence: EventSequence,
    pub terminal: RuntimeTurnTerminal,
    pub message: Option<String>,
    pub diagnostic: Option<RuntimeTerminalDiagnostic>,
    pub started_at_ms: Option<u64>,
    pub completed_at_ms: u64,
    pub binding_id: RuntimeBindingId,
    pub driver_generation: RuntimeDriverGeneration,
    pub surface_revision: SurfaceRevision,
    pub surface_digest: SurfaceDigest,
    pub source_thread_id: String,
    pub source_turn_id: Option<String>,
    pub terminal_hook_effect_binding: Option<RuntimeTerminalHookEffectBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunControlEffectKind {
    DeliveryConvergence,
    WaitProducerTerminalConvergence,
    LifecycleTerminalConvergence,
    TerminalHookEffects,
}

impl AgentRunControlEffectKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeliveryConvergence => "agent_run_delivery_convergence",
            Self::WaitProducerTerminalConvergence => "wait_producer_terminal_convergence",
            Self::LifecycleTerminalConvergence => "lifecycle_terminal_convergence",
            Self::TerminalHookEffects => "terminal_hook_effects",
        }
    }
}

impl TryFrom<&str> for AgentRunControlEffectKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "agent_run_delivery_convergence" => Ok(Self::DeliveryConvergence),
            "wait_producer_terminal_convergence" => Ok(Self::WaitProducerTerminalConvergence),
            "lifecycle_terminal_convergence" => Ok(Self::LifecycleTerminalConvergence),
            "terminal_hook_effects" => Ok(Self::TerminalHookEffects),
            other => Err(format!("unknown AgentRun control effect kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunControlEffectStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone)]
pub struct AgentRunControlEffectRecord {
    pub id: Uuid,
    pub dedup_key: String,
    pub presentation_thread_id: PresentationThreadId,
    pub presentation_turn_id: PresentationTurnId,
    pub terminal_event_sequence: EventSequence,
    pub effect_kind: AgentRunControlEffectKind,
    pub payload: serde_json::Value,
    pub status: AgentRunControlEffectStatus,
    pub claim_token: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct NewAgentRunControlEffectRecord {
    pub dedup_key: String,
    pub presentation_thread_id: PresentationThreadId,
    pub presentation_turn_id: PresentationTurnId,
    pub terminal_event_sequence: EventSequence,
    pub effect_kind: AgentRunControlEffectKind,
    pub payload: serde_json::Value,
}

#[async_trait]
pub trait AgentRunControlEffectStore: Send + Sync {
    async fn insert_or_get(
        &self,
        effect: NewAgentRunControlEffectRecord,
    ) -> Result<AgentRunControlEffectRecord, String>;

    async fn claim(
        &self,
        dedup_key: &str,
        owner: &str,
        lease_duration_ms: i64,
    ) -> Result<Option<AgentRunControlEffectRecord>, String>;

    async fn mark_succeeded(&self, effect_id: Uuid, claim_token: Uuid) -> Result<(), String>;

    async fn mark_failed(
        &self,
        effect_id: Uuid,
        claim_token: Uuid,
        error: String,
    ) -> Result<(), String>;
}

#[async_trait]
pub trait AgentRunDeliveryTerminalConvergencePort: Send + Sync {
    async fn converge_delivery_terminal(
        &self,
        input: &AgentRunTerminalControlInput,
    ) -> Result<(), String>;
}

#[async_trait]
pub trait AgentRunWaitProducerTerminalConvergencePort: Send + Sync {
    async fn converge_wait_producer_terminal(
        &self,
        input: &AgentRunTerminalControlInput,
    ) -> Result<(), String>;
}

#[async_trait]
pub trait AgentRunLifecycleTerminalConvergencePort: Send + Sync {
    async fn observe_lifecycle_terminal(
        &self,
        presentation_thread_id: &PresentationThreadId,
        terminal: RuntimeTurnTerminal,
    ) -> Result<(), String>;
}

/// Executes canonical SessionTerminal hooks for the exact surface revision bound to the effect.
#[async_trait]
pub trait AgentRunTerminalHookEffectPort: Send + Sync {
    async fn execute_terminal_hooks(
        &self,
        input: &AgentRunTerminalControlInput,
    ) -> Result<(), String>;
}

#[async_trait]
pub trait AgentRunControlEffectPort: Send + Sync {
    async fn observe_runtime_terminal(
        &self,
        input: AgentRunTerminalControlInput,
    ) -> Result<(), String>;
}
