use std::collections::HashMap;

use agent_client_protocol::{
    ContentBlock, ContentChunk, ImageContent, SessionId, SessionNotification, SessionUpdate,
    TextContent, ToolCall, ToolCallContent, ToolCallId, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind,
};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};
use agentdash_agent::{AgentEvent, AgentMessage, AgentToolResult, ContentPart};

fn make_meta(
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: u32,
    event: Option<AgentDashEventV1>,
) -> agent_client_protocol::Meta {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());
    trace.entry_index = Some(entry_index);

    let agentdash = AgentDashMetaV1::new()
        .source(Some(source.clone()))
        .trace(Some(trace))
        .event(event);

    merge_agentdash_meta(None, &agentdash).expect("agentdash meta 不应为空")
}

fn make_tool_call_draft_event(
    tool_call_id: &str,
    tool_name: &str,
    phase: &'static str,
    delta: Option<&str>,
    draft_input: &str,
    is_parseable: bool,
) -> AgentDashEventV1 {
    let mut event = AgentDashEventV1::new("tool_call_draft");
    event.message = Some(format!("工具 `{tool_name}` 参数草稿更新"));
    event.data = Some(serde_json::json!({
        "toolCallId": tool_call_id,
        "toolName": tool_name,
        "phase": phase,
        "delta": delta,
        "draftInput": draft_input,
        "isParseable": is_parseable,
    }));
    event
}

struct EventDescription {
    event_type: &'static str,
    severity: &'static str,
    message: String,
    data: serde_json::Value,
}

fn make_event_notification(
    session_id: &SessionId,
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: u32,
    desc: EventDescription,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());
    trace.entry_index = Some(entry_index);

    let mut event = AgentDashEventV1::new(desc.event_type);
    event.severity = Some(desc.severity.to_string());
    event.message = Some(desc.message);
    event.data = Some(desc.data);

    let agentdash = AgentDashMetaV1::new()
        .source(Some(source.clone()))
        .trace(Some(trace))
        .event(Some(event));

    SessionNotification::new(
        session_id.clone(),
        SessionUpdate::SessionInfoUpdate(
            agent_client_protocol::SessionInfoUpdate::new()
                .meta(merge_agentdash_meta(None, &agentdash).unwrap_or_default()),
        ),
    )
}

fn ensure_chunk_message_id(
    cache: &mut HashMap<String, String>,
    turn_id: &str,
    entry_index: u32,
    chunk_kind: &str,
) -> String {
    let key = format!("{turn_id}:{entry_index}:{chunk_kind}");
    if let Some(existing) = cache.get(&key) {
        return existing.clone();
    }
    let generated = uuid::Uuid::new_v4().to_string();
    cache.insert(key, generated.clone());
    generated
}

#[derive(Debug, Default, Clone)]
pub(super) struct ChunkEmitState {
    emitted_text: String,
    seen_delta: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ToolCallEmitState {
    entry_index: u32,
    title: String,
    kind: ToolKind,
    raw_input: Option<serde_json::Value>,
}

fn chunk_stream_key(turn_id: &str, entry_index: u32, chunk_kind: &str) -> String {
    format!("{turn_id}:{entry_index}:{chunk_kind}")
}

fn map_tool_kind(tool_name: &str) -> ToolKind {
    match tool_name {
        "read_file" | "fs_read" | "list_directory" | "fs_list" | "fs_glob" | "canvases_list" => {
            ToolKind::Read
        }
        "write_file" | "fs_write" | "fs_apply_patch" | "canvas_start" | "bind_canvas_data" => {
            ToolKind::Edit
        }
        "search" | "fs_search" | "fs_grep" => ToolKind::Search,
        "shell" | "shell_exec" => ToolKind::Execute,
        "fetch" | "web_fetch" => ToolKind::Fetch,
        "think" => ToolKind::Think,
        "switch_mode" => ToolKind::SwitchMode,
        _ => ToolKind::Other,
    }
}

fn message_tool_call_info<'a>(
    message: &'a AgentMessage,
    tool_call_id: &str,
) -> Option<&'a agentdash_agent::ToolCallInfo> {
    match message {
        AgentMessage::Assistant { tool_calls, .. } => tool_calls
            .iter()
            .find(|tool_call| tool_call.id == tool_call_id),
        _ => None,
    }
}

