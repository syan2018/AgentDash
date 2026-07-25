use std::collections::BTreeMap;

use agentdash_agent_protocol::{
    AgentDashThreadItem, BackboneEvent, CanonicalConversationRecord, ContextFrame,
    ContextFrameSource, PlatformEvent, PresentationDurability, codex_app_server_protocol as codex,
    codex_user_input_to_text,
};
use agentdash_agent_runtime_contract::ManagedRuntimeSnapshot;
use agentdash_contracts::session::{
    SessionAttachmentContextContributionResponse, SessionContextUsageAnalysisResponse,
    SessionContextUsageCategoryResponse, SessionContextUsageItemResponse,
    SessionMessageContextBreakdownResponse, SessionProjectionMessageRefResponse,
    SessionProjectionSegmentProvenanceResponse, SessionProjectionSegmentViewResponse,
    SessionProjectionViewResponse, SessionToolContextContributionResponse,
};
use serde_json::Value;

const PROJECTION_KIND: &str = "managed_runtime_canonical_context";

#[derive(Default)]
struct ToolContribution {
    call_tokens: u64,
    result_tokens: u64,
}

struct SegmentContent {
    segment_type: &'static str,
    role: &'static str,
    preview: String,
    token_estimate: u64,
    attachment_tokens: u64,
    attachment_names: Vec<String>,
    tool_name: Option<String>,
    tool_call_tokens: u64,
    tool_result_tokens: u64,
}

pub fn project_managed_runtime_context(
    snapshot: &ManagedRuntimeSnapshot,
) -> SessionProjectionViewResponse {
    project_records(
        snapshot.thread_id.as_str(),
        snapshot.revision.0,
        &snapshot.conversation_history,
    )
}

