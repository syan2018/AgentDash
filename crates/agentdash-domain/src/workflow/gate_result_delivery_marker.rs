use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::common::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateResultDeliveryStatus {
    Pending,
    DeliveredToWaiter,
    QueuedForParentContinuation,
    DispatchedToParent,
}

impl GateResultDeliveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::DeliveredToWaiter => "delivered_to_waiter",
            Self::QueuedForParentContinuation => "queued_for_parent_continuation",
            Self::DispatchedToParent => "dispatched_to_parent",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pending" => Ok(Self::Pending),
            "delivered_to_waiter" => Ok(Self::DeliveredToWaiter),
            "queued_for_parent_continuation" => Ok(Self::QueuedForParentContinuation),
            "dispatched_to_parent" => Ok(Self::DispatchedToParent),
            other => Err(DomainError::InvalidConfig(format!(
                "gate_result_delivery_markers.status: unsupported value `{other}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateResultDeliveryMarker {
    pub gate_id: Uuid,
    pub result_attempt: i32,
    pub status: GateResultDeliveryStatus,
    pub target_run_id: Option<Uuid>,
    pub target_agent_id: Option<Uuid>,
    pub target_waiter_ref: Option<String>,
    pub mailbox_message_id: Option<Uuid>,
    pub command_receipt_id: Option<Uuid>,
    pub claim_token: Option<Uuid>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RegisterGateResultWaiterRequest {
    pub gate_id: Uuid,
    pub result_attempt: i32,
    pub waiter_ref: String,
    pub target_run_id: Uuid,
    pub target_agent_id: Uuid,
    pub claim_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ClaimGateResultWaiterRequest {
    pub gate_id: Uuid,
    pub result_attempt: i32,
    pub waiter_ref: String,
    pub target_run_id: Uuid,
    pub target_agent_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct ClaimGateResultParentContinuationRequest {
    pub gate_id: Uuid,
    pub result_attempt: i32,
    pub target_run_id: Uuid,
    pub target_agent_id: Uuid,
    pub claim_token: Uuid,
    pub claim_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CompleteGateResultParentContinuationRequest {
    pub gate_id: Uuid,
    pub result_attempt: i32,
    pub claim_token: Uuid,
    pub mailbox_message_id: Option<Uuid>,
    pub command_receipt_id: Option<Uuid>,
    pub dispatched_to_parent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateResultDeliveryClaim {
    Claimed(GateResultDeliveryMarker),
    Existing(GateResultDeliveryMarker),
}

impl GateResultDeliveryClaim {
    pub fn marker(&self) -> &GateResultDeliveryMarker {
        match self {
            Self::Claimed(marker) | Self::Existing(marker) => marker,
        }
    }

    pub fn claimed(&self) -> bool {
        matches!(self, Self::Claimed(_))
    }
}

#[async_trait::async_trait]
pub trait GateResultDeliveryMarkerRepository: Send + Sync {
    async fn register_waiter(
        &self,
        request: RegisterGateResultWaiterRequest,
    ) -> Result<GateResultDeliveryMarker, DomainError>;

    async fn claim_waiter_delivery(
        &self,
        request: ClaimGateResultWaiterRequest,
    ) -> Result<GateResultDeliveryClaim, DomainError>;

    async fn claim_parent_continuation(
        &self,
        request: ClaimGateResultParentContinuationRequest,
    ) -> Result<GateResultDeliveryClaim, DomainError>;

    async fn complete_parent_continuation(
        &self,
        request: CompleteGateResultParentContinuationRequest,
    ) -> Result<GateResultDeliveryMarker, DomainError>;

    async fn get(
        &self,
        gate_id: Uuid,
        result_attempt: i32,
    ) -> Result<Option<GateResultDeliveryMarker>, DomainError>;
}
