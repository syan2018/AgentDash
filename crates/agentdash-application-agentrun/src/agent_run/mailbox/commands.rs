use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunMailboxCommandOutcome {
    Launched,
    Queued,
    Steered,
    Deleted,
    Resumed,
    Blocked,
    Failed,
}

impl AgentRunMailboxCommandOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Launched => "launched",
            Self::Queued => "queued",
            Self::Steered => "steered",
            Self::Deleted => "deleted",
            Self::Resumed => "resumed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunMailboxScheduleTrigger {
    UserMessageSubmitted,
    AgentLoopTurnBoundary,
    AgentRunTurnBoundary,
    ManualResume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunMailboxCommandTarget {
    pub address: AgentRunRuntimeAddress,
    pub message_stream: Option<MessageStreamProjectionRef>,
}

impl AgentRunMailboxCommandTarget {
    pub fn new(address: AgentRunRuntimeAddress) -> Self {
        Self {
            address,
            message_stream: None,
        }
    }

    pub fn with_message_stream(mut self, message_stream: MessageStreamProjectionRef) -> Self {
        self.message_stream = Some(message_stream);
        self
    }

    pub fn from_runtime_session_adapter(
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Uuid,
        runtime_session_id: impl Into<String>,
    ) -> Self {
        Self::new(AgentRunRuntimeAddress {
            run_id,
            agent_id,
            frame_id,
        })
        .with_message_stream(MessageStreamProjectionRef {
            runtime_session_id: runtime_session_id.into(),
            trace_kind: MessageStreamTraceKind::ConnectorRuntimeSession,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxUserMessageCommand {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: String,
    pub source: MailboxSourceIdentity,
    pub schedule_on_submit: bool,
    pub input: Vec<UserInputBlock>,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub identity: Option<AuthIdentity>,
    /// `Some("steer")` = 明确注入 active turn；其余情况排队（pending）。
    pub delivery_intent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxUserMessageTargetCommand {
    pub target: AgentRunMailboxCommandTarget,
    pub source: MailboxSourceIdentity,
    pub schedule_on_submit: bool,
    pub input: Vec<UserInputBlock>,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub identity: Option<AuthIdentity>,
    /// `Some("steer")` = 明确注入 active turn；其余情况排队（pending）。
    pub delivery_intent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxControlCommand {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: String,
    pub message_id: Option<Uuid>,
    pub client_command_id: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxControlTargetCommand {
    pub target: AgentRunMailboxCommandTarget,
    pub message_id: Option<Uuid>,
    pub client_command_id: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxCommandResult {
    pub command_receipt: AgentRunCommandReceiptView,
    pub outcome: AgentRunMailboxCommandOutcome,
    pub mailbox_message: Option<AgentRunMailboxMessage>,
    pub accepted_refs: Option<AgentRunAcceptedRefs>,
    pub runtime_state: Option<SessionExecutionState>,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxScheduleOutcome {
    pub outcome: AgentRunMailboxCommandOutcome,
    pub mailbox_message: AgentRunMailboxMessage,
    pub accepted_refs: Option<AgentRunAcceptedRefs>,
}
