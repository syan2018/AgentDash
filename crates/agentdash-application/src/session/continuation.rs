use std::collections::{HashMap, HashSet};

use agent_client_protocol::{
    ContentBlock, SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate,
    ToolCallContent, ToolCallStatus,
};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};
use agentdash_agent_types::{AgentMessage, ContentPart, StopReason, ToolCallInfo};
use agentdash_spi::content_block_to_text;

use super::persistence::PersistedSessionEvent;

// ─── Continuation transcript 构建 ─────────────────────────────

#[derive(Debug, Clone)]
struct ContinuationTranscriptEntry {
    order: u64,
    role: &'static str,
    turn_id: Option<String>,
    text: String,
}

#[derive(Debug, Clone, Default)]
struct ContinuationToolState {
    order: u64,
    turn_id: Option<String>,
    title: Option<String>,
    status: Option<String>,
    raw_input: Option<String>,
    raw_output: Option<String>,
    content_preview: Option<String>,
}

pub(super) fn build_continuation_system_context_from_events(
    owner_context: Option<&str>,
    events: &[PersistedSessionEvent],
) -> Option<String> {
    if events.is_empty() {
        return owner_context
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
    }

    let mut transcript_entries: Vec<ContinuationTranscriptEntry> = Vec::new();
    let mut message_index: HashMap<String, usize> = HashMap::new();
    let mut tool_states: HashMap<String, ContinuationToolState> = HashMap::new();

    for event in events {
        match &event.notification.update {
            SessionUpdate::UserMessageChunk(chunk) => {
                if let Some(text) = content_block_to_transcript_text(&chunk.content) {
                    merge_transcript_chunk(
                        &mut transcript_entries,
                        &mut message_index,
                        message_key("user", event, chunk.message_id.as_deref()),
                        event.event_seq,
                        "用户",
                        event.turn_id.clone(),
                        &text,
                    );
                }
            }
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let Some(text) = content_block_to_transcript_text(&chunk.content) {
                    merge_transcript_chunk(
                        &mut transcript_entries,
                        &mut message_index,
                        message_key("assistant", event, chunk.message_id.as_deref()),
                        event.event_seq,
                        "助手",
                        event.turn_id.clone(),
                        &text,
                    );
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                if let Some(text) = content_block_to_transcript_text(&chunk.content) {
                    merge_transcript_chunk(
                        &mut transcript_entries,
                        &mut message_index,
                        message_key("thought", event, chunk.message_id.as_deref()),
                        event.event_seq,
                        "助手思考",
                        event.turn_id.clone(),
                        &text,
                    );
                }
            }
            SessionUpdate::ToolCall(call) => {
                tool_states.insert(
                    call.tool_call_id.to_string(),
                    ContinuationToolState {
                        order: event.event_seq,
                        turn_id: event.turn_id.clone(),
                        title: Some(call.title.clone()),
                        status: Some(format!("{:?}", call.status).to_ascii_lowercase()),
                        raw_input: call.raw_input.as_ref().map(json_preview),
                        raw_output: call.raw_output.as_ref().map(json_preview),
                        content_preview: (!call.content.is_empty())
                            .then(|| json_preview(&serde_json::json!(call.content))),
                    },
                );
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let tool_state = tool_states
                    .entry(update.tool_call_id.to_string())
                    .or_insert_with(|| ContinuationToolState {
                        order: event.event_seq,
                        turn_id: event.turn_id.clone(),
                        ..ContinuationToolState::default()
                    });
                tool_state.order = tool_state.order.min(event.event_seq);
                if tool_state.turn_id.is_none() {
                    tool_state.turn_id = event.turn_id.clone();
                }
                if let Some(title) = update.fields.title.clone() {
                    tool_state.title = Some(title);
                }
                if let Some(status) = update.fields.status {
                    tool_state.status = Some(format!("{status:?}").to_ascii_lowercase());
                }
                if let Some(raw_input) = update.fields.raw_input.as_ref() {
                    tool_state.raw_input = Some(json_preview(raw_input));
                }
                if let Some(raw_output) = update.fields.raw_output.as_ref() {
                    tool_state.raw_output = Some(json_preview(raw_output));
                }
                if let Some(content) = update.fields.content.as_ref() {
                    tool_state.content_preview = Some(json_preview(&serde_json::json!(content)));
                }
            }
            _ => {}
        }
    }

    transcript_entries.sort_by_key(|entry| entry.order);
    let mut ordered_tool_states = tool_states.into_iter().collect::<Vec<_>>();
    ordered_tool_states.sort_by_key(|(_, state)| state.order);

    let mut sections = Vec::new();
    if let Some(owner) = owner_context
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        sections.push(format!("## Owner Context\n\n{owner}"));
    }

    let mut history_lines = Vec::new();
    history_lines.push(
        "以下内容由 session 仓储事件重建，用于在当前进程缺少 live runtime 时恢复连续会话语义。请将其视为本 session 已经发生过的事实，并在此基础上继续处理新的用户输入。"
            .to_string(),
    );

    if !transcript_entries.is_empty() {
        history_lines.push(String::new());
        history_lines.push("### Transcript".to_string());
        for entry in transcript_entries {
            let turn_line = entry
                .turn_id
                .as_deref()
                .map(|turn_id| format!(" ({turn_id})"))
                .unwrap_or_default();
            history_lines.push(format!("#### {}{}", entry.role, turn_line));
            history_lines.push(entry.text);
            history_lines.push(String::new());
        }
    }

    if !ordered_tool_states.is_empty() {
        history_lines.push("### Tool State".to_string());
        for (tool_call_id, state) in ordered_tool_states {
            let mut lines = vec![format!(
                "- tool_call_id: {tool_call_id}\n  title: {}\n  status: {}",
                state.title.as_deref().unwrap_or("-"),
                state.status.as_deref().unwrap_or("-"),
            )];
            if let Some(turn_id) = state.turn_id.as_deref() {
                lines.push(format!("  turn_id: {turn_id}"));
            }
            if let Some(raw_input) = state.raw_input.as_deref() {
                lines.push(format!("  raw_input: {raw_input}"));
            }
            if let Some(raw_output) = state.raw_output.as_deref() {
                lines.push(format!("  raw_output: {raw_output}"));
            } else if let Some(content_preview) = state.content_preview.as_deref() {
                lines.push(format!("  content: {content_preview}"));
            }
            history_lines.push(lines.join("\n"));
        }
    }

    sections.push(format!("## Session Continuation\n\n{}", history_lines.join("\n")));
    Some(sections.join("\n\n"))
}

