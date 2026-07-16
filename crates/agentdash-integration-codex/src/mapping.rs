use std::collections::BTreeMap;

use agentdash_agent_runtime_contract::{
    DriverItemId, DriverTurnId, ImmutablePresentationEvent, PresentationDurability, RuntimeEvent,
    RuntimeInput, RuntimeInteractionId, RuntimeItemContent, RuntimeItemId, RuntimeItemTerminal,
    RuntimeTurnId, RuntimeTurnTerminal,
};
use codex_app_server_protocol as codex;
use serde_json::Value;
use thiserror::Error;

use crate::rpc::{RpcServerNotification, RpcServerRequest};

/// Main's Codex bridge answers these request families immediately and does not
/// turn them into presentation interactions. Newer 0.144.1 request families
/// return `None` and continue through the typed Runtime interaction path.
pub(crate) fn main_automatic_server_response(
    request: &RpcServerRequest,
) -> Result<Option<Value>, MappingError> {
    admit_server_request(&request.method, &request.params)?;
    Ok(match request.method.as_str() {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            Some(serde_json::json!({ "decision": "acceptForSession" }))
        }
        "item/tool/requestUserInput" => Some(serde_json::json!({ "answers": {} })),
        "item/permissions/requestApproval" | "item/tool/call" | "mcpServer/elicitation/request" => {
            None
        }
        other => return Err(MappingError::UnsupportedMethod(other.to_string())),
    })
}

pub(crate) fn dynamic_tool_interaction_request(
    params: Value,
) -> Result<agentdash_agent_runtime_contract::RuntimeInteractionRequest, MappingError> {
    Ok(
        agentdash_agent_runtime_contract::RuntimeInteractionRequest::DynamicToolExecution {
            params: strict_interaction_params::<codex::DynamicToolCallParams, _>(params)?,
        },
    )
}

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
    pub runtime_event: Option<RuntimeEvent>,
    pub presentation: ImmutablePresentationEvent,
}

