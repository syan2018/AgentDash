//! AgentFrameBuilder — 从 StepActivation / CapabilityResolver / Context
//! projection 等输入收束 AgentFrame revision 的唯一构造路径。
//!
//! ## 设计定位
//!
//! - **唯一事实源**：capability / context / VFS / MCP surface 只从 builder 写入
//!   frame revision，不再由 SessionConstructionPlan、AgentFrameHookRuntime、live
//!   session maps 等并行事实源决定。
//! - **不可变快照**：`build()` 产出新 revision，旧 revision 保持不变，
//!   revision 序列天然提供 provenance。
//! - **面向 dispatch**：`LifecycleDispatchService` 在创建 agent 后通过
//!   builder 产出带 surface 的 initial frame，取代当前 `new_initial` 裸构造。

use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository, AgentProcedureRef};
use agentdash_domain::DomainError;
use agentdash_spi::{AgentConfig, CapabilityState, SessionMcpServer, Vfs};
use uuid::Uuid;

use crate::session::capability_state::capability_state_to_frame_surfaces;

/// AgentFrame 的 builder，收束所有 runtime surface 输入为单次 revision。
///
/// 每次 `build()` 创建一个新 revision 并持久化；调用方应在
/// capability / context / VFS / MCP 任一维度变更时构造新 builder 并 build。
pub struct AgentFrameBuilder {
    agent_id: Uuid,
    procedure_ref: Option<AgentProcedureRef>,
    context_slice: Option<serde_json::Value>,
    capability_surface: Option<serde_json::Value>,
    vfs_surface: Option<serde_json::Value>,
    mcp_surface: Option<serde_json::Value>,
    execution_profile: Option<serde_json::Value>,
    runtime_session_refs: Vec<Uuid>,
    graph_instance_id: Option<Uuid>,
    activity_key: Option<String>,
    created_by_kind: String,
    created_by_id: Option<String>,
}

impl AgentFrameBuilder {
    pub fn new(agent_id: Uuid) -> Self {
        Self {
            agent_id,
            procedure_ref: None,
            context_slice: None,
            capability_surface: None,
            vfs_surface: None,
            mcp_surface: None,
            execution_profile: None,
            runtime_session_refs: Vec::new(),
            graph_instance_id: None,
            activity_key: None,
            created_by_kind: "frame_builder".to_string(),
            created_by_id: None,
        }
    }

    pub fn with_procedure(mut self, procedure_ref: AgentProcedureRef) -> Self {
        self.procedure_ref = Some(procedure_ref);
        self
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

    /// 从结构化 `Vec<SessionMcpServer>` 填充 mcp_surface。
    pub fn with_mcp_servers(mut self, servers: &[SessionMcpServer]) -> Self {
        if servers.is_empty() {
            self.mcp_surface = None;
        } else {
            self.mcp_surface = serde_json::to_value(servers).ok();
        }
        self
    }

    /// 从结构化 `AgentConfig` 填充 execution_profile surface。
    ///
    /// execution profile 记录每个 frame revision 使用的执行器配置，
    /// RuntimeLaunchRequest.from_frame() 会投影此字段用于 connector 启动。
    pub fn with_execution_profile(mut self, config: &AgentConfig) -> Self {
        self.execution_profile = serde_json::to_value(config).ok();
        self
    }

    /// 从已有 JSON 值填充 execution_profile（用于 frame revision carry-forward）。
    pub fn with_execution_profile_raw(mut self, profile: serde_json::Value) -> Self {
        self.execution_profile = Some(profile);
        self
    }

    pub fn with_runtime_session(mut self, session_id: Uuid) -> Self {
        self.runtime_session_refs.push(session_id);
        self
    }

    pub fn with_graph_instance(mut self, graph_instance_id: Uuid, activity_key: impl Into<String>) -> Self {
        self.graph_instance_id = Some(graph_instance_id);
        self.activity_key = Some(activity_key.into());
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
    pub async fn build(
        &self,
        repo: &dyn AgentFrameRepository,
    ) -> Result<AgentFrame, DomainError> {
        let next_revision = match repo.get_current(self.agent_id).await? {
            Some(current) => current.revision + 1,
            None => 1,
        };

        let session_refs_json = if self.runtime_session_refs.is_empty() {
            None
        } else {
            Some(serde_json::json!(
                self.runtime_session_refs
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
            ))
        };

        let procedure_id = self.procedure_ref.as_ref().and_then(|r| match r {
            AgentProcedureRef::ById(id) => Some(*id),
            AgentProcedureRef::ByKey { .. } => None,
        });

        let mut frame = AgentFrame::new_revision(
            self.agent_id,
            next_revision,
            &self.created_by_kind,
        );
        frame.procedure_id = procedure_id;
        frame.graph_instance_id = self.graph_instance_id;
        frame.activity_key = self.activity_key.clone();
        frame.effective_capability_json = self.capability_surface.clone();
        frame.context_slice_json = self.context_slice.clone();
        frame.vfs_surface_json = self.vfs_surface.clone();
        frame.mcp_surface_json = self.mcp_surface.clone();
        frame.runtime_session_refs_json = session_refs_json;
        frame.execution_profile_json = self.execution_profile.clone();
        frame.created_by_id = self.created_by_id.clone();

        repo.create(&frame).await?;
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct InMemoryFrameRepo {
        items: Mutex<Vec<AgentFrame>>,
    }

    #[async_trait::async_trait]
    impl AgentFrameRepository for InMemoryFrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(frame.clone());
            Ok(())
        }
        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
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
        async fn find_by_runtime_session(
            &self,
            _runtime_session_id: &str,
        ) -> Result<Option<AgentFrame>, DomainError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn build_creates_initial_revision_when_no_prior_frame() {
        let repo = InMemoryFrameRepo::default();
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
    async fn build_increments_revision() {
        let repo = InMemoryFrameRepo::default();
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
    async fn build_with_runtime_session_refs() {
        let repo = InMemoryFrameRepo::default();
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session(session_id)
            .build(&repo)
            .await
            .expect("build");

        let refs = frame.runtime_session_refs_json.unwrap();
        let arr = refs.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].as_str().unwrap(), session_id.to_string());
    }

    #[tokio::test]
    async fn build_with_graph_instance() {
        let repo = InMemoryFrameRepo::default();
        let agent_id = Uuid::new_v4();
        let gi_id = Uuid::new_v4();

        let frame = AgentFrameBuilder::new(agent_id)
            .with_graph_instance(gi_id, "implement")
            .build(&repo)
            .await
            .expect("build");

        assert_eq!(frame.graph_instance_id, Some(gi_id));
        assert_eq!(frame.activity_key.as_deref(), Some("implement"));
    }

    #[tokio::test]
    async fn build_with_procedure_ref_by_id() {
        let repo = InMemoryFrameRepo::default();
        let agent_id = Uuid::new_v4();
        let proc_id = Uuid::new_v4();

        let frame = AgentFrameBuilder::new(agent_id)
            .with_procedure(AgentProcedureRef::ById(proc_id))
            .build(&repo)
            .await
            .expect("build");

        assert_eq!(frame.procedure_id, Some(proc_id));
    }
}
