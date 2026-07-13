use std::collections::{BTreeMap, HashMap};

use agentdash_agent_protocol::{
    AgentDashNativeThreadItem, AgentDashThreadItem, BackboneEvent, PlatformEvent,
    codex_app_server_protocol as codex, user_input_blocks_to_content_parts,
};
use agentdash_agent_types::{
    AgentMessage, ContentPart, MessageRef, StopReason, ToolCallInfo, estimate_content_tokens,
    estimate_message_tokens,
};
use agentdash_contracts::session::{
    SessionAttachmentContextContributionResponse, SessionContextUsageAnalysisResponse,
    SessionContextUsageCategoryResponse, SessionContextUsageItemResponse,
    SessionMessageContextBreakdownResponse, SessionProjectionMessageRefResponse,
    SessionProjectionSegmentProvenanceResponse, SessionProjectionSegmentViewResponse,
    SessionProjectionSourceRangeResponse, SessionProjectionViewResponse,
    SessionToolContextContributionResponse,
};
use agentdash_spi::{
    context_usage_kind,
    hooks::{ContextFrame, ContextFrameSection},
};
use serde::Deserialize;

use super::{AgentRunJournalEvent, AgentRunJournalQuery, AgentRunJournalService};
use crate::error::WorkflowApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunContextCompactionArchive {
    pub compaction_id: String,
    pub lifecycle_item_id: String,
    pub projection_version: u64,
    pub completed_event_seq: u64,
    pub source_start_event_seq: Option<u64>,
    pub source_end_event_seq: Option<u64>,
    pub summary: String,
    pub tokens_before: u64,
    pub messages_compacted: u32,
    pub trigger: Option<String>,
    pub strategy: Option<String>,
    pub phase: Option<String>,
    pub turn_id: Option<String>,
    pub entry_index: Option<u32>,
}

impl AgentRunJournalService {
    pub async fn list_context_compaction_archives(
        &self,
        query: AgentRunJournalQuery,
    ) -> Result<Vec<AgentRunContextCompactionArchive>, WorkflowApplicationError> {
        let page = self.load_visible_journal_page(query, 0, u32::MAX).await?;
        Ok(context_compaction_archives(page.events))
    }

    pub async fn build_context_projection_read_model(
        &self,
        query: AgentRunJournalQuery,
    ) -> Result<SessionProjectionViewResponse, WorkflowApplicationError> {
        let session_id = super::agent_run_journal_session_id(query.run_id, query.agent_id);
        let page = self.load_visible_journal_page(query, 0, u32::MAX).await?;
        Ok(build_context_projection(session_id, page.events))
    }
}

fn context_compaction_archives(
    events: Vec<AgentRunJournalEvent>,
) -> Vec<AgentRunContextCompactionArchive> {
    let mut projection_version = 0_u64;
    let mut archives = Vec::new();
    for event in events {
        let Some(presentation) = event.record.as_presentation() else {
            continue;
        };
        let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) =
            &presentation.event
        else {
            continue;
        };
        if key != "context_compacted" {
            continue;
        }
        let Ok(fact) = serde_json::from_value::<ContextCompactedFact>(value.clone()) else {
            continue;
        };
        projection_version = fact
            .projection_version
            .unwrap_or_else(|| projection_version.saturating_add(1));
        let carrier = event.record.carrier();
        archives.push(AgentRunContextCompactionArchive {
            compaction_id: fact
                .compaction_id
                .unwrap_or_else(|| format!("compaction-{}", fact.lifecycle_item_id)),
            lifecycle_item_id: fact.lifecycle_item_id,
            projection_version,
            completed_event_seq: event.journal_seq,
            source_start_event_seq: fact.source_start_event_seq,
            source_end_event_seq: fact.source_end_event_seq,
            summary: fact.summary,
            tokens_before: fact.tokens_before,
            messages_compacted: fact.messages_compacted,
            trigger: fact.trigger,
            strategy: fact.strategy,
            phase: fact.phase,
            turn_id: carrier.coordinate.source_turn_id.clone(),
            entry_index: carrier.coordinate.source_entry_index,
        });
    }
    archives
}

fn build_context_projection(
    session_id: String,
    events: Vec<AgentRunJournalEvent>,
) -> SessionProjectionViewResponse {
    let head_event_seq = events.last().map_or(0, |event| event.journal_seq);
    let mut projection_version = 0_u64;
    let mut active_compaction_id = None;
    let mut segments = transcript_segments(&session_id, &events);
    let mut usage_items = Vec::new();

    for event in events {
        let Some(presentation) = event.record.as_presentation() else {
            continue;
        };
        let carrier = event.record.carrier();
        match &presentation.event {
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                if key == "context_frame" =>
            {
                if let Ok(frame) = serde_json::from_value::<ContextFrame>(value.clone()) {
                    usage_items.extend(context_usage_items_from_context_frame(
                        &frame,
                        Some(event.journal_seq),
                        carrier.coordinate.source_turn_id.clone(),
                    ));
                }
            }
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
                if key == "context_compacted" =>
            {
                let Ok(fact) = serde_json::from_value::<ContextCompactedFact>(value.clone()) else {
                    continue;
                };
                projection_version = fact
                    .projection_version
                    .unwrap_or_else(|| projection_version.saturating_add(1));
                active_compaction_id = fact
                    .compaction_id
                    .clone()
                    .or_else(|| Some(format!("compaction-{}", fact.lifecycle_item_id)));
                if let Some(segment) = compaction_segment(
                    &session_id,
                    event.journal_seq,
                    &fact,
                    projection_version,
                    carrier.coordinate.source_turn_id.as_deref(),
                    carrier.coordinate.source_entry_index,
                    segments.len(),
                ) {
                    if let Some(end_event_seq) = segment
                        .source_range
                        .as_ref()
                        .map(|range| range.end_event_seq)
                    {
                        segments.retain(|existing| {
                            existing
                                .source_event_seq
                                .is_none_or(|sequence| sequence > end_event_seq)
                        });
                    }
                    let insert_at = segment
                        .source_range
                        .as_ref()
                        .map(|range| {
                            segments
                                .iter()
                                .position(|existing| {
                                    existing
                                        .source_event_seq
                                        .is_some_and(|sequence| sequence > range.end_event_seq)
                                })
                                .unwrap_or(segments.len())
                        })
                        .unwrap_or(segments.len());
                    segments.insert(insert_at, segment);
                }
            }
            _ => {}
        }
    }
    for (index, segment) in segments.iter_mut().enumerate() {
        segment.sort_order = index as u32;
    }
    let context_usage = usage_analysis(&segments, usage_items);
    let context_item_tokens = context_usage
        .items
        .iter()
        .filter(|item| !item.deferred)
        .map(|item| item.token_estimate)
        .sum::<u64>();
    let segment_tokens = segments
        .iter()
        .filter_map(|segment| segment.token_estimate)
        .sum::<u64>();
    SessionProjectionViewResponse {
        session_id,
        projection_kind: "model_context".to_string(),
        projection_version,
        head_event_seq,
        active_compaction_id,
        token_estimate: (!segments.is_empty() || context_item_tokens > 0)
            .then_some(segment_tokens.saturating_add(context_item_tokens)),
        message_count: segments.len() as u64,
        segments,
        context_usage,
    }
}

#[derive(Default)]
struct AssistantTranscriptState {
    order: u64,
    turn_id: String,
    entry_index: u32,
    text_deltas: String,
    reasoning_deltas: String,
    final_text: Option<String>,
    final_reasoning: Option<String>,
    tool_calls: Vec<ToolCallInfo>,
}

struct TranscriptMessage {
    order: u64,
    turn_id: String,
    entry_index: u32,
    message: AgentMessage,
}

