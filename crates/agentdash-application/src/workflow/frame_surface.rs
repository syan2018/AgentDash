//! AgentFrame typed surface 读取扩展。
//!
//! AgentFrame 上的 JSON 字段 (effective_capability_json, vfs_surface_json 等)
//! 通过 `AgentFrameSurfaceExt` trait 提供类型安全的反序列化读取，
//! 避免每个消费者各自 parse，替代此前散落在各处的 JSON 反序列化逻辑。

use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::{
    AgentConfig, CapabilityState, RuntimeMcpServerDeclaration, SessionContextBundle, Vfs,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Frame 上记录的 context bundle 摘要。
///
/// 对应 `AgentFrameBuilder::with_context_bundle_summary` 写入的 JSON 结构，
/// 只保留 bundle 元信息，不含完整 `SessionContextBundle` 的 fragment 列表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameContextBundleSummary {
    pub bundle_id: Uuid,
    pub session_id: String,
    pub phase_tag: String,
    pub fragment_count: usize,
}

impl FrameContextBundleSummary {
    pub fn from_bundle(bundle: &SessionContextBundle) -> Self {
        Self {
            bundle_id: bundle.bundle_id,
            session_id: bundle.session_id.to_string(),
            phase_tag: bundle.phase_tag.clone(),
            fragment_count: bundle.bootstrap_fragments.len(),
        }
    }
}

/// Frame construction 产出的可执行 surface 草稿。
///
/// Draft 是写入 `AgentFrame` revision 前的 typed handoff，承载 capability、
/// VFS、MCP、context bundle summary 与 execution profile surface。过渡期内
/// launch 仍读取 `FrameLaunchEnvelope` 上的既有字段，但这些字段应从同一份
/// draft 派生，避免 construction pipeline 继续维护隐式并列 projection。
#[derive(Debug, Clone, Default)]
pub struct FrameSurfaceDraft {
    pub capability_state: Option<CapabilityState>,
    pub vfs: Option<Vfs>,
    pub mcp_servers: Vec<RuntimeMcpServerDeclaration>,
    pub context_bundle_summary: Option<FrameContextBundleSummary>,
    pub execution_profile: Option<AgentConfig>,
}

impl FrameSurfaceDraft {
    pub fn from_frame(frame: &AgentFrame) -> Self {
        Self {
            capability_state: frame.typed_capability_state(),
            vfs: frame.typed_vfs(),
            mcp_servers: frame.typed_mcp_servers(),
            context_bundle_summary: frame.context_bundle_summary(),
            execution_profile: frame.typed_execution_profile(),
        }
    }

    pub fn with_context_bundle_summary(mut self, bundle: &SessionContextBundle) -> Self {
        self.context_bundle_summary = Some(FrameContextBundleSummary::from_bundle(bundle));
        self
    }
}

/// AgentFrame 的 typed surface 读取扩展。
///
/// AgentFrame 上的 JSON 字段 (effective_capability_json, vfs_surface_json 等)
/// 通过此 trait 提供类型安全的反序列化读取，避免每个消费者各自 parse。
pub trait AgentFrameSurfaceExt {
    fn typed_capability_state(&self) -> Option<CapabilityState>;
    fn typed_vfs(&self) -> Option<Vfs>;
    fn typed_mcp_servers(&self) -> Vec<RuntimeMcpServerDeclaration>;
    fn typed_execution_profile(&self) -> Option<AgentConfig>;
    /// 原始 context_slice JSON value，缺失返回 `Value::Null`。
    fn context_slice_value(&self) -> serde_json::Value;
    /// frame 上记录的 context bundle 摘要 (bundle_id, phase_tag, fragment_count)。
    ///
    /// 只有当 `context_slice_json` 包含 `with_context_bundle_summary` 写入的
    /// 结构时才能成功反序列化；其他格式或缺失均返回 `None`。
    fn context_bundle_summary(&self) -> Option<FrameContextBundleSummary>;
}