impl MappedEvent {
    /// Notifications normally have no JSON-RPC request coordinate. Codex's
    /// `serverRequest/resolved` is the exception: its protected payload keeps
    /// the standard `RequestId`, while the carrier exposes the same identity
    /// as a correlation string.
    pub fn source_request_id(&self) -> Option<String> {
        match &self.presentation.event {
            agentdash_agent_protocol::BackboneEvent::ServerRequestResolved(value) => {
                Some(value.request_id.to_string())
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct MappedInteraction {
    pub source_turn_id: DriverTurnId,
    pub source_item_id: Option<DriverItemId>,
    pub turn_id: RuntimeTurnId,
    pub interaction_id: RuntimeInteractionId,
    pub source_request_id: String,
    pub event: RuntimeEvent,
    pub presentation: Option<ImmutablePresentationEvent>,
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
    #[error(
        "Codex method {method} is valid without turnId, but the managed Runtime interaction boundary currently requires a turn"
    )]
    UnsupportedThreadScopedInteraction { method: String },
    #[error("Codex item payload is invalid: {0}")]
    InvalidItemPayload(String),
    #[error("Codex item failed owned protocol conformance: {0}")]
    OwnedProtocolMismatch(String),
    #[error("Codex dynamic tool `{tool}` has no effective presentation route")]
    MissingToolPresentationRoute { tool: String },
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
        if admit_notification(&method, &params)? == NotificationDisposition::TypedNoop {
            return Ok(None);
        }
        let presentation = presentation_notification(&method, &params)?;
        match method.as_str() {
            "turn/started" => {
                let source = nested_string(&params, &["turn", "id"])
                    .or_else(|| string(&params, "turnId"))
                    .ok_or_else(|| missing(&method, "turn.id"))?;
                let source_turn_id = driver_turn(&source)?;
                let presentation_turn_id =
                    agentdash_agent_runtime_contract::PresentationTurnId::new(source.clone())
                        .expect("validated Codex source turn identity");
                let canonical = self.turn_for(&source)?;
                Ok(Some(MappedEvent {
                    source_turn_id: Some(source_turn_id),
                    source_item_id: None,
                    runtime_event: Some(RuntimeEvent::TurnStarted {
                        turn_id: canonical,
                        presentation_turn_id,
                    }),
                    presentation: required_presentation(&method, presentation)?,
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
                    runtime_event: Some(RuntimeEvent::TurnTerminal {
                        turn_id: canonical,
                        terminal,
                        message: None,
                        diagnostic: None,
                    }),
                    presentation: required_presentation(&method, presentation)?,
                }))
            }
            "item/started" => self.map_item(
                &method,
                &params,
                false,
                required_presentation(&method, presentation)?,
            ),
            "item/completed" => self.map_item(
                &method,
                &params,
                true,
                required_presentation(&method, presentation)?,
            ),
            "item/agentMessage/delta"
            | "item/reasoning/textDelta"
            | "item/reasoning/summaryTextDelta"
            | "item/plan/delta"
            | "item/commandExecution/outputDelta"
            | "item/fileChange/outputDelta" => {
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
                    runtime_event: Some(RuntimeEvent::ConversationDelta {
                        turn_id,
                        item_id,
                        delta: match method.as_str() {
                            "item/agentMessage/delta" => agentdash_agent_runtime_contract::RuntimeConversationDelta::AgentMessage { delta },
                            "item/reasoning/textDelta" => agentdash_agent_runtime_contract::RuntimeConversationDelta::ReasoningText { delta },
                            "item/reasoning/summaryTextDelta" => agentdash_agent_runtime_contract::RuntimeConversationDelta::ReasoningSummary { delta },
                            "item/plan/delta" => agentdash_agent_runtime_contract::RuntimeConversationDelta::Plan { delta },
                            "item/commandExecution/outputDelta" => agentdash_agent_runtime_contract::RuntimeConversationDelta::CommandOutput { delta },
                            "item/fileChange/outputDelta" => agentdash_agent_runtime_contract::RuntimeConversationDelta::FileChangeOutput { delta },
                            _ => unreachable!("method admission is exhaustive"),
                        },
                    }),
                    presentation: required_presentation(&method, presentation)?,
                }))
            }
            "item/mcpToolCall/progress" => {
                let source_turn =
                    string(&params, "turnId").ok_or_else(|| missing(&method, "turnId"))?;
                let source_item =
                    string(&params, "itemId").ok_or_else(|| missing(&method, "itemId"))?;
                let message =
                    string(&params, "message").ok_or_else(|| missing(&method, "message"))?;
                Ok(Some(MappedEvent { source_turn_id:Some(driver_turn(&source_turn)?), source_item_id:Some(driver_item(&source_item)?), runtime_event:Some(RuntimeEvent::ConversationDelta { turn_id:self.turn_for(&source_turn)?, item_id:self.item_for(&source_item)?, delta:agentdash_agent_runtime_contract::RuntimeConversationDelta::McpProgress { message } }), presentation:required_presentation(&method, presentation)? }))
            }
            "thread/tokenUsage/updated" => {
                let source_turn =
                    string(&params, "turnId").ok_or_else(|| missing(&method, "turnId"))?;
                let usage = params
                    .get("tokenUsage")
                    .and_then(|value| value.get("last"))
                    .ok_or_else(|| missing(&method, "tokenUsage.last"))?;
                let number = |field: &'static str| {
                    usage
                        .get(field)
                        .and_then(Value::as_u64)
                        .ok_or_else(|| missing(&method, field))
                };
                Ok(Some(MappedEvent {
                    source_turn_id: Some(driver_turn(&source_turn)?),
                    source_item_id: None,
                    runtime_event: Some(RuntimeEvent::TokenUsageUpdated {
                        turn_id: self.turn_for(&source_turn)?,
                        usage: agentdash_agent_runtime_contract::RuntimeTokenUsage {
                            input_tokens: number("inputTokens")?,
                            cached_input_tokens: number("cachedInputTokens")?,
                            output_tokens: number("outputTokens")?,
                            reasoning_output_tokens: number("reasoningOutputTokens")?,
                            total_tokens: number("totalTokens")?,
                        },
                    }),
                    presentation: required_presentation(&method, presentation)?,
                }))
            }
            "error" => {
                let source_turn =
                    string(&params, "turnId").ok_or_else(|| missing(&method, "turnId"))?;
                let error = params
                    .get("error")
                    .ok_or_else(|| missing(&method, "error"))?;
                Ok(Some(MappedEvent {
                    source_turn_id: Some(driver_turn(&source_turn)?),
                    source_item_id: None,
                    runtime_event: Some(RuntimeEvent::ConversationError {
                        turn_id: Some(self.turn_for(&source_turn)?),
                        error: agentdash_agent_runtime_contract::RuntimeConversationError {
                            code: None,
                            message: string(error, "message")
                                .ok_or_else(|| missing(&method, "error.message"))?,
                            retryable: params
                                .get("willRetry")
                                .and_then(Value::as_bool)
                                .ok_or_else(|| missing(&method, "willRetry"))?,
                            details: None,
                        },
                    }),
                    presentation: required_presentation(&method, presentation)?,
                }))
            }
            "thread/compacted" => Ok(Some(MappedEvent {
                source_turn_id: string(&params, "turnId").map(driver_turn).transpose()?,
                source_item_id: None,
                runtime_event: Some(RuntimeEvent::DriverContextCompactedOpaque),
                presentation: required_presentation(&method, presentation)?,
            })),
            // Hook notifications are consumed by the adapter's native-hook reconciliation.
            // They are admitted with their generated types but do not create Main session facts.
            "hook/started" | "hook/completed" => Ok(None),
            // These Main presentation families do not mutate canonical Runtime lifecycle state.
            // They still need a presentation-only Driver fact; the Driver pump handles that
            // separately from this Runtime state mapping.
            "turn/diff/updated" | "turn/plan/updated" => Ok(Some(MappedEvent {
                source_turn_id: string(&params, "turnId").map(driver_turn).transpose()?,
                source_item_id: None,
                runtime_event: None,
                presentation: required_presentation(&method, presentation)?,
            })),
            "thread/name/updated" => Ok(presentation.map(|presentation| MappedEvent {
                source_turn_id: None,
                source_item_id: None,
                runtime_event: None,
                presentation,
            })),
            "item/autoApprovalReview/started"
            | "item/autoApprovalReview/completed"
            | "item/reasoning/summaryPartAdded"
            | "item/commandExecution/terminalInteraction"
            | "item/fileChange/patchUpdated"
            | "serverRequest/resolved"
            | "model/rerouted"
            | "model/verification"
            | "turn/moderationMetadata"
            | "model/safetyBuffering/updated"
            | "warning"
            | "guardianWarning"
            | "deprecationNotice"
            | "configWarning" => Ok(Some(MappedEvent {
                source_turn_id: string(&params, "turnId").map(driver_turn).transpose()?,
                source_item_id: string(&params, "itemId").map(driver_item).transpose()?,
                runtime_event: None,
                presentation: required_presentation(&method, presentation)?,
            })),
            "thread/status/changed" => {
                let source_status = params
                    .pointer("/status/type")
                    .or_else(|| params.pointer("/thread/status/type"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| missing(&method, "status.type"))?;
                let status = match source_status {
                    "active" | "idle" | "notLoaded" => {
                        agentdash_agent_runtime_contract::RuntimeThreadStatus::Active
                    }
                    "systemError" => {
                        agentdash_agent_runtime_contract::RuntimeThreadStatus::Desynchronized
                    }
                    other => {
                        return Err(MappingError::InvalidTerminal {
                            method,
                            status: other.to_string(),
                        });
                    }
                };
                Ok(Some(MappedEvent {
                    source_turn_id: None,
                    source_item_id: None,
                    runtime_event: Some(RuntimeEvent::ThreadStatusChanged { status }),
                    presentation: required_presentation(&method, presentation)?,
                }))
            }
            _ => Err(MappingError::UnsupportedMethod(method)),
        }
    }

    pub fn map_server_request(
        &mut self,
        request: &RpcServerRequest,
    ) -> Result<MappedInteraction, MappingError> {
        admit_server_request(&request.method, &request.params)?;
        let presentation = interaction_presentation(request)?;
        let source_turn = string(&request.params, "turnId").ok_or_else(|| {
            if request.method == "mcpServer/elicitation/request" {
                MappingError::UnsupportedThreadScopedInteraction {
                    method: request.method.clone(),
                }
            } else {
                missing(&request.method, "turnId")
            }
        })?;
        let source_item = string(&request.params, "itemId");
        let turn_id = self.turn_for(&source_turn)?;
        let source_request_id = rpc_coordinate(&request.id);
        let interaction_key = format!(
            "{}:{}:{}:{}",
            source_turn,
            source_item.as_deref().unwrap_or("thread"),
            request.method.replace('/', "-"),
            source_request_id,
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
                let params: codex::PermissionsRequestApprovalParams =
                    serde_json::from_value(request.params.clone())
                        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?;
                agentdash_agent_runtime_contract::RuntimeInteractionRequest::workspace_permission_approval(
                    params.item_id,
                    params.cwd.display().to_string(),
                    serde_json::to_value(params.permissions)
                        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?,
                    params.reason,
                    params.started_at_ms,
                )
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
            source_request_id,
            event: RuntimeEvent::InteractionRequested {
                turn_id,
                item_id,
                interaction_id,
                request: interaction_request,
            },
            presentation,
        })
    }

    fn map_item(
        &mut self,
        method: &str,
        params: &Value,
        completed: bool,
        presentation: ImmutablePresentationEvent,
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
            runtime_event: Some(event),
            presentation,
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

fn strict_owned<V, O>(value: &Value) -> Result<O, MappingError>
where
    V: serde::de::DeserializeOwned,
    O: serde::de::DeserializeOwned,
{
    serde_json::from_value::<V>(value.clone())
        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?;
    serde_json::from_value::<O>(value.clone())
        .map_err(|error| MappingError::OwnedProtocolMismatch(error.to_string()))
}

fn strict_transcode<V, O>(value: &Value) -> Result<(), MappingError>
where
    V: serde::de::DeserializeOwned,
    O: serde::de::DeserializeOwned,
{
    strict_owned::<V, O>(value).map(drop)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationDisposition {
    Projected,
    TypedNoop,
}

const PROJECTED_SERVER_NOTIFICATION_METHODS: [&str; 34] = [
    "error",
    "thread/status/changed",
    "thread/name/updated",
    "thread/tokenUsage/updated",
    "turn/started",
    "hook/started",
    "turn/completed",
    "hook/completed",
    "turn/diff/updated",
    "turn/plan/updated",
    "item/started",
    "item/autoApprovalReview/started",
    "item/autoApprovalReview/completed",
    "item/completed",
    "item/agentMessage/delta",
    "item/plan/delta",
    "item/commandExecution/outputDelta",
    "item/commandExecution/terminalInteraction",
    "item/fileChange/outputDelta",
    "item/fileChange/patchUpdated",
    "serverRequest/resolved",
    "item/mcpToolCall/progress",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/summaryPartAdded",
    "item/reasoning/textDelta",
    "thread/compacted",
    "model/rerouted",
    "model/verification",
    "turn/moderationMetadata",
    "model/safetyBuffering/updated",
    "warning",
    "guardianWarning",
    "deprecationNotice",
    "configWarning",
];

const TYPED_NOOP_SERVER_NOTIFICATION_METHODS: [&str; 34] = [
    "thread/started",
    "thread/archived",
    "thread/deleted",
    "thread/unarchived",
    "thread/closed",
    "skills/changed",
    "thread/goal/updated",
    "thread/goal/cleared",
    "thread/settings/updated",
    "command/exec/outputDelta",
    "process/outputDelta",
    "process/exited",
    "mcpServer/oauthLogin/completed",
    "mcpServer/startupStatus/updated",
    "account/updated",
    "account/rateLimits/updated",
    "app/list/updated",
    "remoteControl/status/changed",
    "externalAgentConfig/import/progress",
    "externalAgentConfig/import/completed",
    "fs/changed",
    "fuzzyFileSearch/sessionUpdated",
    "fuzzyFileSearch/sessionCompleted",
    "thread/realtime/started",
    "thread/realtime/itemAdded",
    "thread/realtime/transcript/delta",
    "thread/realtime/transcript/done",
    "thread/realtime/outputAudio/delta",
    "thread/realtime/sdp",
    "thread/realtime/error",
    "thread/realtime/closed",
    "windows/worldWritableWarning",
    "windowsSandbox/setupCompleted",
    "account/login/completed",
];

fn admit_notification(
    method: &str,
    params: &Value,
) -> Result<NotificationDisposition, MappingError> {
    use agentdash_agent_protocol::generated::codex_v2::server_notification as owned;

    if !PROJECTED_SERVER_NOTIFICATION_METHODS.contains(&method)
        && !TYPED_NOOP_SERVER_NOTIFICATION_METHODS.contains(&method)
    {
        return Err(MappingError::UnsupportedMethod(method.to_string()));
    }
    let notification = strict_owned::<codex::ServerNotification, owned::ServerNotification>(
        &serde_json::json!({ "method": method, "params": params }),
    )?;
    Ok(classify_owned_notification(notification))
}

/// Keep this match exhaustive over the generated 0.144.1 protocol. A protocol
/// regeneration that adds a notification must make an explicit projection or
/// typed no-op decision here before the adapter can compile again.
fn classify_owned_notification(
    notification: agentdash_agent_protocol::generated::codex_v2::server_notification::ServerNotification,
) -> NotificationDisposition {
    use agentdash_agent_protocol::generated::codex_v2::server_notification::ServerNotification as N;

    match notification {
        N::Error(_)
        | N::ThreadStatusChanged(_)
        | N::ThreadNameUpdated(_)
        | N::ThreadTokenUsageUpdated(_)
        | N::TurnStarted(_)
        | N::HookStarted(_)
        | N::TurnCompleted(_)
        | N::HookCompleted(_)
        | N::TurnDiffUpdated(_)
        | N::TurnPlanUpdated(_)
        | N::ItemStarted(_)
        | N::ItemAutoApprovalReviewStarted(_)
        | N::ItemAutoApprovalReviewCompleted(_)
        | N::ItemCompleted(_)
        | N::ItemAgentMessageDelta(_)
        | N::ItemPlanDelta(_)
        | N::ItemCommandExecutionOutputDelta(_)
        | N::ItemCommandExecutionTerminalInteraction(_)
        | N::ItemFileChangeOutputDelta(_)
        | N::ItemFileChangePatchUpdated(_)
        | N::ServerRequestResolved(_)
        | N::ItemMcpToolCallProgress(_)
        | N::ItemReasoningSummaryTextDelta(_)
        | N::ItemReasoningSummaryPartAdded(_)
        | N::ItemReasoningTextDelta(_)
        | N::ThreadCompacted(_)
        | N::ModelRerouted(_)
        | N::ModelVerification(_)
        | N::TurnModerationMetadata(_)
        | N::ModelSafetyBufferingUpdated(_)
        | N::Warning(_)
        | N::GuardianWarning(_)
        | N::DeprecationNotice(_)
        | N::ConfigWarning(_) => NotificationDisposition::Projected,
        N::ThreadStarted(_)
        | N::ThreadArchived(_)
        | N::ThreadDeleted(_)
        | N::ThreadUnarchived(_)
        | N::ThreadClosed(_)
        | N::SkillsChanged(_)
        | N::ThreadGoalUpdated(_)
        | N::ThreadGoalCleared(_)
        | N::ThreadSettingsUpdated(_)
        | N::CommandExecOutputDelta(_)
        | N::ProcessOutputDelta(_)
        | N::ProcessExited(_)
        | N::McpServerOauthLoginCompleted(_)
        | N::McpServerStartupStatusUpdated(_)
        | N::AccountUpdated(_)
        | N::AccountRateLimitsUpdated(_)
        | N::AppListUpdated(_)
        | N::RemoteControlStatusChanged(_)
        | N::ExternalAgentConfigImportProgress(_)
        | N::ExternalAgentConfigImportCompleted(_)
        | N::FsChanged(_)
        | N::FuzzyFileSearchSessionUpdated(_)
        | N::FuzzyFileSearchSessionCompleted(_)
        | N::ThreadRealtimeStarted(_)
        | N::ThreadRealtimeItemAdded(_)
        | N::ThreadRealtimeTranscriptDelta(_)
        | N::ThreadRealtimeTranscriptDone(_)
        | N::ThreadRealtimeOutputAudioDelta(_)
        | N::ThreadRealtimeSdp(_)
        | N::ThreadRealtimeError(_)
        | N::ThreadRealtimeClosed(_)
        | N::WindowsWorldWritableWarning(_)
        | N::WindowsSandboxSetupCompleted(_)
        | N::AccountLoginCompleted(_) => NotificationDisposition::TypedNoop,
    }
}

fn presentation(
    durability: PresentationDurability,
    event: agentdash_agent_protocol::BackboneEvent,
) -> Option<ImmutablePresentationEvent> {
    Some(ImmutablePresentationEvent::new(durability, event))
}

fn required_presentation(
    method: &str,
    presentation: Option<ImmutablePresentationEvent>,
) -> Result<ImmutablePresentationEvent, MappingError> {
    presentation.ok_or_else(|| MappingError::UnsupportedMethod(method.to_string()))
}

fn presentation_notification(
    method: &str,
    params: &Value,
) -> Result<Option<ImmutablePresentationEvent>, MappingError> {
    use PresentationDurability::{Durable, Ephemeral};
    use agentdash_agent_protocol::generated::codex_v2::server_notification as owned;
    use agentdash_agent_protocol::{
        BackboneEvent, ItemCompletedNotification, ItemStartedNotification, PlatformEvent,
    };
    match method {
        "turn/started" => {
            strict_owned::<codex::TurnStartedNotification, owned::TurnStartedNotification>(params)
                .map(|value| presentation(Durable, BackboneEvent::TurnStarted(value)))
        }
        "turn/completed" => strict_owned::<
            codex::TurnCompletedNotification,
            owned::TurnCompletedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::TurnCompleted(value))),
        "item/started" => {
            strict_owned::<codex::ItemStartedNotification, owned::ItemStartedNotification>(params)
                .map(|value| {
                    presentation(
                        Durable,
                        BackboneEvent::ItemStarted(ItemStartedNotification {
                            item: value.item.into(),
                            thread_id: value.thread_id,
                            turn_id: value.turn_id,
                            started_at_ms: value.started_at_ms,
                        }),
                    )
                })
        }
        "item/completed" => strict_owned::<
            codex::ItemCompletedNotification,
            owned::ItemCompletedNotification,
        >(params)
        .map(|value| {
            presentation(
                Durable,
                BackboneEvent::ItemCompleted(ItemCompletedNotification {
                    item: value.item.into(),
                    thread_id: value.thread_id,
                    turn_id: value.turn_id,
                    completed_at_ms: value.completed_at_ms,
                }),
            )
        }),
        "item/agentMessage/delta" => strict_owned::<
            codex::AgentMessageDeltaNotification,
            owned::AgentMessageDeltaNotification,
        >(params)
        .map(|value| presentation(Ephemeral, BackboneEvent::AgentMessageDelta(value))),
        "item/reasoning/textDelta" => strict_owned::<
            codex::ReasoningTextDeltaNotification,
            owned::ReasoningTextDeltaNotification,
        >(params)
        .map(|value| presentation(Ephemeral, BackboneEvent::ReasoningTextDelta(value))),
        "item/reasoning/summaryTextDelta" => strict_owned::<
            codex::ReasoningSummaryTextDeltaNotification,
            owned::ReasoningSummaryTextDeltaNotification,
        >(params)
        .map(|value| presentation(Ephemeral, BackboneEvent::ReasoningSummaryDelta(value))),
        "item/plan/delta" => {
            strict_owned::<codex::PlanDeltaNotification, owned::PlanDeltaNotification>(params)
                .map(|value| presentation(Durable, BackboneEvent::PlanDelta(value)))
        }
        "item/commandExecution/outputDelta" => strict_owned::<
            codex::CommandExecutionOutputDeltaNotification,
            owned::CommandExecutionOutputDeltaNotification,
        >(params)
        .map(|value| presentation(Ephemeral, BackboneEvent::CommandOutputDelta(value))),
        "item/fileChange/outputDelta" => strict_owned::<
            codex::FileChangeOutputDeltaNotification,
            owned::FileChangeOutputDeltaNotification,
        >(params)
        .map(|value| presentation(Ephemeral, BackboneEvent::FileChangeDelta(value))),
        "item/mcpToolCall/progress" => strict_owned::<
            codex::McpToolCallProgressNotification,
            owned::McpToolCallProgressNotification,
        >(params)
        .map(|value| presentation(Ephemeral, BackboneEvent::McpToolCallProgress(value))),
        "turn/diff/updated" => strict_owned::<
            codex::TurnDiffUpdatedNotification,
            owned::TurnDiffUpdatedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::TurnDiffUpdated(value))),
        "turn/plan/updated" => strict_owned::<
            codex::TurnPlanUpdatedNotification,
            owned::TurnPlanUpdatedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::TurnPlanUpdated(value))),
        "thread/tokenUsage/updated" => strict_owned::<
            codex::ThreadTokenUsageUpdatedNotification,
            owned::ThreadTokenUsageUpdatedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::TokenUsageUpdated(value.into()))),
        "thread/status/changed" => strict_owned::<
            codex::ThreadStatusChangedNotification,
            owned::ThreadStatusChangedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::ThreadStatusChanged(value))),
        "thread/name/updated" => strict_owned::<
            codex::ThreadNameUpdatedNotification,
            owned::ThreadNameUpdatedNotification,
        >(params)
        .map(|value| {
            value.thread_name.and_then(|title| {
                let title = title.trim();
                (!title.is_empty()).then(|| {
                    ImmutablePresentationEvent::new(
                        Durable,
                        BackboneEvent::Platform(PlatformEvent::SourceSessionTitleUpdated {
                            executor_session_id: Some(value.thread_id),
                            title: title.to_string(),
                            preview: None,
                            source: "codex".to_string(),
                        }),
                    )
                })
            })
        }),
        "error" => strict_owned::<codex::ErrorNotification, owned::ErrorNotification>(params)
            .map(|value| presentation(Durable, BackboneEvent::Error(value))),
        "thread/compacted" => strict_owned::<
            codex::ContextCompactedNotification,
            owned::ContextCompactedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::ExecutorContextCompacted(value))),
        "item/autoApprovalReview/started" => strict_owned::<
            codex::ItemGuardianApprovalReviewStartedNotification,
            owned::ItemGuardianApprovalReviewStartedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::AutoApprovalReviewStarted(value))),
        "item/autoApprovalReview/completed" => strict_owned::<
            codex::ItemGuardianApprovalReviewCompletedNotification,
            owned::ItemGuardianApprovalReviewCompletedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::AutoApprovalReviewCompleted(value))),
        "item/reasoning/summaryPartAdded" => strict_owned::<
            codex::ReasoningSummaryPartAddedNotification,
            owned::ReasoningSummaryPartAddedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::ReasoningSummaryPartAdded(value))),
        "item/commandExecution/terminalInteraction" => strict_owned::<
            codex::TerminalInteractionNotification,
            owned::TerminalInteractionNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::TerminalInteraction(value))),
        "item/fileChange/patchUpdated" => strict_owned::<
            codex::FileChangePatchUpdatedNotification,
            owned::FileChangePatchUpdatedNotification,
        >(params)
        .map(|value| presentation(Ephemeral, BackboneEvent::FileChangePatchUpdated(value))),
        "serverRequest/resolved" => strict_owned::<
            codex::ServerRequestResolvedNotification,
            owned::ServerRequestResolvedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::ServerRequestResolved(value))),
        "model/rerouted" => strict_owned::<
            codex::ModelReroutedNotification,
            owned::ModelReroutedNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::ModelRerouted(value))),
        "model/verification" => strict_owned::<
            codex::ModelVerificationNotification,
            owned::ModelVerificationNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::ModelVerification(value))),
        "turn/moderationMetadata" => strict_owned::<
            codex::TurnModerationMetadataNotification,
            owned::TurnModerationMetadataNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::TurnModerationMetadata(value))),
        "model/safetyBuffering/updated" => strict_owned::<
            codex::ModelSafetyBufferingUpdatedNotification,
            owned::ModelSafetyBufferingUpdatedNotification,
        >(params)
        .map(|value| presentation(Ephemeral, BackboneEvent::ModelSafetyBufferingUpdated(value))),
        "warning" => strict_owned::<codex::WarningNotification, owned::WarningNotification>(params)
            .map(|value| presentation(Durable, BackboneEvent::Warning(value))),
        "guardianWarning" => strict_owned::<
            codex::GuardianWarningNotification,
            owned::GuardianWarningNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::GuardianWarning(value))),
        "deprecationNotice" => strict_owned::<
            codex::DeprecationNoticeNotification,
            owned::DeprecationNoticeNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::DeprecationNotice(value))),
        "configWarning" => strict_owned::<
            codex::ConfigWarningNotification,
            owned::ConfigWarningNotification,
        >(params)
        .map(|value| presentation(Durable, BackboneEvent::ConfigWarning(value))),
        "hook/started" => strict_transcode::<
            codex::HookStartedNotification,
            owned::HookStartedNotification,
        >(params)
        .map(|()| None),
        "hook/completed" => strict_transcode::<
            codex::HookCompletedNotification,
            owned::HookCompletedNotification,
        >(params)
        .map(|()| None),
        other => Err(MappingError::UnsupportedMethod(other.to_string())),
    }
}