fn project_records(
    session_id: &str,
    projection_version: u64,
    records: &[CanonicalConversationRecord],
) -> SessionProjectionViewResponse {
    let active_compaction = records
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, record)| {
            let BackboneEvent::ItemCompleted(notification) = &record.presentation.envelope.event
            else {
                return None;
            };
            notification
                .item
                .is_context_compaction()
                .then(|| (index, notification.item.id().to_owned()))
        });
    let message_boundary = active_compaction.as_ref().map_or(0, |(index, _)| index + 1);

    let mut segments = Vec::new();
    let mut messages = SessionMessageContextBreakdownResponse {
        user_message_tokens: 0,
        assistant_message_tokens: 0,
        tool_call_tokens: 0,
        tool_result_tokens: 0,
        attachment_tokens: 0,
    };
    let mut tools = BTreeMap::<String, ToolContribution>::new();
    let mut attachments = BTreeMap::<String, u64>::new();
    let mut latest_context_frames = BTreeMap::<String, (usize, &ContextFrame)>::new();

    for (record_index, record) in records.iter().enumerate() {
        if record.presentation.durability != PresentationDurability::Durable {
            continue;
        }
        if let BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(changed)) =
            &record.presentation.envelope.event
        {
            latest_context_frames.insert(changed.frame.id.clone(), (record_index, &changed.frame));
        }
        if record_index < message_boundary {
            continue;
        }
        let Some(content) = segment_content(&record.presentation.envelope.event) else {
            continue;
        };
        match content.role {
            "user" => messages.user_message_tokens += content.token_estimate,
            "assistant" | "reasoning" => {
                messages.assistant_message_tokens += content.token_estimate;
            }
            "tool" => {
                messages.tool_call_tokens += content.tool_call_tokens;
                messages.tool_result_tokens += content.tool_result_tokens;
            }
            _ => {}
        }
        messages.attachment_tokens += content.attachment_tokens;
        for name in &content.attachment_names {
            *attachments.entry(name.clone()).or_default() += estimate_tokens(name);
        }
        if let Some(name) = &content.tool_name {
            let contribution = tools.entry(name.clone()).or_default();
            contribution.call_tokens += content.tool_call_tokens;
            contribution.result_tokens += content.tool_result_tokens;
        }

        let envelope = &record.presentation.envelope;
        let sort_order = u32::try_from(segments.len()).unwrap_or(u32::MAX);
        let turn_id = event_turn_id(&envelope.event)
            .or(envelope.trace.turn_id.as_deref())
            .unwrap_or(session_id);
        segments.push(SessionProjectionSegmentViewResponse {
            id: record.presentation_id.clone(),
            sort_order,
            segment_type: content.segment_type.to_owned(),
            role: content.role.to_owned(),
            origin: "projection".to_owned(),
            synthetic: false,
            projection_kind: PROJECTION_KIND.to_owned(),
            message_ref: SessionProjectionMessageRefResponse {
                turn_id: turn_id.to_owned(),
                entry_index: envelope.trace.entry_index.unwrap_or(sort_order),
            },
            source_event_seq: None,
            source_range: None,
            projection_segment_id: Some(record.presentation_id.clone()),
            preview: preview(&content.preview),
            token_estimate: Some(content.token_estimate),
            attachment_tokens: content.attachment_tokens,
            attachment_names: content.attachment_names,
            tool_names: content.tool_name.into_iter().collect(),
            provenance: SessionProjectionSegmentProvenanceResponse {
                compaction_id: active_compaction.as_ref().map(|(_, id)| id.clone()),
                projection_version: Some(projection_version),
                segment_type: Some(content.segment_type.to_owned()),
                strategy: None,
                trigger: None,
                phase: None,
            },
        });
    }

    let mut context_frames = latest_context_frames.into_values().collect::<Vec<_>>();
    context_frames.sort_by_key(|(record_index, _)| *record_index);
    let mut categories = BTreeMap::<String, SessionContextUsageCategoryResponse>::new();
    let mut items = Vec::new();
    let mut context_tokens = 0;
    for (record_index, frame) in context_frames {
        let tokens = estimate_tokens(&frame.rendered_text);
        context_tokens += tokens;
        let kind = frame.kind.as_key().to_owned();
        let label =
            non_empty(&frame.delivery_metadata.frontend_label).unwrap_or_else(|| kind.clone());
        let source = context_frame_source(frame.source).to_owned();
        let category =
            categories
                .entry(kind.clone())
                .or_insert_with(|| SessionContextUsageCategoryResponse {
                    kind: kind.clone(),
                    label: label.clone(),
                    token_estimate: 0,
                    source: source.clone(),
                    deferred: false,
                });
        category.token_estimate += tokens;
        items.push(SessionContextUsageItemResponse {
            kind,
            label,
            name: frame.id.clone(),
            token_estimate: tokens,
            source,
            deferred: false,
            source_event_seq: None,
            turn_id: records[record_index]
                .presentation
                .envelope
                .trace
                .turn_id
                .clone(),
        });
    }

    let mut top_tools = tools
        .into_iter()
        .map(
            |(name, contribution)| SessionToolContextContributionResponse {
                name,
                call_tokens: contribution.call_tokens,
                result_tokens: contribution.result_tokens,
            },
        )
        .collect::<Vec<_>>();
    top_tools.sort_by_key(|tool| std::cmp::Reverse(tool.call_tokens + tool.result_tokens));
    let mut top_attachments = attachments
        .into_iter()
        .map(|(name, tokens)| SessionAttachmentContextContributionResponse { name, tokens })
        .collect::<Vec<_>>();
    top_attachments.sort_by_key(|attachment| std::cmp::Reverse(attachment.tokens));

    let segment_tokens = segments
        .iter()
        .filter_map(|segment| segment.token_estimate)
        .sum::<u64>();
    SessionProjectionViewResponse {
        session_id: session_id.to_owned(),
        projection_kind: PROJECTION_KIND.to_owned(),
        projection_version,
        head_event_seq: projection_version,
        active_compaction_id: active_compaction.map(|(_, id)| id),
        token_estimate: Some(segment_tokens + context_tokens),
        message_count: segments.len() as u64,
        segments,
        context_usage: SessionContextUsageAnalysisResponse {
            categories: categories.into_values().collect(),
            items,
            messages,
            top_tools,
            top_attachments,
        },
    }
}

