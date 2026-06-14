use crate::session::SessionExecutionState;

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
