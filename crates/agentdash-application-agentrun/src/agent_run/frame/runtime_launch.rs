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

use agentdash_application_ports::frame_launch_envelope as launch_port;
use agentdash_domain::backend::{
    RuntimeBackendAnchor, RuntimeBackendAnchorError, RuntimeBackendAnchorSource,
};
use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::hooks::ContextFrame;
use agentdash_spi::{
    AgentConfig, AuthIdentity, CapabilityState, DiscoveredGuideline, MemoryDiscoveryOutput,
    RuntimeMcpServer, SessionContextBundle, Vfs,
};
use uuid::Uuid;

use crate::agent_run::frame::surface::FrameSurfaceDraft;
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

/// 来自 `LaunchCommand` / `FrameAssemblyLaunchExtras` 的请求意图，
/// 不含任何 frame surface 数据。
#[derive(Debug, Clone, Default)]
pub struct FrameLaunchIntent {
    pub input: Option<Vec<agentdash_agent_protocol::UserInputBlock>>,
    pub environment_variables: HashMap<String, String>,
    pub identity: Option<AuthIdentity>,
    pub terminal_hook_effect_binding: Option<TerminalHookEffectBinding>,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub discovered_memory: MemoryDiscoveryOutput,
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
    SurfaceMismatch {
        field: &'static str,
        expected_source: &'static str,
    },
}

impl std::fmt::Display for FrameLaunchSurfaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => {
                write!(f, "FrameLaunchSurface 缺少 launch 必需字段 `{field}`")
            }
            Self::SurfaceMismatch {
                field,
                expected_source,
            } => {
                write!(
                    f,
                    "FrameLaunchSurface 字段 `{field}` 与 `{expected_source}` 不一致"
                )
            }
        }
    }
}

impl std::error::Error for FrameLaunchSurfaceError {}

impl FrameLaunchSurface {
    pub fn new(
        capability_state: CapabilityState,
        vfs: Vfs,
        mcp_servers: Vec<RuntimeMcpServer>,
        execution_profile: AgentConfig,
    ) -> Result<Self, FrameLaunchSurfaceError> {
        if capability_state.vfs.active.as_ref() != Some(&vfs) {
            return Err(FrameLaunchSurfaceError::SurfaceMismatch {
                field: "capability_state.vfs.active",
                expected_source: "FrameSurfaceDraft.vfs",
            });
        }
        if capability_state.tool.mcp_servers != mcp_servers {
            return Err(FrameLaunchSurfaceError::SurfaceMismatch {
                field: "capability_state.tool.mcp_servers",
                expected_source: "FrameSurfaceDraft.mcp_servers",
            });
        }

        Ok(Self {
            capability_state,
            vfs,
            mcp_servers,
            execution_profile,
        })
    }

    pub fn from_surface_draft(draft: &FrameSurfaceDraft) -> Result<Self, FrameLaunchSurfaceError> {
        let capability_state = draft
            .capability_state
            .clone()
            .ok_or(FrameLaunchSurfaceError::MissingField("capability_state"))?;
        let vfs = draft
            .vfs
            .clone()
            .ok_or(FrameLaunchSurfaceError::MissingField("vfs"))?;
        let execution_profile = draft
            .execution_profile
            .clone()
            .ok_or(FrameLaunchSurfaceError::MissingField("execution_profile"))?;

        Self::new(
            capability_state,
            vfs,
            draft.mcp_servers.clone(),
            execution_profile,
        )
    }

    pub fn write_back_to_surface_draft(&self, draft: &mut FrameSurfaceDraft) {
        draft.capability_state = Some(self.capability_state.clone());
        draft.vfs = Some(self.vfs.clone());
        draft.mcp_servers = self.mcp_servers.clone();
        draft.execution_profile = Some(self.execution_profile.clone());
    }

    pub fn runtime_backend_anchor(
        &self,
        source_detail: Option<String>,
    ) -> Result<Option<RuntimeBackendAnchor>, RuntimeBackendAnchorError> {
        runtime_backend_anchor_from_vfs(&self.vfs, source_detail)
    }
}

