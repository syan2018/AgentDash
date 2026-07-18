use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentChange, AgentChangePage, AgentChangesQuery, AgentCommandEnvelope,
    AgentCommandReceipt, AgentEffectIdentity, AgentEffectInspection, AgentHookDecision,
    AgentHookInvocation, AgentHostCallbackError, AgentReadQuery, AgentServiceDescriptor,
    AgentServiceError, AgentServiceInstanceId, AgentSnapshot, AgentSourceCoordinate,
    AgentToolInvocation, AgentToolResult, AppliedAgentSurfaceReceipt, ApplyBoundAgentSurface,
    CreateAgentCommand, ForkAgentCommand, ForkAgentReceipt, ResumeAgentCommand,
    RevokeBoundAgentSurface,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Schema root for the Complete Agent transport vocabulary.
///
/// Complete Agent frames remain independently schema-checkable while the production-generated
/// Runtime Wire bindings continue to describe the active Driver route. The hard cut can make this
/// closure canonical without inventing parallel DTOs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireCompleteAgentSchema {
    pub request: RuntimeWireAgentServiceRequest,
    pub response: RuntimeWireAgentServiceResponse,
    pub change: RuntimeWireAgentChangeNotification,
    pub callback_request: RuntimeWireAgentHostCallbackRequest,
    pub callback_response: RuntimeWireAgentHostCallbackResponse,
}

/// Remote Complete Agent binding selected by the Host.
///
/// The instance identity routes the frame to one registered service. The generation fences every
/// binding-scoped request and notification; it must agree with the generation carried by command
/// or callback metadata when that metadata is present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireAgentBindingTarget {
    pub service_instance_id: AgentServiceInstanceId,
    pub binding_generation: AgentBindingGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireAgentServiceDescribeRequest {
    pub service_instance_id: AgentServiceInstanceId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", content = "request", rename_all = "snake_case")]
pub enum RuntimeWireAgentServiceRequest {
    Describe(RuntimeWireAgentServiceDescribeRequest),
    Create {
        target: RuntimeWireAgentBindingTarget,
        command: CreateAgentCommand,
    },
    Resume {
        target: RuntimeWireAgentBindingTarget,
        command: ResumeAgentCommand,
    },
    Fork {
        target: RuntimeWireAgentBindingTarget,
        command: ForkAgentCommand,
    },
    Execute {
        target: RuntimeWireAgentBindingTarget,
        command: AgentCommandEnvelope,
    },
    Read {
        target: RuntimeWireAgentBindingTarget,
        query: AgentReadQuery,
    },
    Changes {
        target: RuntimeWireAgentBindingTarget,
        query: AgentChangesQuery,
    },
    Inspect {
        target: RuntimeWireAgentBindingTarget,
        effect_id: AgentEffectIdentity,
    },
    ApplySurface {
        target: RuntimeWireAgentBindingTarget,
        command: ApplyBoundAgentSurface,
    },
    RevokeSurface {
        target: RuntimeWireAgentBindingTarget,
        command: RevokeBoundAgentSurface,
    },
}

impl RuntimeWireAgentServiceRequest {
    /// Rejects a frame whose routing fence disagrees with the typed command fence.
    ///
    /// Read, changes, and inspect do not carry command metadata, so their target is the sole
    /// generation authority. Describe is instance-scoped and intentionally precedes binding.
    pub fn validate_generation(&self) -> Result<(), RuntimeWireGenerationFenceError> {
        let (target, carried) = match self {
            Self::Describe(_) | Self::Read { .. } | Self::Changes { .. } | Self::Inspect { .. } => {
                return Ok(());
            }
            Self::Create { target, command } => (target, command.meta.binding_generation),
            Self::Resume { target, command } => (target, command.meta.binding_generation),
            Self::Fork { target, command } => (target, command.meta.binding_generation),
            Self::Execute { target, command } => (target, command.meta.binding_generation),
            Self::ApplySurface { target, command } => {
                (target, command.callbacks.binding_generation)
            }
            Self::RevokeSurface { target, command } => (target, command.binding_generation),
        };
        if target.binding_generation != carried {
            return Err(RuntimeWireGenerationFenceError {
                expected: target.binding_generation,
                received: carried,
            });
        }
        Ok(())
    }

    pub fn target(&self) -> Option<&RuntimeWireAgentBindingTarget> {
        match self {
            Self::Describe(_) => None,
            Self::Create { target, .. }
            | Self::Resume { target, .. }
            | Self::Fork { target, .. }
            | Self::Execute { target, .. }
            | Self::Read { target, .. }
            | Self::Changes { target, .. }
            | Self::Inspect { target, .. }
            | Self::ApplySurface { target, .. }
            | Self::RevokeSurface { target, .. } => Some(target),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Error)]
#[error("stale Complete Agent binding generation: expected {expected:?}, received {received:?}")]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireGenerationFenceError {
    pub expected: AgentBindingGeneration,
    pub received: AgentBindingGeneration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", content = "result", rename_all = "snake_case")]
pub enum RuntimeWireAgentServiceResponse {
    Describe(Result<Box<AgentServiceDescriptor>, AgentServiceError>),
    Create(Result<Box<AgentCommandReceipt>, AgentServiceError>),
    Resume(Result<Box<AgentCommandReceipt>, AgentServiceError>),
    Fork(Result<Box<ForkAgentReceipt>, AgentServiceError>),
    Execute(Result<Box<AgentCommandReceipt>, AgentServiceError>),
    Read(Result<Box<AgentSnapshot>, AgentServiceError>),
    Changes(Result<Box<AgentChangePage>, AgentServiceError>),
    Inspect(Result<Box<AgentEffectInspection>, AgentServiceError>),
    ApplySurface(Result<Box<AppliedAgentSurfaceReceipt>, AgentServiceError>),
    RevokeSurface(Result<Box<AgentCommandReceipt>, AgentServiceError>),
}

/// Ordered source change delivered by a remote Complete Agent.
///
/// `AgentChange.cursor` is the source-owned ordering coordinate. Runtime Wire frame identity and
/// ack provide transport replay; `binding_generation` prevents an old placement from advancing the
/// current binding after reconnect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeWireAgentChangeNotification {
    pub target: RuntimeWireAgentBindingTarget,
    pub source: AgentSourceCoordinate,
    pub change: AgentChange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "callback", content = "request", rename_all = "snake_case")]
