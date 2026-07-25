use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateResultDeliveryMarker {
    pub gate_id: Uuid,
    pub result_attempt: i32,
    pub status: GateResultDeliveryStatus,
    pub target_run_id: Option<Uuid>,
    pub target_agent_id: Option<Uuid>,
    pub target_waiter_ref: Option<String>,
    pub input_handoff_id: Option<Uuid>,
    pub accepted_operation_id: Option<String>,
    pub claim_token: Option<Uuid>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateResultDeliveryState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    marker: Option<GateResultDeliveryMarker>,
}

impl GateResultDeliveryState {
    pub fn marker(&self, result_attempt: i32) -> Option<&GateResultDeliveryMarker> {
        self.marker
            .as_ref()
            .filter(|marker| marker.result_attempt == result_attempt)
    }

    pub fn register_waiter(
        &mut self,
        request: RegisterGateResultWaiterRequest,
        now: DateTime<Utc>,
    ) -> GateResultDeliveryMarker {
        if let Some(marker) = self
            .marker
            .as_mut()
            .filter(|marker| marker.result_attempt == request.result_attempt)
        {
            if marker.status == GateResultDeliveryStatus::Pending {
                marker.target_run_id = Some(request.target_run_id);
                marker.target_agent_id = Some(request.target_agent_id);
                marker.target_waiter_ref = Some(request.waiter_ref);
                marker.claim_expires_at = Some(request.claim_expires_at);
                marker.updated_at = now;
            }
            return marker.clone();
        }

        let marker = GateResultDeliveryMarker {
            gate_id: request.gate_id,
            result_attempt: request.result_attempt,
            status: GateResultDeliveryStatus::Pending,
            target_run_id: Some(request.target_run_id),
            target_agent_id: Some(request.target_agent_id),
            target_waiter_ref: Some(request.waiter_ref),
            input_handoff_id: None,
            accepted_operation_id: None,
            claim_token: None,
            claim_expires_at: Some(request.claim_expires_at),
            created_at: now,
            updated_at: now,
        };
        self.marker = Some(marker.clone());
        marker
    }

    pub fn claim_waiter_delivery(
        &mut self,
        request: ClaimGateResultWaiterRequest,
        now: DateTime<Utc>,
    ) -> GateResultDeliveryClaim {
        if let Some(marker) = self
            .marker
            .as_mut()
            .filter(|marker| marker.result_attempt == request.result_attempt)
        {
            if marker.status == GateResultDeliveryStatus::Pending
                && marker.target_waiter_ref.as_deref() == Some(request.waiter_ref.as_str())
                && marker
                    .claim_expires_at
                    .is_none_or(|claim_expires_at| claim_expires_at >= now)
            {
                marker.status = GateResultDeliveryStatus::DeliveredToWaiter;
                marker.claim_token = None;
                marker.claim_expires_at = None;
                marker.updated_at = now;
                return GateResultDeliveryClaim::Claimed(marker.clone());
            }
            return GateResultDeliveryClaim::Existing(marker.clone());
        }

        let marker = GateResultDeliveryMarker {
            gate_id: request.gate_id,
            result_attempt: request.result_attempt,
            status: GateResultDeliveryStatus::DeliveredToWaiter,
            target_run_id: Some(request.target_run_id),
            target_agent_id: Some(request.target_agent_id),
            target_waiter_ref: Some(request.waiter_ref),
            input_handoff_id: None,
            accepted_operation_id: None,
            claim_token: None,
            claim_expires_at: None,
            created_at: now,
            updated_at: now,
        };
        self.marker = Some(marker.clone());
        GateResultDeliveryClaim::Claimed(marker)
    }

    pub fn claim_parent_continuation(
        &mut self,
        request: ClaimGateResultParentContinuationRequest,
        now: DateTime<Utc>,
    ) -> GateResultDeliveryClaim {
        if let Some(marker) = self
            .marker
            .as_mut()
            .filter(|marker| marker.result_attempt == request.result_attempt)
        {
            let waiter_expired = marker.status == GateResultDeliveryStatus::Pending
                && marker
                    .claim_expires_at
                    .is_none_or(|claim_expires_at| claim_expires_at < now);
            let parent_lease_expired = marker.status
                == GateResultDeliveryStatus::QueuedForParentContinuation
                && marker.input_handoff_id.is_none()
                && marker
                    .claim_expires_at
                    .is_none_or(|claim_expires_at| claim_expires_at < now);
            if waiter_expired || parent_lease_expired {
                marker.status = GateResultDeliveryStatus::QueuedForParentContinuation;
                marker.target_run_id = Some(request.target_run_id);
                marker.target_agent_id = Some(request.target_agent_id);
                marker.claim_token = Some(request.claim_token);
                marker.claim_expires_at = Some(request.claim_expires_at);
                marker.updated_at = now;
                return GateResultDeliveryClaim::Claimed(marker.clone());
            }
            return GateResultDeliveryClaim::Existing(marker.clone());
        }

        let marker = GateResultDeliveryMarker {
            gate_id: request.gate_id,
            result_attempt: request.result_attempt,
            status: GateResultDeliveryStatus::QueuedForParentContinuation,
            target_run_id: Some(request.target_run_id),
            target_agent_id: Some(request.target_agent_id),
            target_waiter_ref: None,
            input_handoff_id: None,
            accepted_operation_id: None,
            claim_token: Some(request.claim_token),
            claim_expires_at: Some(request.claim_expires_at),
            created_at: now,
            updated_at: now,
        };
        self.marker = Some(marker.clone());
        GateResultDeliveryClaim::Claimed(marker)
    }