// ─── Restored session messages ─────────────────────────────

#[derive(Debug, Clone)]
struct RestoredUserMessageState {
    order: u64,
    content: Vec<ContentPart>,
}

#[derive(Debug, Clone)]
struct RestoredAssistantMessageState {
    order: u64,
    content: Vec<ContentPart>,
    tool_calls: Vec<ToolCallInfo>,
    tool_call_ids: HashSet<String>,
}

#[derive(Debug, Clone, Default)]
struct RestoredToolResultState {
    order: u64,
    tool_call_id: String,
    call_id: Option<String>,
    tool_name: Option<String>,
    content: Vec<ContentPart>,
    details: Option<serde_json::Value>,
    is_error: bool,
    terminal: bool,
}

#[derive(Debug, Clone)]
enum RestoredMessageEnvelope {
    User {
        order: u64,
        content: Vec<ContentPart>,
    },
    Assistant {
        order: u64,
        content: Vec<ContentPart>,
        tool_calls: Vec<ToolCallInfo>,
    },
    ToolResult {
        order: u64,
        tool_call_id: String,
        call_id: Option<String>,
        tool_name: Option<String>,
        content: Vec<ContentPart>,
        details: Option<serde_json::Value>,
        is_error: bool,
    },
}

pub(super) fn build_restored_session_messages_from_events(
    events: &[PersistedSessionEvent],
) -> Vec<AgentMessage> {
    let mut user_messages: HashMap<String, RestoredUserMessageState> = HashMap::new();
    let mut assistant_messages: HashMap<String, RestoredAssistantMessageState> = HashMap::new();
    let mut tool_results: HashMap<String, RestoredToolResultState> = HashMap::new();

    for event in events {
        match &event.notification.update {
            SessionUpdate::UserMessageChunk(chunk) => {
                if let Some(part) = content_block_to_message_part(&chunk.content) {
                    let key = restored_user_key(event);
                    let state = user_messages
                        .entry(key)
                        .or_insert_with(|| RestoredUserMessageState {
                            order: event.event_seq,
                            content: Vec::new(),
                        });
                    state.content.push(part);
                }
            }
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let Some(part) = content_block_to_message_part(&chunk.content) {
                    let key = restored_assistant_key(event, chunk.message_id.as_deref());
                    let state = assistant_messages
                        .entry(key)
                        .or_insert_with(|| RestoredAssistantMessageState {
                            order: event.event_seq,
                            content: Vec::new(),
                            tool_calls: Vec::new(),
                            tool_call_ids: HashSet::new(),
                        });
                    state.content.push(part);
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                if let Some(part) = content_block_to_reasoning_part(&chunk.content) {
                    let key = restored_assistant_key(event, chunk.message_id.as_deref());
                    let state = assistant_messages
                        .entry(key)
                        .or_insert_with(|| RestoredAssistantMessageState {
                            order: event.event_seq,
                            content: Vec::new(),
                            tool_calls: Vec::new(),
                            tool_call_ids: HashSet::new(),
                        });
                    state.content.push(part);
                }
            }
            SessionUpdate::ToolCall(call) => {
                let key = restored_assistant_key(event, None);
                let state = assistant_messages
                    .entry(key)
                    .or_insert_with(|| RestoredAssistantMessageState {
                        order: event.event_seq,
                        content: Vec::new(),
                        tool_calls: Vec::new(),
                        tool_call_ids: HashSet::new(),
                    });
                state.order = state.order.min(event.event_seq);
                upsert_restored_tool_call(
                    state,
                    call.tool_call_id.0.as_ref(),
                    Some(call.title.as_str()),
                    call.raw_input.as_ref(),
                );
                update_restored_tool_result(
                    &mut tool_results,
                    call.tool_call_id.0.as_ref(),
                    event.event_seq,
                    Some(call.title.as_str()),
                    call.raw_output.as_ref(),
                    Some(&call.content),
                    Some(call.status),
                );
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let key = restored_assistant_key(event, None);
                let state = assistant_messages
                    .entry(key)
                    .or_insert_with(|| RestoredAssistantMessageState {
                        order: event.event_seq,
                        content: Vec::new(),
                        tool_calls: Vec::new(),
                        tool_call_ids: HashSet::new(),
                    });
                state.order = state.order.min(event.event_seq);
                upsert_restored_tool_call(
                    state,
                    update.tool_call_id.0.as_ref(),
                    update.fields.title.as_deref(),
                    update.fields.raw_input.as_ref(),
                );
                update_restored_tool_result(
                    &mut tool_results,
                    update.tool_call_id.0.as_ref(),
                    event.event_seq,
                    update.fields.title.as_deref(),
                    update.fields.raw_output.as_ref(),
                    update.fields.content.as_deref(),
                    update.fields.status,
                );
            }
            _ => {}
        }
    }

    let mut envelopes = Vec::new();
    for state in user_messages.into_values() {
        if state.content.is_empty() {
            continue;
        }
        envelopes.push(RestoredMessageEnvelope::User {
            order: state.order,
            content: state.content,
        });
    }
    for state in assistant_messages.into_values() {
        if state.content.is_empty() && state.tool_calls.is_empty() {
            continue;
        }
        envelopes.push(RestoredMessageEnvelope::Assistant {
            order: state.order,
            content: state.content,
            tool_calls: state.tool_calls,
        });
    }
    for state in tool_results.into_values() {
        if !state.terminal || state.content.is_empty() && state.details.is_none() {
            continue;
        }
        envelopes.push(RestoredMessageEnvelope::ToolResult {
            order: state.order,
            tool_call_id: state.tool_call_id,
            call_id: state.call_id,
            tool_name: state.tool_name,
            content: state.content,
            details: state.details,
            is_error: state.is_error,
        });
    }

    envelopes.sort_by_key(restored_message_order);
    envelopes.into_iter().map(restored_envelope_to_message).collect()
}

