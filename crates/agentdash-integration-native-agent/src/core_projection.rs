use std::collections::{HashMap, HashSet};

use agentdash_agent::{
    AgentMessage, ContentPart, MessageRef, ProjectedEntry, ProjectedTranscript, ProjectionKind,
    StopReason, ToolCallInfo,
};
use agentdash_agent_protocol::{
    AgentDashNativeThreadItem, AgentDashThreadItem, BackboneEvent, ItemStartedNotification,
    ItemUpdatedNotification, UserInputBlock, codex_app_server_protocol as codex,
};
use base64::Engine;
use thiserror::Error;

const RESTORED_TOOL_OUTPUT_MISSING_MARKER: &str = "[restored tool output missing]";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NativeCoreProjectionError {
    #[error("Native Core projection does not support input kind: {kind}")]
    UnsupportedInput { kind: &'static str },
    #[error("Native Core projection requires an inline base64 image")]
    InvalidImage,
}

/// Native-owned durable presentation input. `turn_id` is the presentation turn coordinate.
#[derive(Debug, Clone, Copy)]
pub struct NativeCoreProjectionEvent<'a> {
    pub event_seq: u64,
    pub turn_id: Option<&'a str>,
    pub entry_index: Option<u32>,
    pub event: &'a BackboneEvent,
}

pub fn project_native_core_input(
    input: &[UserInputBlock],
) -> Result<Vec<ContentPart>, NativeCoreProjectionError> {
    input
        .iter()
        .filter_map(|block| match block {
            codex::UserInput::Text { text, .. } => {
                let text = text.trim();
                (!text.is_empty()).then(|| Ok(ContentPart::text(text)))
            }
            codex::UserInput::Image { url, .. } => Some(parse_data_image(url)),
            codex::UserInput::LocalImage { .. } => {
                Some(Err(NativeCoreProjectionError::UnsupportedInput {
                    kind: "local_image",
                }))
            }
            codex::UserInput::Skill { .. } => {
                Some(Err(NativeCoreProjectionError::UnsupportedInput {
                    kind: "skill",
                }))
            }
            codex::UserInput::Mention { .. } => {
                Some(Err(NativeCoreProjectionError::UnsupportedInput {
                    kind: "mention",
                }))
            }
        })
        .collect()
}

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
    final_text: Option<String>,
    final_reasoning: Option<String>,
}

