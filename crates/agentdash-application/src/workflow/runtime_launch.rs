//! FrameLaunchEnvelope — Session 启动的唯一类型体系。
//!
//! ```text
//! FrameRuntimeSurface  ← 只来自 AgentFrame 持久化 surface
//! FrameLaunchIntent    ← 只来自 command/prompt intent
//! FrameLaunchEnvelope  ← Frame construction 输出，字段 non-optional
//! ```
//!
//! `FrameLaunchEnvelope` 是 session construction 到 planner 的唯一传递形式，
//! 让"缺字段"在构造边界暴露而不是到 planner 才兜底检查。

use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::hooks::ContextFrame;
use agentdash_spi::{
    AgentConfig, AuthIdentity, CapabilityState, DiscoveredGuideline, SessionContextBundle,
    SessionMcpServer, Vfs,
};
use uuid::Uuid;

use crate::session::post_turn_handler::TerminalHookEffectBinding;

// ─── FrameRuntimeSurface: 只来自 AgentFrame 持久化 surface ───

/// 从 `AgentFrame` 投影的纯 surface 数据，不可被 command/extras 修改。
#[derive(Debug, Clone)]
pub struct FrameRuntimeSurface {
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub capability_surface: serde_json::Value,
    pub context_slice: serde_json::Value,
    pub vfs_surface: serde_json::Value,
    pub mcp_surface: serde_json::Value,
    pub runtime_session_id: Option<String>,
}

impl FrameRuntimeSurface {
    pub fn from_frame(frame: &AgentFrame, runtime_session_id: Option<String>) -> Self {
        Self {
            agent_id: frame.agent_id,
            frame_id: frame.id,
            frame_revision: frame.revision,
            capability_surface: frame
                .effective_capability_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            context_slice: frame
                .context_slice_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            vfs_surface: frame
                .vfs_surface_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            mcp_surface: frame
                .mcp_surface_json
                .clone()
                .unwrap_or(serde_json::Value::Null),
            runtime_session_id,
        }
    }
}

// ─── FrameLaunchIntent: 只来自 command/prompt intent ───

/// 来自 `LaunchCommand` / `AssemblyLaunchExtras` 的请求意图，
/// 不含任何 frame surface 数据。
#[derive(Debug, Clone, Default)]
pub struct FrameLaunchIntent {
    pub input: Option<Vec<agentdash_agent_protocol::UserInputBlock>>,
    pub environment_variables: HashMap<String, String>,
    pub identity: Option<AuthIdentity>,
    pub terminal_hook_effect_binding: Option<TerminalHookEffectBinding>,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
}

// ─── FrameLaunchEnvelope: construction 输出，字段 non-optional ───

/// Frame construction 到 planner 的传递物。
/// `working_directory`、`executor_config`、`capability_state` 在此保证 non-optional,
/// planner 不需要做"半成品是否 ready"的兜底检查。
#[derive(Debug, Clone)]
pub struct FrameLaunchEnvelope {
    pub surface: FrameRuntimeSurface,
    pub pending_frame: Option<AgentFrame>,
    pub intent: FrameLaunchIntent,
    pub working_directory: PathBuf,
    pub executor_config: AgentConfig,
    pub capability_state: CapabilityState,
    pub vfs: Vfs,
    pub mcp_servers: Vec<SessionMcpServer>,
    pub context_bundle: Option<SessionContextBundle>,
    pub continuation_context_frame: Option<ContextFrame>,
    pub base_capability_state: Option<CapabilityState>,
    pub resolution_trace: LaunchResolutionTrace,
}

/// Launch 过程中 resolution 来源的 trace 数据（仅用于诊断/可观测性）。
#[derive(Debug, Clone, Default)]
pub struct LaunchResolutionTrace {
    pub vfs_source: Option<String>,
    pub mcp_source: Option<String>,
    pub capability_source: Option<String>,
    pub pending_overlay_applied: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::AgentFrame;

    #[test]
    fn frame_runtime_surface_from_frame_projects_all_fields() {
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let mut frame = AgentFrame::new_revision(agent_id, 3, "test");
        frame.effective_capability_json = Some(serde_json::json!({"file_read": true}));
        frame.context_slice_json = Some(serde_json::json!({"project": "demo"}));
        frame.vfs_surface_json = Some(serde_json::json!({"mounts": []}));
        frame.mcp_surface_json = Some(serde_json::json!({"servers": []}));

        let surface = FrameRuntimeSurface::from_frame(&frame, Some(session_id.to_string()));

        assert_eq!(surface.agent_id, agent_id);
        assert_eq!(surface.frame_id, frame.id);
        assert_eq!(surface.frame_revision, 3);
        assert_eq!(surface.runtime_session_id, Some(session_id.to_string()));
        assert_eq!(
            surface.capability_surface,
            serde_json::json!({"file_read": true})
        );
        assert_eq!(
            surface.context_slice,
            serde_json::json!({"project": "demo"})
        );
    }

    #[test]
    fn frame_runtime_surface_from_frame_handles_empty_fields() {
        let agent_id = Uuid::new_v4();
        let frame = AgentFrame::new_initial(agent_id);

        let surface = FrameRuntimeSurface::from_frame(&frame, None);

        assert_eq!(surface.agent_id, agent_id);
        assert_eq!(surface.frame_revision, 1);
        assert!(surface.runtime_session_id.is_none());
        assert!(surface.capability_surface.is_null());
        assert!(surface.context_slice.is_null());
        assert!(surface.vfs_surface.is_null());
        assert!(surface.mcp_surface.is_null());
    }

    #[test]
    fn frame_runtime_surface_uses_explicit_runtime_session_policy() {
        let agent_id = Uuid::new_v4();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let frame = AgentFrame::new_revision(agent_id, 2, "test");

        let primary = FrameRuntimeSurface::from_frame(&frame, Some(s1.to_string()));
        let latest = FrameRuntimeSurface::from_frame(&frame, Some(s2.to_string()));
        assert_eq!(primary.runtime_session_id, Some(s1.to_string()));
        assert_eq!(latest.runtime_session_id, Some(s2.to_string()));
    }
}