// ─── Helper: companion notification ─────────────────────────

pub(super) fn build_companion_human_response_notification(
    session_id: &str,
    turn_id: Option<&str>,
    request_id: &str,
    payload: &serde_json::Value,
    request_type: Option<&str>,
    resumed_waiting_tool: bool,
) -> SessionNotification {
    let summary = payload
        .get("summary")
        .or_else(|| payload.get("note"))
        .or_else(|| payload.get("choice"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let status = payload
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("responded");
    let response_type = payload.get("type").and_then(|v| v.as_str());

    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = turn_id.map(ToString::to_string);

    let mut event = AgentDashEventV1::new("companion_human_response");
    event.severity = Some("info".to_string());
    event.message = Some(format!("[用户回应] status={status} {summary}"));
    event.data = Some(serde_json::json!({
        "request_id": request_id,
        "status": status,
        "summary": summary,
        "payload": payload,
        "request_type": request_type,
        "response_type": response_type,
        "resumed_waiting_tool": resumed_waiting_tool,
    }));

    let source = AgentDashSourceV1::new("agentdash-companion", "human_respond");
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));

    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(
            merge_agentdash_meta(None, &agentdash).expect("构造 companion response meta 不应失败"),
        )),
    )
}

// ─── Private helpers ────────────────────────────────────────

