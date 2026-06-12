use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::common::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunDeliveryCommandStatus {
    Pending,
    Accepted,
    TerminalFailed,
}

impl AgentRunDeliveryCommandStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::TerminalFailed => "terminal_failed",
        }
    }
}

impl TryFrom<&str> for AgentRunDeliveryCommandStatus {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "terminal_failed" => Ok(Self::TerminalFailed),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_delivery_command_receipts.status 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunDeliveryAcceptedRefs {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Option<Uuid>,
    pub frame_revision: Option<i32>,
    pub runtime_session_id: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunDeliveryCommandReceipt {
    pub id: Uuid,
    pub scope_kind: String,
    pub scope_key: String,
    pub client_command_id: String,
    pub request_digest: String,
    pub status: AgentRunDeliveryCommandStatus,
    pub accepted_refs: Option<AgentRunDeliveryAcceptedRefs>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentRunDeliveryCommandReceipt {
    pub scope_kind: String,
    pub scope_key: String,
    pub client_command_id: String,
    pub request_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunDeliveryCommandClaim {
    Created(AgentRunDeliveryCommandReceipt),
    Duplicate(AgentRunDeliveryCommandReceipt),
}

impl AgentRunDeliveryCommandClaim {
    pub fn receipt(&self) -> &AgentRunDeliveryCommandReceipt {
        match self {
            Self::Created(receipt) | Self::Duplicate(receipt) => receipt,
        }
    }

    pub fn duplicate(&self) -> bool {
        matches!(self, Self::Duplicate(_))
    }
}

#[async_trait::async_trait]
pub trait AgentRunDeliveryCommandReceiptRepository: Send + Sync {
    async fn claim(
        &self,
        receipt: NewAgentRunDeliveryCommandReceipt,
    ) -> Result<AgentRunDeliveryCommandClaim, DomainError>;

    async fn mark_accepted(
        &self,
        id: Uuid,
        accepted_refs: AgentRunDeliveryAcceptedRefs,
    ) -> Result<AgentRunDeliveryCommandReceipt, DomainError>;

    async fn mark_terminal_failed(
        &self,
        id: Uuid,
        error_message: String,
    ) -> Result<AgentRunDeliveryCommandReceipt, DomainError>;

    async fn get(&self, id: Uuid) -> Result<Option<AgentRunDeliveryCommandReceipt>, DomainError>;
}
