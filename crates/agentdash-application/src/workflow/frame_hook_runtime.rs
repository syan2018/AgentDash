//! AgentFrameHookRuntime — 以 agent/frame 为主键的 hook runtime facet。
//!
//! ## 设计定位
//!
//! 替代 `HookSessionRuntime` / `SessionHookSnapshot` 的 session-indexed hook API。
//! hook query/resolution 以 `run_id + agent_id + frame_id` 为主语：
//!
//! - 读取 context/capability/VFS/MCP surface：从 `AgentFrame` 读取
//! - advance/resolution：使用 assignment 或 graph instance refs 推进 Activity
//! - session-indexed hook API 只保留 RuntimeTrace adapter 语义
//!
//! ## 当前状态
//!
//! 本模块为 placeholder，定义接口骨架。完整实现在后续 hook migration 任务中推进。

use uuid::Uuid;

/// 以 agent/frame 为主键的 hook runtime facet。
///
/// hook 从此结构读取 effective surface（capability/context/VFS/MCP），
/// 不再从 session 反查 business owner。
#[derive(Debug, Clone)]
pub struct AgentFrameHookRuntime {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
}

/// Hook query scope — 替代 session-indexed `SessionHookSnapshotQuery`。
#[derive(Debug, Clone)]
pub struct FrameHookQuery {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub turn_id: Option<String>,
}

impl AgentFrameHookRuntime {
    pub fn new(run_id: Uuid, agent_id: Uuid, frame_id: Uuid, frame_revision: i32) -> Self {
        Self {
            run_id,
            agent_id,
            frame_id,
            frame_revision,
        }
    }

    /// 从 frame 创建 hook runtime scope。
    pub fn from_frame(
        run_id: Uuid,
        frame: &agentdash_domain::workflow::AgentFrame,
    ) -> Self {
        Self {
            run_id,
            agent_id: frame.agent_id,
            frame_id: frame.id,
            frame_revision: frame.revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_frame_creates_runtime_scope() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame = agentdash_domain::workflow::AgentFrame::new_revision(agent_id, 3, "test");

        let runtime = AgentFrameHookRuntime::from_frame(run_id, &frame);

        assert_eq!(runtime.run_id, run_id);
        assert_eq!(runtime.agent_id, agent_id);
        assert_eq!(runtime.frame_id, frame.id);
        assert_eq!(runtime.frame_revision, 3);
    }
}
