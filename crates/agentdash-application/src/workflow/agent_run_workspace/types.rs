use crate::session::SessionExecutionState;
use crate::session::SessionMeta;
use crate::vfs::ResolvedVfsSurface;
use crate::workflow::lifecycle_run_view_builder::{
    AgentRunView, LifecycleSubjectAssociationView, RuntimeSessionRefView,
};
use agentdash_contracts::workflow::{
    AgentConversationSnapshot, ConversationEffectiveExecutorConfigView,
};
use agentdash_domain::workflow::{AgentRunMailboxMessage, LifecycleAgent, LifecycleRun};
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
pub enum AgentRunWorkspaceControlPlaneStatus {
    Ready,
    Running,
    Cancelling,
    Terminal,
    FrameMissing,
    DeliveryMissing,
}

impl AgentRunWorkspaceControlPlaneStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Cancelling => "cancelling",
            Self::Terminal => "terminal",
            Self::FrameMissing => "frame_missing",
            Self::DeliveryMissing => "delivery_missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceControlPlaneModel {
    pub status: AgentRunWorkspaceControlPlaneStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceActionAvailabilityModel {
    pub enabled: bool,
    pub unavailable_reason: Option<String>,
}

impl AgentRunWorkspaceActionAvailabilityModel {
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            unavailable_reason: None,
        }
    }

    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            unavailable_reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceActionSetModel {
    pub submit_message: AgentRunWorkspaceActionAvailabilityModel,
    pub cancel: AgentRunWorkspaceActionAvailabilityModel,
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
    pub has_delivery_runtime: bool,
    pub has_frame: bool,
}

impl<'a> AgentRunWorkspaceProjectionInput<'a> {
    pub fn new(
        execution_state: &'a SessionExecutionState,
        agent_status: &'a str,
        has_delivery_runtime: bool,
        has_frame: bool,
    ) -> Self {
        Self {
            execution_state,
            agent_status,
            has_delivery_runtime,
            has_frame,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWorkspaceProjectionModel {
    pub state_code: AgentRunWorkspaceStateCode,
    pub active_turn_id: Option<String>,
    pub last_turn_id: Option<String>,
    pub delivery_status: String,
    pub control_plane: AgentRunWorkspaceControlPlaneModel,
    pub actions: AgentRunWorkspaceActionSetModel,
    pub runtime_command_state: AgentRunWorkspaceRuntimeCommandStateModel,
    pub replacement_command: Option<String>,
}

fn serialized_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
