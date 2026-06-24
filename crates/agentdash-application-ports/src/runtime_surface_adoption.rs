use agentdash_agent_types::DynAgentTool;
use agentdash_spi::hooks::HookControlTarget;
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameRuntimeTarget {
    pub frame_id: Uuid,
    pub delivery_runtime_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameHookRuntimeTarget {
    pub control_target: HookControlTarget,
    pub delivery_runtime_session_id: String,
}

impl AgentFrameHookRuntimeTarget {
    pub fn new(
        control_target: HookControlTarget,
        delivery_runtime_session_id: impl Into<String>,
    ) -> Self {
        Self {
            control_target,
            delivery_runtime_session_id: delivery_runtime_session_id.into(),
        }
    }

    pub fn frame_id(&self) -> Uuid {
        self.control_target.frame_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeSurfaceAdoptionError {
    #[error(
        "runtime surface adoption target not found: frame_id={frame_id}, session_id={delivery_runtime_session_id}"
    )]
    MissingTarget {
        frame_id: Uuid,
        delivery_runtime_session_id: String,
    },
    #[error("runtime surface adoption failed: {message}")]
    Failed { message: String },
}

#[async_trait]
pub trait AgentRunActiveRuntimeSurfaceAdopter: Send + Sync {
    async fn adopt_persisted_frame_revision_into_active_runtime(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, RuntimeSurfaceAdoptionError>;
}

#[async_trait]
pub trait RuntimeSurfaceAdoptionPort: Send + Sync {
    async fn adopt_runtime_surface(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, RuntimeSurfaceAdoptionError>;
}