fn restored_user_key(event: &PersistedSessionEvent) -> String {
    event.turn_id
        .as_deref()
        .map(|turn_id| format!("user:turn:{turn_id}"))
        .unwrap_or_else(|| format!("user:event:{}", event.event_seq))
}

fn restored_assistant_key(event: &PersistedSessionEvent, message_id: Option<&str>) -> String {
    if let (Some(turn_id), Some(entry_index)) = (event.turn_id.as_deref(), event.entry_index) {
        return format!("assistant:turn:{turn_id}:{entry_index}");
    }
    if let Some(message_id) = message_id.map(str::trim).filter(|value| !value.is_empty()) {
        return format!("assistant:msg:{message_id}");
    }
    if let Some(tool_call_id) = event
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return format!("assistant:tool:{tool_call_id}");
    }
    format!("assistant:event:{}", event.event_seq)
}

fn upsert_restored_tool_call(
    state: &mut RestoredAssistantMessageState,
    tool_call_id: &str,
    title: Option<&str>,
    raw_input: Option<&serde_json::Value>,
) {
    let resolved_title = title
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("tool_call");
    let arguments = raw_input
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    if let Some(existing) = state.tool_calls.iter_mut().find(|item| item.id == tool_call_id) {
        if existing.name.trim().is_empty() {
            existing.name = resolved_title.to_string();
        }
        if existing.arguments.is_null() || existing.arguments == serde_json::json!({}) {
            existing.arguments = arguments;
        }
        return;
    }

    if state.tool_call_ids.insert(tool_call_id.to_string()) {
        state.tool_calls.push(ToolCallInfo {
            id: tool_call_id.to_string(),
            call_id: Some(tool_call_id.to_string()),
            name: resolved_title.to_string(),
            arguments,
        });
    }
}

fn update_restored_tool_result(
    tool_results: &mut HashMap<String, RestoredToolResultState>,
    tool_call_id: &str,
    order: u64,
    title: Option<&str>,
    raw_output: Option<&serde_json::Value>,
    content: Option<&[ToolCallContent]>,
    status: Option<ToolCallStatus>,
) {
    let state = tool_results
        .entry(tool_call_id.to_string())
        .or_insert_with(|| RestoredToolResultState {
            order,
            tool_call_id: tool_call_id.to_string(),
            call_id: Some(tool_call_id.to_string()),
            ..RestoredToolResultState::default()
        });

    if let Some(title) = title.map(str::trim).filter(|value| !value.is_empty()) {
        state.tool_name = Some(title.to_string());
    }

    let Some(status) = status else {
        return;
    };
    if !matches!(status, ToolCallStatus::Completed | ToolCallStatus::Failed) {
        return;
    }

    state.order = order;
    state.terminal = true;
    state.is_error = matches!(status, ToolCallStatus::Failed);

    if let Some(raw_output) = raw_output {
        if let Ok(decoded) = serde_json::from_value::<agentdash_spi::tool::AgentToolResult>(
            raw_output.clone(),
        ) {
            state.content = decoded.content;
            state.details = decoded.details;
            state.is_error = decoded.is_error || state.is_error;
            return;
        }

        state.details = Some(raw_output.clone());
        if state.content.is_empty() {
            state.content = vec![ContentPart::text(json_preview(raw_output))];
        }
    }

    if let Some(content) = content {
        let next_content = tool_call_content_to_content_parts(content);
        if !next_content.is_empty() {
            state.content = next_content;
        }
    }
}

fn restored_message_order(envelope: &RestoredMessageEnvelope) -> u64 {
    match envelope {
        RestoredMessageEnvelope::User { order, .. }
        | RestoredMessageEnvelope::Assistant { order, .. }
        | RestoredMessageEnvelope::ToolResult { order, .. } => *order,
    }
}

