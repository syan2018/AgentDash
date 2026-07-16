use agentdash_domain::agent_run_mailbox::{AgentRunMailboxMessage, NewAgentRunMailboxMessage};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentRunAcceptedRefs, AgentRunCommandReceipt, NewAgentRunCommandReceipt,
};
use serde_json::Value;
use uuid::Uuid;

/// Product-owned submission intent persisted as one durable unit.
///
/// The receipt owns client-command idempotency. The mailbox row only owns
/// delivery ordering and leases; it must never infer product replay semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentRunMessageSubmission {
    pub receipt: NewAgentRunCommandReceipt,
    /// Optional product reservation acquired before an external owner creates
    /// the AgentRun graph (for example project-agent start).
    pub reserved_receipt_id: Option<Uuid>,
    pub mailbox_message: NewAgentRunMailboxMessage,
    pub acceptance_results: AgentRunMessageAcceptanceResults,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRunMessageSubmissionReservation {
    Created { receipt_id: Uuid },
    Replay { receipt: AgentRunCommandReceipt },
    ReconcileRequired { receipt: AgentRunCommandReceipt },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AgentRunMessageAcceptanceResults {
    pub started: Value,
    pub steered: Value,
    pub failed: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunAcceptedDeliveryKind {
    Started,
    Steered,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRunMessageSubmissionAdmission {
    /// This call durably created both the product receipt and its mailbox row.
    Created {
        receipt_id: Uuid,
        mailbox_message: AgentRunMailboxMessage,
    },
    /// The product command already has an immutable observable result.
    Replay { receipt: AgentRunCommandReceipt },
    /// The command exists but its first observable result was not settled yet.
    /// Reconciliation must use this exact mailbox id; another claimed message
    /// can never be attributed to this submission.
    ReconcileRequired {
        receipt: AgentRunCommandReceipt,
        mailbox_message: AgentRunMailboxMessage,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentRunMessageSubmission {
    pub receipt_id: Uuid,
    pub mailbox_message_id: Uuid,
    pub accepted_refs: AgentRunAcceptedRefs,
    pub result_json: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunMailboxFailedSettlement {
    pub mailbox_message_id: Uuid,
    pub claim_token: Uuid,
    pub error_message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRunMessageSubmissionCompletion {
    Completed { receipt: AgentRunCommandReceipt },
    Replayed { receipt: AgentRunCommandReceipt },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunMailboxAcceptedSettlement {
    pub mailbox_message_id: Uuid,
    pub claim_token: Uuid,
    pub delivery_kind: AgentRunAcceptedDeliveryKind,
    pub accepted_refs: AgentRunAcceptedRefs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunMailboxDeliverySettlementResult {
    pub message: AgentRunMailboxMessage,
}

pub type AgentRunMailboxAcceptedSettlementResult = AgentRunMailboxDeliverySettlementResult;

/// Narrow cross-aggregate settlement boundary consumed by mailbox delivery
/// after canonical Runtime admission.
///
/// The mailbox reports only a delivery fact and canonical references. Product
/// reservation, admission, completion, and replay remain unavailable through
/// this port and are owned by the Message Submission Unit of Work.
#[async_trait::async_trait]
pub trait AgentRunMailboxDeliverySettlementPort: Send + Sync {
    async fn settle_delivery_failed(
        &self,
        failure: AgentRunMailboxFailedSettlement,
    ) -> Result<AgentRunMailboxDeliverySettlementResult, DomainError>;

    async fn settle_delivery_accepted(
        &self,
        settlement: AgentRunMailboxAcceptedSettlement,
    ) -> Result<AgentRunMailboxAcceptedSettlementResult, DomainError>;
}

#[async_trait::async_trait]
pub trait AgentRunMessageSubmissionStore: AgentRunMailboxDeliverySettlementPort {
    /// Loads a product receipt by its reservation identity.
    async fn load_receipt(
        &self,
        receipt_id: Uuid,
    ) -> Result<Option<AgentRunCommandReceipt>, DomainError>;

    /// Loads the product-owned receipt attached to an exact mailbox delivery.
    ///
    /// This lookup deliberately lives on the full Submission store instead of
    /// the mailbox settlement port: queue delivery may report only mailbox
    /// facts, while the product owner is responsible for replaying its frozen
    /// observable result.
    async fn load_receipt_by_mailbox_message(
        &self,
        mailbox_message_id: Uuid,
    ) -> Result<Option<AgentRunCommandReceipt>, DomainError>;

    /// Reserves product idempotency before an external owner creates the run
    /// graph. A later `admit` attaches the exact mailbox row to this receipt.
    async fn reserve(
        &self,
        receipt: NewAgentRunCommandReceipt,
    ) -> Result<AgentRunMessageSubmissionReservation, DomainError>;

    /// Releases a side-effect-free pending reservation after mutable product
    /// admission rejects the new command. Attached or settled receipts are
    /// never removed.
    async fn abandon_reservation(&self, receipt_id: Uuid) -> Result<bool, DomainError>;

    /// Terminally settles a reservation that failed before a mailbox row could
    /// be attached. Replays preserve the same error text.
    async fn fail_reservation(
        &self,
        receipt_id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, DomainError>;

    /// Atomically claims the product command and creates/attaches its exact
    /// mailbox row. No receipt may become externally visible without a durable
    /// message payload to reconcile.
    async fn admit(
        &self,
        submission: NewAgentRunMessageSubmission,
    ) -> Result<AgentRunMessageSubmissionAdmission, DomainError>;

    /// Freezes the first observable product response while leaving a queued
    /// mailbox row untouched. This is used when delivery is deferred or a
    /// scheduler pass advanced a different message.
    async fn complete_submission(
        &self,
        completion: CompleteAgentRunMessageSubmission,
    ) -> Result<AgentRunMessageSubmissionCompletion, DomainError>;
}