fn transcript_segments(
    session_id: &str,
    events: &[AgentRunJournalEvent],
) -> Vec<SessionProjectionSegmentViewResponse> {
    let mut users = HashMap::<String, TranscriptMessage>::new();
    let mut assistants = HashMap::<String, AssistantTranscriptState>::new();
    let mut results = HashMap::<String, TranscriptMessage>::new();
    for journal in events {
        let Some(presentation) = journal.record.as_presentation() else {
            continue;
        };
        let carrier = journal.record.carrier();
        let turn = carrier
            .coordinate
            .source_turn_id
            .clone()
            .unwrap_or_else(|| session_id.to_string());
        let entry = carrier.coordinate.source_entry_index.unwrap_or(0);
        let assistant_key = format!("{turn}:{entry}");
        match &presentation.event {
            BackboneEvent::UserInputSubmitted(input) => {
                users
                    .entry(input.item_id.clone())
                    .or_insert_with(|| TranscriptMessage {
                        order: journal.journal_seq,
                        turn_id: input.turn_id.clone(),
                        entry_index: entry,
                        message: AgentMessage::User {
                            content: user_input_blocks_to_content_parts(&input.content),
                            timestamp: None,
                        },
                    });
            }
            BackboneEvent::AgentMessageDelta(delta) if !delta.delta.is_empty() => {
                let state =
                    assistants
                        .entry(assistant_key)
                        .or_insert_with(|| AssistantTranscriptState {
                            order: journal.journal_seq,
                            turn_id: turn.clone(),
                            entry_index: entry,
                            ..Default::default()
                        });
                state.text_deltas.push_str(&delta.delta);
            }
            BackboneEvent::ReasoningTextDelta(delta) if !delta.delta.is_empty() => {
                let state =
                    assistants
                        .entry(assistant_key)
                        .or_insert_with(|| AssistantTranscriptState {
                            order: journal.journal_seq,
                            turn_id: turn.clone(),
                            entry_index: entry,
                            ..Default::default()
                        });
                state.reasoning_deltas.push_str(&delta.delta);
            }
            BackboneEvent::ReasoningSummaryDelta(delta) if !delta.delta.is_empty() => {
                let state =
                    assistants
                        .entry(assistant_key)
                        .or_insert_with(|| AssistantTranscriptState {
                            order: journal.journal_seq,
                            turn_id: turn.clone(),
                            entry_index: entry,
                            ..Default::default()
                        });
                state.reasoning_deltas.push_str(&delta.delta);
            }
            BackboneEvent::ItemStarted(notification) => apply_tool_item(
                &notification.item,
                journal.journal_seq,
                &turn,
                entry,
                &assistant_key,
                &mut assistants,
                &mut results,
            ),
            BackboneEvent::ItemUpdated(notification) => apply_tool_item(
                &notification.item,
                journal.journal_seq,
                &turn,
                entry,
                &assistant_key,
                &mut assistants,
                &mut results,
            ),
            BackboneEvent::ItemCompleted(notification) => match &notification.item {
                AgentDashThreadItem::Codex(codex::ThreadItem::AgentMessage { text, .. }) => {
                    let state = assistants.entry(assistant_key).or_insert_with(|| {
                        AssistantTranscriptState {
                            order: journal.journal_seq,
                            turn_id: turn.clone(),
                            entry_index: entry,
                            ..Default::default()
                        }
                    });
                    state.final_text = Some(text.clone());
                }
                AgentDashThreadItem::Codex(codex::ThreadItem::Reasoning { content, .. }) => {
                    let state = assistants.entry(assistant_key).or_insert_with(|| {
                        AssistantTranscriptState {
                            order: journal.journal_seq,
                            turn_id: turn.clone(),
                            entry_index: entry,
                            ..Default::default()
                        }
                    });
                    state.final_reasoning = Some(content.join(""));
                }
                item => apply_tool_item(
                    item,
                    journal.journal_seq,
                    &turn,
                    entry,
                    &assistant_key,
                    &mut assistants,
                    &mut results,
                ),
            },
            _ => {}
        }
    }
    let mut messages = users.into_values().collect::<Vec<_>>();
    for state in assistants.into_values() {
        let mut content = Vec::new();
        let reasoning = state.final_reasoning.unwrap_or(state.reasoning_deltas);
        let text = state.final_text.unwrap_or(state.text_deltas);
        if !reasoning.is_empty() {
            content.push(ContentPart::reasoning(reasoning, None, None));
        }
        if !text.is_empty() {
            content.push(ContentPart::text(text));
        }
        if content.is_empty() && state.tool_calls.is_empty() {
            continue;
        }
        let has_tools = !state.tool_calls.is_empty();
        messages.push(TranscriptMessage {
            order: state.order,
            turn_id: state.turn_id,
            entry_index: state.entry_index,
            message: AgentMessage::Assistant {
                content,
                tool_calls: state.tool_calls,
                stop_reason: Some(if has_tools {
                    StopReason::ToolUse
                } else {
                    StopReason::Stop
                }),
                error_message: None,
                usage: None,
                timestamp: None,
            },
        });
    }
    messages.extend(results.into_values());
    messages.sort_by_key(|message| message.order);
    messages
        .into_iter()
        .enumerate()
        .map(|(index, value)| segment_from_message(index, value))
        .collect()
}

fn apply_tool_item(
    item: &AgentDashThreadItem,
    order: u64,
    turn_id: &str,
    entry_index: u32,
    assistant_key: &str,
    assistants: &mut HashMap<String, AssistantTranscriptState>,
    results: &mut HashMap<String, TranscriptMessage>,
) {
    let Some((id, name, arguments, result, details, is_error)) = tool_item_parts(item) else {
        return;
    };
    let state = assistants
        .entry(assistant_key.to_string())
        .or_insert_with(|| AssistantTranscriptState {
            order,
            turn_id: turn_id.to_string(),
            entry_index,
            ..Default::default()
        });
    state.order = state.order.min(order);
    if let Some(call) = state.tool_calls.iter_mut().find(|call| call.id == id) {
        call.name = name.clone();
        call.arguments = arguments.clone();
    } else {
        state.tool_calls.push(ToolCallInfo {
            id: id.clone(),
            call_id: Some(id.clone()),
            name: name.clone(),
            arguments,
        });
    }
    if tool_item_is_terminal(item) {
        results.insert(
            id.clone(),
            TranscriptMessage {
                order,
                turn_id: turn_id.to_string(),
                entry_index,
                message: AgentMessage::ToolResult {
                    tool_call_id: id.clone(),
                    call_id: Some(id),
                    tool_name: Some(name),
                    content: vec![ContentPart::text(result)],
                    details,
                    is_error,
                    timestamp: None,
                },
            },
        );
    }
}

fn tool_item_is_terminal(item: &AgentDashThreadItem) -> bool {
    match item {
        AgentDashThreadItem::Codex(codex::ThreadItem::DynamicToolCall { status, .. }) => matches!(
            status,
            codex::DynamicToolCallStatus::Completed | codex::DynamicToolCallStatus::Failed
        ),
        AgentDashThreadItem::Codex(codex::ThreadItem::McpToolCall { status, .. }) => matches!(
            status,
            codex::McpToolCallStatus::Completed | codex::McpToolCallStatus::Failed
        ),
        AgentDashThreadItem::Codex(codex::ThreadItem::CommandExecution { status, .. }) => matches!(
            status,
            codex::CommandExecutionStatus::Completed
                | codex::CommandExecutionStatus::Failed
                | codex::CommandExecutionStatus::Declined
        ),
        AgentDashThreadItem::Codex(codex::ThreadItem::FileChange { status, .. }) => matches!(
            status,
            codex::PatchApplyStatus::Completed
                | codex::PatchApplyStatus::Failed
                | codex::PatchApplyStatus::Declined
        ),
        AgentDashThreadItem::AgentDash(item) => matches!(
            item.status(),
            codex::DynamicToolCallStatus::Completed | codex::DynamicToolCallStatus::Failed
        ),
        _ => false,
    }
}

