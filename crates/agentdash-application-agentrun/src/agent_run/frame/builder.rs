//! AgentFrameBuilder — AgentRun frame/surface 边界内部的 revision writer
//! primitive。
//!
//! ## 设计定位
//!
//! - **唯一事实源**：capability / context / VFS / MCP surface 只从 builder 写入
//!   frame revision，runtime launch 从已持久化 frame 投影。
//! - **内部 primitive**：业务模块不应直接持有 builder 来拼完整
//!   `CapabilityState` 或 adopt runtime surface；外部变化先进入
//!   `AgentRunFrameSurfaceService` 的 typed command/update boundary。
//! - **不可变快照**：`build()` 产出新 revision，既有 revision 保持不变，
//!   revision 序列天然提供 provenance。
//! - **面向 dispatch**：Lifecycle dispatch facade 在创建 agent 后通过
//!   AgentRun frame materialization port 产出带 surface 的 initial frame。

use agentdash_application_ports::lifecycle_surface_projection::ActivityActivation;
use agentdash_domain::DomainError;
use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository};
use agentdash_platform_spi::{
    AgentConfig, CapabilityState, RuntimeMcpServer, SessionContextBundle, Vfs,
};
use uuid::Uuid;

use crate::agent_run::runtime_capability::{
    capability_state_to_frame_surfaces, compose_vfs_with_overlay_and_directives,
};

use super::surface::{AgentContextSourceSnapshot, FrameContextBundleSummary, FrameSurfaceDraft};

pub struct AgentFrameActivationSurfaceInput<'a> {
    pub activation: &'a ActivityActivation,
    pub base_vfs: Option<&'a Vfs>,
    /// 热更新路径需要从已有 CapabilityState 继承 skill 层（当 activation 自身未产出
    /// skill 时）。冷启动路径传 None。
    pub inherit_skills_from: Option<&'a CapabilityState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameActivationSurface {
    pub capability_state: CapabilityState,
    pub vfs: Vfs,
    pub mcp_servers: Vec<RuntimeMcpServer>,
}

impl AgentFrameActivationSurface {
    pub fn to_surface_draft(&self) -> FrameSurfaceDraft {
        FrameSurfaceDraft {
            capability_state: Some(self.capability_state.clone()),
            vfs: Some(self.vfs.clone()),
            mcp_servers: self.mcp_servers.clone(),
            context_bundle_summary: None,
            context_source_snapshot: None,
            execution_profile: None,
        }
    }
}

pub fn build_lifecycle_activation_surface(
    input: AgentFrameActivationSurfaceInput<'_>,
) -> AgentFrameActivationSurface {
    let vfs = compose_vfs_with_overlay_and_directives(
        input.base_vfs,
        &input.activation.lifecycle_vfs,
        &input.activation.mount_directives,
    );
    let mut capability_state = input.activation.capability_state.clone();
    capability_state.tool.mcp_servers = input.activation.mcp_servers.clone();
    capability_state.vfs.active = Some(vfs.clone());
    if capability_state.skill.skills.is_empty()
        && let Some(base) = input.inherit_skills_from
    {
        capability_state.skill = base.skill.clone();
    }

    AgentFrameActivationSurface {
        capability_state,
        vfs,
        mcp_servers: input.activation.mcp_servers.clone(),
    }
}

/// AgentFrame 的 builder，收束所有 runtime surface 输入为单次 revision。
///
/// 单个 builder 对应一个预分配 identity 的新 revision；`build()` /
/// `build_uncommitted()` 会消费 builder。调用方在 capability / context / VFS /
/// MCP 任一维度变更时构造新的 builder。
pub struct AgentFrameBuilder {
    frame_id: Uuid,
    agent_id: Uuid,
    context_slice: Option<serde_json::Value>,
    context_source_snapshot: Option<serde_json::Value>,
    capability_surface: Option<serde_json::Value>,
    vfs_surface: Option<serde_json::Value>,
    mcp_surface: Option<serde_json::Value>,
    execution_profile: Option<serde_json::Value>,
    hook_plan: Option<serde_json::Value>,
    created_by_kind: String,
    created_by_id: Option<String>,
}

impl AgentFrameBuilder {
    pub fn new(agent_id: Uuid) -> Self {
        Self {
            frame_id: Uuid::new_v4(),
            agent_id,
            context_slice: None,
            context_source_snapshot: None,
            capability_surface: None,
            vfs_surface: None,
            mcp_surface: None,
            execution_profile: None,
            hook_plan: None,
            created_by_kind: "frame_builder".to_string(),
            created_by_id: None,
        }
    }