pub(super) fn convert_event_to_notifications(
    event: &AgentEvent,
    session_id: &SessionId,
    source: &AgentDashSourceV1,
    turn_id: &str,
    entry_index: &mut u32,
    chunk_message_ids: &mut HashMap<String, String>,
    chunk_emit_states: &mut HashMap<String, ChunkEmitState>,
    tool_call_states: &mut HashMap<String, ToolCallEmitState>,
) -> Vec<SessionNotification> {
    fn upsert_tool_call_state(
        tool_call_states: &mut HashMap<String, ToolCallEmitState>,
        entry_index: &mut u32,
        tool_call_id: &str,
        title: String,
        kind: ToolKind,
        raw_input: Option<serde_json::Value>,
    ) -> (ToolCallEmitState, bool) {
        if let Some(existing) = tool_call_states.get_mut(tool_call_id) {
            if !title.trim().is_empty() {
                existing.title = title;
            }
            if existing.kind == ToolKind::Other && kind != ToolKind::Other {
                existing.kind = kind;
            }
            if let Some(raw_input) = raw_input {
                existing.raw_input = Some(raw_input);
            }
            return (existing.clone(), false);
        }

        let state = ToolCallEmitState {
            entry_index: *entry_index,
            title,
            kind,
            raw_input,
        };
        // 不在此处 bump entry_index：tool_call 与包含它的 message 共享 entry_index，
        // 由 MessageEnd 统一推进。否则会导致 MessageEnd 按 bump 后的 index 查 chunk_emit_states
        // 命中空 state，误判"无增量"而把整段文本作为新 chunk 重发一次。
        tool_call_states.insert(tool_call_id.to_string(), state.clone());
        (state, true)
    }

    fn build_tool_call_notification(
        session_id: &SessionId,
        source: &AgentDashSourceV1,
        turn_id: &str,
        tool_call_id: &str,
        state: &ToolCallEmitState,
        status: ToolCallStatus,
    ) -> SessionNotification {
        let meta = make_meta(source, turn_id, state.entry_index, None);
        let mut call = ToolCall::new(ToolCallId::new(tool_call_id.to_string()), &state.title)
            .kind(state.kind)
            .status(status)
            .raw_input(state.raw_input.clone());
        call.meta = Some(meta);
        SessionNotification::new(session_id.clone(), SessionUpdate::ToolCall(call))
    }

    fn build_tool_call_update_notification(
        session_id: &SessionId,
        source: &AgentDashSourceV1,
        turn_id: &str,
        tool_call_id: &str,
        state: &ToolCallEmitState,
        fields: ToolCallUpdateFields,
        event: Option<AgentDashEventV1>,
    ) -> SessionNotification {
        let mut update = ToolCallUpdate::new(ToolCallId::new(tool_call_id.to_string()), fields);
        update.meta = Some(make_meta(source, turn_id, state.entry_index, event));
        SessionNotification::new(session_id.clone(), SessionUpdate::ToolCallUpdate(update))
    }

    fn seed_tool_update_fields(
        state: &ToolCallEmitState,
        status: Option<ToolCallStatus>,
    ) -> ToolCallUpdateFields {
        let mut fields = ToolCallUpdateFields::default();
        fields.title = Some(state.title.clone());
        fields.kind = Some(state.kind);
        fields.status = status;
        if let Some(raw_input) = state.raw_input.clone() {
            fields.raw_input = Some(raw_input);
        }
        fields
    }

    fn upsert_state_from_tool_name(
        tool_call_states: &mut HashMap<String, ToolCallEmitState>,
        entry_index: &mut u32,
        tool_call_id: &str,
        tool_name: &str,
        raw_input: Option<serde_json::Value>,
    ) -> (ToolCallEmitState, bool) {
        upsert_tool_call_state(
            tool_call_states,
            entry_index,
            tool_call_id,
            tool_name.to_string(),
            map_tool_kind(tool_name),
            raw_input,
        )
    }

    fn upsert_state_from_message(
        tool_call_states: &mut HashMap<String, ToolCallEmitState>,
        entry_index: &mut u32,
        message: &AgentMessage,
        tool_call_id: &str,
        fallback_name: &str,
    ) -> (ToolCallEmitState, bool) {
        if let Some(tool_call) = message_tool_call_info(message, tool_call_id) {
            return upsert_tool_call_state(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_call.name.clone(),
                map_tool_kind(&tool_call.name),
                Some(tool_call.arguments.clone()),
            );
        }

        upsert_state_from_tool_name(
            tool_call_states,
            entry_index,
            tool_call_id,
            fallback_name,
            None,
        )
    }

    match event {
        AgentEvent::MessageUpdate { message, event } => match event {
            agentdash_agent::types::AssistantStreamEvent::ToolCallStart {
                tool_call_id,
                name,
                ..
            } => {
                let (state, created) = upsert_state_from_message(
                    tool_call_states,
                    entry_index,
                    message,
                    tool_call_id,
                    name,
                );
                if !created {
                    return Vec::new();
                }
                vec![build_tool_call_notification(
                    session_id,
                    source,
                    turn_id,
                    tool_call_id,
                    &state,
                    ToolCallStatus::Pending,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ToolCallDelta {
                tool_call_id,
                name,
                delta,
                draft,
                is_parseable,
                ..
            } => {
                let (state, _) = upsert_state_from_message(
                    tool_call_states,
                    entry_index,
                    message,
                    tool_call_id,
                    name,
                );
                let fields = seed_tool_update_fields(&state, Some(ToolCallStatus::Pending));
                let draft_event = Some(make_tool_call_draft_event(
                    tool_call_id,
                    name,
                    "delta",
                    Some(delta),
                    draft,
                    *is_parseable,
                ));
                vec![build_tool_call_update_notification(
                    session_id,
                    source,
                    turn_id,
                    tool_call_id,
                    &state,
                    fields,
                    draft_event,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ToolCallEnd { tool_call, .. } => {
                let (state, _) = upsert_tool_call_state(
                    tool_call_states,
                    entry_index,
                    &tool_call.id,
                    tool_call.name.clone(),
                    map_tool_kind(&tool_call.name),
                    Some(tool_call.arguments.clone()),
                );
                let fields = seed_tool_update_fields(&state, Some(ToolCallStatus::Pending));
                let draft_event = serde_json::to_string(&tool_call.arguments)
                    .ok()
                    .map(|draft| {
                        make_tool_call_draft_event(
                            &tool_call.id,
                            &tool_call.name,
                            "end",
                            None,
                            &draft,
                            true,
                        )
                    });
                vec![build_tool_call_update_notification(
                    session_id,
                    source,
                    turn_id,
                    &tool_call.id,
                    &state,
                    fields,
                    draft_event,
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::TextDelta { text, .. } => {
                if text.is_empty() {
                    return Vec::new();
                }
                let meta = make_meta(source, turn_id, *entry_index, None);
                let key = chunk_stream_key(turn_id, *entry_index, "agent_message_chunk");
                let state = chunk_emit_states.entry(key).or_default();
                state.seen_delta = true;
                state.emitted_text.push_str(text);
                let message_id = ensure_chunk_message_id(
                    chunk_message_ids,
                    turn_id,
                    *entry_index,
                    "agent_message_chunk",
                );
                let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                    .message_id(Some(message_id))
                    .meta(Some(meta));
                vec![SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(chunk),
                )]
            }
            agentdash_agent::types::AssistantStreamEvent::ThinkingDelta { text, .. } => {
                if text.is_empty() {
                    return Vec::new();
                }
                let meta = make_meta(source, turn_id, *entry_index, None);
                let key = chunk_stream_key(turn_id, *entry_index, "agent_thought_chunk");
                let state = chunk_emit_states.entry(key).or_default();
                state.seen_delta = true;
                state.emitted_text.push_str(text);
                let message_id = ensure_chunk_message_id(
                    chunk_message_ids,
                    turn_id,
                    *entry_index,
                    "agent_thought_chunk",
                );
                let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                    .message_id(Some(message_id))
                    .meta(Some(meta));
                vec![SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentThoughtChunk(chunk),
                )]
            }
            _ => Vec::new(),
        },

        AgentEvent::MessageEnd { message } => {
            if let AgentMessage::Assistant {
                content,
                error_message,
                tool_calls,
                ..
            } = message
            {
                let reasoning_text = content
                    .iter()
                    .filter_map(ContentPart::extract_reasoning)
                    .collect::<Vec<_>>()
                    .join("");
                let text = error_message.clone().unwrap_or_else(|| {
                    content
                        .iter()
                        .filter_map(ContentPart::extract_text)
                        .collect::<Vec<_>>()
                        .join("")
                });

                let mut notifications = Vec::new();
                if !reasoning_text.is_empty() {
                    let key = chunk_stream_key(turn_id, *entry_index, "agent_thought_chunk");
                    let state = chunk_emit_states.get(&key).cloned().unwrap_or_default();
                    let message_id = ensure_chunk_message_id(
                        chunk_message_ids,
                        turn_id,
                        *entry_index,
                        "agent_thought_chunk",
                    );
                    let to_emit = if state.seen_delta {
                        if reasoning_text == state.emitted_text {
                            None
                        } else if reasoning_text.starts_with(state.emitted_text.as_str()) {
                            let suffix = &reasoning_text[state.emitted_text.len()..];
                            if suffix.is_empty() {
                                None
                            } else {
                                Some(suffix.to_string())
                            }
                        } else {
                            // 单路径约束：流式消息已存在增量链路时，不再走 reconcile 兜底快照。
                            tracing::warn!(
                                turn_id = %turn_id,
                                entry_index = *entry_index,
                                "MessageEnd thought 与已发送增量不一致，已忽略兜底快照"
                            );
                            None
                        }
                    } else {
                        Some(reasoning_text.clone())
                    };
                    if let Some(payload) = to_emit {
                        let meta = make_meta(source, turn_id, *entry_index, None);
                        let chunk =
                            ContentChunk::new(ContentBlock::Text(TextContent::new(payload)))
                                .message_id(Some(message_id))
                                .meta(Some(meta));
                        notifications.push(SessionNotification::new(
                            session_id.clone(),
                            SessionUpdate::AgentThoughtChunk(chunk),
                        ));
                    }
                }
                if !text.is_empty() {
                    let key = chunk_stream_key(turn_id, *entry_index, "agent_message_chunk");
                    let state = chunk_emit_states.get(&key).cloned().unwrap_or_default();
                    let message_id = ensure_chunk_message_id(
                        chunk_message_ids,
                        turn_id,
                        *entry_index,
                        "agent_message_chunk",
                    );
                    let to_emit = if state.seen_delta {
                        if text == state.emitted_text {
                            None
                        } else if text.starts_with(state.emitted_text.as_str()) {
                            let suffix = &text[state.emitted_text.len()..];
                            if suffix.is_empty() {
                                None
                            } else {
                                Some(suffix.to_string())
                            }
                        } else {
                            tracing::warn!(
                                turn_id = %turn_id,
                                entry_index = *entry_index,
                                "MessageEnd text 与已发送增量不一致，已忽略兜底快照"
                            );
                            None
                        }
                    } else {
                        Some(text.clone())
                    };
                    if let Some(payload) = to_emit {
                        let meta = make_meta(source, turn_id, *entry_index, None);
                        let chunk =
                            ContentChunk::new(ContentBlock::Text(TextContent::new(payload)))
                                .message_id(Some(message_id))
                                .meta(Some(meta));
                        notifications.push(SessionNotification::new(
                            session_id.clone(),
                            SessionUpdate::AgentMessageChunk(chunk),
                        ));
                    }
                }

                for tool_call in tool_calls {
                    let (state, created) = upsert_tool_call_state(
                        tool_call_states,
                        entry_index,
                        &tool_call.id,
                        tool_call.name.clone(),
                        map_tool_kind(&tool_call.name),
                        Some(tool_call.arguments.clone()),
                    );
                    if created {
                        notifications.push(build_tool_call_notification(
                            session_id,
                            source,
                            turn_id,
                            &tool_call.id,
                            &state,
                            ToolCallStatus::Pending,
                        ));
                    }
                }

                let has_streamable_content = content.iter().any(|part| {
                    part.extract_text().is_some() || part.extract_reasoning().is_some()
                });
                if has_streamable_content || error_message.is_some() || !tool_calls.is_empty() {
                    *entry_index += 1;
                }
                return notifications;
            }
            Vec::new()
        }

        AgentEvent::ContextCompacted {
            messages,
            newly_compacted_messages,
        } => {
            let Some(AgentMessage::CompactionSummary {
                summary,
                tokens_before,
                messages_compacted,
                compacted_until_ref,
                timestamp,
            }) = messages.first()
            else {
                return Vec::new();
            };

            vec![make_event_notification(
                session_id,
                source,
                turn_id,
                *entry_index,
                EventDescription {
                    event_type: "context_compacted",
                    severity: "info",
                    message: format!(
                        "已压缩 {} 条历史消息，保留最新摘要进入模型窗口",
                        newly_compacted_messages
                    ),
                    data: serde_json::json!({
                        "summary": summary,
                        "tokens_before": tokens_before,
                        "messages_compacted": messages_compacted,
                        "newly_compacted_messages": newly_compacted_messages,
                        "compacted_until_ref": compacted_until_ref,
                        "timestamp_ms": timestamp,
                    }),
                },
            )]
        }

        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            let fields = seed_tool_update_fields(&state, Some(ToolCallStatus::InProgress));
            vec![build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
            )]
        }

        AgentEvent::ToolExecutionUpdate {
            tool_call_id,
            tool_name,
            args,
            partial_result,
            ..
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            let mut fields = seed_tool_update_fields(&state, Some(ToolCallStatus::InProgress));
            fields.raw_output = Some(partial_result.clone());
            if let Some(result) = decode_tool_result(partial_result) {
                let content = content_parts_to_tool_call_content(&result.content);
                if !content.is_empty() {
                    fields.content = Some(content);
                }
            }

            vec![build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
            )]
        }

        AgentEvent::ToolExecutionPendingApproval {
            tool_call_id,
            tool_name,
            args,
            reason,
            details,
            ..
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            let mut notifications = Vec::new();
            let mut fields = seed_tool_update_fields(&state, Some(ToolCallStatus::Pending));
            fields.status = Some(ToolCallStatus::Pending);
            fields.raw_output = Some(serde_json::json!({
                "approval_state": "pending",
                "reason": reason,
                "details": details,
            }));
            fields.content = Some(vec![ToolCallContent::from(ContentBlock::Text(
                TextContent::new(format!("等待审批：{reason}")),
            ))]);

            notifications.push(build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
            ));
            notifications.push(make_event_notification(
                session_id,
                source,
                turn_id,
                state.entry_index,
                EventDescription {
                    event_type: "approval_requested",
                    severity: "warning",
                    message: format!("工具 `{tool_name}` 正等待审批"),
                    data: serde_json::json!({
                        "tool_call_id": tool_call_id,
                        "tool_name": tool_name,
                        "reason": reason,
                        "args": args,
                        "details": details,
                    }),
                },
            ));
            notifications
        }

        AgentEvent::ToolExecutionApprovalResolved {
            tool_call_id,
            tool_name,
            args,
            approved,
            reason,
            ..
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                Some(args.clone()),
            );
            let mut notifications = Vec::new();
            let mut fields = seed_tool_update_fields(
                &state,
                Some(if *approved {
                    ToolCallStatus::InProgress
                } else {
                    ToolCallStatus::Failed
                }),
            );
            fields.status = Some(if *approved {
                ToolCallStatus::InProgress
            } else {
                ToolCallStatus::Failed
            });
            fields.raw_output = Some(serde_json::json!({
                "approval_state": if *approved { "approved" } else { "rejected" },
                "reason": reason,
            }));
            if !approved {
                fields.content = Some(vec![ToolCallContent::from(ContentBlock::Text(
                    TextContent::new(
                        reason
                            .as_deref()
                            .map(|value| format!("审批被拒绝：{value}"))
                            .unwrap_or_else(|| "审批被拒绝".to_string()),
                    ),
                ))]);
            }

            notifications.push(build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
            ));
            notifications.push(make_event_notification(
                session_id,
                source,
                turn_id,
                state.entry_index,
                EventDescription {
                    event_type: "approval_resolved",
                    severity: if *approved { "info" } else { "warning" },
                    message: if *approved {
                        format!("工具 `{tool_name}` 已获批准并继续执行")
                    } else {
                        format!("工具 `{tool_name}` 已被拒绝执行")
                    },
                    data: serde_json::json!({
                        "tool_call_id": tool_call_id,
                        "tool_name": tool_name,
                        "approved": approved,
                        "reason": reason,
                        "args": args,
                    }),
                },
            ));
            notifications
        }

        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
            is_error,
        } => {
            let (state, _) = upsert_state_from_tool_name(
                tool_call_states,
                entry_index,
                tool_call_id,
                tool_name,
                None,
            );

            let result_text = result
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let status = if *is_error {
                ToolCallStatus::Failed
            } else {
                ToolCallStatus::Completed
            };

            let mut fields = seed_tool_update_fields(&state, Some(status));
            fields.status = Some(status);
            fields.raw_output = Some(result.clone());
            if let Some(decoded) = decode_tool_result(result) {
                let content = content_parts_to_tool_call_content(&decoded.content);
                if !content.is_empty() {
                    fields.content = Some(content);
                }
            } else if !result_text.is_empty() {
                fields.content = Some(vec![ToolCallContent::from(ContentBlock::Text(
                    TextContent::new(&result_text),
                ))]);
            }

            vec![build_tool_call_update_notification(
                session_id,
                source,
                turn_id,
                tool_call_id,
                &state,
                fields,
                None,
            )]
        }

        _ => Vec::new(),
    }
}

fn decode_tool_result(value: &serde_json::Value) -> Option<AgentToolResult> {
    serde_json::from_value(value.clone()).ok()
}

fn content_parts_to_tool_call_content(parts: &[ContentPart]) -> Vec<ToolCallContent> {
    parts
        .iter()
        .filter_map(content_part_to_block)
        .map(ToolCallContent::from)
        .collect()
}

fn content_part_to_block(part: &ContentPart) -> Option<ContentBlock> {
    match part {
        ContentPart::Text { text } => Some(ContentBlock::Text(TextContent::new(text))),
        ContentPart::Image { mime_type, data } => {
            Some(ContentBlock::Image(ImageContent::new(data, mime_type)))
        }
        ContentPart::Reasoning { text, .. } => Some(ContentBlock::Text(TextContent::new(text))),
    }
}