    pub fn complete_parent_continuation(
        &mut self,
        request: CompleteGateResultParentContinuationRequest,
        now: DateTime<Utc>,
    ) -> Option<GateResultDeliveryMarker> {
        let marker = self.marker(request.result_attempt)?.clone();
        if marker.claim_token != Some(request.claim_token) {
            return Some(marker);
        }

        let marker = self.marker.as_mut().expect("marker exists");
        marker.status = if request.dispatched_to_parent {
            GateResultDeliveryStatus::DispatchedToParent
        } else {
            GateResultDeliveryStatus::QueuedForParentContinuation
        };
        marker.input_handoff_id = request.input_handoff_id;
        marker.accepted_operation_id = request.accepted_operation_id;
        marker.claim_token = None;
        marker.claim_expires_at = None;
        marker.updated_at = now;
        Some(marker.clone())
    }
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
    pub input_handoff_id: Option<Uuid>,
    pub accepted_operation_id: Option<String>,
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

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn delivery_state_serializes_empty_owner_document() {
        let state = GateResultDeliveryState::default();

        assert_eq!(serde_json::to_value(&state).unwrap(), serde_json::json!({}));
        assert_eq!(
            serde_json::from_value::<GateResultDeliveryState>(serde_json::json!({})).unwrap(),
            state
        );
    }

    #[test]
    fn live_waiter_claim_fences_parent_and_replays_once() {
        let gate_id = Uuid::new_v4();
        let target_run_id = Uuid::new_v4();
        let target_agent_id = Uuid::new_v4();
        let now = Utc::now();
        let mut state = GateResultDeliveryState::default();

        state.register_waiter(
            RegisterGateResultWaiterRequest {
                gate_id,
                result_attempt: 1,
                waiter_ref: "waiter-live".to_string(),
                target_run_id,
                target_agent_id,
                claim_expires_at: now + Duration::minutes(1),
            },
            now,
        );
        let parent = state.claim_parent_continuation(
            ClaimGateResultParentContinuationRequest {
                gate_id,
                result_attempt: 1,
                target_run_id,
                target_agent_id,
                claim_token: Uuid::new_v4(),
                claim_expires_at: now + Duration::minutes(1),
            },
            now,
        );
        assert!(!parent.claimed());
        assert_eq!(parent.marker().status, GateResultDeliveryStatus::Pending);

        let waiter = state.claim_waiter_delivery(
            ClaimGateResultWaiterRequest {
                gate_id,
                result_attempt: 1,
                waiter_ref: "waiter-live".to_string(),
                target_run_id,
                target_agent_id,
            },
            now,
        );
        assert!(waiter.claimed());
        assert_eq!(
            waiter.marker().status,
            GateResultDeliveryStatus::DeliveredToWaiter
        );

        let replay = state.claim_waiter_delivery(
            ClaimGateResultWaiterRequest {
                gate_id,
                result_attempt: 1,
                waiter_ref: "waiter-live".to_string(),
                target_run_id,
                target_agent_id,
            },
            now,
        );
        assert!(!replay.claimed());
        assert_eq!(
            replay.marker().status,
            GateResultDeliveryStatus::DeliveredToWaiter
        );
    }

    #[test]
    fn expired_parent_lease_retries_and_completion_replays_receipt() {
        let gate_id = Uuid::new_v4();
        let target_run_id = Uuid::new_v4();
        let target_agent_id = Uuid::new_v4();
        let now = Utc::now();
        let first_token = Uuid::new_v4();
        let retry_token = Uuid::new_v4();
        let handoff_id = Uuid::new_v4();
        let mut state = GateResultDeliveryState::default();

        let first = state.claim_parent_continuation(
            ClaimGateResultParentContinuationRequest {
                gate_id,
                result_attempt: 2,
                target_run_id,
                target_agent_id,
                claim_token: first_token,
                claim_expires_at: now - Duration::seconds(1),
            },
            now - Duration::minutes(1),
        );
        assert!(first.claimed());

        let retry = state.claim_parent_continuation(
            ClaimGateResultParentContinuationRequest {
                gate_id,
                result_attempt: 2,
                target_run_id,
                target_agent_id,
                claim_token: retry_token,
                claim_expires_at: now + Duration::minutes(1),
            },
            now,
        );
        assert!(retry.claimed());
        assert_eq!(retry.marker().claim_token, Some(retry_token));

        let completed = state
            .complete_parent_continuation(
                CompleteGateResultParentContinuationRequest {
                    gate_id,
                    result_attempt: 2,
                    claim_token: retry_token,
                    input_handoff_id: Some(handoff_id),
                    accepted_operation_id: Some("operation-2".to_string()),
                    dispatched_to_parent: true,
                },
                now,
            )
            .unwrap();
        assert_eq!(
            completed.status,
            GateResultDeliveryStatus::DispatchedToParent
        );

        let replay = state
            .complete_parent_continuation(
                CompleteGateResultParentContinuationRequest {
                    gate_id,
                    result_attempt: 2,
                    claim_token: retry_token,
                    input_handoff_id: Some(Uuid::new_v4()),
                    accepted_operation_id: Some("other-operation".to_string()),
                    dispatched_to_parent: false,
                },
                now + Duration::seconds(1),
            )
            .unwrap();
        assert_eq!(replay.input_handoff_id, Some(handoff_id));
        assert_eq!(replay.accepted_operation_id.as_deref(), Some("operation-2"));
        assert_eq!(replay.status, GateResultDeliveryStatus::DispatchedToParent);
    }
}
