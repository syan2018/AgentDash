use agentdash_agent_types::{
    AgentContextEnvelope, AgentInputMessage, AgentMessage, ContentPart, MessageRef,
    ProjectionSourceRange, estimate_content_tokens, estimate_message_tokens,
};
use agentdash_spi::context_usage_kind;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, RuntimeCompanionAgentEntry, RuntimeMemoryInventoryMode,
    RuntimeMemorySourceEntry, RuntimeSkillEntry, RuntimeToolSchemaEntry,
};

const PROJECTION_PREVIEW_MAX_CHARS: usize = 360;
const TEXT_TOKEN_CHARS_PER_TOKEN: u64 = 4;

#[derive(Debug, Clone)]
pub struct SessionContextProjectionReadModel {
    pub session_id: String,
    pub projection_kind: String,
    pub projection_version: u64,
    pub head_event_seq: u64,
    pub active_compaction_id: Option<String>,
    pub token_estimate: Option<u64>,
    pub message_count: u64,
    pub segments: Vec<SessionProjectionSegmentReadModel>,
    pub context_usage: SessionContextUsageReadModel,
}

#[derive(Debug, Clone)]
pub struct SessionProjectionSourceRangeReadModel {
    pub start_event_seq: u64,
    pub end_event_seq: u64,
}