fn interaction_presentation(
    request: &RpcServerRequest,
) -> Result<Option<ImmutablePresentationEvent>, MappingError> {
    use agentdash_agent_protocol::generated::codex_v2::server_notification::RequestId;
    use agentdash_agent_protocol::{ApprovalRequest, BackboneEvent};

    let request_id: RequestId = serde_json::from_value(request.id.clone())
        .map_err(|error| MappingError::InvalidItemPayload(error.to_string()))?;
    let approval = match request.method.as_str() {
        "item/commandExecution/requestApproval" => ApprovalRequest::CommandExecution {
            request_id,
            params: *strict_interaction_params::<codex::CommandExecutionRequestApprovalParams, _>(
                request.params.clone(),
            )?,
        },
        "item/fileChange/requestApproval" => ApprovalRequest::FileChange {
            request_id,
            params: *strict_interaction_params::<codex::FileChangeRequestApprovalParams, _>(
                request.params.clone(),
            )?,
        },
        "item/permissions/requestApproval" => ApprovalRequest::PermissionsApproval {
            request_id,
            params: *strict_interaction_params::<codex::PermissionsRequestApprovalParams, _>(
                request.params.clone(),
            )?,
        },
        "item/tool/requestUserInput" => ApprovalRequest::ToolUserInput {
            request_id,
            params: *strict_interaction_params::<codex::ToolRequestUserInputParams, _>(
                request.params.clone(),
            )?,
        },
        "item/tool/call" | "mcpServer/elicitation/request" => return Ok(None),
        other => return Err(MappingError::UnsupportedMethod(other.to_string())),
    };
    Ok(presentation(
        PresentationDurability::Durable,
        BackboneEvent::ApprovalRequest(approval),
    ))
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
            RuntimeInput::UserInput { block } => native
                .push(serde_json::to_value(block).expect("generated Codex UserInput serializes")),
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
pub(crate) fn rpc_coordinate(id: &Value) -> String {
    match id {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_main_presentation_fixture_is_deep_equal_without_payload_normalization() {
        let fixture: Value =
            serde_json::from_str(include_str!("../fixtures/main-presentation.json"))
                .expect("valid Codex Main fixture");
        assert_eq!(
            fixture["oracle_commit"],
            "957fa9d60ea3d67efa1bb278fe5b376cf0c34598"
        );
        assert_eq!(fixture["protocol_revision"], "0.144.1");

        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        map.register_item("source-item");
        let mut main = Vec::new();
        let mut current = Vec::new();
        for scenario in fixture["scenarios"].as_array().expect("scenario array") {
            let method = scenario["method"].as_str().expect("method");
            let mapped = map
                .map_notification(RpcServerNotification {
                    method: method.to_string(),
                    params: scenario["params"].clone(),
                })
                .unwrap_or_else(|error| panic!("{method}: {error}"))
                .unwrap_or_else(|| panic!("{method}: expected presentation"));
            let expected_durability = match scenario["durability"].as_str() {
                Some("durable") => PresentationDurability::Durable,
                Some("ephemeral") => PresentationDurability::Ephemeral,
                other => panic!("invalid durability: {other:?}"),
            };
            assert_eq!(
                mapped.presentation.durability, expected_durability,
                "{method}"
            );
            let parity_durability = match expected_durability {
                PresentationDurability::Durable => {
                    agentdash_agent_runtime_test_support::session_parity::PresentationDurability::Durable
                }
                PresentationDurability::Ephemeral => {
                    agentdash_agent_runtime_test_support::session_parity::PresentationDurability::Ephemeral
                }
            };
            main.push(
                agentdash_agent_runtime_test_support::session_parity::NormalizedPresentationEvent {
                    durability: parity_durability,
                    event: scenario["event"].clone(),
                },
            );
            let source_request_id = mapped.source_request_id();
            let carrier = agentdash_agent_runtime_contract::DriverEventEnvelope {
                binding_id: agentdash_agent_runtime_contract::RuntimeBindingId::new(
                    "fixture-binding",
                )
                .expect("fixture binding id"),
                generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration(1),
                operation_id: None,
                source_thread_id: agentdash_agent_runtime_contract::DriverThreadId::new(
                    "source-thread",
                )
                .expect("fixture source thread id"),
                source_turn_id: mapped.source_turn_id,
                source_item_id: mapped.source_item_id,
                source_request_id,
                source_entry_index: None,
                facts: vec![
                    agentdash_agent_runtime_contract::RuntimeJournalFact::Presentation(
                        mapped.presentation,
                    ),
                ],
            };
            assert_eq!(carrier.source_entry_index, None, "{method}");
            let [agentdash_agent_runtime_contract::RuntimeJournalFact::Presentation(presentation)] =
                carrier.facts.as_slice()
            else {
                panic!("{method}: expected one presentation fact");
            };
            current.push(
                agentdash_agent_runtime_test_support::session_parity::NormalizedPresentationEvent {
                    durability: parity_durability,
                    event: serde_json::to_value(&presentation.event).expect("presentation JSON"),
                },
            );
        }
        agentdash_agent_runtime_test_support::session_parity::compare_ordered_presentation_events(
            &main, &current,
        )
        .expect("Codex Main/current protected bodies must be deep equal");
    }

    #[test]
    fn turn_started_keeps_codex_source_as_presentation_turn_identity() {
        let fixture: Value =
            serde_json::from_str(include_str!("../fixtures/main-presentation.json"))
                .expect("valid Codex Main fixture");
        let scenario = fixture["scenarios"]
            .as_array()
            .expect("scenario array")
            .iter()
            .find(|scenario| scenario["method"] == "turn/started")
            .expect("turn started fixture");
        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        let mapped = map
            .map_notification(RpcServerNotification {
                method: "turn/started".to_string(),
                params: scenario["params"].clone(),
            })
            .expect("map turn started")
            .expect("turn started event");
        assert!(matches!(
            mapped.runtime_event,
            Some(RuntimeEvent::TurnStarted {
                turn_id,
                presentation_turn_id,
            }) if turn_id.as_str() == "runtime-turn"
                && presentation_turn_id.as_str() == "source-turn"
        ));
    }

    #[test]
    fn resolved_server_request_keeps_request_id_in_body_and_carrier() {
        let mapped = SourceCoordinateMap::default()
            .map_notification(RpcServerNotification {
                method: "serverRequest/resolved".to_string(),
                params: serde_json::json!({
                    "threadId": "source-thread",
                    "requestId": 42
                }),
            })
            .expect("typed notification")
            .expect("presentation event");

        assert_eq!(mapped.source_request_id().as_deref(), Some("42"));
        let body = serde_json::to_value(mapped.presentation.event).expect("presentation JSON");
        assert_eq!(body["type"], "server_request_resolved");
        assert_eq!(body["payload"]["requestId"], 42);
    }

    #[test]
    fn item_lifecycle_and_delta_keep_the_same_source_identity() {
        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        map.register_item("source-item");
        let notifications = [
            RpcServerNotification {
                method: "item/started".to_string(),
                params: serde_json::json!({
                    "threadId":"source-thread","turnId":"source-turn","startedAtMs":1,
                    "item":{"type":"agentMessage","id":"source-item","text":"","phase":null,"memoryCitation":null}
                }),
            },
            RpcServerNotification {
                method: "item/agentMessage/delta".to_string(),
                params: serde_json::json!({
                    "threadId":"source-thread","turnId":"source-turn","itemId":"source-item","delta":"hello"
                }),
            },
            RpcServerNotification {
                method: "item/completed".to_string(),
                params: serde_json::json!({
                    "threadId":"source-thread","turnId":"source-turn","completedAtMs":2,
                    "item":{"type":"agentMessage","id":"source-item","text":"hello","phase":null,"memoryCitation":null}
                }),
            },
        ];

        let bodies = notifications
            .into_iter()
            .map(|notification| {
                let mapped = map
                    .map_notification(notification)
                    .expect("typed notification")
                    .expect("presentation event");
                assert_eq!(
                    mapped.source_item_id.as_ref().map(DriverItemId::as_str),
                    Some("source-item")
                );
                serde_json::to_value(mapped.presentation.event).expect("presentation JSON")
            })
            .collect::<Vec<_>>();

        assert_eq!(bodies[0]["payload"]["item"]["id"], "source-item");
        assert_eq!(bodies[1]["payload"]["itemId"], "source-item");
        assert_eq!(bodies[2]["payload"]["item"]["id"], "source-item");
    }

    #[test]
    fn nullable_or_blank_thread_name_is_an_admitted_noop() {
        for thread_name in [Value::Null, Value::String("   ".to_string())] {
            let mapped = SourceCoordinateMap::default()
                .map_notification(RpcServerNotification {
                    method: "thread/name/updated".to_string(),
                    params: serde_json::json!({
                        "threadId": "source-thread",
                        "threadName": thread_name
                    }),
                })
                .expect("nullable title is a valid Codex notification");
            assert!(mapped.is_none());
        }
    }

    #[test]
    fn typed_delta_usage_error_and_compaction_matrix_is_admitted() {
        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        map.register_item("source-item");
        let fixtures = [
            (
                "item/reasoning/summaryTextDelta",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","itemId":"source-item","delta":"summary","summaryIndex":0}),
            ),
            (
                "item/commandExecution/outputDelta",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","itemId":"source-item","delta":"out"}),
            ),
            (
                "item/fileChange/outputDelta",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","itemId":"source-item","delta":"patch"}),
            ),
            (
                "item/mcpToolCall/progress",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","itemId":"source-item","message":"working"}),
            ),
            (
                "thread/tokenUsage/updated",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","tokenUsage":{"last":{"inputTokens":1,"cachedInputTokens":2,"outputTokens":3,"reasoningOutputTokens":4,"totalTokens":10},"total":{"inputTokens":1,"cachedInputTokens":2,"outputTokens":3,"reasoningOutputTokens":4,"totalTokens":10},"modelContextWindow":null}}),
            ),
            (
                "error",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn","willRetry":true,"error":{"message":"retry","additionalDetails":null,"codexErrorInfo":null}}),
            ),
            (
                "thread/compacted",
                serde_json::json!({"threadId":"source-thread","turnId":"source-turn"}),
            ),
        ];
        for (method, params) in fixtures {
            assert!(
                map.map_notification(RpcServerNotification {
                    method: method.to_string(),
                    params
                })
                .unwrap_or_else(|error| panic!("{method}: {error}"))
                .is_some()
            );
        }
    }
    use crate::rpc::RpcServerNotification;

    #[test]
    fn standard_user_input_is_projected_to_exact_codex_json() {
        let expected = vec![
            serde_json::json!({
                "type": "text",
                "text": "ask @agent",
                "text_elements": [{
                    "byteRange": { "start": 4, "end": 10 },
                    "placeholder": null
                }]
            }),
            serde_json::json!({ "type": "image", "url": "https://example.test/absent.png" }),
            serde_json::json!({ "type": "image", "detail": null, "url": "https://example.test/null.png" }),
            serde_json::json!({ "type": "image", "detail": "original", "url": "https://example.test/original.png" }),
            serde_json::json!({ "type": "localImage", "detail": null, "path": "C:/workspace/local.png" }),
            serde_json::json!({ "type": "skill", "name": "review", "path": "C:/skills/review/SKILL.md" }),
            serde_json::json!({ "type": "mention", "name": "main.rs", "path": "C:/workspace/src/main.rs" }),
        ];
        let runtime = expected
            .iter()
            .cloned()
            .map(|value| {
                RuntimeInput::user_input(
                    serde_json::from_value(value).expect("generated Codex UserInput"),
                )
            })
            .collect::<Vec<_>>();

        let (input, additional) = map_input(&runtime);

        assert_eq!(input, expected);
        assert!(additional.is_none());
    }

    #[test]
    fn structured_input_uses_typed_additional_context_without_changing_user_input() {
        let (input, additional) = map_input(&[
            RuntimeInput::user_input(
                serde_json::from_value(serde_json::json!({
                    "type": "image",
                    "detail": "high",
                    "url": "data:image/png;base64,AA=="
                }))
                .expect("generated Codex image input"),
            ),
            RuntimeInput::Structured {
                schema: "answer.v1".to_string(),
                value: serde_json::json!({"choice": 2}),
            },
        ]);
        assert_eq!(
            input,
            vec![serde_json::json!({
                "type": "image",
                "detail": "high",
                "url": "data:image/png;base64,AA=="
            })]
        );
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
            event.runtime_event,
            Some(RuntimeEvent::TurnTerminal {
                terminal: RuntimeTurnTerminal::Interrupted,
                ..
            })
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
    fn main_request_policy_is_method_specific_instead_of_unified_approval() {
        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let fixtures = [
            (
                "item/commandExecution/requestApproval",
                serde_json::json!({
                    "threadId":"source-thread","turnId":"source-turn","itemId":"source-item",
                    "approvalId":null,"command":"cargo test","commandActions":[],
                    "environmentId":null,"proposedExecpolicyAmendment":null,
                    "reason":"run","startedAtMs":1
                }),
                Some(serde_json::json!({"decision":"acceptForSession"})),
            ),
            (
                "item/fileChange/requestApproval",
                serde_json::json!({
                    "threadId":"source-thread","turnId":"source-turn","itemId":"source-item",
                    "grantRoot":null,"reason":"write","startedAtMs":2
                }),
                Some(serde_json::json!({"decision":"acceptForSession"})),
            ),
            (
                "item/tool/requestUserInput",
                serde_json::json!({
                    "threadId":"source-thread","turnId":"source-turn","itemId":"source-item",
                    "autoResolutionMs":null,"questions":[]
                }),
                Some(serde_json::json!({"answers":{}})),
            ),
            (
                "item/permissions/requestApproval",
                serde_json::json!({
                    "threadId":"source-thread","turnId":"source-turn","itemId":"source-item",
                    "cwd":cwd,"permissions":{},"reason":"access","startedAtMs":3
                }),
                None,
            ),
        ];
        for (index, (method, params, expected)) in fixtures.into_iter().enumerate() {
            let actual = main_automatic_server_response(&RpcServerRequest {
                id: serde_json::json!(index),
                method: method.to_string(),
                params,
            })
            .unwrap_or_else(|error| panic!("{method}: {error}"));
            assert_eq!(actual, expected, "{method}");
        }
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
    fn permission_interaction_keeps_rpc_request_identity_in_payload_and_carrier_coordinate() {
        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        map.register_item("source-item");
        let mapped = map
            .map_server_request(&RpcServerRequest {
                id: serde_json::json!(17),
                method: "item/permissions/requestApproval".to_string(),
                params: serde_json::json!({
                    "threadId":"source-thread","turnId":"source-turn","itemId":"source-item",
                    "cwd":cwd,"permissions":{},"reason":"access","startedAtMs":125
                }),
            })
            .expect("permission interaction");
        assert_eq!(mapped.source_request_id, "17");
        let body = serde_json::to_value(
            mapped
                .presentation
                .expect("permission approval presentation")
                .event,
        )
        .expect("approval event JSON");
        assert_eq!(body["type"], "approval_request");
        assert_eq!(body["payload"]["kind"], "permissions_approval");
        assert_eq!(body["payload"]["data"]["request_id"], 17);
        assert_eq!(body["payload"]["data"]["params"]["itemId"], "source-item");
    }

    #[test]
    fn nullable_mcp_turn_is_recognized_as_a_thread_scoped_unsupported_capability() {
        let mut map = SourceCoordinateMap::default();
        map.register_turn("source-turn", RuntimeTurnId::new("runtime-turn").unwrap());
        let request = RpcServerRequest {
            id: serde_json::json!(19),
            method: "mcpServer/elicitation/request".to_string(),
            params: serde_json::json!({
                "serverName":"docs","threadId":"source-thread","turnId":null,
                "mode":"url","elicitationId":"elicitation-1",
                "message":"Authorize","url":"https://example.com"
            }),
        };
        let error = map
            .map_server_request(&request)
            .expect_err("thread-scoped interaction cannot borrow an unrelated active turn");
        assert!(matches!(
            error,
            MappingError::UnsupportedThreadScopedInteraction { ref method }
                if method == "mcpServer/elicitation/request"
        ));
        let vendor: codex::McpServerElicitationRequestParams =
            serde_json::from_value(request.params)
                .expect("nullable turnId is valid typed Codex protocol");
        assert_eq!(serde_json::to_value(vendor).unwrap()["turnId"], Value::Null);
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
            event.runtime_event,
            Some(RuntimeEvent::ItemTerminal {
                terminal: RuntimeItemTerminal::Failed { .. },
                ..
            })
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
    fn generated_notification_classification_is_complete_and_disjoint() {
        let projected = PROJECTED_SERVER_NOTIFICATION_METHODS
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let noops = TYPED_NOOP_SERVER_NOTIFICATION_METHODS
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(projected.len(), 34);
        assert_eq!(noops.len(), 34);
        assert!(projected.is_disjoint(&noops));
        assert_eq!(projected.union(&noops).count(), 68);
    }

    #[test]
    fn every_known_unprojected_notification_is_a_typed_noop() {
        let cwd = std::env::current_dir()
            .expect("current directory")
            .to_string_lossy()
            .into_owned();
        let fixtures = [
            (
                "thread/started",
                serde_json::json!({
                    "thread": {
                        "cliVersion": "0.144.1", "createdAt": 1, "cwd": cwd,
                        "ephemeral": false, "id": "source-thread", "modelProvider": "openai",
                        "preview": "", "sessionId": "source-session", "source": "appServer",
                        "status": {"type":"idle"}, "turns": [], "updatedAt": 1
                    }
                }),
            ),
            (
                "thread/archived",
                serde_json::json!({"threadId":"source-thread"}),
            ),
            (
                "thread/deleted",
                serde_json::json!({"threadId":"source-thread"}),
            ),
            (
                "thread/unarchived",
                serde_json::json!({"threadId":"source-thread"}),
            ),
            (
                "thread/closed",
                serde_json::json!({"threadId":"source-thread"}),
            ),
            ("skills/changed", serde_json::json!({})),
            (
                "thread/goal/updated",
                serde_json::json!({
                    "threadId":"source-thread", "turnId":null,
                    "goal": {
                        "threadId":"source-thread", "objective":"finish", "status":"active",
                        "tokenBudget":null, "tokensUsed":0, "timeUsedSeconds":0,
                        "createdAt":1, "updatedAt":1
                    }
                }),
            ),
            (
                "thread/goal/cleared",
                serde_json::json!({"threadId":"source-thread"}),
            ),
            (
                "thread/settings/updated",
                serde_json::json!({
                    "threadId":"source-thread",
                    "threadSettings": {
                        "approvalPolicy":"never", "approvalsReviewer":"user",
                        "collaborationMode":{"mode":"default","settings":{"model":"gpt-5"}},
                        "cwd":cwd, "model":"gpt-5", "modelProvider":"openai",
                        "sandboxPolicy":{"type":"dangerFullAccess"}
                    }
                }),
            ),
            (
                "command/exec/outputDelta",
                serde_json::json!({
                    "capReached":false, "deltaBase64":"", "processId":"process-1",
                    "stream":"stdout"
                }),
            ),
            (
                "process/outputDelta",
                serde_json::json!({
                    "capReached":false, "deltaBase64":"", "processHandle":"process-1",
                    "stream":"stdout"
                }),
            ),
            (
                "process/exited",
                serde_json::json!({
                    "exitCode":0, "processHandle":"process-1", "stderr":"",
                    "stderrCapReached":false, "stdout":"", "stdoutCapReached":false
                }),
            ),
            (
                "mcpServer/oauthLogin/completed",
                serde_json::json!({"name":"docs","success":true}),
            ),
            (
                "mcpServer/startupStatus/updated",
                serde_json::json!({"name":"docs","status":"ready"}),
            ),
            ("account/updated", serde_json::json!({})),
            (
                "account/rateLimits/updated",
                serde_json::json!({"rateLimits":{}}),
            ),
            ("app/list/updated", serde_json::json!({"data":[]})),
            (
                "remoteControl/status/changed",
                serde_json::json!({
                    "installationId":"install-1", "serverName":"remote",
                    "status":"connected"
                }),
            ),
            (
                "externalAgentConfig/import/progress",
                serde_json::json!({"importId":"import-1","itemTypeResults":[]}),
            ),
            (
                "externalAgentConfig/import/completed",
                serde_json::json!({"importId":"import-1","itemTypeResults":[]}),
            ),
            (
                "fs/changed",
                serde_json::json!({"changedPaths":[cwd],"watchId":"watch-1"}),
            ),
            (
                "fuzzyFileSearch/sessionUpdated",
                serde_json::json!({"files":[],"query":"map","sessionId":"search-1"}),
            ),
            (
                "fuzzyFileSearch/sessionCompleted",
                serde_json::json!({"sessionId":"search-1"}),
            ),
            (
                "thread/realtime/started",
                serde_json::json!({"threadId":"source-thread","version":"v1"}),
            ),
            (
                "thread/realtime/itemAdded",
                serde_json::json!({"threadId":"source-thread","item":{"type":"message"}}),
            ),
            (
                "thread/realtime/transcript/delta",
                serde_json::json!({"threadId":"source-thread","role":"user","delta":"hi"}),
            ),
            (
                "thread/realtime/transcript/done",
                serde_json::json!({"threadId":"source-thread","role":"user","text":"hi"}),
            ),
            (
                "thread/realtime/outputAudio/delta",
                serde_json::json!({
                    "threadId":"source-thread",
                    "audio":{"data":"","numChannels":1,"sampleRate":24000}
                }),
            ),
            (
                "thread/realtime/sdp",
                serde_json::json!({"threadId":"source-thread","sdp":"v=0"}),
            ),
            (
                "thread/realtime/error",
                serde_json::json!({"threadId":"source-thread","message":"closed"}),
            ),
            (
                "thread/realtime/closed",
                serde_json::json!({"threadId":"source-thread","reason":null}),
            ),
            (
                "windows/worldWritableWarning",
                serde_json::json!({"extraCount":0,"failedScan":false,"samplePaths":[]}),
            ),
            (
                "windowsSandbox/setupCompleted",
                serde_json::json!({"mode":"unelevated","success":true}),
            ),
            (
                "account/login/completed",
                serde_json::json!({"loginId":null,"success":true}),
            ),
        ];
        assert_eq!(fixtures.len(), TYPED_NOOP_SERVER_NOTIFICATION_METHODS.len());
        for (method, params) in fixtures {
            assert!(
                SourceCoordinateMap::default()
                    .map_notification(RpcServerNotification {
                        method: method.to_string(),
                        params,
                    })
                    .unwrap_or_else(|error| panic!("{method}: {error}"))
                    .is_none(),
                "{method} must not become a protocol violation or presentation event"
            );
        }
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
