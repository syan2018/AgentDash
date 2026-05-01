use std::collections::{HashMap, HashSet};

use agent_client_protocol::{ContentBlock, SessionUpdate, ToolCallContent, ToolCallStatus};
use agentdash_acp_meta::parse_agentdash_meta;
use agentdash_agent_types::{
    AgentMessage, ContentPart, MessageRef, ProjectedEntry, ProjectedTranscript, ProjectionKind,
    StopReason, ToolCallInfo,
};
use agentdash_spi::content_block_to_text;

use super::persistence::PersistedSessionEvent;

// ─── Continuation transcript 构建 ─────────────────────────────

#[derive(Debug, Clone)]
struct CompactionCheckpoint {
    summary: String,
    tokens_before: u64,
    messages_compacted: u32,
    compacted_until_ref: MessageRef,
    timestamp_ms: Option<u64>,
}

pub(super) fn build_continuation_system_context_from_events(
    owner_context: Option<&str>,
    events: &[PersistedSessionEvent],
) -> Option<String> {
    let transcript = build_projected_transcript_from_events(events);
    if events.is_empty() && transcript.is_empty() {
        return owner_context
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
    }

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

    if !transcript.is_empty() {
        history_lines.push(String::new());
        history_lines.push("### Transcript".to_string());
        for entry in &transcript.entries {
            match &entry.message {
                AgentMessage::User { content, .. } => {
                    let text = content
                        .iter()
                        .filter_map(ContentPart::extract_text)
                        .collect::<Vec<_>>()
                        .join("");
                    history_lines.push("#### 用户".to_string());
                    history_lines.push(text);
                }
                AgentMessage::Assistant {
                    content,
                    tool_calls,
                    ..
                } => {
                    let mut text = content
                        .iter()
                        .filter_map(ContentPart::extract_text)
                        .collect::<Vec<_>>()
                        .join("");
                    if !tool_calls.is_empty() {
                        let tool_lines = tool_calls
                            .iter()
                            .map(|tool_call| {
                                format!(
                                    "- {}({})",
                                    tool_call.name,
                                    json_preview(&tool_call.arguments)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        if !text.is_empty() {
                            text.push_str("\n\n");
                        }
                        text.push_str("工具调用：\n");
                        text.push_str(&tool_lines);
                    }
                    history_lines.push("#### 助手".to_string());
                    history_lines.push(text);
                }
                AgentMessage::ToolResult {
                    tool_name,
                    content,
                    details,
                    is_error,
                    ..
                } => {
                    let mut text = content
                        .iter()
                        .filter_map(ContentPart::extract_text)
                        .collect::<Vec<_>>()
                        .join("");
                    if text.is_empty()
                        && let Some(details) = details.as_ref()
                    {
                        text = json_preview(details);
                    }
                    history_lines.push(format!(
                        "#### 工具结果 ({})",
                        tool_name.as_deref().unwrap_or("tool_result")
                    ));
                    if *is_error {
                        history_lines.push(format!("[error]\n{text}"));
                    } else {
                        history_lines.push(text);
                    }
                }
                AgentMessage::CompactionSummary { summary, .. } => {
                    history_lines.push("#### 历史摘要".to_string());
                    history_lines.push(summary.clone());
                }
            }
            history_lines.push(String::new());
        }
    }

    sections.push(format!(
        "## Session Continuation\n\n{}",
        history_lines.join("\n")
    ));
    Some(sections.join("\n\n"))
}

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

/// 公共入口 — 返回 Vec<AgentMessage>，保持 hub.rs 等调用方不变。
pub(super) fn build_restored_session_messages_from_events(
    events: &[PersistedSessionEvent],
) -> Vec<AgentMessage> {
    build_projected_transcript_from_events(events).into_messages()
}

/// 内部入口 — 返回带身份的 ProjectedTranscript。
pub(super) fn build_projected_transcript_from_events(
    events: &[PersistedSessionEvent],
) -> ProjectedTranscript {
    let mut user_messages: HashMap<String, RestoredUserMessageState> = HashMap::new();
    let mut assistant_messages: HashMap<String, RestoredAssistantMessageState> = HashMap::new();
    let mut tool_results: HashMap<String, RestoredToolResultState> = HashMap::new();

    for event in events {
        match &event.notification.update {
            SessionUpdate::UserMessageChunk(chunk) => {
                if let Some(part) = content_block_to_message_part(&chunk.content) {
                    let key = restored_user_key(event);
                    let state =
                        user_messages
                            .entry(key)
                            .or_insert_with(|| RestoredUserMessageState {
                                order: event.event_seq,
                                turn_id: event.turn_id.clone(),
                                entry_index: event.entry_index,
                                content: Vec::new(),
                            });
                    state.content.push(part);
                }
            }
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let Some(part) = content_block_to_message_part(&chunk.content) {
                    let key = restored_assistant_key(event, chunk.message_id.as_deref());
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
                    state.content.push(part);
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                if let Some(part) = content_block_to_reasoning_part(&chunk.content) {
                    let key = restored_assistant_key(event, chunk.message_id.as_deref());
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
                    state.content.push(part);
                }
            }
            SessionUpdate::ToolCall(call) => {
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
                upsert_restored_tool_call(
                    state,
                    call.tool_call_id.0.as_ref(),
                    Some(call.title.as_str()),
                    call.raw_input.as_ref(),
                );
                update_restored_tool_result(
                    &mut tool_results,
                    call.tool_call_id.0.as_ref(),
                    event,
                    Some(call.title.as_str()),
                    call.raw_output.as_ref(),
                    Some(&call.content),
                    Some(call.status),
                );
            }
            SessionUpdate::ToolCallUpdate(update) => {
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
                upsert_restored_tool_call(
                    state,
                    update.tool_call_id.0.as_ref(),
                    update.fields.title.as_deref(),
                    update.fields.raw_input.as_ref(),
                );
                update_restored_tool_result(
                    &mut tool_results,
                    update.tool_call_id.0.as_ref(),
                    event,
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
    let raw_entries: Vec<ProjectedEntry> = envelopes
        .into_iter()
        .map(restored_envelope_to_projected_entry)
        .collect();
    apply_compaction_checkpoint_projected(raw_entries, latest_compaction_checkpoint(events))
}

// ─── Private helpers ────────────────────────────────────────

fn restored_user_key(event: &PersistedSessionEvent) -> String {
    event
        .turn_id
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
    content: Option<&[ToolCallContent]>,
    status: Option<ToolCallStatus>,
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

    let Some(status) = status else {
        return;
    };
    if !matches!(status, ToolCallStatus::Completed | ToolCallStatus::Failed) {
        return;
    }

    state.order = order;
    state.turn_id = event.turn_id.clone();
    state.entry_index = event.entry_index;
    state.terminal = true;
    state.is_error = matches!(status, ToolCallStatus::Failed);

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

/// 从 RestoredMessageEnvelope 构建 ProjectedEntry（带 MessageRef + ProjectionKind）。
fn restored_envelope_to_projected_entry(envelope: RestoredMessageEnvelope) -> ProjectedEntry {
    match envelope {
        RestoredMessageEnvelope::User {
            message_ref,
            content,
            ..
        } => ProjectedEntry {
            message_ref,
            projection_kind: ProjectionKind::Transcript,
            message: AgentMessage::User {
                content,
                timestamp: None,
            },
        },
        RestoredMessageEnvelope::Assistant {
            message_ref,
            content,
            tool_calls,
            ..
        } => ProjectedEntry {
            message_ref,
            projection_kind: ProjectionKind::Transcript,
            message: AgentMessage::Assistant {
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
        },
        RestoredMessageEnvelope::ToolResult {
            message_ref,
            tool_call_id,
            call_id,
            tool_name,
            content,
            details,
            is_error,
            ..
        } => ProjectedEntry {
            message_ref,
            projection_kind: ProjectionKind::Transcript,
            message: AgentMessage::ToolResult {
                tool_call_id,
                call_id,
                tool_name,
                content,
                details,
                is_error,
                timestamp: None,
            },
        },
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

fn latest_compaction_checkpoint(events: &[PersistedSessionEvent]) -> Option<CompactionCheckpoint> {
    events.iter().rev().find_map(extract_compaction_checkpoint)
}

fn extract_compaction_checkpoint(event: &PersistedSessionEvent) -> Option<CompactionCheckpoint> {
    let SessionUpdate::SessionInfoUpdate(info) = &event.notification.update else {
        return None;
    };
    let parsed = parse_agentdash_meta(info.meta.as_ref()?)?;
    let agent_event = parsed.event?;
    if agent_event.r#type != "context_compacted" {
        return None;
    }
    let data = agent_event.data?;
    let compacted_until_ref =
        serde_json::from_value::<MessageRef>(data.get("compacted_until_ref")?.clone()).ok()?;
    Some(CompactionCheckpoint {
        summary: data.get("summary")?.as_str()?.to_string(),
        tokens_before: data
            .get("tokens_before")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default(),
        messages_compacted: data
            .get("messages_compacted")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default(),
        compacted_until_ref,
        timestamp_ms: data.get("timestamp_ms").and_then(serde_json::Value::as_u64),
    })
}

/// 在 ProjectedEntry 列表上应用 compaction checkpoint。
fn apply_compaction_checkpoint_projected(
    raw_entries: Vec<ProjectedEntry>,
    checkpoint: Option<CompactionCheckpoint>,
) -> ProjectedTranscript {
    let Some(checkpoint) = checkpoint else {
        return ProjectedTranscript {
            entries: raw_entries,
        };
    };
    if checkpoint.summary.trim().is_empty() || checkpoint.messages_compacted == 0 {
        return ProjectedTranscript {
            entries: raw_entries,
        };
    }

    let cut = raw_entries
        .iter()
        .position(|e| e.message_ref == checkpoint.compacted_until_ref)
        .map(|pos| pos + 1)
        .unwrap_or(0);

    // 从 cut boundary 回推 compacted_until_ref（保留已 persisted 的 ref 做审计）
    let derived_ref = if cut > 0 {
        Some(raw_entries[cut - 1].message_ref.clone())
    } else {
        Some(checkpoint.compacted_until_ref.clone())
    };

    let summary_ref = MessageRef {
        turn_id: "_compaction_summary".to_string(),
        entry_index: 0,
    };
    let summary_entry = ProjectedEntry {
        message_ref: summary_ref,
        projection_kind: ProjectionKind::CompactionSummary,
        message: AgentMessage::CompactionSummary {
            summary: checkpoint.summary,
            tokens_before: checkpoint.tokens_before,
            messages_compacted: checkpoint.messages_compacted,
            compacted_until_ref: derived_ref,
            timestamp: checkpoint.timestamp_ms,
        },
    };

    let mut entries = vec![summary_entry];
    entries.extend(raw_entries.into_iter().skip(cut));
    ProjectedTranscript { entries }
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
            agent_client_protocol::EmbeddedResourceResource::TextResourceContents(
                text_resource,
            ) => {
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
