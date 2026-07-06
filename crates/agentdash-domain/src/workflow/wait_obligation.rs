use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitSourceDeclaration {
    #[serde(flatten)]
    pub producer: WaitProducerRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitExpectedResultDeclaration {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitProducerTerminalPolicy {
    pub failed: String,
    pub interrupted: String,
    pub completed: String,
}

impl WaitProducerTerminalPolicy {
    pub fn result_for_terminal_state(&self, terminal_state: &str) -> &str {
        match terminal_state {
            "completed" => &self.completed,
            "interrupted" | "cancelled" => &self.interrupted,
            _ => &self.failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitWakeDeclaration {
    pub namespace: String,
    pub target_run_id: Uuid,
    pub target_agent_id: Uuid,
    pub client_command_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitObligationDeclaration {
    pub wait_source: WaitSourceDeclaration,
    pub expected_result: WaitExpectedResultDeclaration,
    pub on_producer_terminal_without_result: WaitProducerTerminalPolicy,
    pub wake: WaitWakeDeclaration,
}

impl WaitObligationDeclaration {
    pub fn companion_agent_run_delivery(
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Option<Uuid>,
        correlation_ref: impl Into<String>,
        target_run_id: Uuid,
        target_agent_id: Uuid,
        gate_id: Uuid,
    ) -> Self {
        let correlation_ref = correlation_ref.into();
        Self {
            wait_source: WaitSourceDeclaration {
                producer: WaitProducerRef::AgentRunDelivery {
                    run_id,
                    agent_id,
                    frame_id,
                },
                correlation_ref: Some(correlation_ref.clone()),
            },
            expected_result: WaitExpectedResultDeclaration {
                kind: "companion_result".to_string(),
                correlation_ref: Some(correlation_ref),
            },
            on_producer_terminal_without_result: WaitProducerTerminalPolicy {
                failed: "failed".to_string(),
                interrupted: "cancelled".to_string(),
                completed: "protocol_failed".to_string(),
            },
            wake: WaitWakeDeclaration {
                namespace: "companion".to_string(),
                target_run_id,
                target_agent_id,
                client_command_id: format!("companion-result:{gate_id}"),
            },
        }
    }

    pub fn from_payload(payload: &Value) -> Option<Self> {
        serde_json::from_value(payload.clone()).ok()
    }

    pub fn write_into_payload(self, payload: Option<Value>) -> Result<Value, serde_json::Error> {
        let mut object = match payload {
            Some(Value::Object(object)) => object,
            Some(other) => {
                let mut object = Map::new();
                object.insert("payload".to_string(), other);
                object
            }
            None => Map::new(),
        };
        let declaration = serde_json::to_value(self)?;
        if let Value::Object(declaration) = declaration {
            for (key, value) in declaration {
                object.insert(key, value);
            }
        }
        Ok(Value::Object(object))
    }
}
