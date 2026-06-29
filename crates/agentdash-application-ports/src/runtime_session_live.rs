use std::sync::Arc;

use agentdash_agent_protocol::UserInputBlock;
use agentdash_spi::hooks::{
    AgentFrameHookSnapshot, ExecutionHookProvider, HookControlTarget, SharedHookRuntime,
};
use agentdash_spi::{CapabilityState, DynAgentRuntimeDelegate};
use async_trait::async_trait;
use uuid::Uuid;

pub struct RuntimeSessionMailboxAutoResumeRequest {
    pub session_id: String,
    pub effect_id: Uuid,
    pub source_turn_id: String,
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
    fn runtime_delegate(
        &self,
        runtime_session_id: String,
        inner: Option<DynAgentRuntimeDelegate>,
    ) -> DynAgentRuntimeDelegate;

    async fn accept_hook_auto_resume_effect(
        &self,
        request: RuntimeSessionMailboxAutoResumeRequest,
    ) -> Result<bool, RuntimeSessionLivePortError>;
}

#[async_trait]
pub trait RuntimeSessionEffectiveCapabilityPort: Send + Sync {
    /// Returns the schema-facing visible `CapabilityState` for runtime tool assembly.
    ///
    /// This is not a Grant admission boundary. Tool-level PermissionGrant facts are
    /// evaluated by AgentRun admission at tool execution time.
    async fn schema_visible_capability_state_for_runtime_session(
        &self,
        runtime_session_id: &str,
        base_state: CapabilityState,
    ) -> Result<CapabilityState, RuntimeSessionLivePortError>;
}

pub struct RuntimeSessionHookTargetRuntimeRequest {
    pub delivery_runtime_session_id: String,
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