pub fn runtime_backend_anchor_from_vfs(
    vfs: &Vfs,
    source_detail: Option<String>,
) -> Result<Option<RuntimeBackendAnchor>, RuntimeBackendAnchorError> {
    let Some(mount) = vfs.default_mount() else {
        return Ok(None);
    };
    let backend_id = mount.backend_id.trim();
    if backend_id.is_empty() {
        return Ok(None);
    }

    let workspace_id = uuid_metadata(&mount.metadata, "workspace_id");
    let workspace_binding_id = uuid_metadata(&mount.metadata, "workspace_binding_id");
    let source = if workspace_binding_id.is_some() || workspace_id.is_some() {
        RuntimeBackendAnchorSource::WorkspaceBinding
    } else {
        RuntimeBackendAnchorSource::System
    };

    Ok(Some(
        RuntimeBackendAnchor::new(backend_id, source)?
            .with_workspace_id(workspace_id)
            .with_workspace_binding_id(workspace_binding_id)
            .with_root_ref(Some(mount.root_ref.clone()))
            .with_source_detail(source_detail),
    ))
}

fn uuid_metadata(metadata: &serde_json::Value, key: &str) -> Option<Uuid> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
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
    pub runtime_backend_anchor: Option<RuntimeBackendAnchor>,
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

    /// Convert the AgentRun-owned construction envelope into the neutral
    /// RuntimeSession launch DTO consumed through application ports.
    pub fn into_runtime_session_launch_envelope(self) -> launch_port::FrameLaunchEnvelope {
        launch_port::FrameLaunchEnvelope {
            surface: launch_port::FrameRuntimeSurface {
                agent_id: self.surface.agent_id,
                frame_id: self.surface.frame_id,
                frame_revision: self.surface.frame_revision,
                capability_surface: self.surface.capability_surface,
                context_slice: self.surface.context_slice,
                vfs_surface: self.surface.vfs_surface,
                mcp_surface: self.surface.mcp_surface,
                runtime_session_id: self.surface.runtime_session_id,
            },
            launch_surface: launch_port::FrameLaunchSurface {
                capability_state: self.launch_surface.capability_state,
                vfs: self.launch_surface.vfs,
                mcp_servers: self.launch_surface.mcp_servers,
                execution_profile: self.launch_surface.execution_profile,
            },
            pending_frame: self.pending_frame,
            intent: launch_port::FrameLaunchIntent {
                input: self.intent.input,
                environment_variables: self.intent.environment_variables,
                identity: self.intent.identity,
                terminal_hook_effect_binding: self.intent.terminal_hook_effect_binding.map(
                    |binding| launch_port::TerminalHookEffectBinding {
                        handler: binding.handler,
                        supported_effect_kinds: binding.supported_effect_kinds,
                    },
                ),
                discovered_guidelines: self.intent.discovered_guidelines,
                discovered_memory: self.intent.discovered_memory,
            },
            working_directory: self.working_directory,
            context_bundle: self.context_bundle,
            continuation_context_frame: self.continuation_context_frame,
            base_capability_state: self.base_capability_state,
            runtime_backend_anchor: self.runtime_backend_anchor,
            resolution_trace: launch_port::LaunchResolutionTrace {
                vfs_source: self.resolution_trace.vfs_source,
                mcp_source: self.resolution_trace.mcp_source,
                capability_source: self.resolution_trace.capability_source,
                pending_overlay_applied: self.resolution_trace.pending_overlay_applied,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::workflow::AgentFrame;
    use agentdash_spi::{McpTransportConfig, ToolCluster};

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

    fn test_vfs(root: &str) -> Vfs {
        Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "test".to_string(),
                backend_id: "backend".to_string(),
                root_ref: root.to_string(),
                capabilities: vec![MountCapability::Read],
                default_write: false,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    fn workspace_vfs(root: &str, workspace_id: Uuid, binding_id: Uuid) -> Vfs {
        Vfs {
            mounts: vec![Mount {
                id: "workspace".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend".to_string(),
                root_ref: root.to_string(),
                capabilities: vec![MountCapability::Read],
                default_write: false,
                display_name: "Workspace".to_string(),
                metadata: serde_json::json!({
                    "workspace_id": workspace_id,
                    "workspace_binding_id": binding_id,
                }),
            }],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    fn test_mcp_server(name: &str) -> RuntimeMcpServer {
        RuntimeMcpServer {
            name: name.to_string(),
            transport: McpTransportConfig::Http {
                url: format!("http://localhost/{name}"),
                headers: Vec::new(),
            },
            uses_relay: false,
        }
    }

    #[test]
    fn frame_launch_surface_requires_vfs_field() {
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(test_vfs("/workspace"));
        let draft = FrameSurfaceDraft {
            capability_state: Some(capability_state),
            execution_profile: Some(AgentConfig::new("PI_AGENT")),
            ..Default::default()
        };

        let error = FrameLaunchSurface::from_surface_draft(&draft)
            .expect_err("missing vfs should reject launch surface");

        assert_eq!(error, FrameLaunchSurfaceError::MissingField("vfs"));
    }

    #[test]
    fn frame_launch_surface_rejects_capability_vfs_mismatch() {
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(test_vfs("/other"));
        let draft = FrameSurfaceDraft {
            capability_state: Some(capability_state),
            vfs: Some(test_vfs("/workspace")),
            execution_profile: Some(AgentConfig::new("PI_AGENT")),
            ..Default::default()
        };

        let error = FrameLaunchSurface::from_surface_draft(&draft)
            .expect_err("capability/vfs mismatch should reject launch surface");

        assert_eq!(
            error,
            FrameLaunchSurfaceError::SurfaceMismatch {
                field: "capability_state.vfs.active",
                expected_source: "FrameSurfaceDraft.vfs",
            }
        );
    }

    #[test]
    fn frame_launch_surface_rejects_capability_mcp_mismatch() {
        let vfs = test_vfs("/workspace");
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(vfs.clone());
        capability_state.tool.mcp_servers = vec![test_mcp_server("capability")];
        let draft = FrameSurfaceDraft {
            capability_state: Some(capability_state),
            vfs: Some(vfs),
            mcp_servers: vec![test_mcp_server("draft")],
            execution_profile: Some(AgentConfig::new("PI_AGENT")),
            ..Default::default()
        };

        let error = FrameLaunchSurface::from_surface_draft(&draft)
            .expect_err("capability/mcp mismatch should reject launch surface");

        assert_eq!(
            error,
            FrameLaunchSurfaceError::SurfaceMismatch {
                field: "capability_state.tool.mcp_servers",
                expected_source: "FrameSurfaceDraft.mcp_servers",
            }
        );
    }

    #[test]
    fn frame_launch_surface_builds_runtime_backend_anchor_from_workspace_binding() {
        let workspace_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        let vfs = workspace_vfs("/workspace", workspace_id, binding_id);
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(vfs.clone());
        let surface = FrameLaunchSurface::new(
            capability_state,
            vfs,
            Vec::new(),
            AgentConfig::new("PI_AGENT"),
        )
        .expect("surface");

        let anchor = surface
            .runtime_backend_anchor(Some("construction.test".to_string()))
            .expect("anchor result")
            .expect("anchor");

        assert_eq!(anchor.backend_id(), "backend");
        assert_eq!(anchor.workspace_id, Some(workspace_id));
        assert_eq!(anchor.workspace_binding_id, Some(binding_id));
        assert_eq!(anchor.root_ref.as_deref(), Some("/workspace"));
        assert_eq!(anchor.source, RuntimeBackendAnchorSource::WorkspaceBinding);
        assert_eq!(anchor.source_detail.as_deref(), Some("construction.test"));
    }

    #[test]
    fn frame_launch_surface_returns_no_anchor_without_backend_id() {
        let vfs = test_vfs("/workspace");
        let mut vfs_without_backend = vfs.clone();
        vfs_without_backend.mounts[0].backend_id = " ".to_string();
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(vfs_without_backend.clone());
        let surface = FrameLaunchSurface::new(
            capability_state,
            vfs_without_backend,
            Vec::new(),
            AgentConfig::new("PI_AGENT"),
        )
        .expect("surface");

        assert!(
            surface
                .runtime_backend_anchor(Some("construction.test".to_string()))
                .expect("anchor result")
                .is_none()
        );
    }
}