    /// 构造 dispatch 阶段的 launch evidence frame。
    ///
    /// 该 revision 只负责稳定记录 run / agent / frame / runtime-session anchor，
    /// 后续 runtime surface 必须由 frame construction / lifecycle composer 写入。
    pub fn new_launch_anchor(agent_id: Uuid, created_by_id: Option<String>) -> Self {
        Self::new(agent_id).with_created_by("dispatch_launch_anchor", created_by_id)
    }

    /// 返回本次构造将持久化的稳定 Frame identity。
    ///
    /// Runtime surface 在 frame 写入前就需要引用该 identity，因此 builder 创建时
    /// 预分配 ID，`build_uncommitted` 不得重新生成。
    pub fn frame_id(&self) -> Uuid {
        self.frame_id
    }

    pub fn with_context(mut self, context_slice: serde_json::Value) -> Self {
        self.context_slice = Some(context_slice);
        self
    }

    pub fn with_capability(mut self, capability_surface: serde_json::Value) -> Self {
        self.capability_surface = Some(capability_surface);
        self
    }

    pub fn with_vfs(mut self, vfs_surface: serde_json::Value) -> Self {
        self.vfs_surface = Some(vfs_surface);
        self
    }

    pub fn with_mcp(mut self, mcp_surface: serde_json::Value) -> Self {
        self.mcp_surface = Some(mcp_surface);
        self
    }

    /// 从 `CapabilityState` 一次性填充 capability / VFS / MCP 三个 surface 列。
    ///
    /// 内部调用 `capability_state_to_frame_surfaces` 拆分，保证写入与
    /// `project_capability_state_from_frame` 读取完全对称。
    pub fn with_capability_state(mut self, state: &CapabilityState) -> Self {
        let surfaces = capability_state_to_frame_surfaces(state);
        self.capability_surface = surfaces.effective_capability_json;
        self.vfs_surface = surfaces.vfs_surface_json;
        self.mcp_surface = surfaces.mcp_surface_json;
        self
    }

    /// 从结构化 `Vfs` 填充 vfs_surface（独立于 CapabilityState 维度）。
    ///
    /// 仅当 compose 逻辑独立产出 VFS（而非从 CapabilityState 中拆分）时使用。
    /// 若通过 `with_capability_state` 设置，VFS 会被自动提取。
    pub fn with_vfs_typed(mut self, vfs: &Vfs) -> Self {
        self.vfs_surface = serde_json::to_value(vfs).ok();
        self
    }

    /// 从结构化 `Vec<RuntimeMcpServer>` 填充 mcp_surface。
    pub fn with_mcp_servers(mut self, servers: &[RuntimeMcpServer]) -> Self {
        self.mcp_surface = serde_json::to_value(servers).ok();
        self
    }

    /// 从结构化 `AgentConfig` 填充 execution_profile surface。
    ///
    /// execution profile 记录每个 frame revision 使用的执行器配置，
    /// FrameConstructionService 会通过 AgentFrameSurfaceExt 投影此字段用于 connector 启动。
    pub fn with_execution_profile(mut self, config: &AgentConfig) -> Self {
        self.execution_profile = serde_json::to_value(config).ok();
        self
    }

    /// 从已有 JSON 值填充 execution_profile（用于 frame revision carry-forward）。
    pub fn with_execution_profile_raw(mut self, profile: serde_json::Value) -> Self {
        self.execution_profile = Some(profile);
        self
    }

    pub fn with_hook_plan_raw(mut self, hook_plan: serde_json::Value) -> Self {
        self.hook_plan = Some(hook_plan);
        self
    }

    pub fn with_context_bundle_summary(mut self, bundle: &SessionContextBundle) -> Self {
        self =
            self.with_frame_context_bundle_summary(&FrameContextBundleSummary::from_bundle(bundle));
        self = self.with_context_source_snapshot(&AgentContextSourceSnapshot::from_bundle(bundle));
        self
    }

    pub fn with_context_source_snapshot(mut self, snapshot: &AgentContextSourceSnapshot) -> Self {
        self.context_source_snapshot = serde_json::to_value(snapshot).ok();
        self
    }

    pub fn with_frame_context_bundle_summary(
        mut self,
        summary: &FrameContextBundleSummary,
    ) -> Self {
        self.context_slice = serde_json::to_value(summary).ok();
        self
    }

