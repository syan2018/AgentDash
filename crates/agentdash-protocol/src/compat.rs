use agent_client_protocol::{
    ContentBlock, ContentChunk, SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate,
    TextContent, UsageUpdate,
};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};

use crate::{BackboneEnvelope, BackboneEvent, PlatformEvent};

/// 过渡期兼容：将 BackboneEnvelope 转换为 ACP SessionNotification。
///
/// 仅用于 application 层在迁移到直接消费 BackboneEnvelope 之前的过渡阶段。
/// P0.4 完成后此函数将被移除。
pub fn envelope_to_session_notification(envelope: &BackboneEnvelope) -> Option<SessionNotification> {
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
            let context_window = usage
                .token_usage
                .model_context_window
                .unwrap_or(0)
                .max(0) as u64;
            let update = UsageUpdate::new(total_tokens, context_window).meta(meta);
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::UsageUpdate(update),
            ))
        }
        BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
            executor_session_id,
        }) => {
            let event = AgentDashEventV1::new("executor_session_bound")
                .message(Some(executor_session_id.clone()))
                .data(Some(
                    serde_json::json!({ "executor_session_id": executor_session_id }),
                ));
            let source = AgentDashSourceV1::new(
                &envelope.source.connector_id,
                &envelope.source.connector_type,
            );
            let mut trace = AgentDashTraceV1::new();
            trace.turn_id = envelope.trace.turn_id.clone();
            let agentdash = AgentDashMetaV1::new()
                .source(Some(source))
                .trace(Some(trace))
                .event(Some(event));
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::SessionInfoUpdate(
                    SessionInfoUpdate::new().meta(merge_agentdash_meta(None, &agentdash)),
                ),
            ))
        }
        BackboneEvent::Platform(PlatformEvent::HookTrace(payload)) => {
            let event = AgentDashEventV1::new("hook_event")
                .message(payload.message.clone())
                .data(payload.data.clone());
            let source = AgentDashSourceV1::new(
                &envelope.source.connector_id,
                &envelope.source.connector_type,
            );
            let mut trace = AgentDashTraceV1::new();
            trace.turn_id = envelope.trace.turn_id.clone();
            let agentdash = AgentDashMetaV1::new()
                .source(Some(source))
                .trace(Some(trace))
                .event(Some(event));
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::SessionInfoUpdate(
                    SessionInfoUpdate::new().meta(merge_agentdash_meta(None, &agentdash)),
                ),
            ))
        }
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) => {
            let event = AgentDashEventV1::new("session_meta_update")
                .message(Some(key.clone()))
                .data(Some(value.clone()));
            let source = AgentDashSourceV1::new(
                &envelope.source.connector_id,
                &envelope.source.connector_type,
            );
            let mut trace = AgentDashTraceV1::new();
            trace.turn_id = envelope.trace.turn_id.clone();
            let agentdash = AgentDashMetaV1::new()
                .source(Some(source))
                .trace(Some(trace))
                .event(Some(event));
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::SessionInfoUpdate(
                    SessionInfoUpdate::new().meta(merge_agentdash_meta(None, &agentdash)),
                ),
            ))
        }
        BackboneEvent::Error(error) => {
            let event = AgentDashEventV1::new("error")
                .message(Some(error.error.message.clone()));
            let source = AgentDashSourceV1::new(
                &envelope.source.connector_id,
                &envelope.source.connector_type,
            );
            let mut trace = AgentDashTraceV1::new();
            trace.turn_id = envelope.trace.turn_id.clone();
            let agentdash = AgentDashMetaV1::new()
                .source(Some(source))
                .trace(Some(trace))
                .event(Some(event));
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::SessionInfoUpdate(
                    SessionInfoUpdate::new().meta(merge_agentdash_meta(None, &agentdash)),
                ),
            ))
        }
        // 其余 Codex 原生事件（item lifecycle, turn lifecycle, plan 等）在过渡期暂不产出 ACP；
        // P0.4 完成后所有消费者直接读 BackboneEnvelope，此桥将被移除。
        _ => {
            let event_type = envelope_event_type_label(&envelope.event);
            let event = AgentDashEventV1::new(event_type);
            let source = AgentDashSourceV1::new(
                &envelope.source.connector_id,
                &envelope.source.connector_type,
            );
            let mut trace = AgentDashTraceV1::new();
            trace.turn_id = envelope.trace.turn_id.clone();
            trace.entry_index = envelope.trace.entry_index;
            let agentdash = AgentDashMetaV1::new()
                .source(Some(source))
                .trace(Some(trace))
                .event(Some(event));
            Some(SessionNotification::new(
                session_id,
                SessionUpdate::SessionInfoUpdate(
                    SessionInfoUpdate::new().meta(merge_agentdash_meta(None, &agentdash)),
                ),
            ))
        }
    }
}

/// 过渡期兼容：将 ACP SessionNotification 转换为 BackboneEnvelope。
///
/// 用于 relay 路径接收远程后端的 ACP 格式通知，转入内部 BackboneEnvelope 流。
pub fn session_notification_to_envelope(
    notification: &SessionNotification,
) -> BackboneEnvelope {
    let session_id = notification.session_id.to_string();
    let parsed = notification
        .meta
        .as_ref()
        .and_then(|m| agentdash_acp_meta::parse_agentdash_meta(m));

    let source = parsed
        .as_ref()
        .and_then(|p| p.source.as_ref())
        .map(|s| crate::SourceInfo {
            connector_id: s.connector_id.clone(),
            connector_type: s.connector_type.clone(),
            executor_id: s.executor_id.clone(),
        })
        .unwrap_or_else(|| crate::SourceInfo {
            connector_id: "relay".to_string(),
            connector_type: "remote".to_string(),
            executor_id: None,
        });

    let trace = parsed
        .as_ref()
        .and_then(|p| p.trace.as_ref())
        .map(|t| crate::TraceInfo {
            turn_id: t.turn_id.clone(),
            entry_index: t.entry_index,
        })
        .unwrap_or_default();

    let event_type = parsed
        .as_ref()
        .and_then(|p| p.event.as_ref())
        .map(|e| e.r#type.as_str());

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
        SessionUpdate::UsageUpdate(usage) => {
            BackboneEvent::TokenUsageUpdated(
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
            )
        }
        SessionUpdate::SessionInfoUpdate(_) => {
            match event_type {
                Some("executor_session_bound") => {
                    let executor_session_id = parsed
                        .as_ref()
                        .and_then(|p| p.event.as_ref())
                        .and_then(|e| e.message.clone())
                        .unwrap_or_default();
                    BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
                        executor_session_id,
                    })
                }
                Some("hook_event") => {
                    let payload = parsed
                        .as_ref()
                        .and_then(|p| p.event.as_ref())
                        .map(|e| crate::HookTracePayload {
                            event_type: Some(e.r#type.clone()),
                            message: e.message.clone(),
                            data: e.data.clone(),
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
                    let value = parsed
                        .and_then(|p| p.event)
                        .and_then(|e| e.data)
                        .unwrap_or(serde_json::Value::Null);
                    BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                }
            }
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
    let mut source = AgentDashSourceV1::new(
        &envelope.source.connector_id,
        &envelope.source.connector_type,
    );
    source.executor_id = envelope.source.executor_id.clone();
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = envelope.trace.turn_id.clone();
    trace.entry_index = envelope.trace.entry_index;
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace));
    merge_agentdash_meta(None, &agentdash)
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
