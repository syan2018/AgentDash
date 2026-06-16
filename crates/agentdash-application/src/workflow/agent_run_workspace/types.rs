use crate::session::SessionExecutionState;
use crate::session::SessionMeta;
use crate::vfs::ResolvedVfsSurface;
use crate::workflow::lifecycle_run_view_builder::{
    AgentRunView, LifecycleSubjectAssociationView, RuntimeSessionRefView,
};
use agentdash_contracts::workflow::{
    AgentConversationSnapshot, ConversationEffectiveExecutorConfigView, SubjectRefDto,
};
use agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage;
use agentdash_domain::workflow::{LifecycleAgent, LifecycleRun};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AgentRunWorkspaceQueryInput {
    pub run: LifecycleRun,
    pub agent: LifecycleAgent,
}

#[derive(Debug, Clone)]
pub struct AgentRunWorkspaceSnapshot {
    pub run: LifecycleRun,
    pub agent: LifecycleAgent,
    pub shell: AgentRunWorkspaceShellModel,
    pub delivery_runtime_session_id: Option<String>,
    pub delivery_trace_meta: Option<AgentRunWorkspaceTraceMetaModel>,
    pub projection: AgentRunWorkspaceProjectionModel,
    pub agent_view: Option<AgentRunView>,
    pub frame_runtime: Option<AgentRunWorkspaceFrameRuntimeModel>,
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
    pub mailbox: AgentRunWorkspaceMailboxStateModel,
    pub mailbox_messages: Vec<AgentRunMailboxMessage>,
    pub resource_surface: Option<ResolvedVfsSurface>,
    pub conversation: AgentConversationSnapshot,
}

/// 列表视图专用的轻量投影。
///
/// 只解析侧栏 / 主区列表实际消费的字段（标题、投递状态、subject 归属），
/// 刻意跳过 vfs surface、lifecycle run view、mailbox、conversation 等重量级解析，
/// 避免列表为每个主 Run 走一遍详情快照。
#[derive(Debug, Clone)]
pub struct AgentRunListProjection {
    pub run: LifecycleRun,
    pub agent: LifecycleAgent,
    pub shell: AgentRunWorkspaceShellModel,
    pub agent_role: String,
    /// 面向用户的身份标识：绑定 Project Agent 的显示名（preset.display_name || name）；
    /// 无绑定（ad-hoc / 已删除）时为 None。
    pub project_agent_label: Option<String>,
    pub delivery_runtime_session_id: Option<String>,
    pub delivery_trace_meta: Option<AgentRunWorkspaceTraceMetaModel>,
    pub subject_ref: Option<SubjectRefDto>,
    pub subject_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceShellModel {
    pub display_title: String,
    pub title_source: String,
    pub workspace_status: String,
    pub delivery_status: String,
    pub last_turn_id: Option<String>,
    pub last_activity_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceTraceMetaModel {
    pub runtime_session_id: String,
    pub last_event_seq: u64,
    pub executor_session_id: Option<String>,
    pub trace_title: String,
    pub trace_title_source: String,
    pub delivery_status: String,
    pub last_turn_id: Option<String>,
    pub terminal_summary: Option<String>,
    pub updated_at: i64,
}

impl AgentRunWorkspaceTraceMetaModel {
    pub fn from_session_meta(meta: &SessionMeta) -> Self {
        Self {
            runtime_session_id: meta.id.clone(),
            last_event_seq: meta.last_event_seq,
            executor_session_id: meta.executor_session_id.clone(),
            trace_title: meta.title.clone(),
            trace_title_source: serialized_string(&meta.title_source),
            delivery_status: serialized_string(&meta.last_delivery_status),
            last_turn_id: meta.last_turn_id.clone(),
            terminal_summary: meta.last_terminal_message.clone(),
            updated_at: meta.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunWorkspaceFrameRuntimeModel {
    pub frame_ref: AgentRunWorkspaceFrameRefModel,
    pub capability_surface: Value,
    pub context_slice: Value,
    pub vfs_surface: Value,
    pub mcp_surface: Value,
    pub runtime_session_refs: Vec<RuntimeSessionRefView>,
    pub execution_profile: Option<Value>,
    pub effective_executor_config: Option<ConversationEffectiveExecutorConfigView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceFrameRefModel {
    pub agent_id: String,
    pub frame_id: String,
    pub revision: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceMailboxStateModel {
    pub paused: bool,
    pub pause_reason: Option<String>,
    pub message: Option<String>,
    pub can_resume: bool,
    pub hide_system_steer_messages: bool,
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunWorkspaceRuntimeCommandStatus {
    Idle,
    Running,
    Cancelling,
    Completed,
    Failed,
    Interrupted,
}

impl AgentRunWorkspaceRuntimeCommandStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Cancelling => "cancelling",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceRuntimeCommandStateModel {
    pub status: AgentRunWorkspaceRuntimeCommandStatus,
    pub turn_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentRunWorkspaceProjectionInput<'a> {
    pub execution_state: &'a SessionExecutionState,
    pub agent_status: &'a str,
}

impl<'a> AgentRunWorkspaceProjectionInput<'a> {
    pub fn new(execution_state: &'a SessionExecutionState, agent_status: &'a str) -> Self {
        Self {
            execution_state,
            agent_status,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceProjectionModel {
    pub state_code: AgentRunWorkspaceStateCode,
    pub active_turn_id: Option<String>,
    pub last_turn_id: Option<String>,
    pub delivery_status: String,
}

fn serialized_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
