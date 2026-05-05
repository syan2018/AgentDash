use agent_client_protocol::{
    ContentBlock, ContentChunk, SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate,
    TextContent, ToolCallStatus, UsageUpdate,
};
use codex_app_server_protocol as codex;
use serde_json::{Map, Value, json};

use crate::{BackboneEnvelope, BackboneEvent, PlatformEvent};

const AGENTDASH_NS: &str = "agentdash";
const AGENTDASH_META_VERSION: u32 = 1;

/// 过渡期兼容：将 BackboneEnvelope 转换为 ACP SessionNotification。
///
/// 仅用于 application 层在迁移到直接消费 BackboneEnvelope 之前的过渡阶段。
/// P0.4 完成后此函数将被移除。
pub fn envelope_to_session_notification(
    envelope: &BackboneEnvelope,
) -> Option<SessionNotification> {
    let session_id = SessionId::new(envelope.session_id.clone());
    let meta = build_acp_meta(envelope);

    match &envelope.event {
        BackboneEvent::AgentMessageDelta(delta) => {
            if delta.delta.is_empty() {
                return None;
            }
            let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(&delta.delta)))
                .message_id(Some(format!(
                    "{}:agent_message_chunk",
                    envelope.trace.turn_id.as_deref().unwrap_or("unknown")
                )))
                .meta(meta);
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::AgentMessageChunk(chunk),
            ))
        }
        BackboneEvent::ReasoningTextDelta(delta) => {
            if delta.delta.is_empty() {
                return None;
            }
            let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(&delta.delta)))
                .message_id(Some(format!(
                    "{}:agent_thought_chunk",
                    envelope.trace.turn_id.as_deref().unwrap_or("unknown")
                )))
                .meta(meta);
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::AgentThoughtChunk(chunk),
            ))
        }
        BackboneEvent::ReasoningSummaryDelta(delta) => {
            if delta.delta.is_empty() {
                return None;
            }
            let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(&delta.delta)))
                .message_id(Some(format!(
                    "{}:agent_thought_chunk",
                    envelope.trace.turn_id.as_deref().unwrap_or("unknown")
                )))
                .meta(meta);
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::AgentThoughtChunk(chunk),
            ))
        }
        BackboneEvent::TokenUsageUpdated(usage) => {
            let total_tokens = usage.token_usage.total.total_tokens.max(0) as u64;
            let context_window = usage.token_usage.model_context_window.unwrap_or(0).max(0) as u64;
            let update = UsageUpdate::new(total_tokens, context_window).meta(meta);
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::UsageUpdate(update),
            ))
        }
        BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
            executor_session_id,
        }) => Some(wrap_session_info_update(
            session_id,
            envelope,
            "executor_session_bound",
            Some(executor_session_id.clone()),
            Some(json!({ "executor_session_id": executor_session_id })),
        )),
        BackboneEvent::Platform(PlatformEvent::HookTrace(payload)) => {
            let hook_data = payload
                .data
                .as_ref()
                .and_then(|data| serde_json::to_value(data).ok());
            Some(wrap_session_info_update(
                session_id,
                envelope,
                "hook_event",
                payload.message.clone(),
                hook_data,
            ))
        }
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) => {
            Some(wrap_session_info_update(
                session_id,
                envelope,
                "session_meta_update",
                Some(key.clone()),
                Some(value.clone()),
            ))
        }
        BackboneEvent::Error(error) => Some(wrap_session_info_update(
            session_id,
            envelope,
            "error",
            Some(error.error.message.clone()),
            None,
        )),
        _ => {
            let event_type = envelope_event_type_label(&envelope.event);
            Some(wrap_session_info_update(
                session_id,
                envelope,
                event_type,
                None,
                None,
            ))
        }
    }
}

