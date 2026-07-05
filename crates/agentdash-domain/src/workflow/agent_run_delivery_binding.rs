use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::runtime_session_anchor::RuntimeSessionExecutionAnchor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryBindingStatus {
    Ready,
    Running,
    Terminal,
    Lost,
    FrameMissing,
    DeliveryMissing,
}

impl DeliveryBindingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeliveryBindingStatus::Ready => "ready",
            DeliveryBindingStatus::Running => "running",
            DeliveryBindingStatus::Terminal => "terminal",
            DeliveryBindingStatus::Lost => "lost",
            DeliveryBindingStatus::FrameMissing => "frame_missing",
            DeliveryBindingStatus::DeliveryMissing => "delivery_missing",
        }
    }
}

impl std::fmt::Display for DeliveryBindingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryBindingStatusParseError;

impl std::fmt::Display for DeliveryBindingStatusParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid delivery binding status")
    }
}

impl std::error::Error for DeliveryBindingStatusParseError {}

impl FromStr for DeliveryBindingStatus {
    type Err = DeliveryBindingStatusParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ready" => Ok(DeliveryBindingStatus::Ready),
            "running" => Ok(DeliveryBindingStatus::Running),
            "terminal" => Ok(DeliveryBindingStatus::Terminal),
            "lost" => Ok(DeliveryBindingStatus::Lost),
            "frame_missing" => Ok(DeliveryBindingStatus::FrameMissing),
            "delivery_missing" => Ok(DeliveryBindingStatus::DeliveryMissing),
            _ => Err(DeliveryBindingStatusParseError),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunDeliveryBinding {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: String,
    pub launch_frame_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestration_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_attempt: Option<u32>,
    pub status: DeliveryBindingStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_message: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentRunDeliveryBinding {
    pub fn from_anchor(
        anchor: &RuntimeSessionExecutionAnchor,
        status: DeliveryBindingStatus,
        observed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            run_id: anchor.run_id,
            agent_id: anchor.agent_id,
            runtime_session_id: anchor.runtime_session_id.clone(),
            launch_frame_id: anchor.launch_frame_id,
            orchestration_id: anchor.orchestration_id,
            node_path: anchor.node_path.clone(),
            node_attempt: anchor.node_attempt,
            status,
            active_turn_id: None,
            last_turn_id: None,
            terminal_state: None,
            terminal_message: None,
            observed_at,
            updated_at: observed_at,
        }
    }

    pub fn mark_running(mut self, turn_id: impl Into<String>, updated_at: DateTime<Utc>) -> Self {
        let turn_id = turn_id.into();
        self.status = DeliveryBindingStatus::Running;
        self.active_turn_id = Some(turn_id.clone());
        self.last_turn_id = Some(turn_id);
        self.terminal_state = None;
        self.terminal_message = None;
        self.updated_at = updated_at;
        self
    }

    pub fn mark_terminal(
        mut self,
        turn_id: impl Into<String>,
        terminal_state: impl Into<String>,
        terminal_message: Option<String>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        self.status = DeliveryBindingStatus::Terminal;
        self.active_turn_id = None;
        self.last_turn_id = Some(turn_id.into());
        self.terminal_state = Some(terminal_state.into());
        self.terminal_message = terminal_message;
        self.updated_at = updated_at;
        self
    }

    pub fn with_updated_at(mut self, updated_at: DateTime<Utc>) -> Self {
        self.updated_at = updated_at;
        self
    }
}

#[cfg(test)]
mod agent_run_delivery_binding_tests {
    use super::*;

    #[test]
    fn delivery_binding_status_slug_round_trips() {
        for status in [
            DeliveryBindingStatus::Ready,
            DeliveryBindingStatus::Running,
            DeliveryBindingStatus::Terminal,
            DeliveryBindingStatus::Lost,
            DeliveryBindingStatus::FrameMissing,
            DeliveryBindingStatus::DeliveryMissing,
        ] {
            assert_eq!(
                DeliveryBindingStatus::from_str(status.as_str()).unwrap(),
                status
            );
        }
        assert!(DeliveryBindingStatus::from_str("canceling").is_err());
    }

    #[test]
    fn binding_from_anchor_preserves_launch_evidence() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let launch_frame_id = Uuid::new_v4();
        let orchestration_id = Uuid::new_v4();
        let observed_at = Utc::now();

        let anchor = RuntimeSessionExecutionAnchor::new_orchestration_dispatch(
            "runtime-a",
            run_id,
            launch_frame_id,
            agent_id,
            orchestration_id,
            "root.plan",
            2,
        );

        let binding = AgentRunDeliveryBinding::from_anchor(
            &anchor,
            DeliveryBindingStatus::Ready,
            observed_at,
        );

        assert_eq!(binding.run_id, run_id);
        assert_eq!(binding.agent_id, agent_id);
        assert_eq!(binding.runtime_session_id, "runtime-a");
        assert_eq!(binding.launch_frame_id, launch_frame_id);
        assert_eq!(binding.orchestration_id, Some(orchestration_id));
        assert_eq!(binding.node_path.as_deref(), Some("root.plan"));
        assert_eq!(binding.node_attempt, Some(2));
        assert_eq!(binding.status, DeliveryBindingStatus::Ready);
        assert_eq!(binding.active_turn_id, None);
        assert_eq!(binding.last_turn_id, None);
        assert_eq!(binding.terminal_state, None);
        assert_eq!(binding.terminal_message, None);
        assert_eq!(binding.observed_at, observed_at);
        assert_eq!(binding.updated_at, observed_at);
    }

    #[test]
    fn binding_running_and_terminal_state_transitions_are_owned_by_binding() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let launch_frame_id = Uuid::new_v4();
        let observed_at = Utc::now();
        let running_at = observed_at + chrono::TimeDelta::seconds(1);
        let terminal_at = running_at + chrono::TimeDelta::seconds(1);
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-a",
            run_id,
            launch_frame_id,
            agent_id,
        );

        let binding = AgentRunDeliveryBinding::from_anchor(
            &anchor,
            DeliveryBindingStatus::Ready,
            observed_at,
        )
        .mark_running("turn-1", running_at);

        assert_eq!(binding.status, DeliveryBindingStatus::Running);
        assert_eq!(binding.active_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(binding.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(binding.updated_at, running_at);

        let binding = binding.mark_terminal(
            "turn-1",
            "failed",
            Some("provider failed".to_string()),
            terminal_at,
        );

        assert_eq!(binding.status, DeliveryBindingStatus::Terminal);
        assert_eq!(binding.active_turn_id, None);
        assert_eq!(binding.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(binding.terminal_state.as_deref(), Some("failed"));
        assert_eq!(binding.terminal_message.as_deref(), Some("provider failed"));
        assert_eq!(binding.updated_at, terminal_at);
    }
}
