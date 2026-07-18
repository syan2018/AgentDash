//! AgentDash-owned transport-neutral Runtime Wire frames.

mod complete_agent;

pub use complete_agent::*;

use agentdash_agent_runtime_contract::{
    DriverBindRequest, DriverBinding, DriverCommandEnvelope, DriverDescribeRequest,
    DriverDispatchReceipt, DriverError, DriverEventEnvelope, DriverInspection,
    DriverInspectionQuery, OperationReceipt, RuntimeCommandEnvelope, RuntimeDescriptor,
    RuntimeEventEnvelope, RuntimeEventSubscription, RuntimeExecuteError, RuntimeJournalRecord,
    RuntimeSnapshot, RuntimeSnapshotError, RuntimeSnapshotQuery, RuntimeSubscribeError,
};
use agentdash_integration_api::{
    DriverCompactionActivationRequest, DriverContextActivation, DriverContextCheckpointRequest,
    DriverHookDecision, DriverHookInvocation, DriverSurfaceRequest, DriverToolInvocation,
    DriverToolOutcome, DriverToolSurface, DriverTranscript, DriverTranscriptRequest,
    MaterializedDriverSurface,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

pub const RUNTIME_WIRE_PROTOCOL_REVISION: u32 = 3;

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
    Execute(RuntimeCommandEnvelope),
    Snapshot(RuntimeSnapshotQuery),
    Events(RuntimeEventSubscription),
    DriverDescribe(DriverDescribeRequest),
    DriverBind(DriverBindRequest),
    DriverDispatch(DriverCommandEnvelope),
    DriverInspect(DriverInspectionQuery),
    HostPort(Box<RuntimeWireHostPortRequest>),
    #[schemars(skip)]
    #[ts(skip)]
    AgentService(Box<RuntimeWireAgentServiceRequest>),
    #[schemars(skip)]
    #[ts(skip)]
    AgentHostCallback(Box<RuntimeWireAgentHostCallbackRequest>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "method", content = "result", rename_all = "snake_case")]
pub enum RuntimeWireResponse {
    Execute(RuntimeWireExecuteResult),
    Snapshot(RuntimeWireSnapshotResult),
    Events(RuntimeWireSubscribeResult),
    DriverDescribe(RuntimeWireDriverDescribeResult),
    DriverBind(RuntimeWireDriverBindResult),
    DriverDispatch(RuntimeWireDriverDispatchResult),
    DriverInspect(RuntimeWireDriverInspectResult),
    HostPort(RuntimeWireHostPortResponse),
    #[schemars(skip)]
    #[ts(skip)]
    AgentService(RuntimeWireAgentServiceResponse),
    #[schemars(skip)]
    #[ts(skip)]
    AgentHostCallback(RuntimeWireAgentHostCallbackResponse),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "port", content = "request", rename_all = "snake_case")]
pub enum RuntimeWireHostPortRequest {
    Transcript(DriverTranscriptRequest),
    SurfaceMaterialize(DriverSurfaceRequest),
    ToolSetMaterialize {
        binding_id: agentdash_agent_runtime_contract::RuntimeBindingId,
        revision: agentdash_agent_runtime_contract::ToolSetRevision,
        digest: String,
    },
    ContextCheckpoint(DriverContextCheckpointRequest),
    CompactionActivation(DriverCompactionActivationRequest),
    ToolInvoke(DriverToolInvocation),
    HookExecute(DriverHookInvocation),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireHostPortError {
    pub reason: String,
    pub retryable: bool,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "port", content = "result", rename_all = "snake_case")]
