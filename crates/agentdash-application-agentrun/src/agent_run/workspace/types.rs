use crate::agent_run::AgentRunOwnershipModel;
use crate::agent_run::lifecycle_read_model_facade::{
    AgentRunView, LifecycleSubjectAssociationView, RuntimeThreadRefView,
};
use crate::agent_run::{AgentConversationSnapshotModel, ConversationEffectiveExecutorConfigModel};
use agentdash_application_vfs::ResolvedVfsSurface;
use agentdash_domain::workflow::{LifecycleAgent, LifecycleRun};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AgentRunWorkspaceQueryInput {
    pub run: LifecycleRun,
    pub agent: LifecycleAgent,
    pub viewer_user_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRunWorkspaceSnapshot {
    pub run: LifecycleRun,
    pub agent: LifecycleAgent,
    pub ownership: AgentRunOwnershipModel,
    pub shell: AgentRunWorkspaceShellModel,
    pub runtime_thread_id: Option<String>,
    pub state: AgentRunWorkspaceStateModel,
    pub agent_view: Option<AgentRunView>,
    pub frame_runtime: Option<AgentRunWorkspaceFrameRuntimeModel>,
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
    pub resource_surface: Option<ResolvedVfsSurface>,
    pub resource_surface_coordinate: Option<AgentRunResourceSurfaceCoordinateModel>,
    pub conversation: AgentConversationSnapshotModel,
}

/// 列表视图专用的轻量条目。
///
/// 只解析侧栏 / 主区列表实际消费的字段（标题、投递状态、subject 归属），
/// 刻意跳过 vfs surface、lifecycle run view、mailbox、conversation 等重量级解析，
/// 避免列表为每个主 Run 走一遍详情快照。
#[derive(Debug, Clone)]
pub struct AgentRunListItem {
    pub run: LifecycleRun,
    pub agent: LifecycleAgent,
    pub shell: AgentRunWorkspaceShellModel,
    /// 面向用户的身份标识：绑定 Project Agent 的显示名（preset.display_name || name）；
    /// 无绑定（ad-hoc / 已删除）时为 None。
    pub project_agent_label: Option<String>,
    pub runtime_thread_id: Option<String>,
    pub subject_ref: Option<SubjectRefModel>,
    pub subject_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRefModel {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceShellModel {
    pub display_title: String,
    pub title_source: String,
    pub delivery_status: String,
    pub last_turn_id: Option<String>,
    pub last_activity_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunWorkspaceFrameRuntimeModel {
    pub frame_ref: AgentRunWorkspaceFrameRefModel,
    pub capability_surface: Value,
    pub context_slice: Value,
    pub vfs_surface: Value,
    pub mcp_surface: Value,
    pub runtime_thread_refs: Vec<RuntimeThreadRefView>,
    pub execution_profile: Option<Value>,
    pub effective_executor_config: Option<ConversationEffectiveExecutorConfigModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceFrameRefModel {
    pub agent_id: String,
    pub frame_id: String,
    pub revision: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunResourceSurfaceCoordinateModel {
    pub surface_frame_ref: AgentRunWorkspaceFrameRefModel,
    pub source_anchor: Option<AgentRunResourceSurfaceSourceAnchorModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunResourceSurfaceSourceAnchorModel {
    pub runtime_thread_id: String,
    pub launch_frame_id: String,
    pub orchestration_id: Option<String>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
    pub delivery_status: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunWorkspaceStateCode {
    Ready,
    StartingClaimed,
    RunningActive,
    Cancelling,
    Completed,
    Failed,
    Interrupted,
    Lost,
}

impl AgentRunWorkspaceStateCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::StartingClaimed => "starting_claimed",
            Self::RunningActive => "running_active",
            Self::Cancelling => "cancelling",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Lost => "lost",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceStateModel {
    pub state_code: AgentRunWorkspaceStateCode,
    pub active_turn_id: Option<String>,
    pub last_turn_id: Option<String>,
    pub delivery_status: String,
}