fn restored_envelope_to_message(envelope: RestoredMessageEnvelope) -> AgentMessage {
    match envelope {
        RestoredMessageEnvelope::User { content, .. } => AgentMessage::User {
            content,
            timestamp: None,
        },
        RestoredMessageEnvelope::Assistant {
            content, tool_calls, ..
        } => AgentMessage::Assistant {
            content,
            tool_calls: tool_calls.clone(),
            stop_reason: Some(if tool_calls.is_empty() {
                StopReason::Stop
            } else {
                StopReason::ToolUse
            }),
            error_message: None,
            usage: None,
            timestamp: None,
        },
        RestoredMessageEnvelope::ToolResult {
            tool_call_id,
            call_id,
            tool_name,
            content,
            details,
            is_error,
            ..
        } => AgentMessage::ToolResult {
            tool_call_id,
            call_id,
            tool_name,
            content,
            details,
            is_error,
            timestamp: None,
        },
    }
}

fn message_key(
    role: &str,
    event: &PersistedSessionEvent,
    message_id: Option<&str>,
) -> String {
    if let Some(message_id) = message_id.map(str::trim).filter(|value| !value.is_empty()) {
        return format!("{role}:msg:{message_id}");
    }
    if let (Some(turn_id), Some(entry_index)) = (event.turn_id.as_deref(), event.entry_index) {
        return format!("{role}:turn:{turn_id}:{entry_index}");
    }
    format!("{role}:event:{}", event.event_seq)
}

fn merge_transcript_chunk(
    entries: &mut Vec<ContinuationTranscriptEntry>,
    index: &mut HashMap<String, usize>,
    key: String,
    order: u64,
    role: &'static str,
    turn_id: Option<String>,
    incoming_text: &str,
) {
    if let Some(existing_index) = index.get(&key).copied() {
        if let Some(entry) = entries.get_mut(existing_index) {
            entry.text = merge_continuation_text(&entry.text, incoming_text);
            return;
        }
    }
    index.insert(key, entries.len());
    entries.push(ContinuationTranscriptEntry {
        order,
        role,
        turn_id,
        text: incoming_text.to_string(),
    });
}

fn merge_continuation_text(previous: &str, incoming: &str) -> String {
    if incoming.is_empty() || incoming == previous {
        return previous.to_string();
    }
    if previous.is_empty() || incoming.starts_with(previous) {
        return incoming.to_string();
    }

    format!("{previous}{incoming}")
}

fn content_block_to_transcript_text(block: &ContentBlock) -> Option<String> {
    content_block_to_rendered_text(block)
}

fn content_block_to_message_part(block: &ContentBlock) -> Option<ContentPart> {
    match block {
        ContentBlock::Image(image) => Some(ContentPart::Image {
            mime_type: image.mime_type.clone(),
            data: image.data.clone(),
        }),
        _ => content_block_to_rendered_text(block).map(ContentPart::text),
    }
}

fn content_block_to_reasoning_part(block: &ContentBlock) -> Option<ContentPart> {
    content_block_to_rendered_text(block).map(|text| ContentPart::reasoning(text, None, None))
}

fn content_block_to_rendered_text(block: &ContentBlock) -> Option<String> {
    if is_owner_context_resource_block(block) {
        return None;
    }

    content_block_to_text(block)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn is_owner_context_resource_block(block: &ContentBlock) -> bool {
    match block {
        ContentBlock::Resource(resource) => match &resource.resource {
            agent_client_protocol::EmbeddedResourceResource::TextResourceContents(text_resource) => {
                let uri = text_resource.uri.as_str();
                uri.starts_with("agentdash://project-context/")
                    || uri.starts_with("agentdash://story-context/")
                    || uri.starts_with("agentdash://task-context/")
            }
            _ => false,
        },
        _ => false,
    }
}

fn tool_call_content_to_content_parts(content: &[ToolCallContent]) -> Vec<ContentPart> {
    let mut parts = Vec::new();
    for item in content {
        match item {
            ToolCallContent::Content(content) => {
                if let Some(part) = content_block_to_message_part(&content.content) {
                    parts.push(part);
                }
            }
            ToolCallContent::Diff(diff) => {
                parts.push(ContentPart::text(format!(
                    "<diff path=\"{}\">\n{}\n</diff>",
                    diff.path.display(),
                    diff.new_text
                )));
            }
            ToolCallContent::Terminal(terminal) => {
                parts.push(ContentPart::text(format!(
                    "[terminal:{}]",
                    terminal.terminal_id.0
                )));
            }
            _ => {}
        }
    }
    parts
}

fn json_preview(value: &serde_json::Value) -> String {
    const MAX_LEN: usize = 320;
    let rendered = value.to_string();
    if rendered.len() <= MAX_LEN {
        rendered
    } else {
        let shortened: String = rendered.chars().take(MAX_LEN).collect();
        format!("{shortened}...")
    }
}