fn segment_from_message(
    index: usize,
    value: TranscriptMessage,
) -> SessionProjectionSegmentViewResponse {
    let role = message_role(&value.message).to_string();
    let preview = message_preview(&value.message);
    let tool_names = message_tool_names(&value.message);
    let attachment_tokens = message_attachment_tokens(&value.message);
    let attachment_names = message_attachment_names(&value.message);
    SessionProjectionSegmentViewResponse {
        id: format!("original_event:{index}"),
        sort_order: index as u32,
        segment_type: "original_event".to_string(),
        role,
        origin: "event".to_string(),
        synthetic: false,
        projection_kind: "model_context".to_string(),
        message_ref: SessionProjectionMessageRefResponse {
            turn_id: value.turn_id,
            entry_index: value.entry_index,
        },
        source_event_seq: Some(value.order),
        source_range: None,
        projection_segment_id: None,
        preview,
        token_estimate: Some(estimate_message_tokens(&value.message)),
        attachment_tokens,
        attachment_names,
        tool_names,
        provenance: SessionProjectionSegmentProvenanceResponse {
            compaction_id: None,
            projection_version: Some(0),
            segment_type: None,
            strategy: None,
            trigger: None,
            phase: None,
        },
    }
}

#[cfg(test)]
fn presentation_segments(
    session_id: &str,
    event_seq: u64,
    event: &BackboneEvent,
    source_turn_id: Option<&str>,
    source_entry_index: Option<u32>,
    projection_version: u64,
    index: usize,
) -> Vec<SessionProjectionSegmentViewResponse> {
    let (event_turn_id, messages) = match event {
        BackboneEvent::UserInputSubmitted(input) => (
            Some(input.turn_id.as_str()),
            vec![AgentMessage::User {
                content: user_input_blocks_to_content_parts(&input.content),
                timestamp: None,
            }],
        ),
        BackboneEvent::ItemCompleted(notification) => (
            Some(notification.turn_id.as_str()),
            completed_item_messages(&notification.item),
        ),
        _ => return Vec::new(),
    };
    let turn_id = source_turn_id
        .or(event_turn_id)
        .unwrap_or(session_id)
        .to_string();
    let entry_index = source_entry_index.unwrap_or(index as u32);
    messages
        .into_iter()
        .enumerate()
        .map(|(offset, message)| {
            let role = message_role(&message);
            let preview = message_preview(&message);
            let tool_names = message_tool_names(&message);
            let attachment_tokens = message_attachment_tokens(&message);
            let attachment_names = message_attachment_names(&message);
            SessionProjectionSegmentViewResponse {
                id: format!("original_event:{}", index + offset),
                sort_order: (index + offset) as u32,
                segment_type: "original_event".to_string(),
                role: role.to_string(),
                origin: "event".to_string(),
                synthetic: false,
                projection_kind: "model_context".to_string(),
                message_ref: SessionProjectionMessageRefResponse {
                    turn_id: turn_id.clone(),
                    entry_index,
                },
                source_event_seq: Some(event_seq),
                source_range: None,
                projection_segment_id: None,
                preview,
                token_estimate: Some(estimate_message_tokens(&message)),
                attachment_tokens,
                attachment_names,
                tool_names,
                provenance: SessionProjectionSegmentProvenanceResponse {
                    compaction_id: None,
                    projection_version: Some(projection_version),
                    segment_type: None,
                    strategy: None,
                    trigger: None,
                    phase: None,
                },
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn compaction_segment(
    session_id: &str,
    event_seq: u64,
    fact: &ContextCompactedFact,
    projection_version: u64,
    source_turn_id: Option<&str>,
    source_entry_index: Option<u32>,
    index: usize,
) -> Option<SessionProjectionSegmentViewResponse> {
    let summary = fact.summary.trim();
    if summary.is_empty() {
        return None;
    }
    let compaction_id = fact
        .compaction_id
        .clone()
        .or_else(|| Some(format!("compaction-{}", fact.lifecycle_item_id)));
    let source_range = fact
        .source_start_event_seq
        .zip(fact.source_end_event_seq)
        .map(
            |(start_event_seq, end_event_seq)| SessionProjectionSourceRangeResponse {
                start_event_seq,
                end_event_seq,
            },
        );
    let message_ref = fact
        .compacted_until_ref
        .as_ref()
        .map(|value| SessionProjectionMessageRefResponse {
            turn_id: value.turn_id.clone(),
            entry_index: value.entry_index,
        })
        .unwrap_or_else(|| SessionProjectionMessageRefResponse {
            turn_id: source_turn_id.unwrap_or(session_id).to_string(),
            entry_index: source_entry_index.unwrap_or(index as u32),
        });
    let projection_segment_id = fact
        .projection_segment_id
        .clone()
        .or_else(|| compaction_id.as_ref().map(|id| format!("{id}-summary")));
    Some(SessionProjectionSegmentViewResponse {
        id: projection_segment_id
            .clone()
            .unwrap_or_else(|| "compaction-summary".to_string()),
        sort_order: index as u32,
        segment_type: "summary_chunk".to_string(),
        role: "compaction_summary".to_string(),
        origin: "projection".to_string(),
        synthetic: true,
        projection_kind: "model_context".to_string(),
        message_ref,
        source_event_seq: Some(event_seq),
        source_range,
        projection_segment_id,
        preview: summary.to_string(),
        token_estimate: Some(estimate_message_tokens(&AgentMessage::CompactionSummary {
            summary: summary.to_string(),
            tokens_before: fact.tokens_before,
            messages_compacted: fact.messages_compacted,
            compacted_until_ref: fact.compacted_until_ref.clone(),
            timestamp: None,
        })),
        attachment_tokens: 0,
        attachment_names: Vec::new(),
        tool_names: Vec::new(),
        provenance: SessionProjectionSegmentProvenanceResponse {
            compaction_id,
            projection_version: Some(projection_version),
            segment_type: Some("compaction_summary".to_string()),
            strategy: fact.strategy.clone(),
            trigger: fact.trigger.clone(),
            phase: fact.phase.clone(),
        },
    })
}

fn context_usage_items_from_context_frame(
    frame: &ContextFrame,
    source_event_seq: Option<u64>,
    turn_id: Option<String>,
) -> Vec<SessionContextUsageItemResponse> {
    frame
        .sections
        .iter()
        .flat_map(|section| context_usage_items_from_section(section, source_event_seq, &turn_id))
        .collect()
}

fn context_usage_items_from_section(
    section: &ContextFrameSection,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> Vec<SessionContextUsageItemResponse> {
    let item = |kind: &str, label: &str, name: &str, text: String, source: &str, deferred| {
        SessionContextUsageItemResponse {
            kind: kind.to_string(),
            label: label.to_string(),
            name: name.to_string(),
            token_estimate: estimate_tokens(&text),
            source: source.to_string(),
            deferred,
            source_event_seq,
            turn_id: turn_id.clone(),
        }
    };
    match section {
        ContextFrameSection::Identity {
            title, fragments, ..
        } => vec![item(
            context_usage_kind::SYSTEM_DEVELOPER,
            "System / Developer",
            title,
            fragments
                .iter()
                .map(|entry| entry.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
            "context_frame",
            false,
        )],
        ContextFrameSection::AssignmentContext {
            title, fragments, ..
        } => {
            let mut values = Vec::new();
            for (kind, label, name) in [
                (
                    context_usage_kind::SYSTEM_DEVELOPER,
                    "System / Developer",
                    title.as_str(),
                ),
                (context_usage_kind::AGENTS, "Agents", "Companion Agents"),
            ] {
                let text = fragments
                    .iter()
                    .filter(|entry| {
                        entry
                            .context_usage_kind
                            .as_deref()
                            .is_some_and(|value| value.eq_ignore_ascii_case(kind))
                    })
                    .map(|entry| entry.content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if !text.is_empty() {
                    values.push(item(kind, label, name, text, "context_frame", false));
                }
            }
            values
        }
        ContextFrameSection::CapabilityKeyDelta {
            added_capabilities,
            removed_capabilities,
            effective_capabilities,
        } => capability_item(
            "Capability Keys",
            [
                added_capabilities,
                removed_capabilities,
                effective_capabilities,
            ],
            source_event_seq,
            turn_id,
        ),
        ContextFrameSection::ToolPathDelta {
            blocked_tool_paths,
            unblocked_tool_paths,
            whitelisted_tool_paths,
            removed_whitelist_paths,
        } => capability_item(
            "Tool Path Delta",
            [
                blocked_tool_paths,
                unblocked_tool_paths,
                whitelisted_tool_paths,
                removed_whitelist_paths,
            ],
            source_event_seq,
            turn_id,
        ),
        ContextFrameSection::McpServerDelta {
            added_mcp_servers,
            removed_mcp_servers,
            changed_mcp_servers,
        } => capability_item(
            "MCP Server Delta",
            [added_mcp_servers, removed_mcp_servers, changed_mcp_servers],
            source_event_seq,
            turn_id,
        ),
        ContextFrameSection::VfsDelta {
            vfs_mounts_added,
            vfs_mounts_removed,
            default_mount_before,
            default_mount_after,
        } => {
            let text = format!(
                "added: {vfs_mounts_added:?}\nremoved: {vfs_mounts_removed:?}\ndefault: {default_mount_before:?} -> {default_mount_after:?}"
            );
            vec![item(
                context_usage_kind::CAPABILITY_STATE,
                "Capability State",
                "VFS Delta",
                text,
                "capability_state",
                false,
            )]
        }
        ContextFrameSection::ToolSchemaDelta { added_tools } => added_tools
            .iter()
            .filter_map(|tool| {
                let kind = tool.context_usage_kind.as_deref()?;
                let label = if kind.eq_ignore_ascii_case(context_usage_kind::MCP_TOOLS) {
                    "MCP Tools"
                } else if kind.eq_ignore_ascii_case(context_usage_kind::SYSTEM_TOOLS) {
                    "System Tools"
                } else {
                    return None;
                };
                Some(item(
                    kind,
                    label,
                    &tool.name,
                    format!(
                        "{}\n{}\n{}",
                        tool.name, tool.description, tool.parameters_schema
                    ),
                    "tool_schema",
                    false,
                ))
            })
            .collect(),
        ContextFrameSection::SkillDelta {
            added_skills,
            removed_skills,
            changed_skills,
        } => added_skills
            .iter()
            .chain(removed_skills)
            .chain(changed_skills)
            .filter_map(|skill| {
                let kind = skill.context_usage_kind.as_deref()?;
                kind.eq_ignore_ascii_case(context_usage_kind::SKILLS)
                    .then(|| {
                        item(
                            kind,
                            "Skills",
                            skill.display_name.as_deref().unwrap_or(&skill.name),
                            format!("{}\n{}\n{}", skill.name, skill.description, skill.file_path),
                            "skill_registry",
                            skill.disable_model_invocation,
                        )
                    })
            })
            .collect(),
        ContextFrameSection::MemoryInventory {
            mode,
            sources,
            added_sources,
            removed_sources,
            changed_sources,
            ..
        } => {
            use agentdash_spi::hooks::RuntimeMemoryInventoryMode;
            let entries: Vec<_> = match mode {
                RuntimeMemoryInventoryMode::Snapshot => sources.iter().collect(),
                RuntimeMemoryInventoryMode::Delta => added_sources
                    .iter()
                    .chain(removed_sources)
                    .chain(changed_sources)
                    .collect(),
            };
            entries
                .into_iter()
                .filter_map(|entry| {
                    let kind = entry.context_usage_kind.as_deref()?;
                    kind.eq_ignore_ascii_case(context_usage_kind::MEMORY)
                        .then(|| {
                            item(
                                kind,
                                "Memory",
                                if entry.display_name.is_empty() {
                                    &entry.source_key
                                } else {
                                    &entry.display_name
                                },
                                format!("{entry:?}"),
                                "memory_inventory",
                                false,
                            )
                        })
                })
                .collect()
        }
        ContextFrameSection::CompanionAgentRosterDelta {
            effective_agents, ..
        } => effective_agents
            .iter()
            .filter_map(|agent| {
                let kind = agent.context_usage_kind.as_deref()?;
                kind.eq_ignore_ascii_case(context_usage_kind::AGENTS)
                    .then(|| {
                        item(
                            kind,
                            "Agents",
                            if agent.display_name.is_empty() {
                                &agent.agent_key
                            } else {
                                &agent.display_name
                            },
                            format!("{}\n{}", agent.agent_key, agent.executor),
                            "capability_state",
                            false,
                        )
                    })
            })
            .collect(),
        ContextFrameSection::SystemNotice {
            title,
            summary,
            body,
        } => vec![item(
            context_usage_kind::SYSTEM_DEVELOPER,
            "System / Developer",
            title,
            body.as_deref().unwrap_or(summary).to_string(),
            "context_frame",
            false,
        )],
        ContextFrameSection::PendingAction {
            title,
            instructions,
            injections,
            ..
        } => {
            let mut text = instructions.join("\n\n");
            for injection in injections.iter().filter(|entry| {
                entry.context_usage_kind.as_deref().is_some_and(|kind| {
                    kind.eq_ignore_ascii_case(context_usage_kind::SYSTEM_DEVELOPER)
                })
            }) {
                text.push_str("\n\n");
                text.push_str(&injection.content);
            }
            vec![item(
                context_usage_kind::SYSTEM_DEVELOPER,
                "System / Developer",
                title,
                text,
                "context_frame",
                false,
            )]
        }
        ContextFrameSection::AutoResume { title, prompt, .. } => vec![item(
            context_usage_kind::SYSTEM_DEVELOPER,
            "System / Developer",
            title,
            prompt.clone(),
            "context_frame",
            false,
        )],
        ContextFrameSection::CompactionSummary {
            title,
            summary,
            tokens_before,
            messages_compacted,
            ..
        } => vec![item(
            "compaction_summary",
            "Compaction Summary",
            title,
            format!(
                "messages_compacted: {messages_compacted}\ntokens_before: {tokens_before}\n\n{summary}"
            ),
            "context_frame",
            false,
        )],
        ContextFrameSection::Environment { title, summary, .. }
        | ContextFrameSection::UserContext { title, summary, .. } => vec![item(
            context_usage_kind::SYSTEM_DEVELOPER,
            "System / Developer",
            title,
            summary.clone(),
            "context_frame",
            false,
        )],
        ContextFrameSection::UserPreferences { title, items, .. } => vec![item(
            context_usage_kind::MEMORY,
            "Memory",
            title,
            items.join("\n"),
            "context_frame",
            false,
        )],
        ContextFrameSection::ProjectGuidelines { title, entries, .. } => vec![item(
            context_usage_kind::MEMORY,
            "Memory",
            title,
            entries
                .iter()
                .map(|entry| format!("{}\n{}", entry.path, entry.content))
                .collect::<Vec<_>>()
                .join("\n\n"),
            "context_frame",
            false,
        )],
    }
}

fn capability_item<const N: usize>(
    name: &str,
    groups: [&Vec<String>; N],
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> Vec<SessionContextUsageItemResponse> {
    let text = groups
        .iter()
        .flat_map(|group| group.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        return Vec::new();
    }
    vec![SessionContextUsageItemResponse {
        kind: context_usage_kind::CAPABILITY_STATE.to_string(),
        label: "Capability State".to_string(),
        name: name.to_string(),
        token_estimate: estimate_tokens(&text),
        source: "capability_state".to_string(),
        deferred: false,
        source_event_seq,
        turn_id: turn_id.clone(),
    }]
}

#[cfg(test)]
fn completed_item_messages(item: &AgentDashThreadItem) -> Vec<AgentMessage> {
    match item {
        AgentDashThreadItem::Codex(codex::ThreadItem::AgentMessage { text, .. }) => {
            vec![AgentMessage::Assistant {
                content: vec![ContentPart::text(text)],
                tool_calls: Vec::new(),
                stop_reason: Some(StopReason::Stop),
                error_message: None,
                usage: None,
                timestamp: None,
            }]
        }
        AgentDashThreadItem::Codex(codex::ThreadItem::Reasoning {
            summary, content, ..
        }) => {
            let text = summary
                .iter()
                .chain(content.iter())
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty())
                .then(|| AgentMessage::Assistant {
                    content: vec![ContentPart::reasoning(text, None, None)],
                    tool_calls: Vec::new(),
                    stop_reason: Some(StopReason::Stop),
                    error_message: None,
                    usage: None,
                    timestamp: None,
                })
                .into_iter()
                .collect()
        }
        _ => tool_item_parts(item)
            .map(|(id, name, arguments, result, details, is_error)| {
                vec![
                    AgentMessage::Assistant {
                        content: Vec::new(),
                        tool_calls: vec![ToolCallInfo {
                            id: id.clone(),
                            call_id: Some(id.clone()),
                            name: name.clone(),
                            arguments,
                        }],
                        stop_reason: Some(StopReason::ToolUse),
                        error_message: None,
                        usage: None,
                        timestamp: None,
                    },
                    AgentMessage::ToolResult {
                        tool_call_id: id.clone(),
                        call_id: Some(id),
                        tool_name: Some(name),
                        content: vec![ContentPart::text(result)],
                        details,
                        is_error,
                        timestamp: None,
                    },
                ]
            })
            .unwrap_or_default(),
    }
}

fn tool_item_parts(
    item: &AgentDashThreadItem,
) -> Option<(
    String,
    String,
    serde_json::Value,
    String,
    Option<serde_json::Value>,
    bool,
)> {
    match item {
        AgentDashThreadItem::Codex(codex::ThreadItem::CommandExecution {
            id,
            command,
            cwd,
            aggregated_output,
            ..
        }) => Some((
            id.clone(),
            "command_execution".to_string(),
            serde_json::json!({ "command": command, "cwd": cwd }),
            aggregated_output
                .as_ref()
                .and_then(Option::as_ref)
                .cloned()
                .unwrap_or_else(|| missing_tool_output(id, "command_execution")),
            aggregated_output
                .as_ref()
                .and_then(Option::as_ref)
                .is_none()
                .then(|| missing_tool_details(id, "command_execution")),
            aggregated_output
                .as_ref()
                .and_then(Option::as_ref)
                .is_none(),
        )),
        AgentDashThreadItem::Codex(codex::ThreadItem::FileChange { id, .. }) => Some((
            id.clone(),
            "file_change".to_string(),
            serde_json::json!({}),
            format!(
                "[restored tool output missing]\ntool_call_id: {id}\ntool_name: file_change\nThe original tool output was not available in persisted session events. A restored placeholder preserves the tool-call/tool-result pair so continuation can proceed."
            ),
            Some(serde_json::json!({
                "type": "restored_tool_output_missing",
                "tool_call_id": id,
                "tool_name": "file_change",
            })),
            true,
        )),
        AgentDashThreadItem::Codex(codex::ThreadItem::McpToolCall {
            id,
            tool,
            arguments,
            result: tool_result,
            error,
            ..
        }) => {
            let details = tool_result
                .as_ref()
                .and_then(Option::as_ref)
                .and_then(|value| serde_json::to_value(value).ok())
                .or_else(|| {
                    error
                        .as_ref()
                        .and_then(Option::as_ref)
                        .map(|value| serde_json::Value::String(value.message.clone()))
                });
            let missing = details.is_none();
            let preview = details
                .as_ref()
                .map(json_preview)
                .unwrap_or_else(|| missing_tool_output(id, tool));
            Some((
                id.clone(),
                tool.clone(),
                arguments.clone(),
                preview,
                details.or_else(|| Some(missing_tool_details(id, tool))),
                error.as_ref().and_then(Option::as_ref).is_some() || missing,
            ))
        }
        AgentDashThreadItem::Codex(codex::ThreadItem::DynamicToolCall {
            id,
            tool,
            arguments,
            content_items,
            ..
        }) => Some((
            id.clone(),
            tool.clone(),
            arguments.clone(),
            content_items
                .as_ref()
                .and_then(Option::as_ref)
                .map(|items| dynamic_content_preview(items))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| missing_tool_output(id, tool)),
            content_items
                .as_ref()
                .and_then(Option::as_ref)
                .is_none()
                .then(|| missing_tool_details(id, tool)),
            content_items.as_ref().and_then(Option::as_ref).is_none(),
        )),
        AgentDashThreadItem::Codex(codex::ThreadItem::CollabAgentToolCall { id, tool, .. }) => {
            Some((
                id.clone(),
                format!("{tool:?}"),
                serde_json::Value::Null,
                missing_tool_output(id, &format!("{tool:?}")),
                Some(missing_tool_details(id, &format!("{tool:?}"))),
                true,
            ))
        }
        AgentDashThreadItem::AgentDash(native) => Some((
            native.id().to_string(),
            native.tool_name().to_string(),
            native
                .arguments()
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            native_result(native)
                .unwrap_or_else(|| missing_tool_output(native.id(), native.tool_name())),
            native_result(native)
                .is_none()
                .then(|| missing_tool_details(native.id(), native.tool_name())),
            native.success() == Some(false) || native_result(native).is_none(),
        )),
        _ => None,
    }
}

fn missing_tool_output(id: &str, name: &str) -> String {
    format!(
        "[restored tool output missing]\ntool_call_id: {id}\ntool_name: {name}\nThe original tool output was not available in persisted session events. A restored placeholder preserves the tool-call/tool-result pair so continuation can proceed."
    )
}

fn missing_tool_details(id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "restored_tool_output_missing",
        "tool_call_id": id,
        "tool_name": name,
    })
}

fn dynamic_content_preview(items: &[codex::DynamicToolCallOutputContentItem]) -> String {
    items
        .iter()
        .map(|item| match item {
            codex::DynamicToolCallOutputContentItem::InputText { text } => text.clone(),
            codex::DynamicToolCallOutputContentItem::InputImage { image_url } => {
                format!("[image output: {image_url}]")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn json_preview(value: &serde_json::Value) -> String {
    let rendered = value.to_string();
    if rendered.chars().count() <= 320 {
        rendered
    } else {
        format!("{}...", rendered.chars().take(320).collect::<String>())
    }
}

fn native_result(item: &AgentDashNativeThreadItem) -> Option<String> {
    match item {
        AgentDashNativeThreadItem::ShellExec {
            aggregated_output, ..
        }
        | AgentDashNativeThreadItem::TerminalControl {
            aggregated_output, ..
        } => aggregated_output.clone(),
        AgentDashNativeThreadItem::FsRead { content_items, .. }
        | AgentDashNativeThreadItem::FsGrep { content_items, .. }
        | AgentDashNativeThreadItem::FsGlob { content_items, .. } => content_items
            .as_ref()
            .and_then(|items| serde_json::to_string(items).ok()),
    }
}

fn message_role(message: &AgentMessage) -> &'static str {
    match message {
        AgentMessage::User { .. } => "user",
        AgentMessage::Assistant { .. } => "assistant",
        AgentMessage::ToolResult { .. } => "tool_result",
        AgentMessage::CompactionSummary { .. } => "compaction_summary",
    }
}

fn message_preview(message: &AgentMessage) -> String {
    let value = message
        .first_text()
        .map(str::to_owned)
        .or_else(|| {
            let AgentMessage::Assistant { tool_calls, .. } = message else {
                return None;
            };
            (!tool_calls.is_empty()).then(|| {
                format!(
                    "tool_calls: {}",
                    tool_calls
                        .iter()
                        .map(|tool| tool.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
        })
        .unwrap_or_else(|| message_role(message).to_string());
    bounded_preview(&value)
}

fn message_tool_names(message: &AgentMessage) -> Vec<String> {
    match message {
        AgentMessage::Assistant { tool_calls, .. } => {
            tool_calls.iter().map(|call| call.name.clone()).collect()
        }
        AgentMessage::ToolResult { tool_name, .. } => tool_name.clone().into_iter().collect(),
        _ => Vec::new(),
    }
}

fn message_content(message: &AgentMessage) -> &[ContentPart] {
    match message {
        AgentMessage::User { content, .. }
        | AgentMessage::Assistant { content, .. }
        | AgentMessage::ToolResult { content, .. } => content,
        AgentMessage::CompactionSummary { .. } => &[],
    }
}

fn message_attachment_tokens(message: &AgentMessage) -> u64 {
    message_content(message)
        .iter()
        .filter(|part| matches!(part, ContentPart::Image { .. }))
        .map(|part| estimate_content_tokens(std::slice::from_ref(part)))
        .sum()
}

fn message_attachment_names(message: &AgentMessage) -> Vec<String> {
    message_content(message)
        .iter()
        .enumerate()
        .filter_map(|(index, part)| match part {
            ContentPart::Image { mime_type, .. } => Some(format!("{mime_type} image #{index}")),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct ContextCompactedFact {
    #[serde(default)]
    lifecycle_item_id: String,
    summary: String,
    #[serde(default)]
    tokens_before: u64,
    #[serde(default)]
    messages_compacted: u32,
    #[serde(default)]
    compaction_id: Option<String>,
    #[serde(default)]
    projection_segment_id: Option<String>,
    #[serde(default)]
    projection_version: Option<u64>,
    #[serde(default)]
    compacted_until_ref: Option<MessageRef>,
    #[serde(default)]
    source_start_event_seq: Option<u64>,
    #[serde(default)]
    source_end_event_seq: Option<u64>,
    #[serde(default)]
    strategy: Option<String>,
    #[serde(default)]
    trigger: Option<String>,
    #[serde(default)]
    phase: Option<String>,
}

fn usage_analysis(
    segments: &[SessionProjectionSegmentViewResponse],
    items: Vec<SessionContextUsageItemResponse>,
) -> SessionContextUsageAnalysisResponse {
    let mut messages = SessionMessageContextBreakdownResponse {
        user_message_tokens: 0,
        assistant_message_tokens: 0,
        tool_call_tokens: 0,
        tool_result_tokens: 0,
        attachment_tokens: 0,
    };
    let mut tools = BTreeMap::<String, (u64, u64)>::new();
    let mut attachments = BTreeMap::<String, u64>::new();
    for segment in segments {
        let tokens = segment.token_estimate.unwrap_or_default();
        match segment.role.as_str() {
            "user" => messages.user_message_tokens += tokens,
            "assistant" if segment.tool_names.is_empty() => {
                messages.assistant_message_tokens += tokens
            }
            "assistant" => messages.tool_call_tokens += tokens,
            "compaction_summary" => messages.assistant_message_tokens += tokens,
            "tool_call" => messages.tool_call_tokens += tokens,
            "tool_result" => messages.tool_result_tokens += tokens,
            _ => {}
        }
        messages.attachment_tokens += segment.attachment_tokens;
        for name in &segment.tool_names {
            let entry = tools.entry(name.clone()).or_default();
            if segment.role == "tool_result" {
                entry.1 += tokens
            } else {
                entry.0 += tokens
            }
        }
        for name in &segment.attachment_names {
            *attachments.entry(name.clone()).or_default() += segment.attachment_tokens;
        }
    }
    let summary_tokens = segments
        .iter()
        .filter(|segment| segment.role == "compaction_summary" || segment.origin == "projection")
        .filter_map(|segment| segment.token_estimate)
        .sum::<u64>();
    let raw_tokens = segments
        .iter()
        .filter(|segment| segment.role != "compaction_summary" && segment.origin != "projection")
        .filter_map(|segment| segment.token_estimate)
        .sum::<u64>();
    let category_specs = [
        (context_usage_kind::SYSTEM_DEVELOPER, "System / Developer"),
        (context_usage_kind::CAPABILITY_STATE, "Capability State"),
        (context_usage_kind::SYSTEM_TOOLS, "System Tools"),
        (context_usage_kind::MCP_TOOLS, "MCP Tools"),
        (context_usage_kind::AGENTS, "Agents"),
        (context_usage_kind::MEMORY, "Memory"),
        (context_usage_kind::SKILLS, "Skills"),
    ];
    let mut categories = category_specs
        .into_iter()
        .map(|(kind, label)| {
            let matching = items
                .iter()
                .filter(|item| item.kind.eq_ignore_ascii_case(kind))
                .collect::<Vec<_>>();
            let token_estimate = matching
                .iter()
                .filter(|item| !item.deferred)
                .map(|item| item.token_estimate)
                .sum();
            let deferred = !matching.is_empty() && matching.iter().all(|item| item.deferred);
            let mut sources = matching
                .iter()
                .map(|item| item.source.as_str())
                .collect::<Vec<_>>();
            sources.sort_unstable();
            sources.dedup();
            SessionContextUsageCategoryResponse {
                kind: kind.to_string(),
                label: label.to_string(),
                token_estimate,
                source: match sources.as_slice() {
                    [] => "projected".to_string(),
                    [source] => (*source).to_string(),
                    _ => "mixed".to_string(),
                },
                deferred,
            }
        })
        .collect::<Vec<_>>();
    categories.extend([
        SessionContextUsageCategoryResponse {
            kind: "messages".to_string(),
            label: "Messages".to_string(),
            token_estimate: raw_tokens.saturating_sub(messages.attachment_tokens),
            source: "local_estimate".to_string(),
            deferred: false,
        },
        SessionContextUsageCategoryResponse {
            kind: "attachments".to_string(),
            label: "Attachments".to_string(),
            token_estimate: messages.attachment_tokens,
            source: "local_estimate".to_string(),
            deferred: false,
        },
        SessionContextUsageCategoryResponse {
            kind: "compaction_summary".to_string(),
            label: "Compaction Summary".to_string(),
            token_estimate: summary_tokens.saturating_add(
                items
                    .iter()
                    .filter(|item| item.kind == "compaction_summary" && !item.deferred)
                    .map(|item| item.token_estimate)
                    .sum(),
            ),
            source: "projected".to_string(),
            deferred: false,
        },
    ]);
    let mut top_tools = tools
        .into_iter()
        .map(
            |(name, (call_tokens, result_tokens))| SessionToolContextContributionResponse {
                name,
                call_tokens,
                result_tokens,
            },
        )
        .collect::<Vec<_>>();
    top_tools.sort_by_key(|row| std::cmp::Reverse(row.call_tokens + row.result_tokens));
    top_tools.truncate(5);
    let mut top_attachments = attachments
        .into_iter()
        .map(|(name, tokens)| SessionAttachmentContextContributionResponse { name, tokens })
        .collect::<Vec<_>>();
    top_attachments.sort_by_key(|row| std::cmp::Reverse(row.tokens));
    top_attachments.truncate(5);
    SessionContextUsageAnalysisResponse {
        categories,
        items,
        messages,
        top_tools,
        top_attachments,
    }
}

fn bounded_preview(value: &str) -> String {
    value.chars().take(360).collect()
}

fn estimate_tokens(value: &str) -> u64 {
    if value.is_empty() {
        0
    } else {
        (value.chars().count() as u64).div_ceil(4).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_protocol::{
        ItemCompletedNotification, UserInputSource, UserInputSubmissionKind,
        UserInputSubmittedNotification, backbone::thread_item, text_user_input_blocks,
    };
    use agentdash_agent_runtime_contract::{
        EventSequence, ImmutablePresentationEvent, PresentationDurability, RuntimeCarrierMetadata,
        RuntimeJournalFact, RuntimeJournalRecord, RuntimePresentationCoordinate, RuntimeRevision,
        RuntimeThreadId,
    };

    fn journal_event(
        sequence: u64,
        entry_index: u32,
        event: BackboneEvent,
    ) -> AgentRunJournalEvent {
        let thread_id = RuntimeThreadId::new("thread-1").expect("runtime thread id");
        let record = RuntimeJournalRecord::new(
            RuntimeCarrierMetadata {
                thread_id: thread_id.clone(),
                recorded_at_ms: sequence,
                sequence: Some(EventSequence(sequence)),
                transient: None,
                revision: RuntimeRevision(sequence),
                operation_id: None,
                append_idempotency_key: None,
                binding_id: None,
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("thread-1".to_string()),
                    source_turn_id: Some("turn-1".to_string()),
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: Some(entry_index),
                },
            },
            RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                event,
            )),
        )
        .expect("journal record");
        AgentRunJournalEvent {
            journal_seq: sequence,
            segment_role: super::super::AgentRunJournalSegmentRole::CurrentDelivery,
            source_runtime_thread_id: thread_id,
            source_event_seq: Some(EventSequence(sequence)),
            record,
        }
    }

    #[test]
    fn full_typed_stream_groups_delta_and_tool_lifecycle_like_main() {
        let delta: codex::AgentMessageDeltaNotification = serde_json::from_value(serde_json::json!({
            "threadId": "thread-1", "turnId": "turn-1", "itemId": "assistant-1", "delta": "hello"
        })).expect("agent delta");
        let started = thread_item::dynamic_tool_call(
            "call-1",
            "read_file",
            serde_json::json!({"path": "a"}),
            codex::DynamicToolCallStatus::InProgress,
            None,
            None,
        );
        let completed = thread_item::dynamic_tool_call(
            "call-1",
            "read_file",
            serde_json::json!({"path": "a"}),
            codex::DynamicToolCallStatus::Completed,
            Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
                text: "done".to_string(),
            }]),
            Some(true),
        );
        let events = vec![
            journal_event(1, 0, BackboneEvent::AgentMessageDelta(delta)),
            journal_event(
                2,
                0,
                BackboneEvent::ItemStarted(agentdash_agent_protocol::ItemStartedNotification::new(
                    started, "thread-1", "turn-1",
                )),
            ),
            journal_event(
                3,
                0,
                BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                    completed, "thread-1", "turn-1",
                )),
            ),
        ];
        let segments = transcript_segments("session-1", &events);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].role, "assistant");
        assert_eq!(segments[0].preview, "hello");
        assert_eq!(segments[0].tool_names, vec!["read_file"]);
        assert_eq!(segments[1].role, "tool_result");
        assert_eq!(segments[1].preview, "done");
        assert_eq!(segments[1].tool_names, vec!["read_file"]);
    }

    #[test]
    fn typed_tool_projection_covers_main_command_file_mcp_and_native_names() {
        let command = AgentDashThreadItem::Codex(
            thread_item::command_execution(
                "command-1",
                "echo ok",
                ".",
                codex::CommandExecutionStatus::Completed,
                Some("ok".to_string()),
                Some(0),
            )
            .expect("command item"),
        );
        let file = AgentDashThreadItem::Codex(
            thread_item::file_change(
                "file-1",
                vec![thread_item::FileChangeSpec::Edit {
                    path: "src/main.rs".to_string(),
                    unified_diff: "@@\n-old\n+new".to_string(),
                }],
                codex::PatchApplyStatus::Completed,
            )
            .expect("file item"),
        );
        let mcp: AgentDashThreadItem = AgentDashThreadItem::Codex(
            serde_json::from_value(serde_json::json!({
                "type": "mcpToolCall", "id": "mcp-1", "server": "files", "tool": "read",
                "arguments": {"path": "a"}, "status": "completed",
                "result": {"content": [{"type": "text", "text": "mcp ok"}]}, "error": null
            }))
            .expect("mcp item"),
        );
        let native = AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ShellExec {
            id: "native-1".to_string(),
            command: "pwd".to_string(),
            cwd: None,
            execution_mode: agentdash_agent_protocol::ShellExecExecutionMode::Platform,
            arguments: serde_json::json!({"command": "pwd"}),
            status: codex::DynamicToolCallStatus::Completed,
            aggregated_output: Some("workspace".to_string()),
            exit_code: Some(0),
            success: Some(true),
        });
        let dynamic = AgentDashThreadItem::Codex(thread_item::dynamic_tool_call(
            "dynamic-1",
            "read_file",
            serde_json::json!({"path": "a"}),
            codex::DynamicToolCallStatus::Completed,
            Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
                text: "done".to_string(),
            }]),
            Some(true),
        ));

        let command = tool_item_parts(&command).expect("command projection");
        let file = tool_item_parts(&file).expect("file projection");
        let mcp = tool_item_parts(&mcp).expect("mcp projection");
        let dynamic = tool_item_parts(&dynamic).expect("dynamic projection");
        let native = tool_item_parts(&native).expect("native projection");
        type ToolProjection = (
            String,
            String,
            serde_json::Value,
            String,
            Option<serde_json::Value>,
            bool,
        );
        let output_kind = |value: &ToolProjection| {
            if value.3.starts_with("[restored tool output missing]") {
                "missing_placeholder"
            } else if value.4.is_some() {
                "structured_result"
            } else {
                "content"
            }
        };
        let row = |family: &str, value: &ToolProjection| {
            serde_json::json!({
                "family": family,
                "tool_name": value.1,
                "output_kind": output_kind(value),
                "details": value.4.is_some(),
                "is_error": value.5,
            })
        };
        let actual = serde_json::json!({
            "oracle": {
                "repository": "D:/Projects/AgentDash-main-reference",
                "commit": "957fa9d60ea3d67efa1bb278fe5b376cf0c34598",
                "sources": [
                    "crates/agentdash-application-runtime-session/src/session/transcript_restore.rs",
                    "crates/agentdash-application-runtime-session/src/session/context_usage_projection.rs"
                ]
            },
            "tool_family_projection": [
                row("command", &command), row("file", &file), row("mcp", &mcp),
                row("dynamic", &dynamic), row("native", &native)
            ]
        });
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/context_projection_main_957fa9d.json"
        ))
        .expect("Main projection golden");
        let expected = serde_json::json!({
            "oracle": fixture["oracle"].clone(),
            "tool_family_projection": fixture["tool_family_projection"].clone(),
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn typed_journal_projection_preserves_message_and_tool_shape() {
        let user = BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
            "thread-1",
            "turn-1",
            "user-1",
            UserInputSubmissionKind::Prompt,
            UserInputSource::core_composer(),
            text_user_input_blocks("hello"),
        ));
        let assistant_item: codex::ThreadItem = serde_json::from_value(serde_json::json!({
            "type": "agentMessage",
            "id": "assistant-1",
            "text": "world"
        }))
        .expect("typed assistant item");
        let assistant = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            assistant_item,
            "thread-1",
            "turn-1",
        ));
        let tool = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            thread_item::dynamic_tool_call(
                "call-1",
                "read_file",
                serde_json::json!({"path": "a"}),
                codex::DynamicToolCallStatus::Completed,
                Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
                    text: "done".to_string(),
                }]),
                Some(true),
            ),
            "thread-1",
            "turn-1",
        ));

        let mut segments =
            presentation_segments("session-1", 1, &user, Some("turn-1"), Some(0), 0, 0);
        segments.extend(presentation_segments(
            "session-1",
            2,
            &assistant,
            Some("turn-1"),
            Some(1),
            0,
            1,
        ));
        segments.extend(presentation_segments(
            "session-1",
            3,
            &tool,
            Some("turn-1"),
            Some(2),
            0,
            2,
        ));

        assert_eq!(
            serde_json::to_value(&segments).expect("segments serialize"),
            serde_json::json!([
                {
                    "id": "original_event:0", "sort_order": 0,
                    "segment_type": "original_event", "role": "user", "origin": "event",
                    "synthetic": false, "projection_kind": "model_context",
                    "message_ref": {"turn_id": "turn-1", "entry_index": 0},
                    "source_event_seq": 1, "preview": "hello", "token_estimate": 6,
                    "provenance": {"projection_version": 0}
                },
                {
                    "id": "original_event:1", "sort_order": 1,
                    "segment_type": "original_event", "role": "assistant", "origin": "event",
                    "synthetic": false, "projection_kind": "model_context",
                    "message_ref": {"turn_id": "turn-1", "entry_index": 1},
                    "source_event_seq": 2, "preview": "world", "token_estimate": 6,
                    "provenance": {"projection_version": 0}
                },
                {
                    "id": "original_event:2", "sort_order": 2,
                    "segment_type": "original_event", "role": "assistant", "origin": "event",
                    "synthetic": false, "projection_kind": "model_context",
                    "message_ref": {"turn_id": "turn-1", "entry_index": 2},
                    "source_event_seq": 3, "preview": "tool_calls: read_file", "token_estimate": 13,
                    "tool_names": ["read_file"], "provenance": {"projection_version": 0}
                },
                {
                    "id": "original_event:3", "sort_order": 3,
                    "segment_type": "original_event", "role": "tool_result", "origin": "event",
                    "synthetic": false, "projection_kind": "model_context",
                    "message_ref": {"turn_id": "turn-1", "entry_index": 2},
                    "source_event_seq": 3, "preview": "done", "token_estimate": 11,
                    "tool_names": ["read_file"], "provenance": {"projection_version": 0}
                }
            ])
        );

        let usage = usage_analysis(&segments, Vec::new());
        assert_eq!(
            serde_json::to_value(&usage.top_tools).expect("tools serialize"),
            serde_json::json!([{"name": "read_file", "call_tokens": 13, "result_tokens": 11}])
        );
    }

    #[test]
    fn complete_projection_deep_equals_main_golden() {
        let fact: ContextCompactedFact = serde_json::from_value(serde_json::json!({
            "lifecycle_item_id": "compact-1",
            "summary": "summary",
            "tokens_before": 100,
            "messages_compacted": 3,
            "compacted_until_ref": {"turn_id": "turn-0", "entry_index": 4},
            "projection_version": 2,
            "source_start_event_seq": 1,
            "source_end_event_seq": 8,
            "strategy": "summary_prefix",
            "trigger": "auto",
            "phase": "pre_provider"
        }))
        .expect("typed compaction fact");
        let segment = compaction_segment("session-1", 9, &fact, 2, None, None, 0)
            .expect("compaction segment");
        assert_eq!(
            serde_json::to_value(&segment).expect("segment serializes"),
            serde_json::json!({
                "id": "compaction-compact-1-summary", "sort_order": 0,
                "segment_type": "summary_chunk", "role": "compaction_summary",
                "origin": "projection", "synthetic": true, "projection_kind": "model_context",
                "message_ref": {"turn_id": "turn-0", "entry_index": 4},
                "source_event_seq": 9, "source_range": {"start_event_seq": 1, "end_event_seq": 8},
                "projection_segment_id": "compaction-compact-1-summary",
                "preview": "summary", "token_estimate": 6,
                "provenance": {"compaction_id": "compaction-compact-1", "projection_version": 2,
                    "segment_type": "compaction_summary", "strategy": "summary_prefix",
                    "trigger": "auto", "phase": "pre_provider"}
            })
        );

        let frame: ContextFrame = serde_json::from_value(serde_json::json!({
            "id": "frame-1", "kind": "system_guidelines", "source": "runtime_context_update",
            "delivery_status": "prepared_for_connector", "delivery_channel": "connector_context",
            "message_role": "system", "rendered_text": "", "created_at_ms": 1,
            "sections": [
                {"kind": "identity", "title": "Identity", "summary": "identity",
                    "fragments": [{"slot": "identity", "label": "prompt", "source": "connector",
                        "content": "system prompt"}]},
                {"kind": "tool_schema_delta", "added_tools": [{"name": "read_file",
                    "description": "Read file", "parameters_schema": {"type": "object"},
                    "context_usage_kind": "system_tools"}]}
            ]
        }))
        .expect("typed context frame");
        let items =
            context_usage_items_from_context_frame(&frame, Some(10), Some("turn-1".to_string()));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "system_developer");
        assert_eq!(items[1].kind, "system_tools");
        let usage = usage_analysis(&[segment.clone()], items);
        assert!(
            usage
                .categories
                .iter()
                .any(|row| row.kind == "system_developer" && row.token_estimate == 4)
        );
        assert!(
            usage
                .categories
                .iter()
                .any(|row| row.kind == "system_tools" && row.token_estimate > 0)
        );
        assert!(
            usage
                .categories
                .iter()
                .any(|row| row.kind == "compaction_summary" && row.token_estimate == 6)
        );

        let user = BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
            "thread-1",
            "turn-1",
            "user-image-1",
            UserInputSubmissionKind::Prompt,
            UserInputSource::core_composer(),
            vec![
                codex::UserInput::Text {
                    text: "hello".to_string(),
                    text_elements: Vec::new(),
                },
                codex::UserInput::Image {
                    detail: None,
                    url: "data:image/png;base64,QUJD".to_string(),
                },
            ],
        ));
        let assistant_item: codex::ThreadItem = serde_json::from_value(serde_json::json!({
            "type": "agentMessage", "id": "assistant-full-1", "text": "world"
        }))
        .expect("assistant item");
        let assistant = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            assistant_item,
            "thread-1",
            "turn-1",
        ));
        let tool = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            thread_item::dynamic_tool_call(
                "call-full-1",
                "read_file",
                serde_json::json!({"path": "a"}),
                codex::DynamicToolCallStatus::Completed,
                Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
                    text: "done".to_string(),
                }]),
                Some(true),
            ),
            "thread-1",
            "turn-1",
        ));
        let mut full_segments =
            presentation_segments("session-1", 9, &user, Some("turn-1"), Some(0), 2, 0);
        full_segments.extend(presentation_segments(
            "session-1",
            10,
            &assistant,
            Some("turn-1"),
            Some(1),
            2,
            1,
        ));
        full_segments.extend(presentation_segments(
            "session-1",
            11,
            &tool,
            Some("turn-1"),
            Some(2),
            2,
            2,
        ));
        full_segments.insert(0, segment);
        for (index, segment) in full_segments.iter_mut().enumerate() {
            segment.sort_order = index as u32;
        }
        let full_usage = usage_analysis(&full_segments, usage.items);
        let token_estimate = full_segments
            .iter()
            .filter_map(|segment| segment.token_estimate)
            .sum::<u64>()
            + full_usage
                .items
                .iter()
                .filter(|item| !item.deferred)
                .map(|item| item.token_estimate)
                .sum::<u64>();
        let full_view = SessionProjectionViewResponse {
            session_id: "session-1".to_string(),
            projection_kind: "model_context".to_string(),
            projection_version: 2,
            head_event_seq: 12,
            active_compaction_id: Some("compaction-compact-1".to_string()),
            token_estimate: Some(token_estimate),
            message_count: full_segments.len() as u64,
            segments: full_segments,
            context_usage: full_usage,
        };
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/context_projection_main_957fa9d.json"
        ))
        .expect("Main projection golden");
        assert_eq!(
            serde_json::to_value(full_view).expect("full projection response"),
            fixture["output"]
        );
    }

    #[test]
    fn compaction_archive_is_derived_from_the_immutable_presentation_journal() {
        let event = journal_event(
            9,
            4,
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "context_compacted".to_string(),
                value: serde_json::json!({
                    "lifecycle_item_id": "item-compact-1",
                    "summary": "Earlier conversation summary",
                    "tokens_before": 100,
                    "messages_compacted": 3,
                    "compaction_id": "compact-1",
                    "projection_version": 2,
                    "source_start_event_seq": 1,
                    "source_end_event_seq": 8,
                    "strategy": "summary_prefix",
                    "trigger": "auto",
                    "phase": "pre_provider"
                }),
            }),
        );

        assert_eq!(
            context_compaction_archives(vec![event]),
            vec![AgentRunContextCompactionArchive {
                compaction_id: "compact-1".to_string(),
                lifecycle_item_id: "item-compact-1".to_string(),
                projection_version: 2,
                completed_event_seq: 9,
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(8),
                summary: "Earlier conversation summary".to_string(),
                tokens_before: 100,
                messages_compacted: 3,
                trigger: Some("auto".to_string()),
                strategy: Some("summary_prefix".to_string()),
                phase: Some("pre_provider".to_string()),
                turn_id: Some("turn-1".to_string()),
                entry_index: Some(4),
            }]
        );
    }
}