/// 过渡期兼容：将 ACP SessionNotification 转换为 BackboneEnvelope。
///
/// 用于 relay 路径接收远程后端的 ACP 格式通知，转入内部 BackboneEnvelope 流。
pub fn session_notification_to_envelope(notification: &SessionNotification) -> BackboneEnvelope {
    let session_id = notification.session_id.to_string();
    let agentdash_meta = notification
        .meta
        .as_ref()
        .and_then(|m| m.get(AGENTDASH_NS))
        .filter(|v| {
            v.get("v")
                .and_then(Value::as_u64)
                .map_or(false, |v| v == u64::from(AGENTDASH_META_VERSION))
        });

    let source = agentdash_meta
        .and_then(|v| v.get("source"))
        .map(|s| crate::SourceInfo {
            connector_id: s
                .get("connectorId")
                .and_then(Value::as_str)
                .unwrap_or("relay")
                .to_string(),
            connector_type: s
                .get("connectorType")
                .and_then(Value::as_str)
                .unwrap_or("remote")
                .to_string(),
            executor_id: s
                .get("executorId")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        })
        .unwrap_or_else(|| crate::SourceInfo {
            connector_id: "relay".to_string(),
            connector_type: "remote".to_string(),
            executor_id: None,
        });

    let trace = agentdash_meta
        .and_then(|v| v.get("trace"))
        .map(|t| crate::TraceInfo {
            turn_id: t
                .get("turnId")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            entry_index: t
                .get("entryIndex")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok()),
        })
        .unwrap_or_default();

    let event_type = agentdash_meta
        .and_then(|v| v.get("event"))
        .and_then(|e| e.get("type"))
        .and_then(Value::as_str);

    let event = match &notification.update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            let text = match &chunk.content {
                ContentBlock::Text(t) => t.text.clone(),
                _ => String::new(),
            };
            BackboneEvent::AgentMessageDelta(
                codex_app_server_protocol::AgentMessageDeltaNotification {
                    thread_id: session_id.clone(),
                    turn_id: trace.turn_id.clone().unwrap_or_default(),
                    item_id: String::new(),
                    delta: text,
                },
            )
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            let text = match &chunk.content {
                ContentBlock::Text(t) => t.text.clone(),
                _ => String::new(),
            };
            BackboneEvent::ReasoningTextDelta(
                codex_app_server_protocol::ReasoningTextDeltaNotification {
                    thread_id: session_id.clone(),
                    turn_id: trace.turn_id.clone().unwrap_or_default(),
                    item_id: String::new(),
                    delta: text,
                    content_index: 0,
                },
            )
        }
        SessionUpdate::UsageUpdate(usage) => BackboneEvent::TokenUsageUpdated(
            codex_app_server_protocol::ThreadTokenUsageUpdatedNotification {
                thread_id: session_id.clone(),
                turn_id: trace.turn_id.clone().unwrap_or_default(),
                token_usage: codex_app_server_protocol::ThreadTokenUsage {
                    total: codex_app_server_protocol::TokenUsageBreakdown {
                        total_tokens: usage.used as i64,
                        input_tokens: 0,
                        cached_input_tokens: 0,
                        output_tokens: 0,
                        reasoning_output_tokens: 0,
                    },
                    last: codex_app_server_protocol::TokenUsageBreakdown {
                        total_tokens: 0,
                        input_tokens: 0,
                        cached_input_tokens: 0,
                        output_tokens: 0,
                        reasoning_output_tokens: 0,
                    },
                    model_context_window: Some(usage.size as i64),
                },
            },
        ),
        SessionUpdate::SessionInfoUpdate(_) => {
            let event_obj = agentdash_meta.and_then(|v| v.get("event"));
            match event_type {
                Some("executor_session_bound") => {
                    let executor_session_id = event_obj
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
                        executor_session_id,
                    })
                }
                Some("hook_event") => {
                    let payload = event_obj
                        .map(|e| crate::HookTracePayload {
                            event_type: e
                                .get("type")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                            message: e
                                .get("message")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                            data: e
                                .get("data")
                                .cloned()
                                .and_then(|raw| {
                                    serde_json::from_value::<crate::HookTraceData>(raw).ok()
                                }),
                        })
                        .unwrap_or_else(|| crate::HookTracePayload {
                            event_type: Some("hook_event".to_string()),
                            message: None,
                            data: None,
                        });
                    BackboneEvent::Platform(PlatformEvent::HookTrace(payload))
                }
                _ => {
                    let key = event_type.unwrap_or("unknown").to_string();
                    let value = event_obj
                        .and_then(|e| e.get("data"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                }
            }
        }
        SessionUpdate::ToolCall(tc) => {
            let status = match tc.status {
                ToolCallStatus::Completed => codex::DynamicToolCallStatus::Completed,
                ToolCallStatus::Failed => codex::DynamicToolCallStatus::Failed,
                _ => codex::DynamicToolCallStatus::InProgress,
            };
            let is_terminal = matches!(
                tc.status,
                ToolCallStatus::Completed | ToolCallStatus::Failed
            );
            let arguments = tc
                .raw_input
                .clone()
                .unwrap_or(Value::Object(Default::default()));
            let content_items = if tc.content.is_empty() {
                None
            } else {
                Some(acp_tool_content_to_codex_items(&tc.content))
            };
            let success = if is_terminal {
                Some(tc.status == ToolCallStatus::Completed)
            } else {
                None
            };
            let item = codex::ThreadItem::DynamicToolCall {
                id: tc.tool_call_id.to_string(),
                tool: tc.title.clone(),
                arguments,
                status,
                content_items,
                success,
                duration_ms: None,
            };
            if is_terminal {
                BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                    item,
                    thread_id: session_id.to_string(),
                    turn_id: trace.turn_id.clone().unwrap_or_default(),
                })
            } else {
                BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                    item,
                    thread_id: session_id.to_string(),
                    turn_id: trace.turn_id.clone().unwrap_or_default(),
                })
            }
        }
        SessionUpdate::ToolCallUpdate(tcu) => {
            let status = match tcu.fields.status {
                Some(ToolCallStatus::Completed) => codex::DynamicToolCallStatus::Completed,
                Some(ToolCallStatus::Failed) => codex::DynamicToolCallStatus::Failed,
                _ => codex::DynamicToolCallStatus::InProgress,
            };
            let is_terminal = matches!(
                tcu.fields.status,
                Some(ToolCallStatus::Completed) | Some(ToolCallStatus::Failed)
            );
            let arguments = tcu
                .fields
                .raw_input
                .clone()
                .unwrap_or(Value::Object(Default::default()));
            let content_items = tcu
                .fields
                .content
                .as_ref()
                .map(|c| acp_tool_content_to_codex_items(c));
            let success = if is_terminal {
                Some(
                    tcu.fields.status == Some(ToolCallStatus::Completed),
                )
            } else {
                None
            };
            let item = codex::ThreadItem::DynamicToolCall {
                id: tcu.tool_call_id.to_string(),
                tool: tcu.fields.title.clone().unwrap_or_default(),
                arguments,
                status,
                content_items,
                success,
                duration_ms: None,
            };
            if is_terminal {
                BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                    item,
                    thread_id: session_id.to_string(),
                    turn_id: trace.turn_id.clone().unwrap_or_default(),
                })
            } else {
                BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                    item,
                    thread_id: session_id.to_string(),
                    turn_id: trace.turn_id.clone().unwrap_or_default(),
                })
            }
        }
        SessionUpdate::UserMessageChunk(chunk) => {
            let value = serde_json::to_value(&chunk.content).unwrap_or_default();
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "user_message_chunk".to_string(),
                value,
            })
        }
        _ => {
            let value = serde_json::to_value(&notification.update).unwrap_or_default();
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "acp_passthrough".to_string(),
                value,
            })
        }
    };

    BackboneEnvelope::new(event, &session_id, source).with_trace(trace)
}

