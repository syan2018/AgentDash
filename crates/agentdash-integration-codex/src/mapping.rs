use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    DriverItemId, DriverTurnId, RuntimeEvent, RuntimeInput, RuntimeInteractionId,
    RuntimeInteractionKind, RuntimeItemContent, RuntimeItemId, RuntimeItemTerminal, RuntimeTurnId,
    RuntimeTurnTerminal,
};
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
                    event: RuntimeEvent::ItemDelta {
                        turn_id,
                        item_id,
                        delta,
                    },
                }))
            }
            "thread/compacted" => Ok(Some(MappedEvent {
                source_turn_id: string(&params, "turnId").map(driver_turn).transpose()?,
                source_item_id: None,
                event: RuntimeEvent::DriverContextCompactedOpaque,
            })),
            "hook/started" | "hook/completed" => Ok(None),
            _ => Ok(None),
        }
    }

    pub fn map_server_request(
        &mut self,
        request: &RpcServerRequest,
    ) -> Result<MappedInteraction, MappingError> {
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
        let (kind, prompt) = match request.method.as_str() {
            "item/commandExecution/requestApproval" => (
                RuntimeInteractionKind::CommandApproval,
                prompt(&request.params, "Approve command execution?"),
            ),
            "item/fileChange/requestApproval" => (
                RuntimeInteractionKind::FileChangeApproval,
                prompt(&request.params, "Approve file changes?"),
            ),
            "item/permissions/requestApproval" => (
                RuntimeInteractionKind::PermissionApproval,
                prompt(&request.params, "Approve requested permissions?"),
            ),
            "item/tool/requestUserInput" => (
                RuntimeInteractionKind::UserInputRequest,
                questions_prompt(&request.params),
            ),
            "item/tool/call" => (
                RuntimeInteractionKind::DynamicToolExecution,
                prompt(&request.params, "Execute dynamic tool?"),
            ),
            "mcpServer/elicitation/request" => (
                RuntimeInteractionKind::McpElicitation,
                prompt(&request.params, "MCP server requests input"),
            ),
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
                interaction_kind: kind,
                prompt,
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
                    final_content: item_content(item),
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
                initial_content: item_content(item),
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

pub(crate) fn item_content(item: &Value) -> RuntimeItemContent {
    match item.get("type").and_then(Value::as_str).unwrap_or_default() {
        "userMessage" => RuntimeItemContent::UserMessage { input: Vec::new() },
        "agentMessage" => RuntimeItemContent::AgentMessage {
            text: string(item, "text").unwrap_or_default(),
        },
        "reasoning" => RuntimeItemContent::Reasoning {
            text: item
                .get("content")
                .map(Value::to_string)
                .unwrap_or_default(),
        },
        "plan" => RuntimeItemContent::Plan {
            steps: string(item, "text").map(|v| vec![v]).unwrap_or_default(),
        },
        "dynamicToolCall" | "mcpToolCall" => RuntimeItemContent::ToolCall {
            name: string(item, "tool")
                .or_else(|| string(item, "name"))
                .unwrap_or_else(|| "tool".to_string()),
            arguments: item.get("arguments").cloned().unwrap_or(Value::Null),
        },
        _ => RuntimeItemContent::AgentMessage {
            text: item.to_string(),
        },
    }
}

fn driver_turn(value: impl AsRef<str>) -> Result<DriverTurnId, MappingError> {
    DriverTurnId::new(value.as_ref()).map_err(|_| MappingError::Missing {
        method: "source coordinate".to_string(),
        field: "turnId",
    })
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
fn prompt(params: &Value, default: &str) -> String {
    string(params, "reason")
        .or_else(|| string(params, "message"))
        .unwrap_or_else(|| default.to_string())
}
fn questions_prompt(params: &Value) -> String {
    params
        .get("questions")
        .map(Value::to_string)
        .unwrap_or_else(|| "Codex requests user input".to_string())
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
        let event = map.map_notification(RpcServerNotification {
            method: "turn/completed".to_string(),
            params: serde_json::json!({ "turn": { "id": "source-turn", "status": "interrupted" } }),
        }).expect("map").expect("event");
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
            params: serde_json::json!({ "turnId": "source-turn", "itemId": "source-item", "reason": "approve" }),
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
    fn failed_item_is_not_projected_as_completed() {
        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        let event = map
            .map_notification(RpcServerNotification {
                method: "item/completed".to_string(),
                params: serde_json::json!({
                    "turnId": "source-turn",
                    "item": { "id": "source-item", "type": "commandExecution", "status": "failed", "error": { "message": "exit 1" } }
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
}
