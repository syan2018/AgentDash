use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    DriverItemId, DriverTurnId, RuntimeEvent, RuntimeInput, RuntimeInteractionId,
    RuntimeItemContent, RuntimeItemId, RuntimeItemTerminal, RuntimeTurnId, RuntimeTurnTerminal,
};
use codex_app_server_protocol as codex;
use serde_json::Value;
use thiserror::Error;

use crate::rpc::{RpcServerNotification, RpcServerRequest};

#[derive(Debug, Default)]
pub(crate) struct SourceCoordinateMap {
    turns: BTreeMap<String, RuntimeTurnId>,
    items: BTreeMap<String, RuntimeItemId>,
    interactions: BTreeMap<String, RuntimeInteractionId>,
}

#[derive(Debug)]
pub(crate) struct MappedEvent {
    pub source_turn_id: Option<DriverTurnId>,
    pub source_item_id: Option<DriverItemId>,
    pub event: RuntimeEvent,
}

#[derive(Debug)]
pub(crate) struct MappedInteraction {
    pub source_turn_id: DriverTurnId,
    pub source_item_id: Option<DriverItemId>,
    pub turn_id: RuntimeTurnId,
    pub interaction_id: RuntimeInteractionId,
    pub event: RuntimeEvent,
}

#[derive(Debug, Error)]
pub(crate) enum MappingError {
    #[error("Codex payload for {method} is missing {field}")]
    Missing { method: String, field: &'static str },
    #[error("Codex payload for {method} contains an invalid terminal status: {status}")]
    InvalidTerminal { method: String, status: String },
    #[error("Codex source coordinate is unknown: {kind}={value}")]
    UnknownCoordinate { kind: &'static str, value: String },
    #[error("Codex method is outside the runtime adapter surface: {0}")]
    UnsupportedMethod(String),
    #[error("Codex item payload is invalid: {0}")]
    InvalidItemPayload(String),
    #[error("Codex item failed owned protocol conformance: {0}")]
    OwnedProtocolMismatch(String),
}

impl SourceCoordinateMap {
    pub fn register_turn(&mut self, source: impl Into<String>, canonical: RuntimeTurnId) {
        self.turns.insert(source.into(), canonical);
    }

    pub fn canonical_turn(&self, source: &str) -> Result<RuntimeTurnId, MappingError> {
        self.turn_for(source)
    }

    pub fn source_turn(&self, canonical: &RuntimeTurnId) -> Result<String, MappingError> {
        self.turns
            .iter()
            .find_map(|(source, mapped)| (mapped == canonical).then(|| source.clone()))
            .ok_or_else(|| MappingError::UnknownCoordinate {
                kind: "canonical_turn",
                value: canonical.as_str().to_string(),
            })
    }

    pub fn register_item(&mut self, source: impl Into<String>) -> RuntimeItemId {
        let source = source.into();
        self.items
            .entry(source.clone())
            .or_insert_with(|| {
                RuntimeItemId::new(format!("codex-item-{source}"))
                    .expect("source item id is non-empty")
            })
            .clone()
    }

    pub fn map_notification(
        &mut self,
        notification: RpcServerNotification,
    ) -> Result<Option<MappedEvent>, MappingError> {
        let method = notification.method;
        let params = notification.params;
        admit_notification(&method, &params)?;
        match method.as_str() {
            "turn/started" => {
                let source = nested_string(&params, &["turn", "id"])
                    .or_else(|| string(&params, "turnId"))
                    .ok_or_else(|| missing(&method, "turn.id"))?;
                let canonical = self.turn_for(&source)?;
                Ok(Some(MappedEvent {
                    source_turn_id: Some(driver_turn(&source)?),
                    source_item_id: None,
                    event: RuntimeEvent::TurnStarted { turn_id: canonical },
                }))
            }
            "turn/completed" => {
                let source = nested_string(&params, &["turn", "id"])
                    .or_else(|| string(&params, "turnId"))
                    .ok_or_else(|| missing(&method, "turn.id"))?;
                let status = nested_string(&params, &["turn", "status"])
                    .or_else(|| string(&params, "status"))
                    .ok_or_else(|| missing(&method, "turn.status"))?;
                let terminal = match status.as_str() {
                    "completed" => RuntimeTurnTerminal::Completed,
                    "interrupted" => RuntimeTurnTerminal::Interrupted,
                    "failed" => RuntimeTurnTerminal::Failed,
                    other => {
                        return Err(MappingError::InvalidTerminal {
                            method,
                            status: other.to_string(),
                        });
                    }
                };
                let canonical = self.turn_for(&source)?;
                Ok(Some(MappedEvent {
                    source_turn_id: Some(driver_turn(&source)?),
                    source_item_id: None,
                    event: RuntimeEvent::TurnTerminal {
                        turn_id: canonical,
                        terminal,
                        message: None,
                    },
                }))
            }
            "item/started" => self.map_item(&method, &params, false),
            "item/completed" => self.map_item(&method, &params, true),
            "item/agentMessage/delta" | "item/reasoning/textDelta" | "item/plan/delta" => {
                let source_turn =
                    string(&params, "turnId").ok_or_else(|| missing(&method, "turnId"))?;
                let source_item =
                    string(&params, "itemId").ok_or_else(|| missing(&method, "itemId"))?;
                let delta = string(&params, "delta").ok_or_else(|| missing(&method, "delta"))?;
                let turn_id = self.turn_for(&source_turn)?;
                let item_id = self.item_for(&source_item)?;
                Ok(Some(MappedEvent {
                    source_turn_id: Some(driver_turn(&source_turn)?),
                    source_item_id: Some(driver_item(&source_item)?),
                    event: RuntimeEvent::ConversationDelta {
                        turn_id,
                        item_id,
                        delta: match method.as_str() {
                            "item/agentMessage/delta" => agentdash_agent_runtime_contract::RuntimeConversationDelta::AgentMessage { delta },
                            "item/reasoning/textDelta" => agentdash_agent_runtime_contract::RuntimeConversationDelta::ReasoningText { delta },
                            "item/plan/delta" => agentdash_agent_runtime_contract::RuntimeConversationDelta::Plan { delta },
                            _ => unreachable!("method admission is exhaustive"),
                        },
                    },
                }))
            }
            "thread/compacted" => Ok(Some(MappedEvent {
                source_turn_id: string(&params, "turnId").map(driver_turn).transpose()?,
                source_item_id: None,
                event: RuntimeEvent::DriverContextCompactedOpaque,
            })),
            "hook/started" | "hook/completed" => Ok(None),
            _ => Err(MappingError::UnsupportedMethod(method)),
        }
    }

    pub fn map_server_request(
        &mut self,
        request: &RpcServerRequest,
    ) -> Result<MappedInteraction, MappingError> {
        admit_server_request(&request.method, &request.params)?;
        let source_turn =
            string(&request.params, "turnId").ok_or_else(|| missing(&request.method, "turnId"))?;
        let source_item = string(&request.params, "itemId");
        let turn_id = self.turn_for(&source_turn)?;
        let interaction_key = format!(
            "{}:{}:{}:{}",
            source_turn,
            source_item.as_deref().unwrap_or("thread"),
            request.method.replace('/', "-"),
            rpc_coordinate(&request.id),
        );
        let interaction_id =
            RuntimeInteractionId::new(format!("codex-interaction-{interaction_key}"))
                .expect("source interaction coordinates are non-empty");
        self.interactions
            .insert(interaction_key, interaction_id.clone());
        let interaction_request = match request.method.as_str() {
            "item/commandExecution/requestApproval" => {
                agentdash_agent_runtime_contract::RuntimeInteractionRequest::CommandApproval {
                    params: strict_interaction_params::<
                        codex::CommandExecutionRequestApprovalParams,
                        _,
                    >(request.params.clone())?,
                }
            }
            "item/fileChange/requestApproval" => {
                agentdash_agent_runtime_contract::RuntimeInteractionRequest::FileChangeApproval {
                    params: strict_interaction_params::<codex::FileChangeRequestApprovalParams, _>(
                        request.params.clone(),
                    )?,
                }
            }
            "item/permissions/requestApproval" => {
                agentdash_agent_runtime_contract::RuntimeInteractionRequest::PermissionApproval {
                    params: strict_interaction_params::<codex::PermissionsRequestApprovalParams, _>(
                        request.params.clone(),
                    )?,
                }
            }
            "item/tool/requestUserInput" => {
                agentdash_agent_runtime_contract::RuntimeInteractionRequest::UserInputRequest {
                    params: strict_interaction_params::<codex::ToolRequestUserInputParams, _>(
                        request.params.clone(),
                    )?,
                }
            }
            "item/tool/call" => {
                agentdash_agent_runtime_contract::RuntimeInteractionRequest::DynamicToolExecution {
                    params: strict_interaction_params::<codex::DynamicToolCallParams, _>(
                        request.params.clone(),
                    )?,
                }
            }
            "mcpServer/elicitation/request" => {
                agentdash_agent_runtime_contract::RuntimeInteractionRequest::McpElicitation {
                    params: strict_interaction_params::<codex::McpServerElicitationRequestParams, _>(
                        request.params.clone(),
                    )?,
                }
            }
            other => return Err(MappingError::UnsupportedMethod(other.to_string())),
        };
        let item_id = source_item
            .as_deref()
            .map(|id| self.item_for(id))
            .transpose()?;
        Ok(MappedInteraction {
            source_turn_id: driver_turn(&source_turn)?,
            source_item_id: source_item.as_deref().map(driver_item).transpose()?,
            turn_id: turn_id.clone(),
            interaction_id: interaction_id.clone(),
            event: RuntimeEvent::InteractionRequested {
                turn_id,
                item_id,
                interaction_id,
                request: interaction_request,
            },
        })
    }

    fn map_item(
        &mut self,
        method: &str,
        params: &Value,
        completed: bool,
    ) -> Result<Option<MappedEvent>, MappingError> {
        let source_turn = string(params, "turnId").ok_or_else(|| missing(method, "turnId"))?;
        let item = params.get("item").ok_or_else(|| missing(method, "item"))?;
        let source_item = string(item, "id").ok_or_else(|| missing(method, "item.id"))?;
        let turn_id = self.turn_for(&source_turn)?;
        let item_id = self
            .items
            .entry(source_item.clone())
            .or_insert_with(|| {
                RuntimeItemId::new(format!("codex-item-{source_item}"))
                    .expect("source item id is non-empty")
            })
            .clone();
        let event = if completed {
            let terminal = match item.get("status").and_then(Value::as_str) {
                Some("failed") => RuntimeItemTerminal::Failed {
                    message: item
                        .get("error")
                        .and_then(|error| {
                            error
                                .get("message")
                                .and_then(Value::as_str)
                                .or_else(|| error.as_str())
                        })
                        .map(ToOwned::to_owned),
                },
                Some("declined" | "cancelled" | "interrupted") => {
                    RuntimeItemTerminal::Cancelled { message: None }
                }
                _ => RuntimeItemTerminal::Completed {
                    final_content: item_content(item)?,
                },
            };
            RuntimeEvent::ItemTerminal {
                turn_id,
                item_id,
                terminal,
            }
        } else {
            RuntimeEvent::ItemStarted {
                turn_id,
                item_id,
                initial_content: item_content(item)?,
            }
        };
        Ok(Some(MappedEvent {
            source_turn_id: Some(driver_turn(&source_turn)?),
            source_item_id: Some(driver_item(&source_item)?),
            event,
        }))
    }

    fn turn_for(&self, source: &str) -> Result<RuntimeTurnId, MappingError> {
        self.turns
            .get(source)
            .cloned()
            .ok_or_else(|| MappingError::UnknownCoordinate {
                kind: "turn",
                value: source.to_string(),
            })
    }

    fn item_for(&self, source: &str) -> Result<RuntimeItemId, MappingError> {
        self.items
            .get(source)
            .cloned()
            .ok_or_else(|| MappingError::UnknownCoordinate {
                kind: "item",
                value: source.to_string(),
            })
    }
}

fn strict_transcode<V, O>(value: &Value) -> Result<(), MappingError>
where
    V: serde::de::DeserializeOwned + serde::Serialize,
    O: serde::de::DeserializeOwned,
{
    let vendor: V = serde_json::from_value(value.clone())
        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?;
    let canonical = serde_json::to_value(vendor)
        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?;
    serde_json::from_value::<O>(canonical)
        .map_err(|error| MappingError::OwnedProtocolMismatch(error.to_string()))?;
    Ok(())
}

fn admit_notification(method: &str, params: &Value) -> Result<(), MappingError> {
    use agentdash_agent_protocol::generated::codex_v2::server_notification as owned;
    match method {
        "turn/started" => strict_transcode::<
            codex::TurnStartedNotification,
            owned::TurnStartedNotification,
        >(params),
        "turn/completed" => strict_transcode::<
            codex::TurnCompletedNotification,
            owned::TurnCompletedNotification,
        >(params),
        "item/started" => strict_transcode::<
            codex::ItemStartedNotification,
            owned::ItemStartedNotification,
        >(params),
        "item/completed" => strict_transcode::<
            codex::ItemCompletedNotification,
            owned::ItemCompletedNotification,
        >(params),
        "item/agentMessage/delta" => strict_transcode::<
            codex::AgentMessageDeltaNotification,
            owned::AgentMessageDeltaNotification,
        >(params),
        "item/reasoning/textDelta" => strict_transcode::<
            codex::ReasoningTextDeltaNotification,
            owned::ReasoningTextDeltaNotification,
        >(params),
        "item/plan/delta" => {
            strict_transcode::<codex::PlanDeltaNotification, owned::PlanDeltaNotification>(params)
        }
        "thread/compacted" => strict_transcode::<
            codex::ContextCompactedNotification,
            owned::ContextCompactedNotification,
        >(params),
        "hook/started" => strict_transcode::<
            codex::HookStartedNotification,
            owned::HookStartedNotification,
        >(params),
        "hook/completed" => strict_transcode::<
            codex::HookCompletedNotification,
            owned::HookCompletedNotification,
        >(params),
        other => Err(MappingError::UnsupportedMethod(other.to_string())),
    }
}

fn admit_server_request(method: &str, params: &Value) -> Result<(), MappingError> {
    match method {
        "item/commandExecution/requestApproval" => strict_transcode::<
            codex::CommandExecutionRequestApprovalParams,
            agentdash_agent_protocol::generated::codex_v2::command_execution_request_approval_params::CommandExecutionRequestApprovalParams,
        >(params),
        "item/fileChange/requestApproval" => strict_transcode::<
            codex::FileChangeRequestApprovalParams,
            agentdash_agent_protocol::generated::codex_v2::file_change_request_approval_params::FileChangeRequestApprovalParams,
        >(params),
        "item/permissions/requestApproval" => strict_transcode::<
            codex::PermissionsRequestApprovalParams,
            agentdash_agent_protocol::generated::codex_v2::permissions_request_approval_params::PermissionsRequestApprovalParams,
        >(params),
        "item/tool/requestUserInput" => strict_transcode::<
            codex::ToolRequestUserInputParams,
            agentdash_agent_protocol::generated::codex_v2::tool_request_user_input_params::ToolRequestUserInputParams,
        >(params),
        "item/tool/call" => strict_transcode::<
            codex::DynamicToolCallParams,
            agentdash_agent_protocol::generated::codex_v2::dynamic_tool_call_params::DynamicToolCallParams,
        >(params),
        "mcpServer/elicitation/request" => strict_transcode::<
            codex::McpServerElicitationRequestParams,
            agentdash_agent_protocol::generated::codex_v2::mcp_server_elicitation_request_params::McpServerElicitationRequestParams,
        >(params),
        other => Err(MappingError::UnsupportedMethod(other.to_string())),
    }
}

pub(crate) fn map_input(
    input: &[RuntimeInput],
) -> (Vec<Value>, Option<serde_json::Map<String, Value>>) {
    let mut native = Vec::new();
    let mut additional = serde_json::Map::new();
    for (index, block) in input.iter().enumerate() {
        match block {
            RuntimeInput::Text { text } => native.push(serde_json::json!({ "type": "text", "text": text, "textElements": [] })),
            RuntimeInput::Image { data_url, .. } => native.push(serde_json::json!({ "type": "image", "url": data_url })),
            RuntimeInput::FileReference { uri, media_type } => native.push(serde_json::json!({ "type": "mention", "name": media_type.as_deref().unwrap_or("resource"), "path": uri })),
            RuntimeInput::Structured { schema, value } => {
                additional.insert(format!("agentdash.structured.{index}"), serde_json::json!({
                    "value": serde_json::to_string(&serde_json::json!({ "schema": schema, "value": value })).expect("JSON value serializes"),
                    "kind": "application"
                }));
            }
        }
    }
    (native, (!additional.is_empty()).then_some(additional))
}

pub(crate) fn item_content(item: &Value) -> Result<RuntimeItemContent, MappingError> {
    let vendor: codex::ThreadItem = serde_json::from_value(item.clone())
        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?;
    let canonical_json = serde_json::to_value(&vendor)
        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?;
    let owned = serde_json::from_value::<
        agentdash_agent_protocol::generated::codex_v2::thread_item::ThreadItem,
    >(canonical_json)
    .map_err(|error| MappingError::OwnedProtocolMismatch(error.to_string()))?;
    Ok(RuntimeItemContent::new(owned.into()))
}

fn driver_turn(value: impl AsRef<str>) -> Result<DriverTurnId, MappingError> {
    DriverTurnId::new(value.as_ref()).map_err(|_| MappingError::Missing {
        method: "source coordinate".to_string(),
        field: "turnId",
    })
}
fn strict_interaction_params<V, O>(value: Value) -> Result<Box<O>, MappingError>
where
    V: serde::de::DeserializeOwned + serde::Serialize,
    O: serde::de::DeserializeOwned,
{
    let vendor: V = serde_json::from_value(value)
        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?;
    let canonical = serde_json::to_value(vendor)
        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?;
    serde_json::from_value(canonical)
        .map(Box::new)
        .map_err(|error| MappingError::OwnedProtocolMismatch(error.to_string()))
}
fn driver_item(value: impl AsRef<str>) -> Result<DriverItemId, MappingError> {
    DriverItemId::new(value.as_ref()).map_err(|_| MappingError::Missing {
        method: "source coordinate".to_string(),
        field: "itemId",
    })
}
fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToOwned::to_owned)
}
fn nested_string(value: &Value, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(value, |value, key| value.get(*key))?
        .as_str()
        .map(ToOwned::to_owned)
}
fn missing(method: &str, field: &'static str) -> MappingError {
    MappingError::Missing {
        method: method.to_string(),
        field,
    }
}
fn rpc_coordinate(id: &Value) -> String {
    match id {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::RpcServerNotification;

    #[test]
    fn structured_and_image_input_are_not_flattened_into_prompt_text() {
        let (input, additional) = map_input(&[
            RuntimeInput::Image {
                mime_type: "image/png".to_string(),
                data_url: "data:image/png;base64,AA==".to_string(),
            },
            RuntimeInput::Structured {
                schema: "answer.v1".to_string(),
                value: serde_json::json!({"choice": 2}),
            },
        ]);
        assert_eq!(input[0]["type"], "image");
        let structured = &additional.expect("structured context")["agentdash.structured.1"];
        assert_eq!(structured["kind"], "application");
        assert!(
            structured["value"]
                .as_str()
                .expect("value")
                .contains("answer.v1")
        );
    }

    #[test]
    fn eof_is_never_synthesized_as_completed() {
        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        let event = map
            .map_notification(RpcServerNotification {
                method: "turn/completed".to_string(),
                params: serde_json::json!({
                    "threadId": "source-thread",
                    "turn": {
                        "id": "source-turn", "status": "interrupted", "items": [],
                        "itemsView": "full", "error": null, "startedAt": null,
                        "completedAt": null, "durationMs": null
                    }
                }),
            })
            .expect("map")
            .expect("event");
        assert!(matches!(
            event.event,
            RuntimeEvent::TurnTerminal {
                terminal: RuntimeTurnTerminal::Interrupted,
                ..
            }
        ));
    }

    #[test]
    fn replayed_server_request_keeps_durable_interaction_identity() {
        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        map.register_item("source-item");
        let request = crate::rpc::RpcServerRequest {
            id: serde_json::json!(1),
            method: "item/commandExecution/requestApproval".to_string(),
            params: serde_json::json!({
                "threadId": "source-thread", "turnId": "source-turn",
                "itemId": "source-item", "startedAtMs": 1,
                "environmentId": null, "reason": "approve"
            }),
        };
        let first = map.map_server_request(&request).expect("first");
        let replay = map.map_server_request(&request).expect("replay");
        assert_eq!(first.interaction_id, replay.interaction_id);

        let mut next = request;
        next.id = serde_json::json!(2);
        let next = map.map_server_request(&next).expect("next request");
        assert_ne!(first.interaction_id, next.interaction_id);
    }

    #[test]
    fn interaction_requests_preserve_generated_owned_params_without_field_projection() {
        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let fixtures = [
            (
                "item/commandExecution/requestApproval",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","itemId":"source-item","approvalId":"approval-1","command":"cargo test","commandActions":[],"environmentId":"env-1","proposedExecpolicyAmendment":["cargo","test"],"reason":"run tests","startedAtMs":123}),
            ),
            (
                "item/fileChange/requestApproval",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","itemId":"source-item","grantRoot":"/workspace","reason":"write","startedAtMs":124}),
            ),
            (
                "item/permissions/requestApproval",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","itemId":"source-item","cwd":cwd,"permissions":{},"reason":"access","startedAtMs":125}),
            ),
            (
                "item/tool/requestUserInput",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","itemId":"source-item","autoResolutionMs":60000,"questions":[{"id":"secret","header":"Token","question":"Enter token","isOther":true,"isSecret":true,"options":[{"label":"Use saved","description":"reuse credential"}]}]}),
            ),
            (
                "mcpServer/elicitation/request",
                serde_json::json!({"serverName":"docs","threadId":"source-thread","turnId":"source-turn","mode":"form","message":"Configure","requestedSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}),
            ),
            (
                "item/tool/call",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","callId":"call-1","namespace":"workspace","tool":"render","arguments":{"uri":"canvas://one"}}),
            ),
        ];
        for (index, (method, params)) in fixtures.into_iter().enumerate() {
            let mut map = SourceCoordinateMap::default();
            map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
            map.register_item("source-item");
            let mapped = map
                .map_server_request(&crate::rpc::RpcServerRequest {
                    id: serde_json::json!(index),
                    method: method.to_string(),
                    params: params.clone(),
                })
                .unwrap_or_else(|error| panic!("{method}: {error}"));
            let RuntimeEvent::InteractionRequested { request, .. } = mapped.event else {
                panic!("interaction request")
            };
            let wire = serde_json::to_value(request).expect("serialize owned request");
            assert_eq!(wire["params"]["threadId"], params["threadId"]);
            assert_eq!(wire["params"]["turnId"], params["turnId"]);
            match method {
                "item/commandExecution/requestApproval" => {
                    assert_eq!(wire["params"]["approvalId"], "approval-1")
                }
                "item/fileChange/requestApproval" => {
                    assert_eq!(wire["params"]["grantRoot"], "/workspace")
                }
                "item/permissions/requestApproval" => {
                    assert_eq!(wire["params"]["cwd"], params["cwd"])
                }
                "item/tool/requestUserInput" => {
                    assert_eq!(wire["params"]["autoResolutionMs"], 60000);
                    assert_eq!(wire["params"]["questions"][0]["isSecret"], true);
                }
                "mcpServer/elicitation/request" => {
                    assert_eq!(wire["params"]["requestedSchema"], params["requestedSchema"])
                }
                "item/tool/call" => assert_eq!(wire["params"]["arguments"], params["arguments"]),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn failed_item_is_not_projected_as_completed() {
        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        let event = map
            .map_notification(RpcServerNotification {
                method: "item/completed".to_string(),
                params: serde_json::json!({
                    "threadId": "source-thread",
                    "turnId": "source-turn",
                    "completedAtMs": 1,
                    "item": {
                        "id": "source-item", "type": "commandExecution",
                        "command": "false", "cwd": "C:/workspace", "processId": null,
                        "source": "agent", "status": "failed", "commandActions": [],
                        "aggregatedOutput": null, "exitCode": 1, "durationMs": null
                    }
                }),
            })
            .expect("map")
            .expect("event");
        assert!(matches!(
            event.event,
            RuntimeEvent::ItemTerminal {
                terminal: RuntimeItemTerminal::Failed { .. },
                ..
            }
        ));
    }

    #[test]
    fn unknown_item_is_rejected_instead_of_flattened_to_agent_text() {
        let error = item_content(&serde_json::json!({
            "id": "source-item",
            "type": "futureItem",
            "payload": { "secret": "structure" }
        }))
        .expect_err("unknown Codex item must fail typed admission");
        assert!(matches!(error, MappingError::InvalidItemPayload(_)));
    }

    #[test]
    fn unknown_notification_method_is_a_typed_protocol_mismatch() {
        let error = SourceCoordinateMap::default()
            .map_notification(RpcServerNotification {
                method: "future/notification".to_string(),
                params: serde_json::json!({}),
            })
            .expect_err("unknown notification must not be ignored");
        assert!(
            matches!(error, MappingError::UnsupportedMethod(method) if method == "future/notification")
        );
    }

    #[test]
    fn supported_item_passes_vendor_and_owned_deserialization_before_projection() {
        let content = item_content(&serde_json::json!({
            "id": "source-item",
            "type": "agentMessage",
            "text": "typed",
            "phase": "commentary"
        }))
        .expect("typed item");
        assert_eq!(content.agent_message_text(), Some("typed"));
    }

    #[test]
    fn owned_thread_item_accepts_omitted_and_null_and_emits_canonical_null() {
        for fixture in [
            serde_json::json!({
                "id": "item-1", "type": "agentMessage", "text": "hello"
            }),
            serde_json::json!({
                "id": "item-1", "type": "agentMessage", "text": "hello",
                "phase": null, "memoryCitation": null
            }),
        ] {
            let vendor: codex::ThreadItem = serde_json::from_value(fixture.clone()).unwrap();
            let vendor_wire = serde_json::to_value(vendor).unwrap();
            let owned: agentdash_agent_protocol::generated::codex_v2::thread_item::ThreadItem =
                serde_json::from_value(vendor_wire).unwrap();
            assert_eq!(
                serde_json::to_value(owned).unwrap(),
                serde_json::json!({
                    "id": "item-1", "type": "agentMessage", "text": "hello",
                    "phase": null, "memoryCitation": null
                })
            );
        }
    }

    #[test]
    fn owned_mcp_elicitation_accepts_omitted_and_null_and_emits_canonical_null() {
        for fixture in [
            serde_json::json!({
                "serverName": "docs", "threadId": "thread-1", "mode": "url",
                "message": "open", "url": "https://example.com", "elicitationId": "e-1"
            }),
            serde_json::json!({
                "serverName": "docs", "threadId": "thread-1", "turnId": null,
                "mode": "url", "_meta": null, "message": "open",
                "url": "https://example.com", "elicitationId": "e-1"
            }),
        ] {
            let vendor: codex::McpServerElicitationRequestParams =
                serde_json::from_value(fixture).unwrap();
            let vendor_wire = serde_json::to_value(vendor).unwrap();
            let owned = serde_json::from_value::<
                agentdash_agent_protocol::generated::codex_v2::mcp_server_elicitation_request_params::McpServerElicitationRequestParams,
            >(vendor_wire)
            .unwrap();
            assert_eq!(
                serde_json::to_value(owned).unwrap(),
                serde_json::json!({
                    "serverName": "docs", "threadId": "thread-1", "turnId": null,
                    "mode": "url", "_meta": null, "message": "open",
                    "url": "https://example.com", "elicitationId": "e-1"
                })
            );
        }
    }

    #[test]
    fn owned_thread_item_overlay_covers_mcp_uri_and_image_generation_paths() {
        let mcp = serde_json::json!({
            "type": "mcpToolCall", "id": "mcp-1", "server": "docs", "tool": "search",
            "status": "inProgress", "arguments": {}, "appContext": null,
            "pluginId": null, "result": null, "error": null, "durationMs": null
        });
        let vendor: codex::ThreadItem = serde_json::from_value(mcp).unwrap();
        let owned: agentdash_agent_protocol::generated::codex_v2::thread_item::ThreadItem =
            serde_json::from_value(serde_json::to_value(vendor).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(owned).unwrap()["mcpAppResourceUri"],
            serde_json::Value::Null
        );

        for fixture in [
            serde_json::json!({
                "type": "imageGeneration", "id": "image-1", "status": "completed",
                "result": "image", "revisedPrompt": null
            }),
            serde_json::json!({
                "type": "imageGeneration", "id": "image-1", "status": "completed",
                "result": "image", "revisedPrompt": null, "savedPath": null
            }),
        ] {
            let vendor: codex::ThreadItem = serde_json::from_value(fixture).unwrap();
            let owned: agentdash_agent_protocol::generated::codex_v2::thread_item::ThreadItem =
                serde_json::from_value(serde_json::to_value(vendor).unwrap()).unwrap();
            let canonical = serde_json::to_value(owned).unwrap();
            assert_eq!(canonical["revisedPrompt"], serde_json::Value::Null);
            assert_eq!(canonical["savedPath"], serde_json::Value::Null);
        }
    }

    #[test]
    fn sleep_duration_remains_required_non_nullable_number() {
        type OwnedThreadItem =
            agentdash_agent_protocol::generated::codex_v2::thread_item::ThreadItem;
        let fixture = serde_json::json!({
            "type": "sleep", "id": "sleep-1", "durationMs": 25
        });
        let owned: OwnedThreadItem = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(serde_json::to_value(owned).unwrap(), fixture);
        assert!(
            serde_json::from_value::<OwnedThreadItem>(
                serde_json::json!({ "type": "sleep", "id": "sleep-1" })
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<OwnedThreadItem>(serde_json::json!({
                "type": "sleep", "id": "sleep-1", "durationMs": null
            }))
            .is_err()
        );
    }
}
