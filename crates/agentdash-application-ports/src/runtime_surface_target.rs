use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_platform_spi::hooks::HookControlTarget;
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
