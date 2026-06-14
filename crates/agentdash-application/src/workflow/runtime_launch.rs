//! FrameLaunchEnvelope — Session 启动的唯一类型体系。
//!
//! ```text
//! FrameRuntimeSurface  ← 只来自 AgentFrame 持久化 surface
//! FrameSurfaceDraft    ← construction 产出的 typed surface handoff
//! FrameLaunchIntent    ← 只来自 command/prompt intent
//! FrameLaunchEnvelope  ← Frame construction 输出，字段 non-optional
//! ```
//!
//! `FrameLaunchEnvelope` 是 FrameConstructionService 到 planner 的唯一传递形式，
//! 让"缺字段"在构造边界暴露而不是到 planner 才兜底检查。

use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::hooks::ContextFrame;
use agentdash_spi::{
    AgentConfig, AuthIdentity, CapabilityState, DiscoveredGuideline, RuntimeMcpServer,
    SessionContextBundle, Vfs,
};
use uuid::Uuid;

use crate::session::post_turn_handler::TerminalHookEffectBinding;
use crate::workflow::frame_surface::FrameSurfaceDraft;

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

// ─── FrameLaunchSurface: planner-facing launch surface，字段 non-optional ───

/// Launch planner / preparation 消费的 typed surface。
///
/// `FrameSurfaceDraft` 仍是 frame construction 写入 `AgentFrame` revision 的草稿形态，
/// 因此部分字段保持 optional。进入 `FrameLaunchEnvelope` 时必须通过本结构完成
/// launch-ready gate，让 planner 不需要 fallback 读取。
#[derive(Debug, Clone)]
pub struct FrameLaunchSurface {
    pub capability_state: CapabilityState,
    pub vfs: Vfs,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub execution_profile: AgentConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameLaunchSurfaceError {
    MissingField(&'static str),
}

impl std::fmt::Display for FrameLaunchSurfaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => {
                write!(f, "FrameLaunchSurface 缺少 launch 必需字段 `{field}`")
            }
        }
    }
}

impl std::error::Error for FrameLaunchSurfaceError {}

impl FrameLaunchSurface {
    pub fn from_surface_draft(draft: &FrameSurfaceDraft) -> Result<Self, FrameLaunchSurfaceError> {
        Ok(Self {
            capability_state: draft
                .capability_state
                .clone()
                .ok_or(FrameLaunchSurfaceError::MissingField("capability_state"))?,
            vfs: draft
                .vfs
                .clone()
                .ok_or(FrameLaunchSurfaceError::MissingField("vfs"))?,
            mcp_servers: draft.mcp_servers.clone(),
            execution_profile: draft
                .execution_profile
                .clone()
                .ok_or(FrameLaunchSurfaceError::MissingField("execution_profile"))?,
        })
    }

    pub fn write_back_to_surface_draft(&self, draft: &mut FrameSurfaceDraft) {
        draft.capability_state = Some(self.capability_state.clone());
        draft.vfs = Some(self.vfs.clone());
        draft.mcp_servers = self.mcp_servers.clone();
        draft.execution_profile = Some(self.execution_profile.clone());
    }
}

// ─── FrameLaunchEnvelope: frame construction 输出，字段 non-optional ───

/// Frame construction 到 planner 的传递物。
/// `working_directory`、`executor_config`、`capability_state` 在此保证 non-optional,
/// planner 不需要做"半成品是否 ready"的兜底检查。
#[derive(Debug, Clone)]
pub struct FrameLaunchEnvelope {
    pub surface: FrameRuntimeSurface,
    /// 写入 AgentFrame revision 的 construction draft。
    pub surface_draft: FrameSurfaceDraft,
    /// Launch planner / preparation 的 non-optional typed surface。
    pub launch_surface: FrameLaunchSurface,
    pub pending_frame: Option<AgentFrame>,
    pub intent: FrameLaunchIntent,
    pub working_directory: PathBuf,
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

impl FrameLaunchEnvelope {
    /// Launch-time capability surface。
    pub fn launch_capability_state(&self) -> &CapabilityState {
        &self.launch_surface.capability_state
    }

    /// Launch-time VFS surface。
    pub fn launch_vfs(&self) -> &Vfs {
        &self.launch_surface.vfs
    }

    /// Launch-time MCP surface。
    pub fn launch_mcp_servers(&self) -> &[RuntimeMcpServer] {
        &self.launch_surface.mcp_servers
    }

    /// Launch-time execution profile。
    pub fn launch_executor_config(&self) -> &AgentConfig {
        &self.launch_surface.execution_profile
    }

    pub fn replace_launch_surface(&mut self, launch_surface: FrameLaunchSurface) {
        launch_surface.write_back_to_surface_draft(&mut self.surface_draft);
        self.launch_surface = launch_surface;
    }
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
