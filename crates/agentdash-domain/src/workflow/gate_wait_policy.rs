use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;
use uuid::Uuid;

pub const GATE_WAIT_POLICY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateWaitPolicyJsonPaths;

impl GateWaitPolicyJsonPaths {
    pub const SCHEMA_VERSION: &'static str = "schema_version";
    pub const WAIT_POLICY: &'static str = "wait_policy";
    pub const SOURCE: &'static str = "source";
    pub const EXPECTED_RESULT: &'static str = "expected_result";
    pub const TERMINAL_POLICY: &'static str = "terminal_policy";
    pub const WAKE_TARGET: &'static str = "wake_target";
    pub const KIND: &'static str = "kind";
    pub const RUN_ID: &'static str = "run_id";
    pub const AGENT_ID: &'static str = "agent_id";
    pub const FRAME_ID: &'static str = "frame_id";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WaitProducerRef {
    AgentRunDelivery {
        run_id: Uuid,
        agent_id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frame_id: Option<Uuid>,
    },
}

impl WaitProducerRef {
    pub fn kind(&self) -> &'static str {
        match self {
            WaitProducerRef::AgentRunDelivery { .. } => "agent_run_delivery",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitExpectedResult {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitTerminalOutcome {
    pub status: String,
    pub failure_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitTerminalPolicy {
    pub failed: WaitTerminalOutcome,
    pub interrupted: WaitTerminalOutcome,
    pub completed: WaitTerminalOutcome,
}

impl WaitTerminalPolicy {
    pub fn outcome_for_terminal_state(&self, terminal_state: &str) -> &WaitTerminalOutcome {
        match terminal_state {
            "completed" => &self.completed,
            "interrupted" | "cancelled" => &self.interrupted,
            _ => &self.failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitWakeTarget {
    pub namespace: String,
    pub target_run_id: Uuid,
    pub target_agent_id: Uuid,
    pub client_command_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateWaitPolicyTemplate {
    pub expected_result: WaitExpectedResult,
    pub terminal_policy: WaitTerminalPolicy,
    pub wake_target: WaitWakeTarget,
}

impl GateWaitPolicyTemplate {
    pub fn into_agent_run_delivery_policy(
        self,
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Option<Uuid>,
        gate_id: Uuid,
    ) -> GateWaitPolicy {
        let mut wake_target = self.wake_target;
        wake_target.client_command_id = wake_target
            .client_command_id
            .replace("{gate_id}", &gate_id.to_string());
        GateWaitPolicy {
            source: WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id,
                frame_id,
            },
            expected_result: self.expected_result,
            terminal_policy: self.terminal_policy,
            wake_target,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateWaitPolicy {
    pub source: WaitProducerRef,
    pub expected_result: WaitExpectedResult,
    pub terminal_policy: WaitTerminalPolicy,
    pub wake_target: WaitWakeTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateWaitPolicyEnvelope {
    pub schema_version: u32,
    pub wait_policy: GateWaitPolicy,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub display: Map<String, Value>,
}

impl GateWaitPolicyEnvelope {
    pub fn new(wait_policy: GateWaitPolicy) -> Self {
        Self {
            schema_version: GATE_WAIT_POLICY_SCHEMA_VERSION,
            wait_policy,
            display: Map::new(),
        }
    }

    pub fn with_display_value(mut self, key: impl Into<String>, value: Value) -> Self {
        if !value.is_null() {
            self.display.insert(key.into(), value);
        }
        self
    }

    pub fn from_payload(payload: &Value) -> Result<Self, GateWaitPolicyPayloadError> {
        let envelope: Self = serde_json::from_value(payload.clone()).map_err(|error| {
            GateWaitPolicyPayloadError::InvalidJson {
                reason: error.to_string(),
            }
        })?;
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn from_payload_opt(payload: &Value) -> Option<Self> {
        Self::from_payload(payload).ok()
    }

    pub fn write_into_payload(
        mut self,
        payload: Option<Value>,
    ) -> Result<Value, serde_json::Error> {
        self.merge_existing_display(payload);
        serde_json::to_value(self)
    }

    pub fn validate(&self) -> Result<(), GateWaitPolicyPayloadError> {
        if self.schema_version != GATE_WAIT_POLICY_SCHEMA_VERSION {
            return Err(GateWaitPolicyPayloadError::UnsupportedSchemaVersion {
                found: self.schema_version,
                expected: GATE_WAIT_POLICY_SCHEMA_VERSION,
            });
        }
        if self.wait_policy.expected_result.kind.trim().is_empty() {
            return Err(GateWaitPolicyPayloadError::MissingField {
                path: "wait_policy.expected_result.kind",
            });
        }
        if self.wait_policy.wake_target.namespace.trim().is_empty() {
            return Err(GateWaitPolicyPayloadError::MissingField {
                path: "wait_policy.wake_target.namespace",
            });
        }
        if self
            .wait_policy
            .wake_target
            .client_command_id
            .trim()
            .is_empty()
        {
            return Err(GateWaitPolicyPayloadError::MissingField {
                path: "wait_policy.wake_target.client_command_id",
            });
        }
        validate_terminal_outcome(
            "wait_policy.terminal_policy.failed",
            &self.wait_policy.terminal_policy.failed,
        )?;
        validate_terminal_outcome(
            "wait_policy.terminal_policy.interrupted",
            &self.wait_policy.terminal_policy.interrupted,
        )?;
        validate_terminal_outcome(
            "wait_policy.terminal_policy.completed",
            &self.wait_policy.terminal_policy.completed,
        )?;
        Ok(())
    }

    fn merge_existing_display(&mut self, payload: Option<Value>) {
        match payload {
            Some(Value::Object(object)) => {
                for (key, value) in object {
                    if key != GateWaitPolicyJsonPaths::SCHEMA_VERSION
                        && key != GateWaitPolicyJsonPaths::WAIT_POLICY
                        && key != "display"
                    {
                        self.display.entry(key).or_insert(value);
                    }
                }
            }
            Some(other) => {
                self.display.entry("payload".to_string()).or_insert(other);
            }
            None => {}
        }
    }

    pub fn json_paths() -> GateWaitPolicyPathNames {
        GateWaitPolicyPathNames {
            schema_version: GateWaitPolicyJsonPaths::SCHEMA_VERSION,
            wait_policy: GateWaitPolicyJsonPaths::WAIT_POLICY,
            source: GateWaitPolicyJsonPaths::SOURCE,
            expected_result: GateWaitPolicyJsonPaths::EXPECTED_RESULT,
            terminal_policy: GateWaitPolicyJsonPaths::TERMINAL_POLICY,
            wake_target: GateWaitPolicyJsonPaths::WAKE_TARGET,
            kind: GateWaitPolicyJsonPaths::KIND,
            run_id: GateWaitPolicyJsonPaths::RUN_ID,
            agent_id: GateWaitPolicyJsonPaths::AGENT_ID,
            frame_id: GateWaitPolicyJsonPaths::FRAME_ID,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateWaitPolicyPathNames {
    pub schema_version: &'static str,
    pub wait_policy: &'static str,
    pub source: &'static str,
    pub expected_result: &'static str,
    pub terminal_policy: &'static str,
    pub wake_target: &'static str,
    pub kind: &'static str,
    pub run_id: &'static str,
    pub agent_id: &'static str,
    pub frame_id: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GateWaitPolicyPayloadError {
    #[error("invalid gate wait policy payload json: {reason}")]
    InvalidJson { reason: String },
    #[error("unsupported gate wait policy schema_version {found}, expected {expected}")]
    UnsupportedSchemaVersion { found: u32, expected: u32 },
    #[error("missing gate wait policy field {path}")]
    MissingField { path: &'static str },
}

fn validate_terminal_outcome(
    path: &'static str,
    outcome: &WaitTerminalOutcome,
) -> Result<(), GateWaitPolicyPayloadError> {
    if outcome.status.trim().is_empty() {
        return Err(GateWaitPolicyPayloadError::MissingField { path });
    }
    if outcome.failure_kind.trim().is_empty() {
        return Err(GateWaitPolicyPayloadError::MissingField { path });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn envelope() -> GateWaitPolicyEnvelope {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        GateWaitPolicyEnvelope::new(GateWaitPolicy {
            source: WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id,
                frame_id: Some(frame_id),
            },
            expected_result: WaitExpectedResult {
                kind: "companion_result".to_string(),
                correlation_ref: Some("dispatch-1".to_string()),
            },
            terminal_policy: WaitTerminalPolicy {
                failed: WaitTerminalOutcome {
                    status: "failed".to_string(),
                    failure_kind: "producer_failed".to_string(),
                },
                interrupted: WaitTerminalOutcome {
                    status: "cancelled".to_string(),
                    failure_kind: "producer_interrupted".to_string(),
                },
                completed: WaitTerminalOutcome {
                    status: "failed".to_string(),
                    failure_kind: "missing_companion_respond".to_string(),
                },
            },
            wake_target: WaitWakeTarget {
                namespace: "companion".to_string(),
                target_run_id: run_id,
                target_agent_id: parent_agent_id,
                client_command_id: "companion-result:gate-1".to_string(),
            },
        })
        .with_display_value("companion_label", json!("reviewer"))
    }

    #[test]
    fn gate_wait_policy_envelope_serializes_and_parses() {
        let payload = envelope()
            .write_into_payload(Some(json!({ "preview": "review requested" })))
            .expect("payload");

        assert_eq!(payload["schema_version"], json!(1));
        assert_eq!(
            payload["wait_policy"]["source"]["kind"],
            json!("agent_run_delivery")
        );
        assert_eq!(payload["display"]["companion_label"], json!("reviewer"));
        assert_eq!(payload["display"]["preview"], json!("review requested"));

        let parsed = GateWaitPolicyEnvelope::from_payload(&payload).expect("typed envelope");
        assert_eq!(parsed.schema_version, GATE_WAIT_POLICY_SCHEMA_VERSION);
        assert_eq!(parsed.display["preview"], json!("review requested"));
        assert_eq!(
            parsed
                .wait_policy
                .terminal_policy
                .outcome_for_terminal_state("completed")
                .failure_kind,
            "missing_companion_respond"
        );
    }

    #[test]
    fn gate_wait_policy_envelope_reports_invalid_schema_version() {
        let mut payload = envelope().write_into_payload(None).expect("payload");
        payload["schema_version"] = json!(2);

        let error = GateWaitPolicyEnvelope::from_payload(&payload).expect_err("invalid schema");
        assert_eq!(
            error,
            GateWaitPolicyPayloadError::UnsupportedSchemaVersion {
                found: 2,
                expected: GATE_WAIT_POLICY_SCHEMA_VERSION,
            }
        );
    }

    #[test]
    fn gate_wait_policy_envelope_reports_missing_required_field() {
        let mut payload = envelope().write_into_payload(None).expect("payload");
        payload["wait_policy"]["wake_target"]["client_command_id"] = json!("");

        let error = GateWaitPolicyEnvelope::from_payload(&payload).expect_err("invalid payload");
        assert_eq!(
            error,
            GateWaitPolicyPayloadError::MissingField {
                path: "wait_policy.wake_target.client_command_id",
            }
        );
    }
}
