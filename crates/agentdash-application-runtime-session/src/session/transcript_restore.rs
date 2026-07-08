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
    /// 终态权威助手正文（来自 turn 收尾 ItemCompleted(AgentMessage)）。
    /// 存在时覆盖 delta 累积文本。
    final_text: Option<String>,
    /// 终态权威 reasoning（来自 turn 收尾 ItemCompleted(Reasoning)）。
    /// 存在时覆盖 delta 累积 reasoning。
    final_reasoning: Option<String>,
}

impl RestoredAssistantMessageState {
    fn new(event: &PersistedSessionEvent) -> Self {
        RestoredAssistantMessageState {
            order: event.event_seq,
            turn_id: event.turn_id.clone(),
            entry_index: event.entry_index,
            content: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_ids: HashSet::new(),
            final_text: None,
            final_reasoning: None,
        }
    }
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
            BackboneEvent::AgentMessageDelta(delta) if !delta.delta.is_empty() => {
                let key = restored_assistant_key(event, Some(&delta.item_id));
                let state = assistant_messages
                    .entry(key)
                    .or_insert_with(|| RestoredAssistantMessageState::new(event));
                state.content.push(ContentPart::text(&delta.delta));
            }
            BackboneEvent::ReasoningTextDelta(delta) if !delta.delta.is_empty() => {
                let key = restored_assistant_key(event, Some(&delta.item_id));
                let state = assistant_messages
                    .entry(key)
                    .or_insert_with(|| RestoredAssistantMessageState::new(event));
                state
                    .content
                    .push(ContentPart::reasoning(&delta.delta, None, None));
            }
            BackboneEvent::ReasoningSummaryDelta(delta) if !delta.delta.is_empty() => {
                let key = restored_assistant_key(event, Some(&delta.item_id));
                let state = assistant_messages
                    .entry(key)
                    .or_insert_with(|| RestoredAssistantMessageState::new(event));
                state
                    .content
                    .push(ContentPart::reasoning(&delta.delta, None, None));
            }
            BackboneEvent::ItemStarted(ItemStartedNotification { item, .. })
            | BackboneEvent::ItemUpdated(ItemUpdatedNotification { item, .. }) => {
                if let Some(tc) = extract_tool_call_from_thread_item(item) {
                    let key = restored_assistant_key(event, None);
                    let state = assistant_messages
                        .entry(key)
                        .or_insert_with(|| RestoredAssistantMessageState::new(event));
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
                    let state = assistant_messages
                        .entry(key)
                        .or_insert_with(|| RestoredAssistantMessageState::new(event));
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
                } else if let Some(final_message) =
                    extract_assistant_message_from_thread_item(&n.item)
                {
                    // 终态助手正文 / reasoning：作为权威来源覆盖 delta 累积。
                    // item_id 与 delta 同源（turn_id:entry_index:msg|reason），
                    // 通过 restored_assistant_key 命中同一 assistant 状态。
                    let item_id = final_message.item_id.clone();
                    let key = restored_assistant_key(event, Some(&item_id));
                    let state = assistant_messages
                        .entry(key)
                        .or_insert_with(|| RestoredAssistantMessageState::new(event));
                    state.order = state.order.min(event.event_seq);
                    match final_message.kind {
                        FinalAssistantKind::Text(text) => {
                            state.final_text = Some(text);
                        }
                        FinalAssistantKind::Reasoning(reasoning) => {
                            state.final_reasoning = Some(reasoning);
                        }
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
        let content = resolve_assistant_content(&state);
        if content.is_empty() && state.tool_calls.is_empty() {
            continue;
        }
        envelopes.push(RestoredMessageEnvelope::Assistant {
            order: state.order,
            message_ref: make_message_ref(state.turn_id.as_deref(), state.entry_index, state.order),
            content,
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

/// 终态助手消息（来自 turn 收尾 ItemCompleted），承载权威正文 / reasoning。
struct FinalAssistantMessage {
    item_id: String,
    kind: FinalAssistantKind,
}

enum FinalAssistantKind {
    Text(String),
    Reasoning(String),
}

/// 从 ThreadItem 中识别终态助手消息（AgentMessage / Reasoning），它们不是 tool call。
/// 这是 Step 0 的权威来源：重放时优先于 delta 累积。
fn extract_assistant_message_from_thread_item(
    item: &AgentDashThreadItem,
) -> Option<FinalAssistantMessage> {
    let AgentDashThreadItem::Codex(item) = item else {
        return None;
    };
    match item {
        codex::ThreadItem::AgentMessage { id, text, .. } => Some(FinalAssistantMessage {
            item_id: id.clone(),
            kind: FinalAssistantKind::Text(text.clone()),
        }),
        codex::ThreadItem::Reasoning { id, content, .. } => Some(FinalAssistantMessage {
            item_id: id.clone(),
            kind: FinalAssistantKind::Reasoning(content.join("")),
        }),
        _ => None,
    }
}

/// 组装助手最终 content：reasoning 在前、text 在后。
/// 终态权威存在时用终态、忽略 delta 累积；否则回退到 delta 累积内容。
fn resolve_assistant_content(state: &RestoredAssistantMessageState) -> Vec<ContentPart> {
    let mut content = Vec::new();

    // reasoning part：final 优先，否则取 delta 累积的 reasoning 部分。
    match &state.final_reasoning {
        Some(reasoning) if !reasoning.is_empty() => {
            content.push(ContentPart::reasoning(reasoning, None, None));
        }
        _ => {
            for part in &state.content {
                if part.extract_reasoning().is_some() {
                    content.push(part.clone());
                }
            }
        }
    }

    // text part：final 优先，否则取 delta 累积的 text 部分。
    match &state.final_text {
        Some(text) if !text.is_empty() => {
            content.push(ContentPart::text(text));
        }
        _ => {
            for part in &state.content {
                if part.extract_text().is_some() {
                    content.push(part.clone());
                }
            }
        }
    }

    content
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
    let bounded = bounded_json_preview_value(value, 0);
    let rendered = bounded.to_string();
    if rendered.len() <= MAX_LEN {
        rendered
    } else {
        let shortened: String = rendered.chars().take(MAX_LEN).collect();
        format!("{shortened}...")
    }
}

fn bounded_json_preview_value(value: &serde_json::Value, depth: usize) -> serde_json::Value {
    const MAX_DEPTH: usize = 8;
    const MAX_ARRAY_ITEMS: usize = 16;
    const MAX_OBJECT_FIELDS: usize = 32;
    const MAX_OBJECT_KEY_CHARS: usize = 96;
    const MAX_STRING_CHARS: usize = 240;

    if depth >= MAX_DEPTH {
        return serde_json::json!({
            "preview": "json_payload_too_deep",
            "max_depth": MAX_DEPTH,
        });
    }

    match value {
        serde_json::Value::Array(items) => {
            let mut bounded = items
                .iter()
                .take(MAX_ARRAY_ITEMS)
                .map(|item| bounded_json_preview_value(item, depth + 1))
                .collect::<Vec<_>>();
            if items.len() > MAX_ARRAY_ITEMS {
                bounded.push(serde_json::json!({
                    "preview": "json_array_truncated",
                    "omitted_items": items.len() - MAX_ARRAY_ITEMS,
                }));
            }
            serde_json::Value::Array(bounded)
        }
        serde_json::Value::Object(map) => {
            let mut bounded = serde_json::Map::new();
            for (key, item) in map.iter().take(MAX_OBJECT_FIELDS) {
                bounded.insert(
                    bounded_json_preview_key(key, MAX_OBJECT_KEY_CHARS),
                    bounded_json_preview_value(item, depth + 1),
                );
            }
            if map.len() > MAX_OBJECT_FIELDS {
                bounded.insert(
                    "__preview_truncated_fields".to_string(),
                    serde_json::json!(map.len() - MAX_OBJECT_FIELDS),
                );
            }
            serde_json::Value::Object(bounded)
        }
        serde_json::Value::String(text) => {
            let mut chars = text.chars();
            let preview = chars.by_ref().take(MAX_STRING_CHARS).collect::<String>();
            if chars.next().is_some() {
                serde_json::Value::String(format!("{preview}..."))
            } else {
                value.clone()
            }
        }
        _ => value.clone(),
    }
}

fn bounded_json_preview_key(key: &str, max_chars: usize) -> String {
    let mut chars = key.chars();
    let preview = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_protocol::{
        BackboneEnvelope, ItemCompletedNotification, ItemUpdatedNotification, SourceInfo,
        TraceInfo, backbone::thread_item,
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
            ephemeral: false,
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

    fn agent_message_delta_event(
        event_seq: u64,
        turn_id: &str,
        entry_index: u32,
        delta: &str,
    ) -> PersistedSessionEvent {
        persisted_event(
            event_seq,
            "agent_message_delta",
            turn_id,
            entry_index,
            BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                thread_id: "session-1".to_string(),
                turn_id: turn_id.to_string(),
                item_id: format!("{turn_id}:{entry_index}:msg"),
                delta: delta.to_string(),
            }),
            None,
        )
    }

    fn reasoning_delta_event(
        event_seq: u64,
        turn_id: &str,
        entry_index: u32,
        delta: &str,
    ) -> PersistedSessionEvent {
        persisted_event(
            event_seq,
            "reasoning_text_delta",
            turn_id,
            entry_index,
            BackboneEvent::ReasoningTextDelta(codex::ReasoningTextDeltaNotification {
                thread_id: "session-1".to_string(),
                turn_id: turn_id.to_string(),
                item_id: format!("{turn_id}:{entry_index}:reason"),
                delta: delta.to_string(),
                content_index: 0,
            }),
            None,
        )
    }

    fn final_agent_message_event(
        event_seq: u64,
        turn_id: &str,
        entry_index: u32,
        text: &str,
    ) -> PersistedSessionEvent {
        let item: AgentDashThreadItem = codex::ThreadItem::AgentMessage {
            id: format!("{turn_id}:{entry_index}:msg"),
            text: text.to_string(),
            phase: None,
            memory_citation: None,
        }
        .into();
        persisted_event(
            event_seq,
            "item_completed",
            turn_id,
            entry_index,
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item,
                "session-1".to_string(),
                turn_id.to_string(),
            )),
            None,
        )
    }

    fn final_reasoning_event(
        event_seq: u64,
        turn_id: &str,
        entry_index: u32,
        content: &str,
    ) -> PersistedSessionEvent {
        let item: AgentDashThreadItem = codex::ThreadItem::Reasoning {
            id: format!("{turn_id}:{entry_index}:reason"),
            summary: vec![],
            content: vec![content.to_string()],
        }
        .into();
        persisted_event(
            event_seq,
            "item_completed",
            turn_id,
            entry_index,
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item,
                "session-1".to_string(),
                turn_id.to_string(),
            )),
            None,
        )
    }

    fn assistant_text(transcript: &ProjectedTranscript) -> String {
        transcript
            .entries
            .iter()
            .find_map(|entry| match &entry.message {
                AgentMessage::Assistant { content, .. } => Some(
                    content
                        .iter()
                        .filter_map(ContentPart::extract_text)
                        .collect::<Vec<_>>()
                        .join(""),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn assistant_reasoning(transcript: &ProjectedTranscript) -> String {
        transcript
            .entries
            .iter()
            .find_map(|entry| match &entry.message {
                AgentMessage::Assistant { content, .. } => Some(
                    content
                        .iter()
                        .filter_map(ContentPart::extract_reasoning)
                        .collect::<Vec<_>>()
                        .join(""),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn json_preview_bounds_deep_and_large_payloads() {
        let mut deep = serde_json::json!("leaf");
        for _ in 0..64 {
            deep = serde_json::json!({ "next": deep });
        }
        let preview = json_preview(&serde_json::json!({
            "deep": deep,
            "wide": (0..64).map(|index| serde_json::json!({ "index": index })).collect::<Vec<_>>(),
            "long": "x".repeat(1024),
        }));
        let bounded = bounded_json_preview_value(
            &serde_json::json!(
                (0..64)
                    .map(|index| serde_json::json!({ "index": index }))
                    .collect::<Vec<_>>()
            ),
            0,
        );
        let bounded_rendered = bounded.to_string();

        assert!(preview.len() <= 323);
        assert!(preview.contains("json_payload_too_deep"));
        assert!(bounded_rendered.contains("json_array_truncated"));
    }

    #[test]
    fn json_preview_bounds_object_keys() {
        let long_key = format!("{}-tail", "k".repeat(1024));
        let preview = json_preview(&serde_json::json!({
            long_key: "value",
        }));

        assert!(preview.len() <= 323);
        assert!(preview.contains("..."));
        assert!(!preview.contains("tail"));
    }

    #[test]
    fn final_agent_message_overrides_delta_accumulation() {
        let turn_id = "turn-final";
        let events = vec![
            agent_message_delta_event(1, turn_id, 0, "part"),
            final_agent_message_event(2, turn_id, 0, "FULL FINAL"),
        ];

        let transcript = build_raw_projected_transcript_from_events(&events);

        // 终态权威覆盖 delta 累积："part" 被忽略，正文为终态。
        assert_eq!(assistant_text(&transcript), "FULL FINAL");
    }

    #[test]
    fn final_agent_message_without_delta_restores_assistant_text() {
        let turn_id = "turn-final-only";
        let events = vec![final_agent_message_event(1, turn_id, 0, "ONLY FINAL")];

        let transcript = build_raw_projected_transcript_from_events(&events);

        assert_eq!(assistant_text(&transcript), "ONLY FINAL");
    }

    #[test]
    fn final_reasoning_overrides_delta_accumulation() {
        let turn_id = "turn-reason";
        let events = vec![
            reasoning_delta_event(1, turn_id, 0, "rpart"),
            final_reasoning_event(2, turn_id, 0, "FULL REASONING"),
        ];

        let transcript = build_raw_projected_transcript_from_events(&events);

        assert_eq!(assistant_reasoning(&transcript), "FULL REASONING");
    }

    #[test]
    fn final_message_and_reasoning_coexist_in_same_assistant_entry() {
        let turn_id = "turn-mixed";
        let events = vec![
            reasoning_delta_event(1, turn_id, 0, "rpart"),
            agent_message_delta_event(2, turn_id, 0, "tpart"),
            final_reasoning_event(3, turn_id, 0, "FINAL REASONING"),
            final_agent_message_event(4, turn_id, 0, "FINAL TEXT"),
        ];

        let transcript = build_raw_projected_transcript_from_events(&events);

        // 一条 assistant entry，reasoning 在前、text 在后，均取终态。
        let assistant_entries = transcript
            .entries
            .iter()
            .filter(|entry| matches!(entry.message, AgentMessage::Assistant { .. }))
            .count();
        assert_eq!(assistant_entries, 1);
        assert_eq!(assistant_reasoning(&transcript), "FINAL REASONING");
        assert_eq!(assistant_text(&transcript), "FINAL TEXT");
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
