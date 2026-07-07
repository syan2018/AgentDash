use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::common::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualContextCompactionRequestStatus {
    Requested,
    Consumed,
    Completed,
    Noop,
    Failed,
}

impl ManualContextCompactionRequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Consumed => "consumed",
            Self::Completed => "completed",
            Self::Noop => "noop",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for ManualContextCompactionRequestStatus {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "requested" => Ok(Self::Requested),
            "consumed" => Ok(Self::Consumed),
            "completed" => Ok(Self::Completed),
            "noop" => Ok(Self::Noop),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::InvalidConfig(format!(
                "runtime_session_compaction_requests.status 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualContextCompactionRequestedMode {
    NextTurn,
    CompactOnly,
}

impl ManualContextCompactionRequestedMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NextTurn => "next_turn",
            Self::CompactOnly => "compact_only",
        }
    }
}

impl TryFrom<&str> for ManualContextCompactionRequestedMode {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "next_turn" => Ok(Self::NextTurn),
            "compact_only" => Ok(Self::CompactOnly),
            other => Err(DomainError::InvalidConfig(format!(
                "runtime_session_compaction_requests.requested_mode 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManualContextCompactionRequest {
    pub id: Uuid,
    pub session_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub command_receipt_id: Uuid,
    pub status: ManualContextCompactionRequestStatus,
    pub requested_mode: ManualContextCompactionRequestedMode,
    pub keep_last_n: Option<i32>,
    pub reserve_tokens: Option<i32>,
    pub request_metadata: Option<Value>,
    pub result_metadata: Option<Value>,
    pub requested_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub consumed_turn_id: Option<String>,
    pub completed_compaction_id: Option<String>,
    pub compacted_until_ref: Option<Value>,
    pub first_kept_ref: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewManualContextCompactionRequest {
    pub session_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub command_receipt_id: Uuid,
    pub requested_mode: ManualContextCompactionRequestedMode,
    pub keep_last_n: Option<i32>,
    pub reserve_tokens: Option<i32>,
    pub request_metadata: Option<Value>,
}

#[async_trait::async_trait]
pub trait ManualContextCompactionRequestRepository: Send + Sync {
    async fn create_requested(
        &self,
        request: NewManualContextCompactionRequest,
    ) -> Result<ManualContextCompactionRequest, DomainError>;

    async fn get_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<ManualContextCompactionRequest>, DomainError>;

    async fn get_by_command_receipt(
        &self,
        command_receipt_id: Uuid,
    ) -> Result<Option<ManualContextCompactionRequest>, DomainError>;

    async fn find_requested_by_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ManualContextCompactionRequest>, DomainError>;

    async fn mark_consumed(
        &self,
        id: Uuid,
        turn_id: String,
    ) -> Result<ManualContextCompactionRequest, DomainError>;

    async fn mark_completed(
        &self,
        id: Uuid,
        compaction_id: String,
        compacted_until_ref: Option<Value>,
        first_kept_ref: Option<Value>,
        result_metadata: Option<Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError>;

    async fn mark_noop(
        &self,
        id: Uuid,
        result_metadata: Option<Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError>;

    async fn mark_failed(
        &self,
        id: Uuid,
        result_metadata: Option<Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::{ManualContextCompactionRequestStatus, ManualContextCompactionRequestedMode};

    #[test]
    fn manual_context_compaction_status_round_trips() {
        for status in [
            ManualContextCompactionRequestStatus::Requested,
            ManualContextCompactionRequestStatus::Consumed,
            ManualContextCompactionRequestStatus::Completed,
            ManualContextCompactionRequestStatus::Noop,
            ManualContextCompactionRequestStatus::Failed,
        ] {
            assert_eq!(
                ManualContextCompactionRequestStatus::try_from(status.as_str()).unwrap(),
                status
            );
        }
        assert!(ManualContextCompactionRequestStatus::try_from("done").is_err());
    }

    #[test]
    fn manual_context_compaction_requested_mode_round_trips() {
        assert_eq!(
            ManualContextCompactionRequestedMode::try_from("next_turn").unwrap(),
            ManualContextCompactionRequestedMode::NextTurn
        );
        assert_eq!(
            ManualContextCompactionRequestedMode::try_from("compact_only").unwrap(),
            ManualContextCompactionRequestedMode::CompactOnly
        );
        assert!(ManualContextCompactionRequestedMode::try_from("running").is_err());
    }
}