impl From<ProjectionSourceRange> for SessionProjectionSourceRangeReadModel {
    fn from(range: ProjectionSourceRange) -> Self {
        Self {
            start_event_seq: range.start_event_seq,
            end_event_seq: range.end_event_seq,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionProjectionMessageRefReadModel {
    pub turn_id: String,
    pub entry_index: u32,
}

impl From<MessageRef> for SessionProjectionMessageRefReadModel {
    fn from(value: MessageRef) -> Self {
        Self {
            turn_id: value.turn_id,
            entry_index: value.entry_index,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionProjectionSegmentProvenanceReadModel {
    pub compaction_id: Option<String>,
    pub projection_version: Option<u64>,
    pub segment_type: Option<String>,
    pub strategy: Option<String>,
    pub trigger: Option<String>,
    pub phase: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionProjectionSegmentReadModel {
    pub id: String,
    pub sort_order: u32,
    pub segment_type: String,
    pub role: String,
    pub origin: String,
    pub synthetic: bool,
    pub projection_kind: String,
    pub message_ref: SessionProjectionMessageRefReadModel,
    pub source_event_seq: Option<u64>,
    pub source_range: Option<SessionProjectionSourceRangeReadModel>,
    pub projection_segment_id: Option<String>,
    pub preview: String,
    pub token_estimate: Option<u64>,
    pub attachment_tokens: u64,
    pub attachment_names: Vec<String>,
    pub tool_names: Vec<String>,
    pub provenance: SessionProjectionSegmentProvenanceReadModel,
}

#[derive(Debug, Clone)]
pub struct SessionContextUsageCategory {
    pub kind: String,
    pub label: String,
    pub token_estimate: u64,
    pub source: String,
    pub deferred: bool,
}

#[derive(Debug, Clone)]
pub struct SessionContextUsageItem {
    pub kind: String,
    pub label: String,
    pub name: String,
    pub token_estimate: u64,
    pub source: String,
    pub deferred: bool,
    pub source_event_seq: Option<u64>,
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionMessageContextBreakdown {
    pub user_message_tokens: u64,
    pub assistant_message_tokens: u64,
    pub tool_call_tokens: u64,
    pub tool_result_tokens: u64,
    pub attachment_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct SessionToolContextContribution {
    pub name: String,
    pub call_tokens: u64,
    pub result_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct SessionAttachmentContextContribution {
    pub name: String,
    pub tokens: u64,
}

#[derive(Debug, Clone)]
pub struct SessionContextUsageReadModel {
    pub categories: Vec<SessionContextUsageCategory>,
    pub items: Vec<SessionContextUsageItem>,
    pub messages: SessionMessageContextBreakdown,
    pub top_tools: Vec<SessionToolContextContribution>,
    pub top_attachments: Vec<SessionAttachmentContextContribution>,
}

pub fn build_session_context_projection_read_model(
    envelope: AgentContextEnvelope,
    context_items: Vec<SessionContextUsageItem>,
) -> SessionContextProjectionReadModel {
    let message_count = u64::try_from(envelope.messages.len()).unwrap_or(u64::MAX);
    let segments: Vec<_> = envelope
        .messages
        .into_iter()
        .enumerate()
        .map(|(index, message)| projection_segment_from_message(index, message))
        .collect();
    let context_item_token_estimate = context_items_token_estimate(&context_items);
    let context_usage = context_usage_analysis(&segments, context_items);
    let token_estimate = envelope
        .token_estimate
        .map(|tokens| tokens.saturating_add(context_item_token_estimate));
    SessionContextProjectionReadModel {
        session_id: envelope.session_id,
        projection_kind: envelope.projection_kind.as_str().to_string(),
        projection_version: envelope.projection_version,
        head_event_seq: envelope.head_event_seq,
        active_compaction_id: envelope.active_compaction_id,
        token_estimate,
        message_count,
        segments,
        context_usage,
    }
}

fn projection_segment_from_message(
    index: usize,
    message: AgentInputMessage,
) -> SessionProjectionSegmentReadModel {
    let provenance = projection_provenance(&message.provenance);
    let segment_type =
        provenance
            .segment_type
            .clone()
            .unwrap_or_else(|| match message.origin.as_str() {
                "projection" => "projection_segment".to_string(),
                _ => "original_event".to_string(),
            });
    let id = message
        .projection_segment_id
        .clone()
        .unwrap_or_else(|| format!("{}:{}", segment_type, index));
    let sort_order = u32::try_from(index).unwrap_or(u32::MAX);
    let role = message_role(&message.message).to_string();
    let preview = message_preview(&message.message);
    let token_estimate = Some(estimate_message_tokens(&message.message));
    let tool_names = message_tool_names(&message.message);
    let attachment_tokens = message_attachment_tokens(&message.message);
    let attachment_names = message_attachment_names(&message.message);
    SessionProjectionSegmentReadModel {
        id,
        sort_order,
        segment_type,
        role,
        origin: message.origin.as_str().to_string(),
        synthetic: message.synthetic,
        projection_kind: message.projection_kind.as_str().to_string(),
        message_ref: message.message_ref.into(),
        source_event_seq: message.source_event_seq,
        source_range: message.source_range.map(Into::into),
        projection_segment_id: message.projection_segment_id,
        preview,
        token_estimate,
        attachment_tokens,
        attachment_names,
        tool_names,
        provenance,
    }
}

fn projection_provenance(value: &serde_json::Value) -> SessionProjectionSegmentProvenanceReadModel {
    SessionProjectionSegmentProvenanceReadModel {
        compaction_id: read_string(value, "compaction_id"),
        projection_version: read_u64(value, "projection_version"),
        segment_type: read_string(value, "segment_type"),
        strategy: read_string(value, "strategy"),
        trigger: read_string(value, "trigger"),
        phase: read_string(value, "phase"),
    }
}

fn read_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn read_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(serde_json::Value::as_u64)
}

fn message_role(message: &AgentMessage) -> &'static str {
    match message {
        AgentMessage::User { .. } => "user",
        AgentMessage::Assistant { .. } => "assistant",
        AgentMessage::ToolResult { .. } => "tool_result",
        AgentMessage::CompactionSummary { .. } => "compaction_summary",
    }
}

fn message_tool_names(message: &AgentMessage) -> Vec<String> {
    match message {
        AgentMessage::Assistant { tool_calls, .. } => tool_calls
            .iter()
            .map(|call| call.name.trim())
            .filter(|name| !name.is_empty())
            .map(ToString::to_string)
            .collect(),
        AgentMessage::ToolResult { tool_name, .. } => tool_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| vec![name.to_string()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn message_attachment_tokens(message: &AgentMessage) -> u64 {
    let content = match message {
        AgentMessage::User { content, .. }
        | AgentMessage::Assistant { content, .. }
        | AgentMessage::ToolResult { content, .. } => content,
        AgentMessage::CompactionSummary { .. } => return 0,
    };
    content
        .iter()
        .filter(|part| matches!(part, ContentPart::Image { .. }))
        .map(|part| estimate_content_tokens(std::slice::from_ref(part)))
        .fold(0_u64, u64::saturating_add)
}

fn message_attachment_names(message: &AgentMessage) -> Vec<String> {
    let content = match message {
        AgentMessage::User { content, .. }
        | AgentMessage::Assistant { content, .. }
        | AgentMessage::ToolResult { content, .. } => content,
        AgentMessage::CompactionSummary { .. } => return Vec::new(),
    };
    content
        .iter()
        .enumerate()
        .filter_map(|(index, part)| match part {
            ContentPart::Image { mime_type, .. } => Some(format!("{mime_type} image #{index}")),
            _ => None,
        })
        .collect()
}

fn context_usage_analysis(
    segments: &[SessionProjectionSegmentReadModel],
    context_items: Vec<SessionContextUsageItem>,
) -> SessionContextUsageReadModel {
    let summary_tokens = sum_segment_tokens(segments, |segment| {
        segment.role == "compaction_summary" || segment.origin == "projection"
    });
    let raw_message_tokens = sum_segment_tokens(segments, |segment| {
        segment.role != "compaction_summary" && segment.origin != "projection"
    });
    let attachment_tokens = segments
        .iter()
        .map(|segment| segment.attachment_tokens)
        .fold(0_u64, u64::saturating_add);
    let message_tokens = raw_message_tokens.saturating_sub(attachment_tokens);
    let compaction_item_refs = context_items
        .iter()
        .filter(|item| item.kind == "compaction_summary")
        .collect::<Vec<_>>();
    let compaction_item_tokens = compaction_item_refs
        .iter()
        .filter(|item| !item.deferred)
        .map(|item| item.token_estimate)
        .fold(0_u64, u64::saturating_add);
    let compaction_source = if compaction_item_refs.is_empty() {
        "projected".to_string()
    } else if summary_tokens > 0 {
        "mixed".to_string()
    } else {
        context_item_category_source(&compaction_item_refs)
    };
    let mut categories = context_item_categories(&context_items);
    categories.extend([
        context_category(
            "messages",
            "Messages",
            message_tokens,
            "local_estimate",
            false,
        ),
        context_category(
            "attachments",
            "Attachments",
            attachment_tokens,
            "local_estimate",
            false,
        ),
        context_category(
            "compaction_summary",
            "Compaction Summary",
            summary_tokens.saturating_add(compaction_item_tokens),
            &compaction_source,
            false,
        ),
    ]);
    SessionContextUsageReadModel {
        categories,
        items: context_items,
        messages: message_context_breakdown(segments),
        top_tools: top_tools(segments),
        top_attachments: top_attachments(segments),
    }
}

fn context_item_categories(items: &[SessionContextUsageItem]) -> Vec<SessionContextUsageCategory> {
    [
        (context_usage_kind::SYSTEM_DEVELOPER, "System / Developer"),
        (context_usage_kind::CAPABILITY_STATE, "Capability State"),
        (context_usage_kind::SYSTEM_TOOLS, "System Tools"),
        (context_usage_kind::MCP_TOOLS, "MCP Tools"),
        (context_usage_kind::AGENTS, "Agents"),
        ("memory", "Memory"),
        (context_usage_kind::SKILLS, "Skills"),
    ]
    .into_iter()
    .map(|(kind, label)| {
        let category_items = items
            .iter()
            .filter(|item| item.kind == kind)
            .collect::<Vec<_>>();
        let token_estimate = category_items
            .iter()
            .filter(|item| !item.deferred)
            .map(|item| item.token_estimate)
            .fold(0_u64, u64::saturating_add);
        let source = context_item_category_source(&category_items);
        let deferred =
            !category_items.is_empty() && category_items.iter().all(|item| item.deferred);
        context_category(kind, label, token_estimate, &source, deferred)
    })
    .collect()
}

fn context_items_token_estimate(items: &[SessionContextUsageItem]) -> u64 {
    items
        .iter()
        .filter(|item| !item.deferred)
        .map(|item| item.token_estimate)
        .fold(0_u64, u64::saturating_add)
}

fn context_item_category_source(items: &[&SessionContextUsageItem]) -> String {
    let mut sources = items
        .iter()
        .map(|item| item.source.as_str())
        .filter(|source| !source.is_empty())
        .collect::<Vec<_>>();
    sources.sort_unstable();
    sources.dedup();
    match sources.as_slice() {
        [] => "projected".to_string(),
        [source] => (*source).to_string(),
        _ => "mixed".to_string(),
    }
}

fn context_category(
    kind: &str,
    label: &str,
    token_estimate: u64,
    source: &str,
    deferred: bool,
) -> SessionContextUsageCategory {
    SessionContextUsageCategory {
        kind: kind.to_string(),
        label: label.to_string(),
        token_estimate,
        source: source.to_string(),
        deferred,
    }
}

pub fn context_usage_items_from_context_frame(
    frame: &ContextFrame,
    source_event_seq: Option<u64>,
    turn_id: Option<String>,
) -> Vec<SessionContextUsageItem> {
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
) -> Vec<SessionContextUsageItem> {
    let trace = ContextUsageItemTrace {
        source_event_seq,
        turn_id: turn_id.clone(),
    };
    match section {
        ContextFrameSection::Identity {
            title,
            effective_prompt,
            ..
        } => vec![context_usage_item(
            context_usage_kind::SYSTEM_DEVELOPER,
            "System / Developer",
            title,
            effective_prompt,
            "context_frame",
            false,
            &trace,
        )],
        ContextFrameSection::AssignmentContext {
            title, fragments, ..
        } => {
            let agent_text = fragments
                .iter()
                .filter(|fragment| {
                    usage_kind_matches(
                        fragment.context_usage_kind.as_deref(),
                        context_usage_kind::AGENTS,
                    )
                })
                .map(|fragment| fragment.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let system_text = fragments
                .iter()
                .filter(|fragment| {
                    usage_kind_matches(
                        fragment.context_usage_kind.as_deref(),
                        context_usage_kind::SYSTEM_DEVELOPER,
                    )
                })
                .map(|fragment| fragment.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let mut items = Vec::new();
            if !system_text.trim().is_empty() {
                items.push(context_usage_item(
                    context_usage_kind::SYSTEM_DEVELOPER,
                    "System / Developer",
                    title,
                    &system_text,
                    "context_frame",
                    false,
                    &trace,
                ));
            }
            if !agent_text.trim().is_empty() {
                items.push(context_usage_item(
                    context_usage_kind::AGENTS,
                    "Agents",
                    "Companion Agents",
                    &agent_text,
                    "context_frame",
                    false,
                    &trace,
                ));
            }
            items
        }
        ContextFrameSection::SystemNotice {
            title,
            summary,
            body,
        } => vec![context_usage_item(
            context_usage_kind::SYSTEM_DEVELOPER,
            "System / Developer",
            title,
            body.as_deref().unwrap_or(summary),
            "context_frame",
            false,
            &trace,
        )],
        ContextFrameSection::PendingAction {
            title,
            instructions,
            injections,
            ..
        } => {
            let mut text_parts = instructions.iter().map(String::as_str).collect::<Vec<_>>();
            text_parts.extend(
                injections
                    .iter()
                    .filter(|injection| {
                        usage_kind_matches(
                            injection.context_usage_kind.as_deref(),
                            context_usage_kind::SYSTEM_DEVELOPER,
                        )
                    })
                    .map(|injection| injection.content.as_str()),
            );
            let mut items = vec![context_usage_item(
                context_usage_kind::SYSTEM_DEVELOPER,
                "System / Developer",
                title,
                &text_parts.join("\n\n"),
                "context_frame",
                false,
                &trace,
            )];
            let agent_text = injections
                .iter()
                .filter(|injection| {
                    usage_kind_matches(
                        injection.context_usage_kind.as_deref(),
                        context_usage_kind::AGENTS,
                    )
                })
                .map(|injection| injection.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            if !agent_text.trim().is_empty() {
                items.push(context_usage_item(
                    context_usage_kind::AGENTS,
                    "Agents",
                    "Companion Agents",
                    &agent_text,
                    "context_frame",
                    false,
                    &trace,
                ));
            }
            items
        }
        ContextFrameSection::AutoResume { title, prompt, .. } => vec![context_usage_item(
            context_usage_kind::SYSTEM_DEVELOPER,
            "System / Developer",
            title,
            prompt,
            "context_frame",
            false,
            &trace,
        )],
        ContextFrameSection::UserPreferences { title, items, .. } => vec![context_usage_item(
            "memory",
            "Memory",
            title,
            &items.join("\n"),
            "context_frame",
            false,
            &trace,
        )],
        ContextFrameSection::ProjectGuidelines { title, entries, .. } => {
            let text = entries
                .iter()
                .map(|entry| format!("{}\n{}", entry.path, entry.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            vec![context_usage_item(
                "memory",
                "Memory",
                title,
                &text,
                "context_frame",
                false,
                &trace,
            )]
        }
        ContextFrameSection::CapabilityKeyDelta {
            added_capabilities,
            removed_capabilities,
            effective_capabilities,
        } => capability_state_usage_items(
            "Capability Keys",
            [
                ("added_capabilities", added_capabilities.as_slice()),
                ("removed_capabilities", removed_capabilities.as_slice()),
                ("effective_capabilities", effective_capabilities.as_slice()),
            ],
            source_event_seq,
            turn_id,
        ),
        ContextFrameSection::ToolPathDelta {
            blocked_tool_paths,
            unblocked_tool_paths,
            whitelisted_tool_paths,
            removed_whitelist_paths,
        } => capability_state_usage_items(
            "Tool Path Delta",
            [
                ("blocked_tool_paths", blocked_tool_paths.as_slice()),
                ("unblocked_tool_paths", unblocked_tool_paths.as_slice()),
                ("whitelisted_tool_paths", whitelisted_tool_paths.as_slice()),
                (
                    "removed_whitelist_paths",
                    removed_whitelist_paths.as_slice(),
                ),
            ],
            source_event_seq,
            turn_id,
        ),
        ContextFrameSection::McpServerDelta {
            added_mcp_servers,
            removed_mcp_servers,
            changed_mcp_servers,
        } => capability_state_usage_items(
            "MCP Server Delta",
            [
                ("added_mcp_servers", added_mcp_servers.as_slice()),
                ("removed_mcp_servers", removed_mcp_servers.as_slice()),
                ("changed_mcp_servers", changed_mcp_servers.as_slice()),
            ],
            source_event_seq,
            turn_id,
        ),
        ContextFrameSection::VfsDelta {
            vfs_mounts_added,
            vfs_mounts_removed,
            default_mount_before,
            default_mount_after,
        } => {
            let mut text = named_values_text([
                ("vfs_mounts_added", vfs_mounts_added.as_slice()),
                ("vfs_mounts_removed", vfs_mounts_removed.as_slice()),
            ]);
            if default_mount_before != default_mount_after {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&format!(
                    "default_mount: {} -> {}",
                    default_mount_before.as_deref().unwrap_or("none"),
                    default_mount_after.as_deref().unwrap_or("none")
                ));
            }
            non_empty_usage_item(
                context_usage_kind::CAPABILITY_STATE,
                "Capability State",
                "VFS Delta",
                &text,
                "capability_state",
                source_event_seq,
                turn_id,
            )
            .into_iter()
            .collect()
        }
        ContextFrameSection::ToolSchemaDelta { added_tools } => added_tools
            .iter()
            .filter_map(|tool| tool_schema_usage_item(tool, source_event_seq, turn_id))
            .collect(),
        ContextFrameSection::SkillDelta {
            added_skills,
            removed_skills,
            changed_skills,
        } => added_skills
            .iter()
            .chain(removed_skills.iter())
            .chain(changed_skills.iter())
            .filter_map(|skill| skill_usage_item(skill, source_event_seq, turn_id))
            .collect(),
        ContextFrameSection::MemoryInventory {
            mode,
            sources,
            added_sources,
            removed_sources,
            changed_sources,
            ..
        } => {
            let visible_sources = match mode {
                RuntimeMemoryInventoryMode::Snapshot => {
                    sources.iter().collect::<Vec<&RuntimeMemorySourceEntry>>()
                }
                RuntimeMemoryInventoryMode::Delta => added_sources
                    .iter()
                    .chain(removed_sources.iter())
                    .chain(changed_sources.iter())
                    .collect::<Vec<&RuntimeMemorySourceEntry>>(),
            };
            visible_sources
                .into_iter()
                .filter_map(|source| memory_source_usage_item(source, source_event_seq, turn_id))
                .collect()
        }
        ContextFrameSection::CompanionAgentRosterDelta {
            effective_agents, ..
        } => effective_agents
            .iter()
            .filter_map(|agent| companion_agent_usage_item(agent, source_event_seq, turn_id))
            .collect(),
        ContextFrameSection::CompactionSummary {
            title,
            summary,
            tokens_before,
            messages_compacted,
            compaction_id,
            projection_version,
            strategy,
            trigger,
            phase,
            source_start_event_seq,
            source_end_event_seq,
            first_kept_event_seq,
            compacted_until_ref,
            timestamp_ms,
        } => {
            let text = compaction_summary_usage_text(
                summary,
                *tokens_before,
                *messages_compacted,
                compaction_id.as_deref(),
                *projection_version,
                strategy.as_deref(),
                trigger.as_deref(),
                phase.as_deref(),
                *source_start_event_seq,
                *source_end_event_seq,
                *first_kept_event_seq,
                compacted_until_ref.as_ref(),
                *timestamp_ms,
            );
            vec![context_usage_item(
                "compaction_summary",
                "Compaction Summary",
                title,
                &text,
                "context_frame",
                false,
                &trace,
            )]
        }
    }
}

fn capability_state_usage_items<const N: usize>(
    label: &str,
    values: [(&str, &[String]); N],
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> Vec<SessionContextUsageItem> {
    let text = named_values_text(values);
    non_empty_usage_item(
        context_usage_kind::CAPABILITY_STATE,
        "Capability State",
        label,
        &text,
        "capability_state",
        source_event_seq,
        turn_id,
    )
    .into_iter()
    .collect()
}

fn named_values_text<const N: usize>(values: [(&str, &[String]); N]) -> String {
    let mut lines = Vec::new();
    for (name, entries) in values {
        if entries.is_empty() {
            continue;
        }
        lines.push(format!("{name}:"));
        lines.extend(entries.iter().map(|entry| format!("- {entry}")));
    }
    lines.join("\n")
}

fn non_empty_usage_item(
    kind: &str,
    label: &str,
    name: &str,
    text: &str,
    source: &str,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> Option<SessionContextUsageItem> {
    (!text.trim().is_empty()).then(|| {
        let trace = ContextUsageItemTrace {
            source_event_seq,
            turn_id: turn_id.clone(),
        };
        context_usage_item(kind, label, name, text, source, false, &trace)
    })
}

#[allow(clippy::too_many_arguments)]
fn compaction_summary_usage_text(
    summary: &str,
    tokens_before: u64,
    messages_compacted: u32,
    compaction_id: Option<&str>,
    projection_version: Option<u64>,
    strategy: Option<&str>,
    trigger: Option<&str>,
    phase: Option<&str>,
    source_start_event_seq: Option<u64>,
    source_end_event_seq: Option<u64>,
    first_kept_event_seq: Option<u64>,
    compacted_until_ref: Option<&serde_json::Value>,
    timestamp_ms: Option<u64>,
) -> String {
    let mut lines = vec![
        format!("messages_compacted: {messages_compacted}"),
        format!("tokens_before: {tokens_before}"),
    ];
    if let Some(value) = timestamp_ms {
        lines.push(format!("timestamp_ms: {value}"));
    }
    if let Some(value) = compaction_id {
        lines.push(format!("compaction_id: {value}"));
    }
    if let Some(value) = projection_version {
        lines.push(format!("projection_version: {value}"));
    }
    if let Some(value) = strategy {
        lines.push(format!("strategy: {value}"));
    }
    if let Some(value) = trigger {
        lines.push(format!("trigger: {value}"));
    }
    if let Some(value) = phase {
        lines.push(format!("phase: {value}"));
    }
    if let Some(value) = source_start_event_seq {
        lines.push(format!("source_start_event_seq: {value}"));
    }
    if let Some(value) = source_end_event_seq {
        lines.push(format!("source_end_event_seq: {value}"));
    }
    if let Some(value) = first_kept_event_seq {
        lines.push(format!("first_kept_event_seq: {value}"));
    }
    if let Some(value) = compacted_until_ref {
        lines.push(format!("compacted_until_ref: {value}"));
    }
    lines.push(String::new());
    lines.push(summary.to_string());
    lines.join("\n")
}

fn tool_schema_usage_item(
    tool: &RuntimeToolSchemaEntry,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> Option<SessionContextUsageItem> {
    let kind = tool.context_usage_kind.as_deref()?;
    let label = tool_usage_label(kind)?;
    let mut text = format!("{}\n{}", tool.name, tool.description);
    if let Some(capability_key) = tool.capability_key.as_deref() {
        text.push('\n');
        text.push_str(capability_key);
    }
    if let Some(tool_path) = tool.tool_path.as_deref() {
        text.push('\n');
        text.push_str(tool_path);
    }
    text.push('\n');
    text.push_str(&tool.parameters_schema.to_string());
    let trace = ContextUsageItemTrace {
        source_event_seq,
        turn_id: turn_id.clone(),
    };
    Some(context_usage_item(
        kind,
        label,
        &tool.name,
        &text,
        "tool_schema",
        false,
        &trace,
    ))
}

fn skill_usage_item(
    skill: &RuntimeSkillEntry,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> Option<SessionContextUsageItem> {
    let name = skill
        .display_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&skill.name);
    let text = [
        name,
        skill.description.as_str(),
        skill.file_path.as_str(),
        skill.capability_key.as_str(),
        skill.provider_key.as_str(),
    ]
    .join("\n");
    let kind = skill.context_usage_kind.as_deref()?;
    let label = skill_usage_label(kind)?;
    let trace = ContextUsageItemTrace {
        source_event_seq,
        turn_id: turn_id.clone(),
    };
    Some(context_usage_item(
        kind,
        label,
        name,
        &text,
        "skill_registry",
        skill.disable_model_invocation,
        &trace,
    ))
}

fn memory_source_usage_item(
    source: &RuntimeMemorySourceEntry,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> Option<SessionContextUsageItem> {
    let kind = source.context_usage_kind.as_deref()?;
    if !usage_kind_matches(Some(kind), context_usage_kind::MEMORY) {
        return None;
    }
    let name = if source.display_name.trim().is_empty() {
        source.source_key.as_str()
    } else {
        source.display_name.as_str()
    };
    let text = [
        name,
        source.provider_key.as_str(),
        source.source_key.as_str(),
        source.source_uri.as_str(),
        source.index_uri.as_str(),
        source.scope.as_str(),
        source.index_status.as_str(),
        source.revision.as_str(),
    ]
    .join("\n");
    let trace = ContextUsageItemTrace {
        source_event_seq,
        turn_id: turn_id.clone(),
    };
    Some(context_usage_item(
        context_usage_kind::MEMORY,
        "Memory",
        name,
        &text,
        "memory_inventory",
        false,
        &trace,
    ))
}

fn companion_agent_usage_item(
    agent: &RuntimeCompanionAgentEntry,
    source_event_seq: Option<u64>,
    turn_id: &Option<String>,
) -> Option<SessionContextUsageItem> {
    let kind = agent.context_usage_kind.as_deref()?;
    if !usage_kind_matches(Some(kind), context_usage_kind::AGENTS) {
        return None;
    }
    let name = if agent.display_name.trim().is_empty() {
        agent.agent_key.as_str()
    } else {
        agent.display_name.as_str()
    };
    let text = [
        agent.agent_key.as_str(),
        agent.executor.as_str(),
        agent.display_name.as_str(),
    ]
    .join("\n");
    let trace = ContextUsageItemTrace {
        source_event_seq,
        turn_id: turn_id.clone(),
    };
    Some(context_usage_item(
        context_usage_kind::AGENTS,
        "Agents",
        name,
        &text,
        "capability_state",
        false,
        &trace,
    ))
}

fn usage_kind_matches(value: Option<&str>, expected: &str) -> bool {
    value
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn tool_usage_label(kind: &str) -> Option<&'static str> {
    if usage_kind_matches(Some(kind), context_usage_kind::MCP_TOOLS) {
        return Some("MCP Tools");
    }
    if usage_kind_matches(Some(kind), context_usage_kind::SYSTEM_TOOLS) {
        return Some("System Tools");
    }
    None
}

fn skill_usage_label(kind: &str) -> Option<&'static str> {
    if usage_kind_matches(Some(kind), context_usage_kind::SKILLS) {
        return Some("Skills");
    }
    None
}

#[derive(Debug, Clone)]
struct ContextUsageItemTrace {
    source_event_seq: Option<u64>,
    turn_id: Option<String>,
}

fn context_usage_item(
    kind: &str,
    label: &str,
    name: &str,
    text: &str,
    source: &str,
    deferred: bool,
    trace: &ContextUsageItemTrace,
) -> SessionContextUsageItem {
    SessionContextUsageItem {
        kind: kind.to_string(),
        label: label.to_string(),
        name: name.trim().to_string(),
        token_estimate: estimate_text_tokens(text),
        source: source.to_string(),
        deferred,
        source_event_seq: trace.source_event_seq,
        turn_id: trace.turn_id.clone(),
    }
}

fn estimate_text_tokens(text: &str) -> u64 {
    let text = text.trim();
    if text.is_empty() {
        return 0;
    }
    let chars = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
    chars
        .saturating_add(TEXT_TOKEN_CHARS_PER_TOKEN - 1)
        .saturating_div(TEXT_TOKEN_CHARS_PER_TOKEN)
        .max(1)
}

fn sum_segment_tokens(
    segments: &[SessionProjectionSegmentReadModel],
    predicate: impl Fn(&SessionProjectionSegmentReadModel) -> bool,
) -> u64 {
    segments
        .iter()
        .filter(|segment| predicate(segment))
        .filter_map(|segment| segment.token_estimate)
        .fold(0_u64, u64::saturating_add)
}

fn message_context_breakdown(
    segments: &[SessionProjectionSegmentReadModel],
) -> SessionMessageContextBreakdown {
    SessionMessageContextBreakdown {
        user_message_tokens: sum_segment_tokens(segments, |segment| segment.role == "user"),
        assistant_message_tokens: sum_segment_tokens(segments, |segment| {
            segment.role == "assistant"
        }),
        tool_call_tokens: sum_tool_call_tokens(segments),
        tool_result_tokens: sum_segment_tokens(segments, |segment| segment.role == "tool_result"),
        attachment_tokens: segments
            .iter()
            .map(|segment| segment.attachment_tokens)
            .fold(0_u64, u64::saturating_add),
    }
}

fn sum_tool_call_tokens(segments: &[SessionProjectionSegmentReadModel]) -> u64 {
    segments
        .iter()
        .filter(|segment| segment.role == "assistant" && !segment.tool_names.is_empty())
        .filter_map(|segment| segment.token_estimate)
        .fold(0_u64, u64::saturating_add)
}

fn top_tools(
    segments: &[SessionProjectionSegmentReadModel],
) -> Vec<SessionToolContextContribution> {
    let mut values: Vec<SessionToolContextContribution> = Vec::new();
    for segment in segments {
        let Some(tokens) = segment.token_estimate else {
            continue;
        };
        for name in &segment.tool_names {
            let Some(row) = values.iter_mut().find(|row| row.name == *name) else {
                values.push(SessionToolContextContribution {
                    name: name.clone(),
                    call_tokens: if segment.role == "assistant" {
                        tokens
                    } else {
                        0
                    },
                    result_tokens: if segment.role == "tool_result" {
                        tokens
                    } else {
                        0
                    },
                });
                continue;
            };
            if segment.role == "assistant" {
                row.call_tokens = row.call_tokens.saturating_add(tokens);
            } else if segment.role == "tool_result" {
                row.result_tokens = row.result_tokens.saturating_add(tokens);
            }
        }
    }
    values.sort_by_key(|row| std::cmp::Reverse(row.call_tokens.saturating_add(row.result_tokens)));
    values.truncate(5);
    values
}

fn top_attachments(
    segments: &[SessionProjectionSegmentReadModel],
) -> Vec<SessionAttachmentContextContribution> {
    let mut values = Vec::new();
    for segment in segments {
        for name in &segment.attachment_names {
            values.push(SessionAttachmentContextContribution {
                name: name.clone(),
                tokens: segment.attachment_tokens,
            });
        }
    }
    values.sort_by_key(|row| std::cmp::Reverse(row.tokens));
    values.truncate(5);
    values
}

fn message_preview(message: &AgentMessage) -> String {
    let text = message
        .first_text()
        .map(ToString::to_string)
        .or_else(|| assistant_tool_call_preview(message))
        .unwrap_or_else(|| message_role(message).to_string());
    truncate_preview(&text)
}

fn assistant_tool_call_preview(message: &AgentMessage) -> Option<String> {
    let AgentMessage::Assistant { tool_calls, .. } = message else {
        return None;
    };
    if tool_calls.is_empty() {
        return None;
    }
    let names = tool_calls
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("tool_calls: {names}"))
}

fn truncate_preview(value: &str) -> String {
    let mut chars = value.chars();
    let preview = chars
        .by_ref()
        .take(PROJECTION_PREVIEW_MAX_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_types::{
        AgentContextEnvelope, AgentInputMessage, AgentMessage, ContentPart, MessageRef,
        ProjectionKind, ProjectionOrigin, ProjectionSourceRange,
    };
    use agentdash_spi::hooks::{
        ContextFrame, ContextFrameSection, RuntimeCompanionAgentEntry, RuntimeContextFragmentEntry,
        RuntimeEventSource, RuntimeSkillEntry, RuntimeToolSchemaEntry,
    };

    use super::*;

    #[test]
    fn projection_view_marks_summary_as_synthetic_projection() {
        let envelope = AgentContextEnvelope {
            session_id: "sess-1".to_string(),
            projection_kind: ProjectionKind::ModelContext,
            projection_version: 2,
            head_event_seq: 42,
            active_compaction_id: Some("compaction-1".to_string()),
            token_estimate: Some(128),
            messages: vec![AgentInputMessage {
                message_ref: MessageRef {
                    turn_id: "_projection:summary".to_string(),
                    entry_index: 0,
                },
                projection_kind: ProjectionKind::CompactionSummary,
                message: AgentMessage::compaction_summary("压缩后的历史摘要", 48000, 12),
                origin: ProjectionOrigin::Projection,
                synthetic: true,
                source_event_seq: None,
                source_range: Some(ProjectionSourceRange {
                    start_event_seq: 1,
                    end_event_seq: 30,
                }),
                projection_segment_id: Some("segment-1".to_string()),
                provenance: serde_json::json!({
                    "compaction_id": "compaction-1",
                    "projection_version": 2,
                    "segment_type": "summary_chunk",
                    "strategy": "summary_prefix",
                    "trigger": "auto",
                    "phase": "pre_provider"
                }),
            }],
        };

        let view = build_session_context_projection_read_model(envelope, Vec::new());

        assert_eq!(view.projection_kind, "model_context");
        assert_eq!(view.projection_version, 2);
        assert_eq!(view.message_count, 1);
        assert_eq!(view.segments[0].origin, "projection");
        assert!(view.segments[0].synthetic);
        assert_eq!(view.segments[0].segment_type, "summary_chunk");
        assert!(view.segments[0].token_estimate.is_some());
        assert!(
            view.context_usage
                .categories
                .iter()
                .any(|category| category.kind == "compaction_summary")
        );
        assert_eq!(view.context_usage.messages.user_message_tokens, 0);
        assert_eq!(
            view.segments[0].provenance.compaction_id.as_deref(),
            Some("compaction-1")
        );
    }

    #[test]
    fn projection_view_reports_attachment_breakdown_from_image_parts() {
        let envelope = AgentContextEnvelope {
            session_id: "sess-1".to_string(),
            projection_kind: ProjectionKind::ModelContext,
            projection_version: 0,
            head_event_seq: 1,
            active_compaction_id: None,
            token_estimate: None,
            messages: vec![AgentInputMessage {
                message_ref: MessageRef {
                    turn_id: "turn-1".to_string(),
                    entry_index: 0,
                },
                projection_kind: ProjectionKind::ModelContext,
                message: AgentMessage::User {
                    content: vec![
                        ContentPart::text("看这张图"),
                        ContentPart::Image {
                            mime_type: "image/png".to_string(),
                            data: "AAECAw==".to_string(),
                        },
                    ],
                    timestamp: None,
                },
                origin: ProjectionOrigin::Event,
                synthetic: false,
                source_event_seq: Some(1),
                source_range: None,
                projection_segment_id: None,
                provenance: serde_json::Value::Null,
            }],
        };

        let view = build_session_context_projection_read_model(envelope, Vec::new());

        assert!(view.context_usage.messages.attachment_tokens > 0);
        let messages_category = view
            .context_usage
            .categories
            .iter()
            .find(|category| category.kind == "messages")
            .expect("messages category should exist");
        assert_eq!(
            messages_category.token_estimate,
            view.segments[0]
                .token_estimate
                .expect("segment token estimate")
                .saturating_sub(view.context_usage.messages.attachment_tokens)
        );
        assert_eq!(view.context_usage.top_attachments.len(), 1);
        assert!(
            view.context_usage.top_attachments[0]
                .name
                .contains("image/png")
        );
    }

    #[test]
    fn projection_view_aggregates_context_frame_usage_items() {
        let envelope = AgentContextEnvelope {
            session_id: "sess-1".to_string(),
            projection_kind: ProjectionKind::ModelContext,
            projection_version: 0,
            head_event_seq: 10,
            active_compaction_id: None,
            token_estimate: Some(20),
            messages: Vec::new(),
        };
        let frame = ContextFrame {
            id: "frame-1".to_string(),
            kind: "system_guidelines".to_string(),
            source: RuntimeEventSource::RuntimeContextUpdate,
            phase_node: None,
            apply_mode: None,
            delivery_status: "prepared_for_connector".to_string(),
            delivery_channel: "connector_context".to_string(),
            message_role: "system".to_string(),
            delivery_metadata: agentdash_spi::ContextDeliveryMetadata::for_frame(
                "system_guidelines",
                "connector_context",
                "system",
            ),
            rendered_text: String::new(),
            created_at_ms: 1,
            sections: vec![
                ContextFrameSection::Identity {
                    title: "Identity".to_string(),
                    summary: "identity".to_string(),
                    base_prompt: "base".to_string(),
                    agent_prompt: None,
                    mode: "default".to_string(),
                    effective_prompt: "You are Codex.".to_string(),
                },
                ContextFrameSection::ProjectGuidelines {
                    title: "Project Guidelines".to_string(),
                    summary: "guidelines".to_string(),
                    entries: vec![agentdash_spi::hooks::ProjectGuidelineEntry {
                        path: "AGENTS.md".to_string(),
                        content: "Use Chinese for user-facing replies.".to_string(),
                    }],
                },
                ContextFrameSection::AssignmentContext {
                    title: "Assignment Context".to_string(),
                    summary: "assignment".to_string(),
                    fragments: vec![RuntimeContextFragmentEntry {
                        slot: "workflow".to_string(),
                        label: "workflow".to_string(),
                        source: "workflow:implement".to_string(),
                        content: "Current workflow step".to_string(),
                        context_usage_kind: Some("system_developer".to_string()),
                    }],
                },
                ContextFrameSection::CapabilityKeyDelta {
                    added_capabilities: vec!["workflow_management".to_string()],
                    removed_capabilities: Vec::new(),
                    effective_capabilities: vec!["workflow_management".to_string()],
                },
                ContextFrameSection::ToolPathDelta {
                    blocked_tool_paths: vec!["shell.exec".to_string()],
                    unblocked_tool_paths: Vec::new(),
                    whitelisted_tool_paths: vec!["workflow.complete".to_string()],
                    removed_whitelist_paths: Vec::new(),
                },
                ContextFrameSection::McpServerDelta {
                    added_mcp_servers: vec!["agentdash-workflow-tools".to_string()],
                    removed_mcp_servers: Vec::new(),
                    changed_mcp_servers: Vec::new(),
                },
                ContextFrameSection::VfsDelta {
                    vfs_mounts_added: vec!["lifecycle".to_string()],
                    vfs_mounts_removed: Vec::new(),
                    default_mount_before: Some("main".to_string()),
                    default_mount_after: Some("lifecycle".to_string()),
                },
                ContextFrameSection::ToolSchemaDelta {
                    added_tools: vec![
                        RuntimeToolSchemaEntry {
                            name: "read_file".to_string(),
                            description: "Read files".to_string(),
                            parameters_schema: serde_json::json!({"type": "object"}),
                            capability_key: None,
                            source: Some("platform:read".to_string()),
                            tool_path: None,
                            context_usage_kind: Some("system_tools".to_string()),
                        },
                        RuntimeToolSchemaEntry {
                            name: "workflow_search".to_string(),
                            description: "Search workflow state".to_string(),
                            parameters_schema: serde_json::json!({"type": "object"}),
                            capability_key: None,
                            source: Some("mcp:workflow".to_string()),
                            tool_path: None,
                            context_usage_kind: Some("mcp_tools".to_string()),
                        },
                    ],
                },
                ContextFrameSection::CompanionAgentRosterDelta {
                    added_agents: vec![RuntimeCompanionAgentEntry {
                        agent_key: "reviewer".to_string(),
                        executor: "PI_AGENT".to_string(),
                        display_name: "Review Agent".to_string(),
                        context_usage_kind: Some("agents".to_string()),
                    }],
                    removed_agent_keys: Vec::new(),
                    changed_agents: Vec::new(),
                    effective_agents: vec![RuntimeCompanionAgentEntry {
                        agent_key: "reviewer".to_string(),
                        executor: "PI_AGENT".to_string(),
                        display_name: "Review Agent".to_string(),
                        context_usage_kind: Some("agents".to_string()),
                    }],
                },
                ContextFrameSection::SkillDelta {
                    added_skills: vec![RuntimeSkillEntry {
                        name: "trellis-start".to_string(),
                        capability_key: "skill:trellis-start".to_string(),
                        provider_key: "local".to_string(),
                        local_name: "trellis-start".to_string(),
                        display_name: None,
                        description: "Start a Trellis session".to_string(),
                        file_path: ".agents/skills/trellis-start/SKILL.md".to_string(),
                        base_dir: None,
                        exposure: Default::default(),
                        disable_model_invocation: false,
                        context_usage_kind: Some("skills".to_string()),
                    }],
                    removed_skills: vec![RuntimeSkillEntry {
                        name: "archived-review".to_string(),
                        capability_key: "skill:archived-review".to_string(),
                        provider_key: "local".to_string(),
                        local_name: "archived-review".to_string(),
                        display_name: None,
                        description: "Archived review skill".to_string(),
                        file_path: ".agents/skills/archived-review/SKILL.md".to_string(),
                        base_dir: None,
                        exposure: Default::default(),
                        disable_model_invocation: false,
                        context_usage_kind: Some("skills".to_string()),
                    }],
                    changed_skills: Vec::new(),
                },
                ContextFrameSection::CompactionSummary {
                    title: "Compaction Summary".to_string(),
                    summary: "压缩后的上下文摘要".to_string(),
                    tokens_before: 48_000,
                    messages_compacted: 12,
                    compaction_id: Some("compact-1".to_string()),
                    projection_version: Some(2),
                    strategy: Some("summary_prefix".to_string()),
                    trigger: Some("auto".to_string()),
                    phase: Some("pre_provider".to_string()),
                    source_start_event_seq: Some(1),
                    source_end_event_seq: Some(8),
                    first_kept_event_seq: Some(9),
                    compacted_until_ref: Some(serde_json::json!({
                        "turn_id": "turn-0",
                        "entry_index": 3
                    })),
                    timestamp_ms: Some(1_710_000_000_000),
                },
            ],
        };
        let items =
            context_usage_items_from_context_frame(&frame, Some(8), Some("turn-1".to_string()));

        let view = build_session_context_projection_read_model(envelope, items);

        assert_eq!(view.context_usage.items.len(), 13);
        assert!(
            view.token_estimate.expect("combined token estimate") > 20,
            "top-level token estimate should include non-message context items"
        );
        for kind in [
            "system_developer",
            "capability_state",
            "agents",
            "memory",
            "system_tools",
            "mcp_tools",
            "skills",
            "compaction_summary",
        ] {
            let category = view
                .context_usage
                .categories
                .iter()
                .find(|category| category.kind == kind)
                .expect("category should exist");
            assert!(category.token_estimate > 0);
            assert_ne!(category.source, "not_loaded");
            assert!(!category.deferred);
        }
    }
}