fn build_acp_meta(envelope: &BackboneEnvelope) -> Option<agent_client_protocol::Meta> {
    let agentdash_value = json!({
        "v": AGENTDASH_META_VERSION,
        "source": {
            "connectorId": envelope.source.connector_id,
            "connectorType": envelope.source.connector_type,
            "executorId": envelope.source.executor_id,
        },
        "trace": {
            "turnId": envelope.trace.turn_id,
            "entryIndex": envelope.trace.entry_index,
        }
    });
    let mut meta = agent_client_protocol::Meta::new();
    meta.insert(AGENTDASH_NS.to_string(), agentdash_value);
    Some(meta)
}

fn wrap_session_info_update(
    session_id: SessionId,
    envelope: &BackboneEnvelope,
    event_type: &str,
    message: Option<String>,
    data: Option<Value>,
) -> SessionNotification {
    let mut event_map = Map::new();
    event_map.insert("type".to_string(), Value::String(event_type.to_string()));
    if let Some(msg) = message {
        event_map.insert("message".to_string(), Value::String(msg));
    }
    if let Some(d) = data {
        event_map.insert("data".to_string(), d);
    }

    let agentdash_value = json!({
        "v": AGENTDASH_META_VERSION,
        "source": {
            "connectorId": envelope.source.connector_id,
            "connectorType": envelope.source.connector_type,
            "executorId": envelope.source.executor_id,
        },
        "trace": {
            "turnId": envelope.trace.turn_id,
            "entryIndex": envelope.trace.entry_index,
        },
        "event": Value::Object(event_map),
    });
    let mut meta = agent_client_protocol::Meta::new();
    meta.insert(AGENTDASH_NS.to_string(), agentdash_value);
    SessionNotification::new(
        session_id,
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(Some(meta))),
    )
}

