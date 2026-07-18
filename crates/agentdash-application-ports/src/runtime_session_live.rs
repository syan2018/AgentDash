use agentdash_agent_runtime_contract::{RuntimeThreadId, RuntimeTurnId};
use std::sync::Arc;

use agentdash_agent_protocol::UserInputBlock;
use agentdash_agent_types::DynRuntimeTurnBoundaryDelegate;
use agentdash_platform_spi::CapabilityState;
use agentdash_platform_spi::hooks::{
    AgentFrameHookSnapshot, ExecutionHookProvider, HookControlTarget, SharedHookRuntime,
};
use async_trait::async_trait;
use uuid::Uuid;

pub struct RuntimeSessionMailboxAutoResumeRequest {
    pub runtime_thread_id: RuntimeThreadId,
    pub effect_id: Uuid,
    pub source_turn_id: RuntimeTurnId,
    pub terminal_event_seq: u64,
    pub input: Vec<UserInputBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeSessionLivePortError {
    #[error("runtime session live port failed: {message}")]
    Failed { message: String },
}

impl RuntimeSessionLivePortError {
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed {
            message: message.into(),
        }
    }
}

#[async_trait]
pub trait RuntimeSessionMailboxRuntimePort: Send + Sync {
    fn turn_boundary_delegate(
        &self,
        runtime_thread_id: RuntimeThreadId,
        inner: Option<DynRuntimeTurnBoundaryDelegate>,
    ) -> DynRuntimeTurnBoundaryDelegate;

    async fn accept_hook_auto_resume_effect(
        &self,
        request: RuntimeSessionMailboxAutoResumeRequest,
    ) -> Result<bool, RuntimeSessionLivePortError>;
}

#[async_trait]
pub trait RuntimeSessionEffectiveCapabilityPort: Send + Sync {
    /// Returns the schema-facing visible `CapabilityState` for runtime tool assembly.
    ///
    /// Tool-level admission is evaluated by AgentRun at execution time.
    async fn schema_visible_capability_state_for_runtime_thread(
        &self,
        runtime_thread_id: &RuntimeThreadId,
        base_state: CapabilityState,
    ) -> Result<CapabilityState, RuntimeSessionLivePortError>;
}

pub struct RuntimeSessionHookTargetRuntimeRequest {
    pub runtime_thread_id: RuntimeThreadId,
    pub control_target: HookControlTarget,
    pub frame_revision: i32,
    pub provider: Arc<dyn ExecutionHookProvider>,
    pub snapshot: AgentFrameHookSnapshot,
}

#[async_trait]
pub trait RuntimeSessionHookTargetPort: Send + Sync {
    async fn build_hook_runtime(
        &self,
        request: RuntimeSessionHookTargetRuntimeRequest,
    ) -> Result<Option<SharedHookRuntime>, RuntimeSessionLivePortError>;
}
