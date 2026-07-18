//! Canonical cross-process framing for Managed Runtime and Complete Agent traffic.

mod complete_agent;

pub use complete_agent::*;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeChangePage, ManagedRuntimeChangesRequest, ManagedRuntimeCommandEnvelope,
    ManagedRuntimeGatewayError, ManagedRuntimeOperationReceipt, ManagedRuntimePlatformChange,
    ManagedRuntimeReadRequest, ManagedRuntimeSnapshot,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

pub const RUNTIME_WIRE_PROTOCOL_REVISION: u32 = 4;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct RuntimeWireFrameId(pub u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireEnvelope {
    pub protocol_revision: u32,
    pub frame_id: RuntimeWireFrameId,
    pub critical: bool,
    pub frame: RuntimeWireFrame,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum RuntimeWireFrame {
    Request(Box<RuntimeWireRequest>),
    Response {
        request_frame_id: RuntimeWireFrameId,
        response: RuntimeWireResponse,
    },
    Notification(Box<RuntimeWireNotification>),
    Ack(RuntimeWireAck),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum RuntimeWireRequest {
    RuntimeExecute(ManagedRuntimeCommandEnvelope),
    RuntimeRead(ManagedRuntimeReadRequest),
    RuntimeChanges(ManagedRuntimeChangesRequest),
    #[ts(skip)]
    AgentService(Box<RuntimeWireAgentServiceRequest>),
    #[ts(skip)]
    AgentHostCallback(Box<RuntimeWireAgentHostCallbackRequest>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "method", content = "result", rename_all = "snake_case")]
pub enum RuntimeWireResponse {
    RuntimeExecute(RuntimeWireExecuteResult),
    RuntimeRead(RuntimeWireReadResult),
    RuntimeChanges(RuntimeWireChangesResult),
    #[ts(skip)]
    AgentService(RuntimeWireAgentServiceResponse),
    #[ts(skip)]
    AgentHostCallback(RuntimeWireAgentHostCallbackResponse),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum RuntimeWireExecuteResult {
    Ok(ManagedRuntimeOperationReceipt),
    Error(ManagedRuntimeGatewayError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum RuntimeWireReadResult {
    Ok(Box<ManagedRuntimeSnapshot>),
    Error(ManagedRuntimeGatewayError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum RuntimeWireChangesResult {
    Ok(Box<ManagedRuntimeChangePage>),
    Error(ManagedRuntimeGatewayError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum RuntimeWireNotification {
    RuntimeChange(Box<ManagedRuntimePlatformChange>),
    #[ts(skip)]
    AgentChange(Box<RuntimeWireAgentChangeNotification>),
    Heartbeat {
        last_received_frame_id: RuntimeWireFrameId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireAck {
    pub through_frame_id: RuntimeWireFrameId,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DecodedRuntimeWireFrame {
    Known(Box<RuntimeWireEnvelope>),
    IgnoredUnknown {
        kind: String,
        frame_id: Option<RuntimeWireFrameId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeProtocolViolation {
    #[error("runtime wire frame is not valid JSON: {message}")]
    MalformedJson { message: String },
    #[error("runtime wire envelope is malformed: {message}")]
    MalformedEnvelope { message: String },
    #[error("unknown critical runtime wire frame: {frame_kind}")]
    UnknownCriticalFrame { frame_kind: String },
    #[error("runtime wire protocol revision {received} is unsupported")]
    UnsupportedRevision { received: u32, supported: u32 },
}

pub fn decode_frame(bytes: &[u8]) -> Result<DecodedRuntimeWireFrame, RuntimeProtocolViolation> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| RuntimeProtocolViolation::MalformedJson {
            message: error.to_string(),
        })?;
    let revision = value
        .get("protocol_revision")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| RuntimeProtocolViolation::MalformedEnvelope {
            message: "protocol_revision is required".to_owned(),
        })? as u32;
    if revision != RUNTIME_WIRE_PROTOCOL_REVISION {
        return Err(RuntimeProtocolViolation::UnsupportedRevision {
            received: revision,
            supported: RUNTIME_WIRE_PROTOCOL_REVISION,
        });
    }

    match serde_json::from_value(value.clone()) {
        Ok(envelope) => Ok(DecodedRuntimeWireFrame::Known(Box::new(envelope))),
        Err(_error) => {
            let kind = value
                .pointer("/frame/kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            let critical = value
                .get("critical")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            if critical {
                return Err(RuntimeProtocolViolation::UnknownCriticalFrame { frame_kind: kind });
            }
            Ok(DecodedRuntimeWireFrame::IgnoredUnknown {
                kind,
                frame_id: value
                    .get("frame_id")
                    .and_then(serde_json::Value::as_u64)
                    .map(RuntimeWireFrameId),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_four_is_the_only_accepted_revision() {
        let bytes = serde_json::to_vec(&RuntimeWireEnvelope {
            protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
            frame_id: RuntimeWireFrameId(1),
            critical: true,
            frame: RuntimeWireFrame::Ack(RuntimeWireAck {
                through_frame_id: RuntimeWireFrameId(0),
            }),
        })
        .expect("encode frame");
        assert!(matches!(
            decode_frame(&bytes),
            Ok(DecodedRuntimeWireFrame::Known(_))
        ));

        let mut old: serde_json::Value = serde_json::from_slice(&bytes).expect("decode test frame");
        old["protocol_revision"] = serde_json::json!(3);
        assert_eq!(
            decode_frame(&serde_json::to_vec(&old).expect("encode old frame")),
            Err(RuntimeProtocolViolation::UnsupportedRevision {
                received: 3,
                supported: 4,
            })
        );
    }

    #[test]
    fn schema_contains_runtime_and_complete_agent_business_frames() {
        let schema = schemars::schema_for!(RuntimeWireEnvelope);
        let schema = serde_json::to_string(&schema).expect("serialize wire schema");
        for family in [
            "runtime_execute",
            "runtime_read",
            "runtime_changes",
            "agent_service",
            "agent_host_callback",
            "agent_change",
            "runtime_change",
        ] {
            assert!(schema.contains(family), "missing {family}");
        }
        assert!(!schema.contains("driver_dispatch"));
        assert!(!schema.contains("journal_fact"));
        assert!(!schema.contains("host_port"));
    }
}