    pub fn with_surface_draft(mut self, draft: &FrameSurfaceDraft) -> Self {
        if let Some(state) = draft.capability_state.as_ref() {
            self = self.with_capability_state(state);
        }
        if let Some(vfs) = draft.vfs.as_ref() {
            self = self.with_vfs_typed(vfs);
        }
        self = self.with_mcp_servers(&draft.mcp_servers);
        if let Some(config) = draft.execution_profile.as_ref() {
            self = self.with_execution_profile(config);
        }
        if let Some(summary) = draft.context_bundle_summary.as_ref() {
            self = self.with_frame_context_bundle_summary(summary);
        }
        if let Some(snapshot) = draft.context_source_snapshot.as_ref() {
            self = self.with_context_source_snapshot(snapshot);
        }
        self
    }

    pub fn with_runtime_session(self, _session_id: impl Into<String>) -> Self {
        self
    }

    pub fn with_created_by(mut self, kind: impl Into<String>, id: Option<String>) -> Self {
        self.created_by_kind = kind.into();
        self.created_by_id = id;
        self
    }

    /// 构建新 revision 并通过 repository 持久化。
    ///
    /// 从 repository 读取当前最新 revision number，递增后创建新 frame。
    pub async fn build(self, repo: &dyn AgentFrameRepository) -> Result<AgentFrame, DomainError> {
        let frame = self.build_uncommitted(repo).await?;
        repo.create(&frame).await?;
        Ok(frame)
    }