fn segment_content(event: &BackboneEvent) -> Option<SegmentContent> {
    match event {
        BackboneEvent::UserInputSubmitted(notification) => {
            let preview = codex_user_input_to_text(&notification.content).ok()?;
            let attachment_names = notification
                .content
                .iter()
                .filter_map(|input| match input {
                    codex::UserInput::Image { url, .. } => Some(url.clone()),
                    codex::UserInput::LocalImage { path, .. } => Some(path.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let attachment_tokens = attachment_names
                .iter()
                .map(|name| estimate_tokens(name))
                .sum();
            Some(SegmentContent {
                segment_type: "user_message",
                role: "user",
                token_estimate: estimate_tokens(&preview),
                preview,
                attachment_tokens,
                attachment_names,
                tool_name: None,
                tool_call_tokens: 0,
                tool_result_tokens: 0,
            })
        }
        BackboneEvent::ItemCompleted(notification) => item_segment_content(&notification.item),
        _ => None,
    }
}

fn item_segment_content(item: &AgentDashThreadItem) -> Option<SegmentContent> {
    if let Some(item) = item.as_codex() {
        match item {
            codex::ThreadItem::UserMessage { content, .. } => {
                let preview = codex_user_input_to_text(content).ok()?;
                return Some(message_content("user_message", "user", preview));
            }
            codex::ThreadItem::AgentMessage { text, .. } => {
                return Some(message_content(
                    "assistant_message",
                    "assistant",
                    text.clone(),
                ));
            }
            codex::ThreadItem::Reasoning {
                summary, content, ..
            } => {
                let text = summary
                    .iter()
                    .chain(content)
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                return Some(message_content("reasoning", "reasoning", text));
            }
            codex::ThreadItem::Plan { text, .. } => {
                return Some(message_content("plan", "assistant", text.clone()));
            }
            codex::ThreadItem::ContextCompaction { .. } => return None,
            _ => {}
        }
    }
    if !item.is_tool_activity() {
        return None;
    }

    let value = serde_json::to_value(item).ok()?;
    let name = tool_name(item, &value);
    let call_text = selected_json(&value, &["arguments", "command", "query", "prompt", "path"]);
    let result_text = selected_json(
        &value,
        &[
            "aggregatedOutput",
            "result",
            "error",
            "contentItems",
            "changes",
            "success",
            "exitCode",
        ],
    );
    let preview_text = if result_text.is_empty() {
        serde_json::to_string(&value).unwrap_or_default()
    } else {
        result_text.clone()
    };
    let call_tokens = estimate_tokens(&call_text);
    let result_tokens = estimate_tokens(&result_text);
    Some(SegmentContent {
        segment_type: "tool_activity",
        role: "tool",
        preview: preview_text,
        token_estimate: call_tokens + result_tokens,
        attachment_tokens: 0,
        attachment_names: Vec::new(),
        tool_name: Some(name),
        tool_call_tokens: call_tokens,
        tool_result_tokens: result_tokens,
    })
}

fn message_content(
    segment_type: &'static str,
    role: &'static str,
    preview: String,
) -> SegmentContent {
    SegmentContent {
        segment_type,
        role,
        token_estimate: estimate_tokens(&preview),
        preview,
        attachment_tokens: 0,
        attachment_names: Vec::new(),
        tool_name: None,
        tool_call_tokens: 0,
        tool_result_tokens: 0,
    }
}

fn tool_name(item: &AgentDashThreadItem, value: &Value) -> String {
    if let AgentDashThreadItem::AgentDash(item) = item {
        return item.tool_name().to_owned();
    }
    value
        .get("tool")
        .and_then(Value::as_str)
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or("tool")
        .to_owned()
}

fn selected_json(value: &Value, keys: &[&str]) -> String {
    let Some(object) = value.as_object() else {
        return String::new();
    };
    let selected = keys
        .iter()
        .filter_map(|key| object.get(*key).map(|value| (*key, value)))
        .collect::<BTreeMap<_, _>>();
    if selected.is_empty() {
        String::new()
    } else {
        serde_json::to_string(&selected).unwrap_or_default()
    }
}

fn event_turn_id(event: &BackboneEvent) -> Option<&str> {
    match event {
        BackboneEvent::UserInputSubmitted(notification) => Some(&notification.turn_id),
        BackboneEvent::ItemCompleted(notification) => Some(&notification.turn_id),
        _ => None,
    }
}

fn estimate_tokens(text: &str) -> u64 {
    let characters = text.chars().count() as u64;
    characters.div_ceil(4)
}

fn preview(text: &str) -> String {
    const LIMIT: usize = 360;
    let mut value = text.chars().take(LIMIT).collect::<String>();
    if text.chars().count() > LIMIT {
        value.push('…');
    }
    value
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

const fn context_frame_source(source: ContextFrameSource) -> &'static str {
    match source {
        ContextFrameSource::RuntimeContextUpdate => "runtime_context_update",
        ContextFrameSource::CompanionResult => "companion_result",
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_protocol::{
        BackboneEnvelope, CanonicalConversationPresentation, CanonicalConversationRecord,
        ItemCompletedNotification, PresentationDurability, SourceInfo, UserInputSource,
        UserInputSubmissionKind, UserInputSubmittedNotification,
        codex_app_server_protocol as codex, text_user_input_blocks,
    };

    use super::*;

    fn record(id: &str, event: BackboneEvent) -> CanonicalConversationRecord {
        CanonicalConversationRecord::new(
            id,
            CanonicalConversationPresentation::new(
                PresentationDurability::Durable,
                BackboneEnvelope::new(
                    event,
                    "thread-1",
                    SourceInfo {
                        connector_id: "native".to_owned(),
                        connector_type: "native".to_owned(),
                        executor_id: None,
                    },
                ),
            ),
        )
    }

    #[test]
    fn projects_authoritative_messages_and_tools_into_context_usage() {
        let records = vec![
            record(
                "user-1",
                BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                    "thread-1",
                    "turn-1",
                    "user-item-1",
                    UserInputSubmissionKind::Prompt,
                    UserInputSource::core_composer(),
                    text_user_input_blocks("请检查当前状态"),
                )),
            ),
            record(
                "assistant-1",
                BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                    codex::ThreadItem::AgentMessage {
                        id: "assistant-item-1".to_owned(),
                        text: "已经完成检查".to_owned(),
                        phase: None,
                        memory_citation: None,
                    },
                    "thread-1",
                    "turn-1",
                )),
            ),
            record(
                "tool-1",
                BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                    codex::ThreadItem::DynamicToolCall {
                        id: "tool-item-1".to_owned(),
                        tool: "read_file".to_owned(),
                        arguments: serde_json::json!({"path": "src/main.rs"}),
                        status: codex::DynamicToolCallStatus::Completed,
                        content_items: None,
                        duration_ms: None,
                        namespace: None,
                        success: Some(Some(true)),
                    },
                    "thread-1",
                    "turn-1",
                )),
            ),
        ];

        let projection = project_records("thread-1", 7, &records);

        assert_eq!(projection.message_count, 3);
        assert!(projection.context_usage.messages.user_message_tokens > 0);
        assert!(projection.context_usage.messages.assistant_message_tokens > 0);
        assert!(projection.context_usage.messages.tool_call_tokens > 0);
        assert_eq!(projection.context_usage.top_tools[0].name, "read_file");
        assert_eq!(projection.projection_version, 7);
    }

    #[test]
    fn compaction_excludes_superseded_message_segments() {
        let records = vec![
            record(
                "old-user",
                BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                    "thread-1",
                    "turn-1",
                    "old-user-item",
                    UserInputSubmissionKind::Prompt,
                    UserInputSource::core_composer(),
                    text_user_input_blocks("旧消息"),
                )),
            ),
            record(
                "compact-1",
                BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                    codex::ThreadItem::ContextCompaction {
                        id: "compaction-1".to_owned(),
                    },
                    "thread-1",
                    "turn-1",
                )),
            ),
            record(
                "new-user",
                BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                    "thread-1",
                    "turn-2",
                    "new-user-item",
                    UserInputSubmissionKind::Prompt,
                    UserInputSource::core_composer(),
                    text_user_input_blocks("新消息"),
                )),
            ),
        ];

        let projection = project_records("thread-1", 8, &records);

        assert_eq!(
            projection.active_compaction_id.as_deref(),
            Some("compaction-1")
        );
        assert_eq!(projection.message_count, 1);
        assert_eq!(projection.segments[0].preview, "新消息");
    }
}