pub enum RuntimeWireHostPortResponse {
    Transcript(Result<Box<DriverTranscript>, RuntimeWireHostPortError>),
    SurfaceMaterialize(Result<Box<MaterializedDriverSurface>, RuntimeWireHostPortError>),
    ToolSetMaterialize(Result<Box<DriverToolSurface>, RuntimeWireHostPortError>),
    ContextCheckpoint(Result<Box<DriverContextActivation>, RuntimeWireHostPortError>),
    CompactionActivation(Result<Box<DriverContextActivation>, RuntimeWireHostPortError>),
    ToolInvoke(Result<Box<DriverToolOutcome>, RuntimeWireHostPortError>),
    HookExecute(Result<Box<DriverHookDecision>, RuntimeWireHostPortError>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum RuntimeWireExecuteResult {
    Ok(OperationReceipt),
    Error(RuntimeExecuteError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum RuntimeWireSnapshotResult {
    Ok(Box<RuntimeSnapshot>),
    Error(RuntimeSnapshotError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum RuntimeWireSubscribeResult {
    Ok { accepted_cursor: u64 },
    Error(RuntimeSubscribeError),
}

macro_rules! driver_result {
    ($name:ident, $value:ty) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
        #[serde(tag = "status", content = "value", rename_all = "snake_case")]
        pub enum $name {
            Ok(Box<$value>),
            Error(DriverError),
        }
    };
}

driver_result!(RuntimeWireDriverDescribeResult, RuntimeDescriptor);
driver_result!(RuntimeWireDriverBindResult, DriverBinding);
driver_result!(RuntimeWireDriverDispatchResult, DriverDispatchReceipt);
driver_result!(RuntimeWireDriverInspectResult, DriverInspection);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum RuntimeWireNotification {
    JournalFact(RuntimeJournalRecord),
    /// Runtime-only state/audit events. Session presentation consumers must use
    /// `journal_fact` and filter for `RuntimeJournalFact::Presentation`.
    RuntimeEvent(RuntimeEventEnvelope),
    DriverEvent(DriverEventEnvelope),
    #[schemars(skip)]
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
    #[error("runtime wire protocol revision {received} is unsupported")]
    UnsupportedRevision { received: u32, supported: u32 },
    #[error("unknown critical runtime wire frame: {frame_kind}")]
    UnknownCriticalFrame { frame_kind: String },
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
            message: "protocol_revision is required".to_string(),
        })? as u32;
    if revision != RUNTIME_WIRE_PROTOCOL_REVISION {
        return Err(RuntimeProtocolViolation::UnsupportedRevision {
            received: revision,
            supported: RUNTIME_WIRE_PROTOCOL_REVISION,
        });
    }

    let frame_kind = value
        .pointer("/frame/kind")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| RuntimeProtocolViolation::MalformedEnvelope {
            message: "frame.kind is required".to_string(),
        })?;
    let known = matches!(frame_kind, "request" | "response" | "notification" | "ack");
    if !known {
        if value
            .get("critical")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            return Err(RuntimeProtocolViolation::UnknownCriticalFrame {
                frame_kind: frame_kind.to_string(),
            });
        }
        return Ok(DecodedRuntimeWireFrame::IgnoredUnknown {
            kind: frame_kind.to_string(),
            frame_id: value
                .get("frame_id")
                .and_then(serde_json::Value::as_u64)
                .map(RuntimeWireFrameId),
        });
    }

    serde_json::from_value(value)
        .map(Box::new)
        .map(DecodedRuntimeWireFrame::Known)
        .map_err(|error| RuntimeProtocolViolation::MalformedEnvelope {
            message: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_critical_frame_is_a_protocol_violation() {
        let error = decode_frame(
            br#"{"protocol_revision":3,"frame_id":7,"critical":true,"frame":{"kind":"future_control","payload":{}}}"#,
        )
        .expect_err("critical frame must fail");
        assert!(matches!(
            error,
            RuntimeProtocolViolation::UnknownCriticalFrame { .. }
        ));
    }

    #[test]
    fn unknown_non_critical_frame_can_be_ignored() {
        let decoded = decode_frame(
            br#"{"protocol_revision":3,"frame_id":7,"critical":false,"frame":{"kind":"future_hint","payload":{}}}"#,
        )
        .expect("non-critical frame may be ignored");
        assert!(matches!(
            decoded,
            DecodedRuntimeWireFrame::IgnoredUnknown { .. }
        ));
    }

    #[test]
    fn concurrent_same_method_responses_keep_distinct_request_correlation() {
        fn response(frame_id: u64, request_frame_id: u64) -> RuntimeWireEnvelope {
            RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: RuntimeWireFrameId(frame_id),
                critical: true,
                frame: RuntimeWireFrame::Response {
                    request_frame_id: RuntimeWireFrameId(request_frame_id),
                    response: RuntimeWireResponse::Snapshot(RuntimeWireSnapshotResult::Error(
                        RuntimeSnapshotError::NotFound,
                    )),
                },
            }
        }

        let first = serde_json::to_value(response(101, 11)).expect("serialize first response");
        let second = serde_json::to_value(response(102, 12)).expect("serialize second response");
        assert_eq!(
            first
                .pointer("/frame/payload/request_frame_id")
                .and_then(serde_json::Value::as_u64),
            Some(11)
        );
        assert_eq!(
            second
                .pointer("/frame/payload/request_frame_id")
                .and_then(serde_json::Value::as_u64),
            Some(12)
        );
        assert_ne!(
            first.pointer("/frame/payload/request_frame_id"),
            second.pointer("/frame/payload/request_frame_id")
        );

        let schema = schemars::schema_for!(RuntimeWireEnvelope);
        let schema = serde_json::to_value(schema).expect("serialize Runtime Wire schema");
        assert!(schema.to_string().contains("request_frame_id"));
    }

    #[test]
    fn journal_fact_wire_schema_keeps_the_typed_backbone_union() {
        let schema = schemars::schema_for!(RuntimeWireEnvelope);
        let schema = serde_json::to_value(schema).expect("serialize Runtime Wire schema");
        let schema = schema.to_string();
        assert!(schema.contains("journal_fact"));
        assert!(schema.contains("BackboneEvent"));
        assert!(schema.contains("item_completed"));
        assert!(schema.contains("platform"));
    }
}
