use std::collections::{HashMap, HashSet};

use base64::Engine;

use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    AgentDashNativeThreadItem, AgentDashThreadItem, BackboneEvent, ItemStartedNotification,
    ItemUpdatedNotification, user_input_blocks_to_content_parts,
};
use agentdash_agent_types::{
    AgentMessage, ContentPart, MessageRef, ProjectedEntry, ProjectedTranscript, ProjectionKind,
    StopReason, ToolCallInfo,
};

use super::persistence::PersistedSessionEvent;

// ─── Restored session messages ─────────────────────────────

#[derive(Debug, Clone)]
struct RestoredUserMessageState {
    order: u64,
    turn_id: Option<String>,
    entry_index: Option<u32>,
    content: Vec<ContentPart>,
}

#[derive(Debug, Clone)]
struct RestoredAssistantMessageState {
    order: u64,
    turn_id: Option<String>,
    entry_index: Option<u32>,
    content: Vec<ContentPart>,
    tool_calls: Vec<ToolCallInfo>,
    tool_call_ids: HashSet<String>,
}

#[derive(Debug, Clone, Default)]
struct RestoredToolResultState {
    order: u64,
    turn_id: Option<String>,
    entry_index: Option<u32>,
    tool_call_id: String,
    call_id: Option<String>,
    tool_name: Option<String>,
    content: Vec<ContentPart>,
    details: Option<serde_json::Value>,
    is_error: bool,
    terminal: bool,
}

const RESTORED_TOOL_OUTPUT_MISSING_MARKER: &str = "[restored tool output missing]";

#[derive(Debug, Clone)]
enum RestoredMessageEnvelope {
    User {
        order: u64,
        message_ref: MessageRef,
        content: Vec<ContentPart>,
    },
    Assistant {
        order: u64,
        message_ref: MessageRef,
        content: Vec<ContentPart>,
        tool_calls: Vec<ToolCallInfo>,
    },
    ToolResult {
        order: u64,
        message_ref: MessageRef,
        tool_call_id: String,
        call_id: Option<String>,
        tool_name: Option<String>,
        content: Vec<ContentPart>,
        details: Option<serde_json::Value>,
        is_error: bool,
    },
}

/// 从持久化事件重建完整原始 transcript，不消费 compaction checkpoint。
pub(super) fn build_raw_projected_transcript_from_events(
    events: &[PersistedSessionEvent],
) -> ProjectedTranscript {
    build_raw_projected_transcript_from_filtered_events(events.iter())
}

/// 从持久化事件重建完整原始 transcript，不消费 compaction checkpoint。
pub(super) fn build_raw_projected_transcript_from_filtered_events<'a>(
    events: impl IntoIterator<Item = &'a PersistedSessionEvent>,
) -> ProjectedTranscript {
    build_raw_projected_transcript_from_iter(events)
}

