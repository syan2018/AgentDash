use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_spi::hooks::HookControlTarget;
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameRuntimeTarget {
    pub frame_id: Uuid,
    pub runtime_thread_id: RuntimeThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameHookRuntimeTarget {
    pub control_target: HookControlTarget,
    pub runtime_thread_id: RuntimeThreadId,
}

impl AgentFrameHookRuntimeTarget {
    pub fn new(control_target: HookControlTarget, runtime_thread_id: RuntimeThreadId) -> Self {
        Self {
            control_target,
            runtime_thread_id,
        }
    }

    pub fn frame_id(&self) -> Uuid {
        self.control_target.frame_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeSurfaceAdoptionError {
    #[error(
        "runtime surface adoption target not found: frame_id={frame_id}, runtime_thread_id={runtime_thread_id}"
    )]
    MissingTarget {
        frame_id: Uuid,
        runtime_thread_id: RuntimeThreadId,
    },
    #[error("runtime surface adoption failed: {message}")]
    Failed { message: String },
}

#[async_trait]
pub trait RuntimeSurfaceAdoptionPort: Send + Sync {
    async fn adopt_runtime_surface(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<(), RuntimeSurfaceAdoptionError>;
}
