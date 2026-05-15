use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::types::PendingCapabilityStateTransition;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCommandStatus {
    Pending,
    Applied,
    Failed,
}

impl RuntimeCommandStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Applied => "applied",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for RuntimeCommandStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" => Ok(Self::Pending),
            "applied" => Ok(Self::Applied),
            "failed" => Ok(Self::Failed),
            other => Err(format!("unknown runtime command status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PendingRuntimeCommandRecord {
    pub id: Uuid,
    pub session_id: String,
    pub transition_id: String,
    pub phase_node: String,
    pub status: RuntimeCommandStatus,
    pub transition: PendingCapabilityStateTransition,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub applied_at_ms: Option<i64>,
    pub failed_at_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_command_status_round_trips_wire_values() {
        assert_eq!(
            RuntimeCommandStatus::try_from("pending"),
            Ok(RuntimeCommandStatus::Pending)
        );
        assert_eq!(RuntimeCommandStatus::Applied.as_str(), "applied");
        assert!(RuntimeCommandStatus::try_from("unknown").is_err());
    }
}
