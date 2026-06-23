use agentdash_spi::hooks::HookControlTarget;
use uuid::Uuid;

/// AgentFrame runtime transition 的主目标。
///
/// `frame_id` 表达要更新的 effective runtime surface；`delivery_runtime_session_id`
/// 用于同步 live connector / runtime registry。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameRuntimeTarget {
    pub frame_id: Uuid,
    pub delivery_runtime_session_id: String,
}

/// Hook runtime 的业务 owner 与 delivery binding。
///
/// `control_target` 表达 hook policy owner；`delivery_runtime_session_id` 用于同步 live connector
/// 和 runtime registry cache。
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