pub enum RuntimeWireAgentHostCallbackRequest {
    Tool(AgentToolInvocation),
    Hook(AgentHookInvocation),
}

impl RuntimeWireAgentHostCallbackRequest {
    pub fn binding_generation(&self) -> AgentBindingGeneration {
        match self {
            Self::Tool(invocation) => invocation.meta.binding_generation,
            Self::Hook(invocation) => invocation.meta.binding_generation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "callback", content = "result", rename_all = "snake_case")]
pub enum RuntimeWireAgentHostCallbackResponse {
    Tool(Result<Box<AgentToolResult>, AgentHostCallbackError>),
    Hook(Result<Box<AgentHookDecision>, AgentHostCallbackError>),
}

#[cfg(test)]
mod tests {
    use agentdash_agent_service_api::{
        AgentCallbackRouteId, AgentCommandId, AgentCommandMeta, AgentEffectIdentity,
        AgentHostCallbackMeta, AgentIdempotencyKey, AgentItemId, AgentToolName, AgentTurnId,
    };
    use serde_json::json;

    use super::*;
    use crate::{
        RUNTIME_WIRE_PROTOCOL_REVISION, RuntimeWireAck, RuntimeWireEnvelope, RuntimeWireFrame,
        RuntimeWireFrameId, RuntimeWireRequest,
    };

    fn id<T>(
        value: &str,
        constructor: impl FnOnce(
            String,
        )
            -> Result<T, agentdash_agent_service_api::InvalidAgentServiceId>,
    ) -> T {
        constructor(value.to_owned()).expect("valid id")
    }

    fn target(generation: u64) -> RuntimeWireAgentBindingTarget {
        RuntimeWireAgentBindingTarget {
            service_instance_id: id("service-1", AgentServiceInstanceId::new),
            binding_generation: AgentBindingGeneration(generation),
        }
    }

    fn command_meta(generation: u64) -> AgentCommandMeta {
        AgentCommandMeta {
            command_id: id("command-1", AgentCommandId::new),
            effect_id: id("effect-1", AgentEffectIdentity::new),
            idempotency_key: id("idempotency-1", AgentIdempotencyKey::new),
            binding_generation: AgentBindingGeneration(generation),
            expected_snapshot_revision: None,
        }
    }

