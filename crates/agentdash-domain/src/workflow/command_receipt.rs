use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::common::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunCommandStatus {
    Pending,
    Accepted,
    TerminalFailed,
}

impl AgentRunCommandStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::TerminalFailed => "terminal_failed",
        }
    }
}

impl TryFrom<&str> for AgentRunCommandStatus {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "terminal_failed" => Ok(Self::TerminalFailed),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_command_receipts.status 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunCommandKind {
    MessageSubmit,
    ProjectAgentStart,
    AgentRunFork,
    AgentRunForkSubmit,
    MailboxPromote,
    MailboxDelete,
    MailboxMove,
    MailboxResume,
    Cancel,
    ContextCompact,
}

impl AgentRunCommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MessageSubmit => "message_submit",
            Self::ProjectAgentStart => "project_agent_start",
            Self::AgentRunFork => "agent_run_fork",
            Self::AgentRunForkSubmit => "agent_run_fork_submit",
            Self::MailboxPromote => "mailbox_promote",
            Self::MailboxDelete => "mailbox_delete",
            Self::MailboxMove => "mailbox_move",
            Self::MailboxResume => "mailbox_resume",
            Self::Cancel => "cancel",
            Self::ContextCompact => "context_compact",
        }
    }
}

impl TryFrom<&str> for AgentRunCommandKind {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "message_submit" => Ok(Self::MessageSubmit),
            "project_agent_start" => Ok(Self::ProjectAgentStart),
            "agent_run_fork" => Ok(Self::AgentRunFork),
            "agent_run_fork_submit" => Ok(Self::AgentRunForkSubmit),
            "mailbox_promote" => Ok(Self::MailboxPromote),
            "mailbox_delete" => Ok(Self::MailboxDelete),
            "mailbox_move" => Ok(Self::MailboxMove),
            "mailbox_resume" => Ok(Self::MailboxResume),
            "cancel" => Ok(Self::Cancel),
            "context_compact" => Ok(Self::ContextCompact),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_command_receipts.command_kind 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunAcceptedRefs {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub frame_revision: Option<i32>,
    pub runtime_session_id: Option<String>,
    pub agent_run_turn_id: Option<String>,
    pub protocol_turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunCommandReceipt {
    pub id: Uuid,
    pub scope_kind: String,
    pub scope_key: String,
    pub command_kind: AgentRunCommandKind,
    pub client_command_id: String,
    pub request_digest: String,
    pub status: AgentRunCommandStatus,
    pub mailbox_message_id: Option<Uuid>,
    pub accepted_refs: Option<AgentRunAcceptedRefs>,
    pub result_json: Option<Value>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentRunCommandReceipt {
    pub scope_kind: String,
    pub scope_key: String,
    pub command_kind: AgentRunCommandKind,
    pub client_command_id: String,
    pub request_digest: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRunCommandClaim {
    Created(AgentRunCommandReceipt),
    Duplicate(AgentRunCommandReceipt),
}

impl AgentRunCommandClaim {
    pub fn receipt(&self) -> &AgentRunCommandReceipt {
        match self {
            Self::Created(receipt) | Self::Duplicate(receipt) => receipt,
        }
    }

    pub fn duplicate(&self) -> bool {
        matches!(self, Self::Duplicate(_))
    }
}

#[async_trait::async_trait]
pub trait AgentRunCommandReceiptRepository: Send + Sync {
    async fn claim(
        &self,
        receipt: NewAgentRunCommandReceipt,
    ) -> Result<AgentRunCommandClaim, DomainError>;

    async fn mark_accepted(
        &self,
        id: Uuid,
        accepted_refs: AgentRunAcceptedRefs,
    ) -> Result<AgentRunCommandReceipt, DomainError>;

    async fn attach_mailbox_message(
        &self,
        id: Uuid,
        mailbox_message_id: Uuid,
    ) -> Result<AgentRunCommandReceipt, DomainError>;

    async fn store_result_json(
        &self,
        id: Uuid,
        result_json: Value,
    ) -> Result<AgentRunCommandReceipt, DomainError>;

    async fn accept_with_result(
        &self,
        id: Uuid,
        accepted_refs: AgentRunAcceptedRefs,
        result_json: Value,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        self.mark_accepted(id, accepted_refs).await?;
        self.store_result_json(id, result_json).await
    }

    async fn mark_terminal_failed(
        &self,
        id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, DomainError>;

    async fn fail_with_result(
        &self,
        id: Uuid,
        error_message: String,
        result_json: Value,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        self.mark_terminal_failed(id, error_message).await?;
        self.store_result_json(id, result_json).await
    }

    async fn get(&self, id: Uuid) -> Result<Option<AgentRunCommandReceipt>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::AgentRunCommandKind;

    #[test]
    fn mailbox_move_command_kind_round_trips() {
        assert_eq!(AgentRunCommandKind::MailboxMove.as_str(), "mailbox_move");
        assert_eq!(
            AgentRunCommandKind::try_from("mailbox_move").expect("mailbox_move command kind"),
            AgentRunCommandKind::MailboxMove
        );
    }

    #[test]
    fn context_compact_command_kind_round_trips() {
        assert_eq!(
            AgentRunCommandKind::ContextCompact.as_str(),
            "context_compact"
        );
        assert_eq!(
            AgentRunCommandKind::try_from("context_compact").expect("context_compact command kind"),
            AgentRunCommandKind::ContextCompact
        );
    }
}