impl AgentFrameSurfaceExt for AgentFrame {
    fn typed_capability_state(&self) -> Option<CapabilityState> {
        self.effective_capability_json
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    fn typed_vfs(&self) -> Option<Vfs> {
        self.vfs_surface_json
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    fn typed_mcp_servers(&self) -> Vec<RuntimeMcpServerDeclaration> {
        self.mcp_surface_json
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    fn typed_execution_profile(&self) -> Option<AgentConfig> {
        self.execution_profile_json
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    fn context_slice_value(&self) -> serde_json::Value {
        self.context_slice_json
            .clone()
            .unwrap_or(serde_json::Value::Null)
    }

    fn context_bundle_summary(&self) -> Option<FrameContextBundleSummary> {
        self.context_slice_json
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::Mount;
    use agentdash_spi::{McpTransportConfig, SessionContextBundle, ToolCluster};

    fn test_mount(id: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "test".to_string(),
            backend_id: "backend-a".to_string(),
            root_ref: format!("test://{id}"),
            capabilities: Vec::new(),
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    fn test_frame() -> AgentFrame {
        AgentFrame::new_revision(Uuid::new_v4(), 1, "test")
    }

    #[test]
    fn typed_capability_state_deserializes_correctly() {
        let state = CapabilityState::from_clusters([ToolCluster::Read, ToolCluster::Write]);
        let json = serde_json::to_value(&state).unwrap();

        let mut frame = test_frame();
        frame.effective_capability_json = Some(json);

        let result = frame.typed_capability_state().expect("should deserialize");
        assert_eq!(result.tool.enabled_clusters, state.tool.enabled_clusters);
    }

    #[test]
    fn typed_vfs_deserializes_correctly() {
        let vfs = Vfs {
            mounts: vec![test_mount("workspace")],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let json = serde_json::to_value(&vfs).unwrap();

        let mut frame = test_frame();
        frame.vfs_surface_json = Some(json);

        let result = frame.typed_vfs().expect("should deserialize");
        assert_eq!(result.mounts.len(), 1);
        assert_eq!(result.mounts[0].id, "workspace");
        assert_eq!(result.default_mount_id.as_deref(), Some("workspace"));
    }

    #[test]
    fn typed_mcp_servers_returns_empty_vec_when_missing() {
        let frame = test_frame();
        assert!(frame.typed_mcp_servers().is_empty());
    }

    #[test]
    fn typed_mcp_servers_deserializes_correctly() {
        let servers = vec![RuntimeMcpServerDeclaration {
            name: "workflow-tools".to_string(),
            transport: McpTransportConfig::Http {
                url: "http://localhost/mcp".to_string(),
                headers: Vec::new(),
            },
            uses_relay: false,
        }];
        let json = serde_json::to_value(&servers).unwrap();

        let mut frame = test_frame();
        frame.mcp_surface_json = Some(json);

        let result = frame.typed_mcp_servers();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "workflow-tools");
    }

    #[test]
    fn typed_execution_profile_deserializes_correctly() {
        let config = AgentConfig::new("PI_AGENT");
        let json = serde_json::to_value(&config).unwrap();

        let mut frame = test_frame();
        frame.execution_profile_json = Some(json);

        let result = frame.typed_execution_profile().expect("should deserialize");
        assert_eq!(result.executor, "PI_AGENT");
    }

    #[test]
    fn context_bundle_summary_from_builder_format() {
        let bundle = SessionContextBundle::new(Uuid::new_v4(), "lifecycle_node");
        let summary_json = serde_json::to_value(FrameContextBundleSummary::from_bundle(&bundle))
            .expect("summary json");

        let mut frame = test_frame();
        frame.context_slice_json = Some(summary_json);

        let summary = frame
            .context_bundle_summary()
            .expect("should deserialize bundle summary");
        assert_eq!(summary.bundle_id, bundle.bundle_id);
        assert_eq!(summary.session_id, bundle.session_id.to_string());
        assert_eq!(summary.phase_tag, "lifecycle_node");
        assert_eq!(summary.fragment_count, 0);
    }

    #[test]
    fn all_none_fields_return_safe_defaults() {
        let frame = test_frame();

        assert!(frame.typed_capability_state().is_none());
        assert!(frame.typed_vfs().is_none());
        assert!(frame.typed_mcp_servers().is_empty());
        assert!(frame.typed_execution_profile().is_none());
        assert!(frame.context_slice_value().is_null());
        assert!(frame.context_bundle_summary().is_none());
    }

    #[test]
    fn context_slice_value_returns_null_when_missing() {
        let frame = test_frame();
        assert_eq!(frame.context_slice_value(), serde_json::Value::Null);
    }

    #[test]
    fn context_slice_value_clones_existing_json() {
        let mut frame = test_frame();
        let json = serde_json::json!({"arbitrary": "data"});
        frame.context_slice_json = Some(json.clone());
        assert_eq!(frame.context_slice_value(), json);
    }

    #[test]
    fn context_bundle_summary_returns_none_for_non_bundle_json() {
        let mut frame = test_frame();
        frame.context_slice_json = Some(serde_json::json!({"project": "test"}));
        assert!(frame.context_bundle_summary().is_none());
    }

    #[test]
    fn frame_surface_draft_reads_all_typed_frame_surfaces() {
        let mut frame = test_frame();
        let capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        let vfs = Vfs {
            mounts: vec![test_mount("workspace")],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let mcp_servers = vec![RuntimeMcpServerDeclaration {
            name: "workflow-tools".to_string(),
            transport: McpTransportConfig::Http {
                url: "http://localhost/mcp".to_string(),
                headers: Vec::new(),
            },
            uses_relay: false,
        }];
        let execution_profile = AgentConfig::new("PI_AGENT");
        let bundle = SessionContextBundle::new(Uuid::new_v4(), "owner_bootstrap");

        frame.effective_capability_json = Some(serde_json::to_value(&capability_state).unwrap());
        frame.vfs_surface_json = Some(serde_json::to_value(&vfs).unwrap());
        frame.mcp_surface_json = Some(serde_json::to_value(&mcp_servers).unwrap());
        frame.execution_profile_json = Some(serde_json::to_value(&execution_profile).unwrap());
        frame.context_slice_json =
            Some(serde_json::to_value(FrameContextBundleSummary::from_bundle(&bundle)).unwrap());

        let draft = FrameSurfaceDraft::from_frame(&frame);

        assert_eq!(draft.capability_state, Some(capability_state));
        assert_eq!(draft.vfs, Some(vfs));
        assert_eq!(draft.mcp_servers, mcp_servers);
        assert_eq!(
            draft
                .execution_profile
                .as_ref()
                .map(|profile| profile.executor.as_str()),
            Some(execution_profile.executor.as_str())
        );
        assert_eq!(
            draft
                .context_bundle_summary
                .map(|summary| summary.bundle_id),
            Some(bundle.bundle_id)
        );
    }
}