impl RestoredAssistantMessageState {
    fn new(event: &NativeCoreProjectionEvent<'_>) -> Self {
        Self {
            order: event.event_seq,
            turn_id: event.turn_id.map(ToOwned::to_owned),
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

pub fn project_native_core_transcript<'a>(
    events: impl IntoIterator<Item = NativeCoreProjectionEvent<'a>>,
) -> Result<ProjectedTranscript, NativeCoreProjectionError> {
    let mut users = HashMap::<String, RestoredUserMessageState>::new();
    let mut assistants = HashMap::<String, RestoredAssistantMessageState>::new();
    let mut tool_results = HashMap::<String, RestoredToolResultState>::new();

    for event in events {
        match event.event {
            BackboneEvent::UserInputSubmitted(input) => {
                let content = project_native_core_input(&input.content)?;
                let state = users
                    .entry(format!("user:item:{}", input.item_id))
                    .or_insert_with(|| RestoredUserMessageState {
                        order: event.event_seq,
                        turn_id: event
                            .turn_id
                            .map(ToOwned::to_owned)
                            .or_else(|| Some(input.turn_id.clone())),
                        entry_index: event.entry_index,
                        content: Vec::new(),
                    });
                state.content.extend(content);
            }
            BackboneEvent::AgentMessageDelta(delta) if !delta.delta.is_empty() => {
                let state = assistants
                    .entry(assistant_key(&event, Some(&delta.item_id)))
                    .or_insert_with(|| RestoredAssistantMessageState::new(&event));
                state.content.push(ContentPart::text(&delta.delta));
            }
            BackboneEvent::ReasoningTextDelta(delta) if !delta.delta.is_empty() => {
                let state = assistants
                    .entry(assistant_key(&event, Some(&delta.item_id)))
                    .or_insert_with(|| RestoredAssistantMessageState::new(&event));
                state
                    .content
                    .push(ContentPart::reasoning(&delta.delta, None, None));
            }
            BackboneEvent::ReasoningSummaryDelta(delta) if !delta.delta.is_empty() => {
                let state = assistants
                    .entry(assistant_key(&event, Some(&delta.item_id)))
                    .or_insert_with(|| RestoredAssistantMessageState::new(&event));
                state
                    .content
                    .push(ContentPart::reasoning(&delta.delta, None, None));
            }
            BackboneEvent::ItemStarted(ItemStartedNotification { item, .. })
            | BackboneEvent::ItemUpdated(ItemUpdatedNotification { item, .. }) => {
                observe_tool_item(&event, item, &mut assistants, &mut tool_results)?;
            }
            BackboneEvent::ItemCompleted(notification) => {
                if extract_tool_call(&notification.item)?.is_some() {
                    observe_tool_item(
                        &event,
                        &notification.item,
                        &mut assistants,
                        &mut tool_results,
                    )?;
                } else if let Some(final_message) = extract_final_assistant(&notification.item) {
                    let state = assistants
                        .entry(assistant_key(&event, Some(&final_message.item_id)))
                        .or_insert_with(|| RestoredAssistantMessageState::new(&event));
                    state.order = state.order.min(event.event_seq);
                    match final_message.kind {
                        FinalAssistantKind::Text(text) => state.final_text = Some(text),
                        FinalAssistantKind::Reasoning(text) => state.final_reasoning = Some(text),
                    }
                }
            }
            _ => {}
        }
    }

    ensure_tool_results(&assistants, &mut tool_results);
    let mut envelopes = Vec::new();
    for state in users.into_values() {
        if !state.content.is_empty() {
            envelopes.push(RestoredMessageEnvelope::User {
                order: state.order,
                message_ref: message_ref(state.turn_id.as_deref(), state.entry_index, state.order),
                content: state.content,
            });
        }
    }
    for state in assistants.into_values() {
        let content = assistant_content(&state);
        if !content.is_empty() || !state.tool_calls.is_empty() {
            envelopes.push(RestoredMessageEnvelope::Assistant {
                order: state.order,
                message_ref: message_ref(state.turn_id.as_deref(), state.entry_index, state.order),
                content,
                tool_calls: state.tool_calls,
            });
        }
    }
    for state in tool_results.into_values() {
        if state.terminal && (!state.content.is_empty() || state.details.is_some()) {
            envelopes.push(RestoredMessageEnvelope::ToolResult {
                order: state.order,
                message_ref: message_ref(state.turn_id.as_deref(), state.entry_index, state.order),
                tool_call_id: state.tool_call_id,
                call_id: state.call_id,
                tool_name: state.tool_name,
                content: state.content,
                details: state.details,
                is_error: state.is_error,
            });
        }
    }
    envelopes.sort_by_key(envelope_order);
    Ok(ProjectedTranscript {
        entries: envelopes.into_iter().map(project_envelope).collect(),
    })
}

fn observe_tool_item(
    event: &NativeCoreProjectionEvent<'_>,
    item: &AgentDashThreadItem,
    assistants: &mut HashMap<String, RestoredAssistantMessageState>,
    tool_results: &mut HashMap<String, RestoredToolResultState>,
) -> Result<(), NativeCoreProjectionError> {
    let Some(tool_call) = extract_tool_call(item)? else {
        return Ok(());
    };
    let assistant_key = assistants
        .iter()
        .find_map(|(key, state)| {
            state
                .tool_call_ids
                .contains(&tool_call.id)
                .then(|| key.clone())
        })
        .unwrap_or_else(|| assistant_key(event, None));
    let state = assistants
        .entry(assistant_key)
        .or_insert_with(|| RestoredAssistantMessageState::new(event));
    state.order = state.order.min(event.event_seq);
    upsert_tool_call(state, &tool_call);
    if tool_call.is_terminal {
        update_tool_result(tool_results, &tool_call, event);
    }
    Ok(())
}

fn assistant_key(event: &NativeCoreProjectionEvent<'_>, message_id: Option<&str>) -> String {
    if let (Some(turn_id), Some(entry_index)) = (event.turn_id, event.entry_index) {
        return format!("assistant:turn:{turn_id}:{entry_index}");
    }
    if let Some(message_id) = message_id.filter(|value| !value.trim().is_empty()) {
        return format!("assistant:msg:{message_id}");
    }
    format!("assistant:event:{}", event.event_seq)
}

fn upsert_tool_call(state: &mut RestoredAssistantMessageState, tool_call: &ExtractedToolCall) {
    if let Some(existing) = state
        .tool_calls
        .iter_mut()
        .find(|existing| existing.id == tool_call.id)
    {
        if existing.name.trim().is_empty() {
            existing.name.clone_from(&tool_call.name);
        }
        if existing.arguments.is_null() || existing.arguments == serde_json::json!({}) {
            existing.arguments = tool_call
                .raw_input
                .clone()
                .unwrap_or_else(|| serde_json::json!({}));
        }
        return;
    }
    if state.tool_call_ids.insert(tool_call.id.clone()) {
        state.tool_calls.push(ToolCallInfo {
            id: tool_call.id.clone(),
            call_id: Some(tool_call.id.clone()),
            name: tool_call.name.clone(),
            arguments: tool_call
                .raw_input
                .clone()
                .unwrap_or_else(|| serde_json::json!({})),
        });
    }
}

fn update_tool_result(
    results: &mut HashMap<String, RestoredToolResultState>,
    tool_call: &ExtractedToolCall,
    event: &NativeCoreProjectionEvent<'_>,
) {
    let state = results
        .entry(tool_call.id.clone())
        .or_insert_with(|| RestoredToolResultState {
            tool_call_id: tool_call.id.clone(),
            call_id: Some(tool_call.id.clone()),
            ..Default::default()
        });
    state.order = event.event_seq;
    state.turn_id = event.turn_id.map(ToOwned::to_owned);
    state.entry_index = event.entry_index;
    state.tool_name = Some(tool_call.name.clone());
    state.terminal = true;
    state.is_error = tool_call.is_error;
    state.content.clone_from(&tool_call.content_parts);
    if let Some(output) = &tool_call.raw_output {
        state.details = Some(output.clone());
        if state.content.is_empty() {
            state.content = vec![ContentPart::text(json_preview(output))];
        }
    }
    if state.content.is_empty() && state.details.is_none() {
        apply_missing_output(state);
    }
}

fn ensure_tool_results(
    assistants: &HashMap<String, RestoredAssistantMessageState>,
    results: &mut HashMap<String, RestoredToolResultState>,
) {
    for assistant in assistants.values() {
        for tool_call in &assistant.tool_calls {
            results.entry(tool_call.id.clone()).or_insert_with(|| {
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
                    ..Default::default()
                };
                apply_missing_output(&mut state);
                state
            });
        }
    }
}

fn apply_missing_output(state: &mut RestoredToolResultState) {
    let tool_name = state
        .tool_name
        .as_deref()
        .unwrap_or("tool_call")
        .to_string();
    state.content = vec![ContentPart::text(format!(
        "{RESTORED_TOOL_OUTPUT_MISSING_MARKER}\ntool_call_id: {}\ntool_name: {tool_name}\nThe original tool output was not available in durable presentation events. A restored placeholder preserves the tool-call/tool-result pair so continuation can proceed.",
        state.tool_call_id
    ))];
    state.details = Some(serde_json::json!({
        "type": "restored_tool_output_missing",
        "tool_call_id": state.tool_call_id,
        "tool_name": tool_name,
    }));
    state.is_error = true;
    state.terminal = true;
}

fn assistant_content(state: &RestoredAssistantMessageState) -> Vec<ContentPart> {
    let mut content = Vec::new();
    if let Some(reasoning) = state
        .final_reasoning
        .as_deref()
        .filter(|text| !text.is_empty())
    {
        content.push(ContentPart::reasoning(reasoning, None, None));
    } else {
        content.extend(
            state
                .content
                .iter()
                .filter(|part| part.extract_reasoning().is_some())
                .cloned(),
        );
    }
    if let Some(text) = state.final_text.as_deref().filter(|text| !text.is_empty()) {
        content.push(ContentPart::text(text));
    } else {
        content.extend(
            state
                .content
                .iter()
                .filter(|part| part.extract_text().is_some())
                .cloned(),
        );
    }
    content
}

fn envelope_order(envelope: &RestoredMessageEnvelope) -> u64 {
    match envelope {
        RestoredMessageEnvelope::User { order, .. }
        | RestoredMessageEnvelope::Assistant { order, .. }
        | RestoredMessageEnvelope::ToolResult { order, .. } => *order,
    }
}

fn project_envelope(envelope: RestoredMessageEnvelope) -> ProjectedEntry {
    match envelope {
        RestoredMessageEnvelope::User {
            order,
            message_ref,
            content,
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
            order,
            message_ref,
            content,
            tool_calls,
        } => ProjectedEntry::event(
            message_ref,
            ProjectionKind::Transcript,
            AgentMessage::Assistant {
                content,
                stop_reason: Some(if tool_calls.is_empty() {
                    StopReason::Stop
                } else {
                    StopReason::ToolUse
                }),
                tool_calls,
                error_message: None,
                usage: None,
                timestamp: None,
            },
            Some(order),
        ),
        RestoredMessageEnvelope::ToolResult {
            order,
            message_ref,
            tool_call_id,
            call_id,
            tool_name,
            content,
            details,
            is_error,
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

fn message_ref(turn_id: Option<&str>, entry_index: Option<u32>, event_seq: u64) -> MessageRef {
    MessageRef {
        turn_id: turn_id
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("_seq:{event_seq}")),
        entry_index: entry_index.unwrap_or(0),
    }
}

struct FinalAssistantMessage {
    item_id: String,
    kind: FinalAssistantKind,
}

enum FinalAssistantKind {
    Text(String),
    Reasoning(String),
}

fn extract_final_assistant(item: &AgentDashThreadItem) -> Option<FinalAssistantMessage> {
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

struct ExtractedToolCall {
    id: String,
    name: String,
    raw_input: Option<serde_json::Value>,
    raw_output: Option<serde_json::Value>,
    content_parts: Vec<ContentPart>,
    is_terminal: bool,
    is_error: bool,
}

fn extract_tool_call(
    item: &AgentDashThreadItem,
) -> Result<Option<ExtractedToolCall>, NativeCoreProjectionError> {
    match item {
        AgentDashThreadItem::Codex(item) => extract_codex_tool_call(item),
        AgentDashThreadItem::AgentDash(item) => extract_native_tool_call(item),
    }
}

fn extract_codex_tool_call(
    item: &codex::ThreadItem,
) -> Result<Option<ExtractedToolCall>, NativeCoreProjectionError> {
    let call = match item {
        codex::ThreadItem::DynamicToolCall {
            id,
            tool,
            arguments,
            status,
            content_items,
            success,
            ..
        } => {
            let success = success.as_ref().and_then(|value| *value);
            ExtractedToolCall {
                id: id.clone(),
                name: tool.clone(),
                raw_input: Some(arguments.clone()),
                raw_output: None,
                content_parts: content_items
                    .as_ref()
                    .and_then(Option::as_ref)
                    .map(|items| content_items_to_parts(items))
                    .transpose()?
                    .unwrap_or_default(),
                is_terminal: matches!(
                    status,
                    codex::DynamicToolCallStatus::Completed | codex::DynamicToolCallStatus::Failed
                ),
                is_error: success == Some(false)
                    || matches!(status, codex::DynamicToolCallStatus::Failed),
            }
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
            let result = result.as_ref().and_then(Option::as_ref);
            let error = error.as_ref().and_then(Option::as_ref);
            ExtractedToolCall {
                id: id.clone(),
                name: tool.clone(),
                raw_input: Some(arguments.clone()),
                raw_output: result
                    .and_then(|value| serde_json::to_value(value).ok())
                    .or_else(|| error.and_then(|value| serde_json::to_value(value).ok())),
                content_parts: Vec::new(),
                is_terminal: matches!(
                    status,
                    codex::McpToolCallStatus::Completed | codex::McpToolCallStatus::Failed
                ),
                is_error: error.is_some() || matches!(status, codex::McpToolCallStatus::Failed),
            }
        }
        codex::ThreadItem::CommandExecution {
            id,
            command,
            status,
            exit_code,
            aggregated_output,
            ..
        } => {
            let exit_code = exit_code.as_ref().and_then(|value| *value);
            ExtractedToolCall {
                id: id.clone(),
                name: "command_execution".to_string(),
                raw_input: Some(serde_json::json!({ "command": command })),
                raw_output: None,
                content_parts: aggregated_output
                    .as_ref()
                    .and_then(Option::as_ref)
                    .map(|output| vec![ContentPart::text(output)])
                    .unwrap_or_default(),
                is_terminal: matches!(
                    status,
                    codex::CommandExecutionStatus::Completed
                        | codex::CommandExecutionStatus::Failed
                        | codex::CommandExecutionStatus::Declined
                ),
                is_error: exit_code.is_some_and(|code| code != 0)
                    || matches!(status, codex::CommandExecutionStatus::Failed),
            }
        }
        codex::ThreadItem::FileChange { id, status, .. } => ExtractedToolCall {
            id: id.clone(),
            name: "file_change".to_string(),
            raw_input: None,
            raw_output: None,
            content_parts: Vec::new(),
            is_terminal: matches!(
                status,
                codex::PatchApplyStatus::Completed
                    | codex::PatchApplyStatus::Failed
                    | codex::PatchApplyStatus::Declined
            ),
            is_error: matches!(status, codex::PatchApplyStatus::Failed),
        },
        _ => return Ok(None),
    };
    Ok(Some(call))
}

fn extract_native_tool_call(
    item: &AgentDashNativeThreadItem,
) -> Result<Option<ExtractedToolCall>, NativeCoreProjectionError> {
    Ok(Some(ExtractedToolCall {
        id: item.id().to_string(),
        name: item.tool_name().to_string(),
        raw_input: item.arguments().cloned(),
        raw_output: None,
        content_parts: item
            .content_items()
            .map(|items| content_items_to_parts(items))
            .transpose()?
            .unwrap_or_else(|| {
                item.shell_output()
                    .map(|output| vec![ContentPart::text(output)])
                    .unwrap_or_default()
            }),
        is_terminal: matches!(
            item.status(),
            codex::DynamicToolCallStatus::Completed | codex::DynamicToolCallStatus::Failed
        ),
        is_error: item.success() == Some(false)
            || matches!(item.status(), codex::DynamicToolCallStatus::Failed),
    }))
}

fn content_items_to_parts(
    items: &[codex::DynamicToolCallOutputContentItem],
) -> Result<Vec<ContentPart>, NativeCoreProjectionError> {
    items
        .iter()
        .filter_map(|item| match item {
            codex::DynamicToolCallOutputContentItem::InputText { text } => {
                (!text.trim().is_empty()).then(|| Ok(ContentPart::text(text)))
            }
            codex::DynamicToolCallOutputContentItem::InputImage { image_url } => {
                Some(parse_data_image(image_url))
            }
        })
        .collect()
}

fn parse_data_image(image_url: &str) -> Result<ContentPart, NativeCoreProjectionError> {
    let rest = image_url
        .trim()
        .strip_prefix("data:")
        .ok_or(NativeCoreProjectionError::InvalidImage)?;
    let (mime_type, data) = rest
        .split_once(";base64,")
        .ok_or(NativeCoreProjectionError::InvalidImage)?;
    let mime_type = mime_type.trim();
    let data = data.trim();
    if !mime_type.starts_with("image/") || data.is_empty() {
        return Err(NativeCoreProjectionError::InvalidImage);
    }
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| NativeCoreProjectionError::InvalidImage)?;
    Ok(ContentPart::Image {
        mime_type: mime_type.to_string(),
        data: data.to_string(),
    })
}

fn json_preview(value: &serde_json::Value) -> String {
    const MAX_CHARS: usize = 320;
    let rendered = value.to_string();
    let mut chars = rendered.chars();
    let preview = chars.by_ref().take(MAX_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}