fn build_raw_projected_transcript_from_iter<'a>(
    events: impl IntoIterator<Item = &'a PersistedSessionEvent>,
) -> ProjectedTranscript {
    let mut user_messages: HashMap<String, RestoredUserMessageState> = HashMap::new();
    let mut assistant_messages: HashMap<String, RestoredAssistantMessageState> = HashMap::new();
    let mut tool_results: HashMap<String, RestoredToolResultState> = HashMap::new();

    for event in events {
        match &event.notification.event {
            BackboneEvent::UserInputSubmitted(input) => {
                let key = format!("user:item:{}", input.item_id);
                let state = user_messages
                    .entry(key)
                    .or_insert_with(|| RestoredUserMessageState {
                        order: event.event_seq,
                        turn_id: event
                            .turn_id
                            .clone()
                            .or_else(|| Some(input.turn_id.clone())),
                        entry_index: event.entry_index,
                        content: Vec::new(),
                    });
                for part in user_input_blocks_to_content_parts(&input.content) {
                    state.content.push(part);
                }
            }
            BackboneEvent::AgentMessageDelta(delta) => {
                if !delta.delta.is_empty() {
                    let key = restored_assistant_key(event, Some(&delta.item_id));
                    let state = assistant_messages.entry(key).or_insert_with(|| {
                        RestoredAssistantMessageState {
                            order: event.event_seq,
                            turn_id: event.turn_id.clone(),
                            entry_index: event.entry_index,
                            content: Vec::new(),
                            tool_calls: Vec::new(),
                            tool_call_ids: HashSet::new(),
                        }
                    });
                    state.content.push(ContentPart::text(&delta.delta));
                }
            }
            BackboneEvent::ReasoningTextDelta(delta) => {
                if !delta.delta.is_empty() {
                    let key = restored_assistant_key(event, Some(&delta.item_id));
                    let state = assistant_messages.entry(key).or_insert_with(|| {
                        RestoredAssistantMessageState {
                            order: event.event_seq,
                            turn_id: event.turn_id.clone(),
                            entry_index: event.entry_index,
                            content: Vec::new(),
                            tool_calls: Vec::new(),
                            tool_call_ids: HashSet::new(),
                        }
                    });
                    state
                        .content
                        .push(ContentPart::reasoning(&delta.delta, None, None));
                }
            }
            BackboneEvent::ReasoningSummaryDelta(delta) => {
                if !delta.delta.is_empty() {
                    let key = restored_assistant_key(event, Some(&delta.item_id));
                    let state = assistant_messages.entry(key).or_insert_with(|| {
                        RestoredAssistantMessageState {
                            order: event.event_seq,
                            turn_id: event.turn_id.clone(),
                            entry_index: event.entry_index,
                            content: Vec::new(),
                            tool_calls: Vec::new(),
                            tool_call_ids: HashSet::new(),
                        }
                    });
                    state
                        .content
                        .push(ContentPart::reasoning(&delta.delta, None, None));
                }
            }
            BackboneEvent::ItemStarted(ItemStartedNotification { item, .. })
            | BackboneEvent::ItemUpdated(ItemUpdatedNotification { item, .. }) => {
                if let Some(tc) = extract_tool_call_from_thread_item(item) {
                    let key = restored_assistant_key(event, None);
                    let state = assistant_messages.entry(key).or_insert_with(|| {
                        RestoredAssistantMessageState {
                            order: event.event_seq,
                            turn_id: event.turn_id.clone(),
                            entry_index: event.entry_index,
                            content: Vec::new(),
                            tool_calls: Vec::new(),
                            tool_call_ids: HashSet::new(),
                        }
                    });
                    state.order = state.order.min(event.event_seq);
                    upsert_restored_tool_call(state, &tc.id, Some(&tc.name), tc.raw_input.as_ref());
                    if tc.is_terminal {
                        update_restored_tool_result(
                            &mut tool_results,
                            &tc.id,
                            event,
                            Some(&tc.name),
                            tc.raw_output.as_ref(),
                            &tc.content_parts,
                            tc.is_error,
                        );
                    }
                }
            }
            BackboneEvent::ItemCompleted(n) => {
                if let Some(tc) = extract_tool_call_from_thread_item(&n.item) {
                    let key = restored_assistant_key(event, None);
                    let state = assistant_messages.entry(key).or_insert_with(|| {
                        RestoredAssistantMessageState {
                            order: event.event_seq,
                            turn_id: event.turn_id.clone(),
                            entry_index: event.entry_index,
                            content: Vec::new(),
                            tool_calls: Vec::new(),
                            tool_call_ids: HashSet::new(),
                        }
                    });
                    state.order = state.order.min(event.event_seq);
                    upsert_restored_tool_call(state, &tc.id, Some(&tc.name), tc.raw_input.as_ref());
                    if tc.is_terminal {
                        update_restored_tool_result(
                            &mut tool_results,
                            &tc.id,
                            event,
                            Some(&tc.name),
                            tc.raw_output.as_ref(),
                            &tc.content_parts,
                            tc.is_error,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    ensure_restored_tool_outputs(&assistant_messages, &mut tool_results);

    let mut envelopes = Vec::new();
    for state in user_messages.into_values() {
        if state.content.is_empty() {
            continue;
        }
        envelopes.push(RestoredMessageEnvelope::User {
            order: state.order,
            message_ref: make_message_ref(state.turn_id.as_deref(), state.entry_index, state.order),
            content: state.content,
        });
    }
    for state in assistant_messages.into_values() {
        if state.content.is_empty() && state.tool_calls.is_empty() {
            continue;
        }
        envelopes.push(RestoredMessageEnvelope::Assistant {
            order: state.order,
            message_ref: make_message_ref(state.turn_id.as_deref(), state.entry_index, state.order),
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
            message_ref: make_message_ref(state.turn_id.as_deref(), state.entry_index, state.order),
            tool_call_id: state.tool_call_id,
            call_id: state.call_id,
            tool_name: state.tool_name,
            content: state.content,
            details: state.details,
            is_error: state.is_error,
        });
    }

    envelopes.sort_by_key(restored_message_order);
    let entries: Vec<ProjectedEntry> = envelopes
        .into_iter()
        .map(restored_envelope_to_projected_entry)
        .collect();
    ProjectedTranscript { entries }
}

// ─── Private helpers ────────────────────────────────────────

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
    let arguments = raw_input.cloned().unwrap_or_else(|| serde_json::json!({}));

    if let Some(existing) = state
        .tool_calls
        .iter_mut()
        .find(|item| item.id == tool_call_id)
    {
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
    event: &PersistedSessionEvent,
    title: Option<&str>,
    raw_output: Option<&serde_json::Value>,
    content_parts: &[ContentPart],
    is_error: bool,
) {
    let order = event.event_seq;
    let state = tool_results
        .entry(tool_call_id.to_string())
        .or_insert_with(|| RestoredToolResultState {
            order,
            turn_id: event.turn_id.clone(),
            entry_index: event.entry_index,
            tool_call_id: tool_call_id.to_string(),
            call_id: Some(tool_call_id.to_string()),
            ..RestoredToolResultState::default()
        });

    if let Some(title) = title.map(str::trim).filter(|value| !value.is_empty()) {
        state.tool_name = Some(title.to_string());
    }

    state.order = order;
    state.turn_id = event.turn_id.clone();
    state.entry_index = event.entry_index;
    state.terminal = true;
    state.is_error = is_error;

    if let Some(raw_output) = raw_output {
        if let Ok(decoded) =
            serde_json::from_value::<agentdash_spi::AgentToolResult>(raw_output.clone())
        {
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

    if !content_parts.is_empty() {
        state.content = content_parts.to_vec();
    }

    if state.content.is_empty() && state.details.is_none() {
        apply_missing_tool_output_placeholder(state);
    }
}

fn ensure_restored_tool_outputs(
    assistant_messages: &HashMap<String, RestoredAssistantMessageState>,
    tool_results: &mut HashMap<String, RestoredToolResultState>,
) {
    for assistant in assistant_messages.values() {
        for tool_call in &assistant.tool_calls {
            tool_results
                .entry(tool_call.id.clone())
                .or_insert_with(|| missing_tool_result_state(assistant, tool_call));
        }
    }
}

fn missing_tool_result_state(
    assistant: &RestoredAssistantMessageState,
    tool_call: &ToolCallInfo,
) -> RestoredToolResultState {
    let mut state = RestoredToolResultState {
        order: assistant.order,
        turn_id: assistant.turn_id.clone(),
        entry_index: assistant.entry_index,
        tool_call_id: tool_call.id.clone(),
        call_id: tool_call
            .call_id
            .clone()
            .or_else(|| Some(tool_call.id.clone())),
        tool_name: Some(tool_call.name.clone()),
        is_error: true,
        terminal: true,
        ..RestoredToolResultState::default()
    };
    apply_missing_tool_output_placeholder(&mut state);
    state
}

fn apply_missing_tool_output_placeholder(state: &mut RestoredToolResultState) {
    let tool_name = state
        .tool_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("tool_call")
        .to_string();
    let tool_call_id = state.tool_call_id.clone();
    state.content = vec![ContentPart::text(format!(
        "{RESTORED_TOOL_OUTPUT_MISSING_MARKER}\ntool_call_id: {tool_call_id}\ntool_name: {tool_name}\nThe original tool output was not available in persisted session events. A restored placeholder preserves the tool-call/tool-result pair so continuation can proceed.",
    ))];
    state.details = Some(serde_json::json!({
        "type": "restored_tool_output_missing",
        "tool_call_id": tool_call_id,
        "tool_name": tool_name,
    }));
    state.is_error = true;
    state.terminal = true;
}

fn restored_message_order(envelope: &RestoredMessageEnvelope) -> u64 {
    match envelope {
        RestoredMessageEnvelope::User { order, .. }
        | RestoredMessageEnvelope::Assistant { order, .. }
        | RestoredMessageEnvelope::ToolResult { order, .. } => *order,
    }
}

/// 从 RestoredMessageEnvelope 构建 ProjectedEntry（带 MessageRef + ProjectionKind）。
fn restored_envelope_to_projected_entry(envelope: RestoredMessageEnvelope) -> ProjectedEntry {
    match envelope {
        RestoredMessageEnvelope::User {
            message_ref,
            content,
            order,
        } => ProjectedEntry::event(
            message_ref,
            ProjectionKind::Transcript,
            AgentMessage::User {
                content,
                timestamp: None,
            },
            Some(order),
        ),
        RestoredMessageEnvelope::Assistant {
            message_ref,
            content,
            tool_calls,
            order,
        } => ProjectedEntry::event(
            message_ref,
            ProjectionKind::Transcript,
            AgentMessage::Assistant {
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
            Some(order),
        ),
        RestoredMessageEnvelope::ToolResult {
            message_ref,
            tool_call_id,
            call_id,
            tool_name,
            content,
            details,
            is_error,
            order,
        } => ProjectedEntry::event(
            message_ref,
            ProjectionKind::Transcript,
            AgentMessage::ToolResult {
                tool_call_id,
                call_id,
                tool_name,
                content,
                details,
                is_error,
                timestamp: None,
            },
            Some(order),
        ),
    }
}

/// 从 turn_id + entry_index 构建 MessageRef。
/// 当 turn_id 缺失时，用 event_seq 作为 fallback（保证唯一性）。
fn make_message_ref(turn_id: Option<&str>, entry_index: Option<u32>, event_seq: u64) -> MessageRef {
    MessageRef {
        turn_id: turn_id
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("_seq:{event_seq}")),
        entry_index: entry_index.unwrap_or(0),
    }
}

// ─── ThreadItem → tool call 提取 ──────────────────────────

struct ExtractedToolCall {
    id: String,
    name: String,
    raw_input: Option<serde_json::Value>,
    raw_output: Option<serde_json::Value>,
    content_parts: Vec<ContentPart>,
    is_terminal: bool,
    is_error: bool,
}

fn extract_tool_call_from_thread_item(item: &AgentDashThreadItem) -> Option<ExtractedToolCall> {
    match item {
        AgentDashThreadItem::Codex(item) => extract_tool_call_from_codex_thread_item(item),
        AgentDashThreadItem::AgentDash(item) => extract_tool_call_from_agentdash_thread_item(item),
    }
}

fn extract_tool_call_from_codex_thread_item(item: &codex::ThreadItem) -> Option<ExtractedToolCall> {
    match item {
        codex::ThreadItem::DynamicToolCall {
            id,
            tool,
            arguments,
            status,
            content_items,
            success,
            ..
        } => {
            let is_terminal = matches!(
                status,
                codex::DynamicToolCallStatus::Completed | codex::DynamicToolCallStatus::Failed
            );
            let content_parts = content_items
                .as_ref()
                .map(|items| codex_content_items_to_parts(items))
                .unwrap_or_default();
            Some(ExtractedToolCall {
                id: id.clone(),
                name: tool.clone(),
                raw_input: Some(arguments.clone()),
                raw_output: None,
                content_parts,
                is_terminal,
                is_error: success == &Some(false)
                    || matches!(status, codex::DynamicToolCallStatus::Failed),
            })
        }
        codex::ThreadItem::McpToolCall {
            id,
            tool,
            arguments,
            status,
            result,
            error,
            ..
        } => {
            let is_terminal = matches!(
                status,
                codex::McpToolCallStatus::Completed | codex::McpToolCallStatus::Failed
            );
            let raw_output = result
                .as_ref()
                .and_then(|r| serde_json::to_value(r).ok())
                .or_else(|| {
                    error
                        .as_ref()
                        .map(|e| serde_json::Value::String(e.message.clone()))
                });
            Some(ExtractedToolCall {
                id: id.clone(),
                name: tool.clone(),
                raw_input: Some(arguments.clone()),
                raw_output,
                content_parts: Vec::new(),
                is_terminal,
                is_error: error.is_some() || matches!(status, codex::McpToolCallStatus::Failed),
            })
        }
        codex::ThreadItem::CommandExecution {
            id,
            command,
            status,
            exit_code,
            aggregated_output,
            ..
        } => {
            let is_terminal = matches!(
                status,
                codex::CommandExecutionStatus::Completed
                    | codex::CommandExecutionStatus::Failed
                    | codex::CommandExecutionStatus::Declined
            );
            let content_parts = aggregated_output
                .as_ref()
                .map(|output| vec![ContentPart::text(output)])
                .unwrap_or_default();
            Some(ExtractedToolCall {
                id: id.clone(),
                name: "command_execution".to_string(),
                raw_input: Some(serde_json::json!({ "command": command })),
                raw_output: None,
                content_parts,
                is_terminal,
                is_error: exit_code.is_some_and(|code| code != 0)
                    || matches!(status, codex::CommandExecutionStatus::Failed),
            })
        }
        codex::ThreadItem::FileChange { id, status, .. } => {
            let is_terminal = matches!(
                status,
                codex::PatchApplyStatus::Completed
                    | codex::PatchApplyStatus::Failed
                    | codex::PatchApplyStatus::Declined
            );
            Some(ExtractedToolCall {
                id: id.clone(),
                name: "file_change".to_string(),
                raw_input: None,
                raw_output: None,
                content_parts: Vec::new(),
                is_terminal,
                is_error: matches!(status, codex::PatchApplyStatus::Failed),
            })
        }
        _ => None,
    }
}

fn extract_tool_call_from_agentdash_thread_item(
    item: &AgentDashNativeThreadItem,
) -> Option<ExtractedToolCall> {
    let is_terminal = matches!(
        item.status(),
        codex::DynamicToolCallStatus::Completed | codex::DynamicToolCallStatus::Failed
    );
    let content_parts = item
        .content_items()
        .map(|items| codex_content_items_to_parts(items))
        .unwrap_or_else(|| {
            item.shell_output()
                .map(|output| vec![ContentPart::text(output)])
                .unwrap_or_default()
        });
    Some(ExtractedToolCall {
        id: item.id().to_string(),
        name: item.tool_name().to_string(),
        raw_input: Some(item.arguments().clone()),
        raw_output: None,
        content_parts,
        is_terminal,
        is_error: item.success() == Some(false)
            || matches!(item.status(), codex::DynamicToolCallStatus::Failed),
    })
}

fn codex_content_items_to_parts(
    items: &[codex::DynamicToolCallOutputContentItem],
) -> Vec<ContentPart> {
    items
        .iter()
        .filter_map(|item| match item {
            codex::DynamicToolCallOutputContentItem::InputText { text } => {
                if text.trim().is_empty() {
                    None
                } else {
                    Some(ContentPart::text(text))
                }
            }
            codex::DynamicToolCallOutputContentItem::InputImage { image_url } => {
                parse_data_image_url(image_url)
                    .or_else(|| Some(ContentPart::text("[image output: unsupported image_url]")))
            }
        })
        .collect()
}

fn parse_data_image_url(image_url: &str) -> Option<ContentPart> {
    let value = image_url.trim();
    let rest = value.strip_prefix("data:")?;
    let (mime_type, data) = rest.split_once(";base64,")?;
    let mime_type = mime_type.trim();
    let data = data.trim();
    if !mime_type.starts_with("image/") || data.is_empty() {
        return None;
    }
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    Some(ContentPart::Image {
        mime_type: mime_type.to_string(),
        data: data.to_string(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_protocol::{
        BackboneEnvelope, ItemCompletedNotification, ItemUpdatedNotification, SourceInfo, TraceInfo,
        backbone::thread_item,
    };

    fn test_source() -> SourceInfo {
        SourceInfo {
            connector_id: "test-connector".to_string(),
            connector_type: "pi_agent".to_string(),
            executor_id: None,
        }
    }

    fn persisted_event(
        event_seq: u64,
        session_update_type: &str,
        turn_id: &str,
        entry_index: u32,
        event: BackboneEvent,
        tool_call_id: Option<&str>,
    ) -> PersistedSessionEvent {
        PersistedSessionEvent {
            session_id: "session-1".to_string(),
            event_seq,
            occurred_at_ms: event_seq as i64,
            committed_at_ms: event_seq as i64,
            session_update_type: session_update_type.to_string(),
            turn_id: Some(turn_id.to_string()),
            entry_index: Some(entry_index),
            tool_call_id: tool_call_id.map(ToString::to_string),
            notification: BackboneEnvelope::new(event, "session-1", test_source()).with_trace(
                TraceInfo {
                    turn_id: Some(turn_id.to_string()),
                    entry_index: Some(entry_index),
                },
            ),
        }
    }

    #[test]
    fn restored_file_change_without_output_gets_placeholder_tool_result() {
        let tool_call_id = "turn_001:tool_011";
        let file_change = thread_item::file_change(
            tool_call_id,
            vec![thread_item::FileChangeSpec::Edit {
                path: "cvs-lifecycle-events-viewer://src/main.tsx".to_string(),
                unified_diff: "@@\n-old\n+new".to_string(),
            }],
            codex::PatchApplyStatus::Completed,
        )
        .expect("file change item should build");
        let event = persisted_event(
            1,
            "item_completed",
            "turn-raw",
            0,
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                file_change,
                "session-1".to_string(),
                "turn-raw".to_string(),
            )),
            Some(tool_call_id),
        );

        let transcript = build_raw_projected_transcript_from_events(&[event]);

        assert_eq!(transcript.entries.len(), 2);
        match &transcript.entries[0].message {
            AgentMessage::Assistant { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].id, tool_call_id);
                assert_eq!(tool_calls[0].call_id.as_deref(), Some(tool_call_id));
                assert_eq!(tool_calls[0].name, "file_change");
            }
            other => panic!("expected restored assistant tool call, got {other:?}"),
        }
        match &transcript.entries[1].message {
            AgentMessage::ToolResult {
                tool_call_id: restored_id,
                call_id,
                tool_name,
                content,
                details,
                is_error,
                ..
            } => {
                assert_eq!(restored_id, tool_call_id);
                assert_eq!(call_id.as_deref(), Some(tool_call_id));
                assert_eq!(tool_name.as_deref(), Some("file_change"));
                assert!(*is_error);
                let text = content
                    .first()
                    .and_then(ContentPart::extract_text)
                    .expect("placeholder should have text");
                assert!(text.contains(RESTORED_TOOL_OUTPUT_MISSING_MARKER));
                assert!(text.contains(tool_call_id));
                assert_eq!(
                    details
                        .as_ref()
                        .and_then(|details| details.get("type"))
                        .and_then(serde_json::Value::as_str),
                    Some("restored_tool_output_missing")
                );
            }
            other => panic!("expected restored placeholder tool result, got {other:?}"),
        }
    }

    #[test]
    fn item_updated_restores_in_flight_tool_call_like_item_started() {
        let tool_call_id = "turn_001:tool_042";
        let file_change = thread_item::file_change(
            tool_call_id,
            vec![thread_item::FileChangeSpec::Edit {
                path: "workspace://src/lib.rs".to_string(),
                unified_diff: "@@\n-old\n+new".to_string(),
            }],
            codex::PatchApplyStatus::InProgress,
        )
        .expect("file change item should build");
        let event = persisted_event(
            1,
            "item_updated",
            "turn-raw",
            0,
            BackboneEvent::ItemUpdated(ItemUpdatedNotification::new(
                file_change,
                "session-1".to_string(),
                "turn-raw".to_string(),
            )),
            Some(tool_call_id),
        );

        let transcript = build_raw_projected_transcript_from_events(&[event]);

        // 与 ItemStarted 一致：in-flight tool call 在重放中可见。
        let assistant = transcript
            .entries
            .iter()
            .find_map(|entry| match &entry.message {
                AgentMessage::Assistant { tool_calls, .. } => Some(tool_calls),
                _ => None,
            })
            .expect("expected restored assistant message with tool call");
        assert_eq!(assistant.len(), 1);
        assert_eq!(assistant[0].id, tool_call_id);
        assert_eq!(assistant[0].name, "file_change");
    }

    #[test]
    fn codex_content_items_restore_input_image_data_url() {
        let parts = codex_content_items_to_parts(&[
            codex::DynamicToolCallOutputContentItem::InputText {
                text: "metadata".to_string(),
            },
            codex::DynamicToolCallOutputContentItem::InputImage {
                image_url: "data:image/png;base64,AAECAw==".to_string(),
            },
        ]);

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].extract_text(), Some("metadata"));
        match &parts[1] {
            ContentPart::Image { mime_type, data } => {
                assert_eq!(mime_type, "image/png");
                assert_eq!(data, "AAECAw==");
            }
            other => panic!("expected image part, got {other:?}"),
        }
    }

    #[test]
    fn codex_content_items_reject_external_image_url() {
        let parts =
            codex_content_items_to_parts(&[codex::DynamicToolCallOutputContentItem::InputImage {
                image_url: "https://example.invalid/image.png".to_string(),
            }]);

        assert_eq!(parts.len(), 1);
        assert_eq!(
            parts[0].extract_text(),
            Some("[image output: unsupported image_url]")
        );
    }
}