    /// 构建新 revision 但不写入仓储。Frame construction 用它把完整
    /// runtime surface 传给 connector，等 connector accepted 后再提交。
    pub async fn build_uncommitted(
        self,
        repo: &dyn AgentFrameRepository,
    ) -> Result<AgentFrame, DomainError> {
        let current = repo.get_latest(self.agent_id).await?;
        let next_revision = match current.as_ref() {
            Some(current) => current.revision + 1,
            None => 1,
        };

        let mut frame = AgentFrame::new_revision_with_id(
            self.frame_id,
            self.agent_id,
            next_revision,
            &self.created_by_kind,
        );
        frame.effective_capability_json = self.capability_surface.clone().or_else(|| {
            current
                .as_ref()
                .and_then(|frame| frame.effective_capability_json.clone())
        });
        frame.context_slice_json = self.context_slice.clone().or_else(|| {
            current
                .as_ref()
                .and_then(|frame| frame.context_slice_json.clone())
        });
        frame.vfs_surface_json = self.vfs_surface.clone().or_else(|| {
            current
                .as_ref()
                .and_then(|frame| frame.vfs_surface_json.clone())
        });
        frame.mcp_surface_json = self.mcp_surface.clone().or_else(|| {
            current
                .as_ref()
                .and_then(|frame| frame.mcp_surface_json.clone())
        });
        frame.execution_profile_json = self.execution_profile.clone().or_else(|| {
            current
                .as_ref()
                .and_then(|frame| frame.execution_profile_json.clone())
        });
        frame.hook_plan = self
            .hook_plan
            .clone()
            .or_else(|| current.as_ref().and_then(|frame| frame.hook_plan.clone()));
        frame.created_by_id = self.created_by_id.clone();
        let mut surface = frame.surface_document();
        surface.context_source_snapshot = self.context_source_snapshot.clone().or_else(|| {
            current
                .as_ref()
                .and_then(|frame| frame.surface_document().context_source_snapshot)
        });
        frame.surface = Some(surface);

        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_ports::lifecycle_surface_projection::KickoffPromptFragment;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::workflow::MountDirective;
    use agentdash_platform_spi::{McpTransportConfig, SessionContextBundle, ToolCluster};
    use std::{collections::BTreeSet, sync::Mutex};

    #[derive(Default)]
    struct FixtureFrameRepo {
        items: Mutex<Vec<AgentFrame>>,
    }

    #[tokio::test]
    async fn preallocated_frame_identity_is_stable_through_build() {
        let repo = FixtureFrameRepo::default();
        let builder = AgentFrameBuilder::new_launch_anchor(Uuid::new_v4(), None);
        let frame_id = builder.frame_id();

        let frame = builder.build_uncommitted(&repo).await.expect("build frame");

        assert_eq!(frame.id, frame_id);
    }

    fn mount(id: &str, provider: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: provider.to_string(),
            backend_id: "backend-a".to_string(),
            root_ref: format!("{provider}://{id}"),
            capabilities: vec![MountCapability::Read, MountCapability::List],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[async_trait::async_trait]
    impl AgentFrameRepository for FixtureFrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(frame.clone());
            Ok(())
        }
        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }
        async fn get_latest(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            let items = self.items.lock().unwrap();
            let mut frames: Vec<_> = items.iter().filter(|f| f.agent_id == agent_id).collect();
            frames.sort_by_key(|f| f.revision);
            Ok(frames.last().cloned().cloned())
        }
        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|f| f.agent_id == agent_id)
                .cloned()
                .collect())
        }
    }

    #[tokio::test]
    async fn build_creates_initial_revision_when_no_prior_frame() {
        let repo = FixtureFrameRepo::default();
        let agent_id = Uuid::new_v4();

        let frame = AgentFrameBuilder::new(agent_id)
            .with_capability(serde_json::json!({"file_read": true}))
            .with_context(serde_json::json!({"project": "test"}))
            .with_created_by("dispatch", None)
            .build(&repo)
            .await
            .expect("build");

        assert_eq!(frame.agent_id, agent_id);
        assert_eq!(frame.revision, 1);
        assert_eq!(frame.created_by_kind, "dispatch");
        assert!(frame.effective_capability_json.is_some());
        assert!(frame.context_slice_json.is_some());
    }

    #[tokio::test]
    async fn build_launch_anchor_marks_dispatch_evidence_revision() {
        let repo = FixtureFrameRepo::default();
        let agent_id = Uuid::new_v4();

        let frame = AgentFrameBuilder::new_launch_anchor(agent_id, Some("runtime-1".to_string()))
            .build(&repo)
            .await
            .expect("launch anchor");

        assert_eq!(frame.agent_id, agent_id);
        assert_eq!(frame.created_by_kind, "dispatch_launch_anchor");
        assert_eq!(frame.created_by_id.as_deref(), Some("runtime-1"));
        assert!(frame.vfs_surface_json.is_none());
        assert!(frame.effective_capability_json.is_none());
    }

    #[tokio::test]
    async fn build_increments_revision() {
        let repo = FixtureFrameRepo::default();
        let agent_id = Uuid::new_v4();

        let frame1 = AgentFrameBuilder::new(agent_id)
            .build(&repo)
            .await
            .expect("frame1");
        assert_eq!(frame1.revision, 1);

        let frame2 = AgentFrameBuilder::new(agent_id)
            .with_vfs(serde_json::json!({"mounts": []}))
            .build(&repo)
            .await
            .expect("frame2");
        assert_eq!(frame2.revision, 2);
        assert!(frame2.vfs_surface_json.is_some());
    }

    #[tokio::test]
    async fn build_with_runtime_session_does_not_persist_frame_refs() {
        let repo = FixtureFrameRepo::default();
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session(session_id.to_string())
            .build(&repo)
            .await
            .expect("build");

        assert_eq!(frame.agent_id, agent_id);
        assert_eq!(frame.revision, 1);
    }

    #[tokio::test]
    async fn build_revision_carries_forward_runtime_surface() {
        let repo = FixtureFrameRepo::default();
        let agent_id = Uuid::new_v4();

        let frame1 = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("session-1")
            .with_execution_profile_raw(serde_json::json!({"executor": "local"}))
            .build(&repo)
            .await
            .expect("frame1");

        let frame2 = AgentFrameBuilder::new(agent_id)
            .with_capability(serde_json::json!({"tools": []}))
            .build(&repo)
            .await
            .expect("frame2");

        assert_eq!(frame2.revision, frame1.revision + 1);
        assert_eq!(
            frame2.execution_profile_json,
            Some(serde_json::json!({"executor": "local"}))
        );
    }

    #[tokio::test]
    async fn lifecycle_activation_surface_outputs_single_coherent_frame_revision() {
        let repo = FixtureFrameRepo::default();
        let agent_id = Uuid::new_v4();
        let activation = ActivityActivation {
            capability_state: CapabilityState::from_clusters([ToolCluster::Read]),
            mcp_servers: vec![RuntimeMcpServer {
                name: "workflow-tools".to_string(),
                transport: McpTransportConfig::Http {
                    url: "http://localhost/mcp".to_string(),
                    headers: Vec::new(),
                },
                uses_relay: false,
                readiness: Default::default(),
            }],
            capability_keys: BTreeSet::from(["file_read".to_string()]),
            kickoff_prompt: KickoffPromptFragment::default(),
            lifecycle_mount: mount("lifecycle", "lifecycle_vfs"),
            lifecycle_vfs: Vfs {
                mounts: vec![mount("lifecycle", "lifecycle_vfs")],
                default_mount_id: None,
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            },
            mount_directives: Vec::<MountDirective>::new(),
        };
        let base_vfs = Vfs {
            mounts: vec![mount("workspace", "relay_fs")],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let context_bundle = SessionContextBundle::new(Uuid::new_v4(), "lifecycle_node");
        let executor_config = AgentConfig::new("PI_AGENT");
        let surface = build_lifecycle_activation_surface(AgentFrameActivationSurfaceInput {
            activation: &activation,
            base_vfs: Some(&base_vfs),
            inherit_skills_from: None,
        });
        let mut draft = surface.to_surface_draft();
        draft.execution_profile = Some(executor_config);
        draft.context_bundle_summary =
            Some(FrameContextBundleSummary::from_bundle(&context_bundle));

        let frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("runtime-1")
            .with_surface_draft(&draft)
            .build(&repo)
            .await
            .expect("frame");

        assert_eq!(
            frame
                .execution_profile_json
                .as_ref()
                .and_then(|value| value.get("executor"))
                .and_then(serde_json::Value::as_str),
            Some("PI_AGENT")
        );
        assert_eq!(
            frame
                .context_slice_json
                .as_ref()
                .and_then(|value| value.get("phase_tag"))
                .and_then(serde_json::Value::as_str),
            Some("lifecycle_node")
        );

        let vfs_mount_ids = frame
            .vfs_surface_json
            .as_ref()
            .and_then(|value| value.get("mounts"))
            .and_then(serde_json::Value::as_array)
            .expect("vfs mounts")
            .iter()
            .filter_map(|mount| mount.get("id").and_then(serde_json::Value::as_str))
            .collect::<BTreeSet<_>>();
        assert_eq!(vfs_mount_ids, BTreeSet::from(["workspace", "lifecycle"]));

        let mcp_names = frame
            .mcp_surface_json
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .expect("mcp surface")
            .iter()
            .filter_map(|server| server.get("name").and_then(serde_json::Value::as_str))
            .collect::<BTreeSet<_>>();
        assert_eq!(mcp_names, BTreeSet::from(["workflow-tools"]));
        assert!(
            frame.effective_capability_json.is_some(),
            "capability surface should be written by the same frame revision"
        );
    }

    #[tokio::test]
    async fn build_revision_writes_frame_surface_draft() {
        let repo = FixtureFrameRepo::default();
        let agent_id = Uuid::new_v4();
        let vfs = Vfs {
            mounts: vec![mount("workspace", "relay_fs")],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let mut capability_state = CapabilityState::from_clusters([ToolCluster::Read]);
        capability_state.vfs.active = Some(vfs.clone());
        let mcp_servers = vec![RuntimeMcpServer {
            name: "draft-tools".to_string(),
            transport: McpTransportConfig::Http {
                url: "http://localhost/draft".to_string(),
                headers: Vec::new(),
            },
            uses_relay: false,
            readiness: Default::default(),
        }];
        capability_state.tool.mcp_servers = mcp_servers.clone();
        let bundle = SessionContextBundle::new(Uuid::new_v4(), "owner_bootstrap");
        let draft = FrameSurfaceDraft {
            capability_state: Some(capability_state),
            vfs: Some(vfs),
            mcp_servers,
            context_bundle_summary: Some(FrameContextBundleSummary::from_bundle(&bundle)),
            context_source_snapshot: Some(AgentContextSourceSnapshot::from_bundle(&bundle)),
            execution_profile: Some(AgentConfig::new("PI_AGENT")),
        };

        let frame = AgentFrameBuilder::new(agent_id)
            .with_surface_draft(&draft)
            .build(&repo)
            .await
            .expect("frame");

        assert!(frame.effective_capability_json.is_some());
        assert!(frame.vfs_surface_json.is_some());
        assert!(frame.mcp_surface_json.is_some());
        assert_eq!(
            frame
                .context_slice_json
                .as_ref()
                .and_then(|value| value.get("bundle_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            Some(bundle.bundle_id.to_string())
        );
        assert_eq!(
            frame
                .execution_profile_json
                .as_ref()
                .and_then(|value| value.get("executor"))
                .and_then(serde_json::Value::as_str),
            Some("PI_AGENT")
        );
    }

    #[tokio::test]
    async fn build_revision_materializes_empty_mcp_closure() {
        let repo = FixtureFrameRepo::default();
        let frame = AgentFrameBuilder::new(Uuid::new_v4())
            .with_mcp_servers(&[])
            .build(&repo)
            .await
            .expect("frame");

        assert_eq!(frame.mcp_surface_json, Some(serde_json::json!([])));
    }
}