    #[test]
    fn complete_agent_public_operations_are_in_the_wire_schema() {
        let schema = schemars::schema_for!(RuntimeWireCompleteAgentSchema);
        let schema = serde_json::to_string(&schema).expect("serialize schema");
        for operation in [
            "describe",
            "create",
            "resume",
            "fork",
            "execute",
            "read",
            "changes",
            "inspect",
            "apply_surface",
            "revoke_surface",
            "callback_request",
            "callback_response",
            "AgentChange",
        ] {
            assert!(schema.contains(operation), "missing {operation}");
        }
    }

    #[test]
    fn mismatched_command_generation_is_rejected_before_dispatch() {
        let request = RuntimeWireAgentServiceRequest::Create {
            target: target(7),
            command: CreateAgentCommand {
                meta: command_meta(6),
                requested_source: None,
                initial_context: None,
            },
        };

        assert_eq!(
            request.validate_generation(),
            Err(RuntimeWireGenerationFenceError {
                expected: AgentBindingGeneration(7),
                received: AgentBindingGeneration(6),
            })
        );
    }

    #[test]
    fn callback_reuses_request_correlation_and_transport_ack() {
        let callback = RuntimeWireAgentHostCallbackRequest::Tool(AgentToolInvocation {
            meta: AgentHostCallbackMeta {
                route_id: id("route-1", AgentCallbackRouteId::new),
                binding_generation: AgentBindingGeneration(9),
                source: id("source-1", AgentSourceCoordinate::new),
                turn_id: id("turn-1", AgentTurnId::new),
                item_id: Some(id("item-1", AgentItemId::new)),
                interaction_id: None,
                effect_id: id("callback-effect-1", AgentEffectIdentity::new),
                idempotency_key: id("callback-idem-1", AgentIdempotencyKey::new),
                deadline_at_ms: 42,
            },
            tool: id("tool-1", AgentToolName::new),
            arguments: json!({"path": "README.md"}),
        });
        let request = RuntimeWireEnvelope {
            protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
            frame_id: RuntimeWireFrameId(10),
            critical: true,
            frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::AgentHostCallback(
                Box::new(callback.clone()),
            ))),
        };
        let encoded = serde_json::to_vec(&request).expect("serialize callback");
        let decoded: RuntimeWireEnvelope =
            serde_json::from_slice(&encoded).expect("deserialize callback");

        assert_eq!(decoded, request);
        assert_eq!(callback.binding_generation(), AgentBindingGeneration(9));

        let ack = RuntimeWireEnvelope {
            protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
            frame_id: RuntimeWireFrameId(11),
            critical: true,
            frame: RuntimeWireFrame::Ack(RuntimeWireAck {
                through_frame_id: RuntimeWireFrameId(10),
            }),
        };
        assert_eq!(
            serde_json::to_value(ack)
                .expect("serialize ack")
                .pointer("/frame/payload/through_frame_id")
                .and_then(serde_json::Value::as_u64),
            Some(10)
        );
    }

    #[test]
    fn agent_change_keeps_source_cursor_and_generation_fence() {
        let notification = RuntimeWireAgentChangeNotification {
            target: target(12),
            source: id("source-1", AgentSourceCoordinate::new),
            change: AgentChange {
                cursor: id(
                    "cursor-41",
                    agentdash_agent_service_api::AgentSourceCursor::new,
                ),
                source_revision: None,
                occurred_at_ms: 99,
                payload: agentdash_agent_service_api::AgentChangePayload::SnapshotInvalidated {
                    reason: "source gap".to_owned(),
                },
            },
        };
        let value = serde_json::to_value(notification).expect("serialize change");

        assert_eq!(
            value
                .pointer("/target/binding_generation")
                .and_then(serde_json::Value::as_u64),
            Some(12)
        );
        assert_eq!(
            value
                .pointer("/change/cursor")
                .and_then(serde_json::Value::as_str),
            Some("cursor-41")
        );

        let envelope = RuntimeWireEnvelope {
            protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
            frame_id: RuntimeWireFrameId(42),
            critical: true,
            frame: RuntimeWireFrame::Notification(Box::new(
                crate::RuntimeWireNotification::AgentChange(Box::new(
                    serde_json::from_value(value).expect("deserialize notification"),
                )),
            )),
        };
        assert_eq!(
            serde_json::to_value(envelope)
                .expect("serialize envelope")
                .pointer("/frame/payload/method")
                .and_then(serde_json::Value::as_str),
            Some("agent_change")
        );
    }
}