fn acp_tool_content_to_codex_items(
    content: &[agent_client_protocol::ToolCallContent],
) -> Vec<codex::DynamicToolCallOutputContentItem> {
    content
        .iter()
        .filter_map(|item| match item {
            agent_client_protocol::ToolCallContent::Content(c) => {
                let text = match &c.content {
                    ContentBlock::Text(t) => t.text.clone(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                };
                Some(codex::DynamicToolCallOutputContentItem::InputText { text })
            }
            agent_client_protocol::ToolCallContent::Diff(diff) => {
                Some(codex::DynamicToolCallOutputContentItem::InputText {
                    text: format!(
                        "<diff path=\"{}\">\n{}\n</diff>",
                        diff.path.display(),
                        diff.new_text
                    ),
                })
            }
            agent_client_protocol::ToolCallContent::Terminal(terminal) => {
                Some(codex::DynamicToolCallOutputContentItem::InputText {
                    text: format!("[terminal:{}]", terminal.terminal_id.0),
                })
            }
            _ => None,
        })
        .collect()
}

fn envelope_event_type_label(event: &BackboneEvent) -> &'static str {
    match event {
        BackboneEvent::AgentMessageDelta(_) => "agent_message_delta",
        BackboneEvent::ReasoningTextDelta(_) => "reasoning_text_delta",
        BackboneEvent::ReasoningSummaryDelta(_) => "reasoning_summary_delta",
        BackboneEvent::ItemStarted(_) => "item_started",
        BackboneEvent::ItemCompleted(_) => "item_completed",
        BackboneEvent::CommandOutputDelta(_) => "command_output_delta",
        BackboneEvent::FileChangeDelta(_) => "file_change_delta",
        BackboneEvent::McpToolCallProgress(_) => "mcp_tool_call_progress",
        BackboneEvent::TurnStarted(_) => "turn_started",
        BackboneEvent::TurnCompleted(_) => "turn_completed",
        BackboneEvent::TurnDiffUpdated(_) => "turn_diff_updated",
        BackboneEvent::TurnPlanUpdated(_) => "turn_plan_updated",
        BackboneEvent::PlanDelta(_) => "plan_delta",
        BackboneEvent::TokenUsageUpdated(_) => "token_usage_updated",
        BackboneEvent::ThreadStatusChanged(_) => "thread_status_changed",
        BackboneEvent::ContextCompacted(_) => "context_compacted",
        BackboneEvent::ApprovalRequest(_) => "approval_request",
        BackboneEvent::Error(_) => "error",
        BackboneEvent::Platform(_) => "platform_event",
    }
}
