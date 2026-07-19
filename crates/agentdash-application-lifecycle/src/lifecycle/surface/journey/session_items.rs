use std::collections::BTreeMap;

use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{AgentDashNativeThreadItem, AgentDashThreadItem, BackboneEvent};
use agentdash_platform_spi::PersistedSessionEvent;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::lifecycle::{lifecycle_path_for_tool_result, readable_aliases_from_item_id};

use super::{JourneyResult, to_json_pretty};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionItemView {
    Items,
    Messages,
    Tools,
    Writes,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionItemSummary {
    pub item_index: usize,
    pub item_id: String,
    pub item_kind: String,
    pub path: String,
    pub first_event_seq: u64,
    pub last_event_seq: u64,
    pub status: Option<String>,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionToolResultMetadata {
    pub session_id: String,
    pub item_id: String,
    pub turn_alias: String,
    pub body_alias: String,
    pub item_kind: String,
    pub tool_name: String,
    pub item_status: Option<String>,
    pub first_event_seq: u64,
    pub last_event_seq: u64,
    pub preview: String,
    pub lifecycle_path: String,
    pub metadata_path: String,
    pub result_path: String,
    pub body_status: SessionLargeBodyStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionLargeBodyStatus {
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionToolResultBodyProjection {
    Available { text: String },
    Truncated { text: String, truncation: Value },
    Unavailable { status: SessionLargeBodyStatus },
}

#[derive(Debug, Clone)]
pub struct SessionItemProjection {
    pub summary: SessionItemSummary,
    pub content: SessionItemContent,
    pub raw_events: Vec<PersistedSessionEvent>,
}

#[derive(Debug, Clone)]
pub enum SessionItemContent {
    Message {
        role: &'static str,
        text: String,
        raw_blocks: Vec<Value>,
    },
    Reasoning {
        text: String,
    },
    Tool {
        item: AgentDashThreadItem,
    },
    Compaction {
        summary: String,
        raw_value: Option<Value>,
    },
    Event,
}

pub fn tool_result_body_for_projection(
    projection: &SessionItemProjection,
) -> SessionToolResultBodyProjection {
    let SessionItemContent::Tool { item } = &projection.content else {
        return SessionToolResultBodyProjection::Unavailable {
            status: unavailable_tool_result_status(&projection.summary.item_id),
        };
    };
    let item_value = serde_json::to_value(item).ok();
    let Some(text) = tool_result_body(item) else {
        return SessionToolResultBodyProjection::Unavailable {
            status: unavailable_tool_result_status(&projection.summary.item_id),
        };
    };
    match item_value.as_ref().and_then(find_truncation_metadata) {
        Some(truncation) => SessionToolResultBodyProjection::Truncated { text, truncation },
        None => SessionToolResultBodyProjection::Available { text },
    }
}

pub fn tool_result_metadata_for_projection(
    session_id: &str,
    projection: &SessionItemProjection,
) -> Option<SessionToolResultMetadata> {
    tool_result_metadata_for_projection_with_status(
        session_id,
        projection,
        unavailable_tool_result_status(&projection.summary.item_id),
    )
}

pub fn tool_result_metadata_for_projection_with_status(
    session_id: &str,
    projection: &SessionItemProjection,
    body_status: SessionLargeBodyStatus,
) -> Option<SessionToolResultMetadata> {
    let SessionItemContent::Tool { item } = &projection.content else {
        return None;
    };
    let item_id = projection.summary.item_id.clone();
    let (turn_alias, body_alias) = readable_aliases_from_item_id(&item_id);
    let metadata_path = format!("session/tool-results/{turn_alias}/{body_alias}/metadata.json");
    let result_path = format!("session/tool-results/{turn_alias}/{body_alias}/result.txt");
    let lifecycle_path = lifecycle_path_for_tool_result(&turn_alias, &body_alias);
    let item_value = serde_json::to_value(item).ok();
    let preview = tool_result_preview(item)
        .or_else(|| item_value.as_ref().and_then(find_tool_result_preview))
        .unwrap_or_else(|| projection.summary.preview.clone());
    let truncation = item_value.as_ref().and_then(find_truncation_metadata);

    Some(SessionToolResultMetadata {
        session_id: session_id.to_string(),
        item_id,
        turn_alias,
        body_alias,
        item_kind: projection.summary.item_kind.clone(),
        tool_name: thread_item_name(item),
        item_status: projection.summary.status.clone(),
        first_event_seq: projection.summary.first_event_seq,
        last_event_seq: projection.summary.last_event_seq,
        preview,
        lifecycle_path,
        metadata_path,
        result_path,
        body_status,
        truncation,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummaryArchiveEntry {
    pub item_index: usize,
    pub compaction_id: String,
    pub path: String,
    pub projection_version: u64,
    pub source_start_event_seq: Option<u64>,
    pub source_end_event_seq: Option<u64>,
    pub completed_event_seq: u64,
    pub status: SessionCompactionArchiveStatus,
    pub preview: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionCompactionArchiveStatus {
    ProjectionCommitted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCompactionArchive {
    pub id: String,
    pub lifecycle_item_id: String,
    pub projection_version: u64,
    pub completed_event_seq: u64,
    pub source_start_event_seq: Option<u64>,
    pub source_end_event_seq: Option<u64>,
    pub summary: String,
    pub trigger: Option<String>,
    pub phase: Option<String>,
    pub strategy: Option<String>,
    pub token_stats_json: Value,
    pub diagnostics_json: Value,
    pub turn_id: Option<String>,
    pub entry_index: Option<u32>,
    pub status: SessionCompactionArchiveStatus,
}

pub fn session_summary_archives(
    mut compactions: Vec<SessionCompactionArchive>,
) -> Vec<(SessionSummaryArchiveEntry, SessionCompactionArchive)> {
    compactions.sort_by_key(|record| (record.completed_event_seq, record.projection_version));

    let mut entries = Vec::new();
    for (idx, compaction) in compactions.into_iter().enumerate() {
        let preview = preview_text(&compaction.summary);
        let compacted_until = compaction
            .source_end_event_seq
            .map(|seq| format!("until_{seq}"))
            .unwrap_or_else(|| "until_unknown".to_string());
        let file_name = format!(
            "{:04}__{}__{}.md",
            idx + 1,
            sanitize_path_part(&compaction.id, 48),
            compacted_until
        );
        entries.push((
            SessionSummaryArchiveEntry {
                item_index: idx + 1,
                compaction_id: compaction.id.clone(),
                path: format!("session/summaries/{file_name}"),
                projection_version: compaction.projection_version,
                source_start_event_seq: compaction.source_start_event_seq,
                source_end_event_seq: compaction.source_end_event_seq,
                completed_event_seq: compaction.completed_event_seq,
                status: compaction.status,
                preview,
            },
            compaction,
        ));
    }
    entries
}

pub fn summary_archive_markdown(compaction: &SessionCompactionArchive) -> String {
    let metadata = json!({
        "compaction_id": compaction.id,
        "projection_version": compaction.projection_version,
        "status": compaction.status,
        "source_start_event_seq": compaction.source_start_event_seq,
        "source_end_event_seq": compaction.source_end_event_seq,
        "completed_event_seq": compaction.completed_event_seq,
        "trigger": compaction.trigger,
        "strategy": compaction.strategy,
        "token_stats": compaction.token_stats_json,
        "diagnostics": compaction.diagnostics_json,
    });
    format!(
        "# Context Compaction {}\n\n```json\n{}\n```\n\n{}",
        compaction.id,
        serde_json::to_string_pretty(&metadata).unwrap_or_else(|_| "{}".to_string()),
        compaction.summary
    )
}

pub fn session_item_projections(events: &[PersistedSessionEvent]) -> Vec<SessionItemProjection> {
    let mut builders: BTreeMap<String, SessionItemBuilder> = BTreeMap::new();

    for event in events {
        match &event.notification.event {
            BackboneEvent::UserInputSubmitted(input) => {
                let item_id = input.item_id.clone();
                let builder = builders
                    .entry(item_id.clone())
                    .or_insert_with(|| SessionItemBuilder::new(item_id, "user_message"));
                builder.apply_event(event);
                builder.role = Some("user");
                builder.raw_value = serde_json::to_value(input).ok();
                for content in &input.content {
                    if let Some(text) = codex_user_input_preview(content) {
                        builder.text.push_str(&text);
                    }
                }
            }
            BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value },
            ) if key == "context_compacted" => {
                let item_id = value
                    .get("lifecycle_item_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("compaction:event:{}", event.event_seq));
                let builder = builders
                    .entry(item_id.clone())
                    .or_insert_with(|| SessionItemBuilder::new(item_id, "context_compaction"));
                builder.apply_event(event);
                builder.raw_value = Some(value.clone());
                if let Some(summary) = value.get("summary").and_then(Value::as_str) {
                    builder.text = summary.to_string();
                }
                builder.status = Some("projection_committed".to_string());
            }
            BackboneEvent::AgentMessageDelta(delta) => {
                let builder = builders.entry(delta.item_id.clone()).or_insert_with(|| {
                    SessionItemBuilder::new(delta.item_id.clone(), "agent_message")
                });
                builder.apply_event(event);
                builder.role = Some("agent");
                builder.text.push_str(&delta.delta);
            }
            BackboneEvent::ReasoningTextDelta(delta) => {
                let builder = builders
                    .entry(delta.item_id.clone())
                    .or_insert_with(|| SessionItemBuilder::new(delta.item_id.clone(), "reasoning"));
                builder.apply_event(event);
                builder.text.push_str(&delta.delta);
            }
            BackboneEvent::ReasoningSummaryDelta(delta) => {
                let builder = builders
                    .entry(delta.item_id.clone())
                    .or_insert_with(|| SessionItemBuilder::new(delta.item_id.clone(), "reasoning"));
                builder.apply_event(event);
                builder.text.push_str(&delta.delta);
            }
            BackboneEvent::ItemStarted(notification) => {
                apply_thread_item_event(&mut builders, event, &notification.item);
            }
            BackboneEvent::ItemUpdated(notification) => {
                apply_thread_item_event(&mut builders, event, &notification.item);
            }
            BackboneEvent::ItemCompleted(notification) => {
                // Step 0：终态助手消息（AgentMessage / Reasoning）是权威正文来源，
                // 不能被当成通用 thread_item（否则 delta-less 会话丢正文、被误标 thread_item）。
                match &notification.item {
                    AgentDashThreadItem::Codex(codex::ThreadItem::AgentMessage {
                        id,
                        text,
                        ..
                    }) => {
                        let builder = builders.entry(id.clone()).or_insert_with(|| {
                            SessionItemBuilder::new(id.clone(), "agent_message")
                        });
                        builder.apply_event(event);
                        builder.role = Some("agent");
                        // 终态权威：覆盖（而非 append）任何已累积的 delta。
                        builder.text = text.clone();
                    }
                    AgentDashThreadItem::Codex(codex::ThreadItem::Reasoning {
                        id,
                        content,
                        ..
                    }) => {
                        let builder = builders
                            .entry(id.clone())
                            .or_insert_with(|| SessionItemBuilder::new(id.clone(), "reasoning"));
                        builder.apply_event(event);
                        builder.text = content.join("");
                    }
                    _ => apply_thread_item_event(&mut builders, event, &notification.item),
                }
            }
            BackboneEvent::CommandOutputDelta(delta) => {
                let builder = builders
                    .entry(delta.item_id.clone())
                    .or_insert_with(|| SessionItemBuilder::new(delta.item_id.clone(), "tool"));
                builder.apply_event(event);
                builder.text.push_str(&delta.delta);
            }
            BackboneEvent::FileChangeDelta(delta) => {
                let builder = builders
                    .entry(delta.item_id.clone())
                    .or_insert_with(|| SessionItemBuilder::new(delta.item_id.clone(), "tool"));
                builder.apply_event(event);
                builder.text.push_str(&delta.delta);
            }
            BackboneEvent::McpToolCallProgress(delta) => {
                let builder = builders
                    .entry(delta.item_id.clone())
                    .or_insert_with(|| SessionItemBuilder::new(delta.item_id.clone(), "tool"));
                builder.apply_event(event);
                builder.text.push_str(&delta.message);
            }
            _ => {}
        }
    }

    let mut projections = builders
        .into_values()
        .filter_map(SessionItemBuilder::finish)
        .collect::<Vec<_>>();
    projections.sort_by_key(|projection| projection.summary.first_event_seq);
    for (idx, projection) in projections.iter_mut().enumerate() {
        projection.summary.item_index = idx + 1;
        projection.summary.path = format!(
            "session/items/{}",
            item_file_name(projection, SessionItemView::Items)
        );
    }
    projections
}

fn codex_user_input_preview(
    input: &agentdash_agent_protocol::codex_app_server_protocol::UserInput,
) -> Option<String> {
    match input {
        agentdash_agent_protocol::codex_app_server_protocol::UserInput::Text { text, .. } => {
            Some(text.clone())
        }
        other => serde_json::to_string(other).ok(),
    }
}

pub fn filter_session_items(
    projections: &[SessionItemProjection],
    view: SessionItemView,
) -> Vec<SessionItemProjection> {
    projections
        .iter()
        .filter(|projection| match view {
            SessionItemView::Items => true,
            SessionItemView::Messages => matches!(
                projection.content,
                SessionItemContent::Message {
                    role: "user" | "agent",
                    ..
                }
            ),
            SessionItemView::Tools => matches!(projection.content, SessionItemContent::Tool { .. }),
            SessionItemView::Writes => matches!(
                &projection.content,
                SessionItemContent::Tool { item } if is_successful_write_item(item)
            ),
        })
        .cloned()
        .collect()
}

pub fn view_root(view: SessionItemView) -> &'static str {
    match view {
        SessionItemView::Items => "session/items",
        SessionItemView::Messages => "session/messages",
        SessionItemView::Tools => "session/tools",
        SessionItemView::Writes => "session/writes",
    }
}

pub fn item_file_name(projection: &SessionItemProjection, view: SessionItemView) -> String {
    let summary = &projection.summary;
    let ordinal = format!("{:04}", summary.item_index);
    match (view, &projection.content) {
        (SessionItemView::Messages, SessionItemContent::Message { role, text, .. }) => format!(
            "{ordinal}__{}__{}__{}.md",
            sanitize_path_part(&summary.item_id, 48),
            role,
            sanitize_path_part(&preview_text(text), 56)
        ),
        (SessionItemView::Tools | SessionItemView::Writes, SessionItemContent::Tool { item }) => {
            format!(
                "{ordinal}__{}__{}__{}.json",
                sanitize_path_part(&summary.item_id, 48),
                sanitize_path_part(&thread_item_name(item), 48),
                sanitize_path_part(&thread_item_target(item), 80)
            )
        }
        _ => format!(
            "{ordinal}__{}__{}__{}.json",
            sanitize_path_part(&summary.item_id, 48),
            sanitize_path_part(&summary.item_kind, 32),
            sanitize_path_part(&summary.preview, 56)
        ),
    }
}

pub fn item_summary_for_view(
    projection: &SessionItemProjection,
    view: SessionItemView,
) -> SessionItemSummary {
    let mut summary = projection.summary.clone();
    let root = view_root(view);
    summary.path = format!("{root}/{}", item_file_name(projection, view));
    summary
}

pub fn render_item_content(
    projection: &SessionItemProjection,
    view: SessionItemView,
) -> JourneyResult<String> {
    match (view, &projection.content) {
        (SessionItemView::Messages, SessionItemContent::Message { role, text, .. }) => {
            Ok(format!("# {role} message\n\n{text}"))
        }
        (SessionItemView::Tools | SessionItemView::Writes, SessionItemContent::Tool { item }) => {
            to_json_pretty(item)
        }
        (
            _,
            SessionItemContent::Message {
                role,
                text,
                raw_blocks,
            },
        ) => to_json_pretty(&json!({
            "summary": projection.summary,
            "role": role,
            "text": text,
            "raw_blocks": raw_blocks,
            "events": projection.raw_events,
        })),
        (_, SessionItemContent::Reasoning { text }) => to_json_pretty(&json!({
            "summary": projection.summary,
            "text": text,
            "events": projection.raw_events,
        })),
        (_, SessionItemContent::Tool { item }) => to_json_pretty(&json!({
            "summary": projection.summary,
            "item": item,
            "events": projection.raw_events,
        })),
        (_, SessionItemContent::Compaction { summary, raw_value }) => to_json_pretty(&json!({
            "summary": projection.summary,
            "compaction_summary": summary,
            "raw_value": raw_value,
            "events": projection.raw_events,
        })),
        (_, SessionItemContent::Event) => to_json_pretty(&json!({
            "summary": projection.summary,
            "events": projection.raw_events,
        })),
    }
}

struct SessionItemBuilder {
    item_id: String,
    item_kind: String,
    role: Option<&'static str>,
    text: String,
    raw_blocks: Vec<Value>,
    raw_value: Option<Value>,
    thread_item: Option<AgentDashThreadItem>,
    raw_events: Vec<PersistedSessionEvent>,
    status: Option<String>,
}

impl SessionItemBuilder {
    fn new(item_id: String, item_kind: &str) -> Self {
        Self {
            item_id,
            item_kind: item_kind.to_string(),
            role: None,
            text: String::new(),
            raw_blocks: Vec::new(),
            raw_value: None,
            thread_item: None,
            raw_events: Vec::new(),
            status: None,
        }
    }

    fn apply_event(&mut self, event: &PersistedSessionEvent) {
        self.raw_events.push(event.clone());
    }

    fn finish(mut self) -> Option<SessionItemProjection> {
        self.raw_events.sort_by_key(|event| event.event_seq);
        let first = self.raw_events.first()?.event_seq;
        let last = self.raw_events.last()?.event_seq;
        let content = if let Some(item) = self.thread_item {
            if is_context_compaction_item(&item) {
                SessionItemContent::Compaction {
                    summary: self.text.clone(),
                    raw_value: self.raw_value.clone(),
                }
            } else {
                SessionItemContent::Tool { item }
            }
        } else if let Some(role) = self.role {
            SessionItemContent::Message {
                role,
                text: self.text.clone(),
                raw_blocks: self.raw_blocks.clone(),
            }
        } else if self.item_kind == "reasoning" {
            SessionItemContent::Reasoning {
                text: self.text.clone(),
            }
        } else if self.item_kind == "context_compaction" {
            SessionItemContent::Compaction {
                summary: self.text.clone(),
                raw_value: self.raw_value.clone(),
            }
        } else {
            SessionItemContent::Event
        };
        let preview = match &content {
            SessionItemContent::Tool { item } => thread_item_preview(item),
            _ => preview_text(&self.text),
        };
        Some(SessionItemProjection {
            summary: SessionItemSummary {
                item_index: 0,
                item_id: self.item_id,
                item_kind: self.item_kind,
                path: String::new(),
                first_event_seq: first,
                last_event_seq: last,
                status: self.status,
                preview,
            },
            content,
            raw_events: self.raw_events,
        })
    }
}

fn apply_thread_item_event(
    builders: &mut BTreeMap<String, SessionItemBuilder>,
    event: &PersistedSessionEvent,
    item: &AgentDashThreadItem,
) {
    let item_id = item.id().to_string();
    let item_kind = if is_context_compaction_item(item) {
        "context_compaction"
    } else {
        "tool"
    };
    let builder = builders
        .entry(item_id.clone())
        .or_insert_with(|| SessionItemBuilder::new(item_id, item_kind));
    builder.apply_event(event);
    builder.item_kind = item_kind.to_string();
    builder.thread_item = Some(item.clone());
    builder.status = thread_item_status(item);
    if builder.text.is_empty() {
        builder.text = thread_item_preview(item);
    }
}

fn is_context_compaction_item(item: &AgentDashThreadItem) -> bool {
    matches!(
        item,
        AgentDashThreadItem::Codex(codex::ThreadItem::ContextCompaction { .. })
    )
}

fn thread_item_status(item: &AgentDashThreadItem) -> Option<String> {
    match item {
        AgentDashThreadItem::Codex(item) => match item {
            codex::ThreadItem::DynamicToolCall { status, .. } => {
                Some(format!("{status:?}").to_ascii_lowercase())
            }
            codex::ThreadItem::McpToolCall { status, .. } => {
                Some(format!("{status:?}").to_ascii_lowercase())
            }
            codex::ThreadItem::CommandExecution { status, .. } => {
                Some(format!("{status:?}").to_ascii_lowercase())
            }
            codex::ThreadItem::FileChange { status, .. } => {
                Some(format!("{status:?}").to_ascii_lowercase())
            }
            codex::ThreadItem::ContextCompaction { .. } => Some("completed".to_string()),
            _ => None,
        },
        AgentDashThreadItem::AgentDash(item) => {
            Some(format!("{:?}", item.status()).to_ascii_lowercase())
        }
    }
}

fn thread_item_name(item: &AgentDashThreadItem) -> String {
    match item {
        AgentDashThreadItem::Codex(item) => match item {
            codex::ThreadItem::DynamicToolCall { tool, .. } => tool.clone(),
            codex::ThreadItem::McpToolCall { server, tool, .. } => format!("{server}_{tool}"),
            codex::ThreadItem::CommandExecution { .. } => "shell_exec".to_string(),
            codex::ThreadItem::FileChange { .. } => "file_change".to_string(),
            codex::ThreadItem::ContextCompaction { .. } => "context_compaction".to_string(),
            codex::ThreadItem::CollabAgentToolCall { tool, .. } => serde_json::to_value(tool)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "collab_agent_tool".to_string()),
            _ => "thread_item".to_string(),
        },
        AgentDashThreadItem::AgentDash(item) => item.tool_name().to_string(),
    }
}

fn thread_item_target(item: &AgentDashThreadItem) -> String {
    match item {
        AgentDashThreadItem::Codex(item) => {
            let item_id = AgentDashThreadItem::Codex(item.clone()).id().to_string();
            match item {
                codex::ThreadItem::DynamicToolCall { arguments, .. } => value_target(arguments),
                codex::ThreadItem::McpToolCall { arguments, .. } => value_target(arguments),
                codex::ThreadItem::CommandExecution { command, cwd, .. } => {
                    format!("{cwd} {command}")
                }
                codex::ThreadItem::FileChange { changes, .. } => changes
                    .iter()
                    .map(|change| change.path.to_string())
                    .collect::<Vec<_>>()
                    .join("_"),
                codex::ThreadItem::ContextCompaction { id } => id.clone(),
                _ => item_id,
            }
        }
        AgentDashThreadItem::AgentDash(item) => match item {
            AgentDashNativeThreadItem::ShellExec { command, cwd, .. } => cwd
                .as_ref()
                .map(|cwd| format!("{cwd} {command}"))
                .unwrap_or_else(|| command.clone()),
            AgentDashNativeThreadItem::TerminalControl {
                operation,
                terminal_id,
                ..
            } => format!("{operation} {terminal_id}"),
            AgentDashNativeThreadItem::FsRead { path, .. } => path.clone(),
            AgentDashNativeThreadItem::FsGrep { pattern, path, .. } => path
                .as_ref()
                .map(|path| format!("{pattern} in {path}"))
                .unwrap_or_else(|| pattern.clone()),
            AgentDashNativeThreadItem::FsGlob { pattern, path, .. } => path
                .as_ref()
                .map(|path| format!("{pattern} in {path}"))
                .unwrap_or_else(|| pattern.clone()),
        },
    }
}

fn thread_item_preview(item: &AgentDashThreadItem) -> String {
    let name = thread_item_name(item);
    let target = thread_item_target(item);
    if target.is_empty() {
        name
    } else {
        format!("{name} {target}")
    }
}

fn tool_result_preview(item: &AgentDashThreadItem) -> Option<String> {
    match item {
        AgentDashThreadItem::Codex(codex::ThreadItem::DynamicToolCall {
            content_items, ..
        }) => content_items
            .as_ref()
            .and_then(Option::as_deref)
            .and_then(|items| content_items_preview(items)),
        AgentDashThreadItem::Codex(codex::ThreadItem::CommandExecution {
            aggregated_output,
            ..
        }) => aggregated_output.clone().flatten(),
        AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ShellExec {
            aggregated_output,
            ..
        })
        | AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::TerminalControl {
            aggregated_output,
            ..
        }) => aggregated_output.clone(),
        AgentDashThreadItem::AgentDash(item) => item
            .content_items()
            .and_then(|items| content_items_preview(items)),
        _ => None,
    }
}

fn tool_result_body(item: &AgentDashThreadItem) -> Option<String> {
    match item {
        AgentDashThreadItem::Codex(codex::ThreadItem::CommandExecution {
            aggregated_output,
            ..
        }) => aggregated_output.as_ref().and_then(Option::as_ref).cloned(),
        AgentDashThreadItem::Codex(codex::ThreadItem::DynamicToolCall {
            content_items, ..
        }) => content_items
            .as_ref()
            .and_then(Option::as_ref)
            .and_then(|items| content_items_body(items)),
        AgentDashThreadItem::Codex(codex::ThreadItem::McpToolCall { result, error, .. }) => result
            .as_ref()
            .and_then(Option::as_ref)
            .and_then(mcp_tool_result_body)
            .or_else(|| {
                error
                    .as_ref()
                    .and_then(Option::as_ref)
                    .map(|error| error.message.clone())
            }),
        AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ShellExec {
            aggregated_output,
            ..
        })
        | AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::TerminalControl {
            aggregated_output,
            ..
        }) => aggregated_output.clone(),
        AgentDashThreadItem::AgentDash(item) => item
            .content_items()
            .and_then(|items| content_items_body(items)),
        _ => None,
    }
}

fn content_items_body(items: &[codex::DynamicToolCallOutputContentItem]) -> Option<String> {
    if items.iter().all(|item| {
        matches!(
            item,
            codex::DynamicToolCallOutputContentItem::InputText { .. }
        )
    }) {
        return Some(
            items
                .iter()
                .map(|item| match item {
                    codex::DynamicToolCallOutputContentItem::InputText { text } => text.as_str(),
                    codex::DynamicToolCallOutputContentItem::InputImage { .. } => unreachable!(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    serde_json::to_string_pretty(items).ok()
}

fn mcp_tool_result_body(result: &codex::McpToolCallResult) -> Option<String> {
    let text_content = result
        .content
        .iter()
        .map(|content| {
            let object = content.as_object()?;
            (object.get("type")?.as_str()? == "text")
                .then(|| object.get("text").and_then(Value::as_str))
                .flatten()
        })
        .collect::<Option<Vec<_>>>();
    if result.meta.is_none()
        && result.structured_content.is_none()
        && let Some(text_content) = text_content
    {
        return Some(text_content.join("\n"));
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct McpToolResultBody<'a> {
        content: &'a [Value],
        structured_content: &'a Option<Value>,
        #[serde(rename = "_meta")]
        meta: &'a Option<Value>,
    }
    serde_json::to_string_pretty(&McpToolResultBody {
        content: &result.content,
        structured_content: &result.structured_content,
        meta: &result.meta,
    })
    .ok()
}

fn content_items_preview(items: &[codex::DynamicToolCallOutputContentItem]) -> Option<String> {
    let text = items
        .iter()
        .filter_map(|item| match item {
            codex::DynamicToolCallOutputContentItem::InputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

fn find_tool_result_preview(value: &Value) -> Option<String> {
    match value {
        Value::Object(object) => {
            for key in ["aggregated_output", "text", "preview"] {
                if let Some(text) = object.get(key).and_then(Value::as_str)
                    && !text.is_empty()
                {
                    return Some(text.to_string());
                }
            }
            object.values().find_map(find_tool_result_preview)
        }
        Value::Array(values) => values.iter().find_map(find_tool_result_preview),
        _ => None,
    }
}

fn find_truncation_metadata(value: &Value) -> Option<Value> {
    match value {
        Value::Object(object) => {
            if let Some(truncation) = object.get("truncation")
                && truncation.is_object()
            {
                return Some(truncation.clone());
            }
            object.values().find_map(find_truncation_metadata)
        }
        Value::Array(values) => values.iter().find_map(find_truncation_metadata),
        _ => None,
    }
}

pub fn unavailable_tool_result_status(item_id: &str) -> SessionLargeBodyStatus {
    let (turn_alias, body_alias) = readable_aliases_from_item_id(item_id);
    let lifecycle_path = lifecycle_path_for_tool_result(&turn_alias, &body_alias);
    SessionLargeBodyStatus {
        status: "cache_miss".to_string(),
        message: format!(
            "[tool result cache missing]\nlifecycle_path: {lifecycle_path}\nitem_id: {item_id}\nThe original tool result is not available from the session cache."
        ),
    }
}

fn is_successful_write_item(item: &AgentDashThreadItem) -> bool {
    is_write_item(item) && is_successful_item(item)
}

fn is_write_item(item: &AgentDashThreadItem) -> bool {
    if matches!(
        item,
        AgentDashThreadItem::Codex(codex::ThreadItem::FileChange { .. })
    ) {
        return true;
    }
    let name = thread_item_name(item).to_ascii_lowercase();
    name.contains("write")
        || name.contains("apply_patch")
        || name.contains("patch")
        || name.contains("edit")
}

fn is_successful_item(item: &AgentDashThreadItem) -> bool {
    match item {
        AgentDashThreadItem::Codex(item) => match item {
            codex::ThreadItem::DynamicToolCall {
                status, success, ..
            } => {
                matches!(status, codex::DynamicToolCallStatus::Completed)
                    && success.as_ref().copied().flatten().unwrap_or(true)
            }
            codex::ThreadItem::McpToolCall { status, error, .. } => {
                matches!(status, codex::McpToolCallStatus::Completed) && error.is_none()
            }
            codex::ThreadItem::CommandExecution {
                status, exit_code, ..
            } => {
                matches!(status, codex::CommandExecutionStatus::Completed)
                    && exit_code.as_ref().copied().flatten().unwrap_or(0) == 0
            }
            codex::ThreadItem::FileChange { status, .. } => {
                matches!(status, codex::PatchApplyStatus::Completed)
            }
            _ => false,
        },
        AgentDashThreadItem::AgentDash(item) => {
            matches!(item.status(), codex::DynamicToolCallStatus::Completed)
                && item.success().unwrap_or(true)
        }
    }
}

fn value_target(value: &Value) -> String {
    for key in ["path", "file", "cwd", "query", "command", "pattern"] {
        if let Some(text) = value.get(key).and_then(Value::as_str)
            && !text.trim().is_empty()
        {
            return text.to_string();
        }
    }
    json_preview(value)
}

fn json_preview(value: &Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_default();
    preview_text(&text)
}

fn preview_text(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "empty".to_string();
    }
    let words = collapsed.split_whitespace().take(10).collect::<Vec<_>>();
    if words.len() > 1 {
        return words.join("_");
    }
    collapsed.chars().take(32).collect()
}

fn sanitize_path_part(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    let mut last_was_sep = false;
    for ch in value.chars().take(max_chars) {
        let keep = ch.is_alphanumeric() || matches!(ch, '-' | '_');
        if keep {
            output.push(ch);
            last_was_sep = false;
        } else if !last_was_sep {
            output.push('_');
            last_was_sep = true;
        }
    }
    let trimmed = output.trim_matches('_');
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_protocol::backbone::item::ItemCompletedNotification;
    use agentdash_agent_protocol::{BackboneEnvelope, SourceInfo, TraceInfo};

    fn source_info() -> SourceInfo {
        SourceInfo {
            connector_id: "test".to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: None,
        }
    }

    fn item_completed_event(event_seq: u64, item: AgentDashThreadItem) -> PersistedSessionEvent {
        let envelope = BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item,
                "session-1".to_string(),
                "turn-1".to_string(),
            )),
            "session-1",
            source_info(),
        )
        .with_trace(TraceInfo {
            turn_id: Some("turn-1".to_string()),
            entry_index: Some(0),
        });
        PersistedSessionEvent {
            session_id: "session-1".to_string(),
            event_seq,
            occurred_at_ms: event_seq as i64,
            committed_at_ms: event_seq as i64,
            session_update_type: "item_completed".to_string(),
            turn_id: Some("turn-1".to_string()),
            entry_index: Some(0),
            tool_call_id: None,
            ephemeral: false,
            notification: envelope,
        }
    }

    /// P1-a：仅终态助手消息 / reasoning（无 delta，Step 1 后的稀疏会话）应投影为
    /// agent_message / reasoning，承载终态正文，且不被误标成 thread_item/tool。
    #[test]
    fn final_assistant_items_project_message_and_reasoning_without_delta() {
        let msg_item: AgentDashThreadItem = codex::ThreadItem::AgentMessage {
            id: "turn-1:0:msg".to_string(),
            text: "FULL FINAL".to_string(),
            phase: None,
            memory_citation: None,
        }
        .into();
        let reason_item: AgentDashThreadItem = codex::ThreadItem::Reasoning {
            id: "turn-1:0:reason".to_string(),
            summary: vec![],
            content: vec!["because ".to_string(), "reasons".to_string()],
        }
        .into();

        let events = vec![
            item_completed_event(1, msg_item),
            item_completed_event(2, reason_item),
        ];
        let projections = session_item_projections(&events);

        let message = projections
            .iter()
            .find(|p| p.summary.item_id == "turn-1:0:msg")
            .expect("agent message projection");
        match &message.content {
            SessionItemContent::Message { role, text, .. } => {
                assert_eq!(*role, "agent");
                assert_eq!(text, "FULL FINAL");
            }
            other => panic!("expected agent message content, got {other:?}"),
        }
        assert_eq!(message.summary.item_kind, "agent_message");

        let reasoning = projections
            .iter()
            .find(|p| p.summary.item_id == "turn-1:0:reason")
            .expect("reasoning projection");
        match &reasoning.content {
            SessionItemContent::Reasoning { text } => {
                assert_eq!(text, "because reasons");
            }
            other => panic!("expected reasoning content, got {other:?}"),
        }
        assert_eq!(reasoning.summary.item_kind, "reasoning");
    }

    /// 终态助手正文是权威：与既有 delta 累积共存时用 `=` 覆盖，不在 delta 之上 append。
    #[test]
    fn final_assistant_text_overrides_accumulated_delta() {
        let delta_event = {
            let envelope = BackboneEnvelope::new(
                BackboneEvent::AgentMessageDelta(codex::AgentMessageDeltaNotification {
                    delta: "partial".to_string(),
                    thread_id: "session-1".to_string(),
                    turn_id: "turn-1".to_string(),
                    item_id: "turn-1:0:msg".to_string(),
                }),
                "session-1",
                source_info(),
            );
            PersistedSessionEvent {
                session_id: "session-1".to_string(),
                event_seq: 1,
                occurred_at_ms: 1,
                committed_at_ms: 1,
                session_update_type: "agent_message_delta".to_string(),
                turn_id: Some("turn-1".to_string()),
                entry_index: Some(0),
                tool_call_id: None,
                ephemeral: false,
                notification: envelope,
            }
        };
        let msg_item: AgentDashThreadItem = codex::ThreadItem::AgentMessage {
            id: "turn-1:0:msg".to_string(),
            text: "FULL FINAL".to_string(),
            phase: None,
            memory_citation: None,
        }
        .into();

        let events = vec![delta_event, item_completed_event(2, msg_item)];
        let projections = session_item_projections(&events);
        let message = projections
            .iter()
            .find(|p| p.summary.item_id == "turn-1:0:msg")
            .expect("agent message projection");
        match &message.content {
            SessionItemContent::Message { text, .. } => assert_eq!(text, "FULL FINAL"),
            other => panic!("expected agent message content, got {other:?}"),
        }
    }

    #[test]
    fn terminal_control_projects_explicit_target_status_and_result() {
        let item = AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::TerminalControl {
            id: "turn-1:0:terminal".to_string(),
            operation: "write".to_string(),
            terminal_id: "terminal-7".to_string(),
            arguments: serde_json::json!({"terminalId": "terminal-7", "input": "pwd\n"}),
            input: Some("pwd\n".to_string()),
            cols: None,
            rows: None,
            state: Some("running".to_string()),
            aggregated_output: Some("D:\\workspace\r\n".to_string()),
            exit_code: None,
            status: codex::DynamicToolCallStatus::Completed,
            success: Some(true),
        });

        assert_eq!(thread_item_name(&item), "terminal_control");
        assert_eq!(thread_item_target(&item), "write terminal-7");
        assert_eq!(
            tool_result_preview(&item).as_deref(),
            Some("D:\\workspace\r\n")
        );
        assert!(is_successful_item(&item));

        let projections = session_item_projections(&[item_completed_event(1, item)]);
        let projection = projections.first().expect("terminal control projection");
        assert_eq!(projection.summary.item_kind, "tool");
        assert_eq!(projection.summary.status.as_deref(), Some("completed"));
        assert_eq!(
            item_file_name(projection, SessionItemView::Tools),
            "0001__turn-1_0_terminal__terminal_control__write_terminal-7.json"
        );
    }

    #[test]
    fn terminal_thread_items_are_the_authority_for_tool_result_bodies() {
        let command = AgentDashThreadItem::Codex(codex::ThreadItem::CommandExecution {
            id: "command-1".to_string(),
            command: "pwd".to_string(),
            cwd: "D:\\workspace".to_string().into(),
            process_id: None,
            status: codex::CommandExecutionStatus::Completed,
            command_actions: Vec::new(),
            aggregated_output: Some(Some("D:\\workspace\r\n".to_string())),
            exit_code: Some(Some(0)),
            duration_ms: Some(Some(12)),
            source: codex::CommandExecutionSource::Agent,
        });
        let dynamic = AgentDashThreadItem::Codex(codex::ThreadItem::DynamicToolCall {
            id: "dynamic-1".to_string(),
            tool: "fs_read".to_string(),
            namespace: None,
            arguments: json!({"path": "README.md"}),
            status: codex::DynamicToolCallStatus::Completed,
            content_items: Some(Some(vec![
                codex::DynamicToolCallOutputContentItem::InputText {
                    text: "line one".to_string(),
                },
                codex::DynamicToolCallOutputContentItem::InputText {
                    text: "line two".to_string(),
                },
            ])),
            success: Some(Some(true)),
            duration_ms: None,
        });

        let projections = session_item_projections(&[
            item_completed_event(1, command),
            item_completed_event(2, dynamic),
        ]);
        assert_eq!(
            tool_result_body_for_projection(&projections[0]),
            SessionToolResultBodyProjection::Available {
                text: "D:\\workspace\r\n".to_string(),
            }
        );
        assert_eq!(
            tool_result_body_for_projection(&projections[1]),
            SessionToolResultBodyProjection::Available {
                text: "line one\nline two".to_string(),
            }
        );
    }

    #[test]
    fn native_shell_and_fs_terminal_items_expose_explicit_result_bodies() {
        let shell = AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ShellExec {
            id: "shell-1".to_string(),
            command: "pwd".to_string(),
            cwd: Some("D:\\workspace".to_string()),
            execution_mode: agentdash_agent_protocol::ShellExecExecutionMode::Platform,
            arguments: json!({"command": "pwd"}),
            status: codex::DynamicToolCallStatus::Completed,
            aggregated_output: Some("D:\\workspace\r\n".to_string()),
            exit_code: Some(0),
            success: Some(true),
        });
        let fs_read = AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::FsRead {
            id: "fs-1".to_string(),
            path: "README.md".to_string(),
            offset: None,
            limit: None,
            arguments: json!({"path": "README.md"}),
            status: codex::DynamicToolCallStatus::Completed,
            content_items: Some(vec![
                codex::DynamicToolCallOutputContentItem::InputText {
                    text: "first".to_string(),
                },
                codex::DynamicToolCallOutputContentItem::InputText {
                    text: "second".to_string(),
                },
            ]),
            success: Some(true),
        });
        let projections = session_item_projections(&[
            item_completed_event(1, shell),
            item_completed_event(2, fs_read),
        ]);

        assert_eq!(
            tool_result_body_for_projection(&projections[0]),
            SessionToolResultBodyProjection::Available {
                text: "D:\\workspace\r\n".to_string(),
            }
        );
        assert_eq!(
            tool_result_body_for_projection(&projections[1]),
            SessionToolResultBodyProjection::Available {
                text: "first\nsecond".to_string(),
            }
        );
    }

    #[test]
    fn explicit_null_tool_result_body_remains_unavailable() {
        let command = AgentDashThreadItem::Codex(codex::ThreadItem::CommandExecution {
            id: "command-null".to_string(),
            command: "pwd".to_string(),
            cwd: "D:\\workspace".to_string().into(),
            process_id: None,
            status: codex::CommandExecutionStatus::Completed,
            command_actions: Vec::new(),
            aggregated_output: Some(None),
            exit_code: Some(Some(0)),
            duration_ms: None,
            source: codex::CommandExecutionSource::Agent,
        });
        let projections = session_item_projections(&[item_completed_event(1, command)]);

        assert_eq!(
            tool_result_body_for_projection(&projections[0]),
            SessionToolResultBodyProjection::Unavailable {
                status: unavailable_tool_result_status("command-null"),
            }
        );
    }

    #[test]
    fn mcp_result_preserves_structure_and_reports_explicit_truncation() {
        let item: AgentDashThreadItem = serde_json::from_value(json!({
            "type": "mcpToolCall",
            "id": "mcp-1",
            "server": "files",
            "tool": "read",
            "arguments": {"path": "large.log"},
            "status": "completed",
            "result": {
                "content": [{"type": "text", "text": "partial body"}],
                "_meta": {
                    "truncation": {"policy": "head_tail", "originalBytes": 9000}
                },
                "structuredContent": {"rows": 10}
            }
        }))
        .expect("mcp terminal item");
        let projections = session_item_projections(&[item_completed_event(1, item)]);

        match tool_result_body_for_projection(&projections[0]) {
            SessionToolResultBodyProjection::Truncated { text, truncation } => {
                assert!(text.contains("partial body"));
                assert!(text.contains("structuredContent"));
                assert_eq!(truncation["policy"], "head_tail");
                assert_eq!(truncation["originalBytes"], 9000);
            }
            other => panic!("expected truncated MCP result body, got {other:?}"),
        }
    }

    #[test]
    fn canonical_compaction_archive_preserves_main_summary_path_and_markdown() {
        let archive = SessionCompactionArchive {
            id: "compact-1".to_string(),
            lifecycle_item_id: "item-compact-1".to_string(),
            projection_version: 2,
            completed_event_seq: 9,
            source_start_event_seq: Some(1),
            source_end_event_seq: Some(8),
            summary: "Earlier conversation summary".to_string(),
            trigger: Some("auto".to_string()),
            phase: Some("pre_provider".to_string()),
            strategy: Some("summary_prefix".to_string()),
            token_stats_json: serde_json::json!({
                "messages_compacted": 3,
                "tokens_before": 100
            }),
            diagnostics_json: Value::Null,
            turn_id: Some("turn-1".to_string()),
            entry_index: Some(4),
            status: SessionCompactionArchiveStatus::ProjectionCommitted,
        };

        let entries = session_summary_archives(vec![archive.clone()]);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].0.path,
            "session/summaries/0001__compact-1__until_8.md"
        );
        assert_eq!(entries[0].0.completed_event_seq, 9);
        assert_eq!(entries[0].0.preview, "Earlier_conversation_summary");

        let markdown = summary_archive_markdown(&archive);
        assert!(markdown.starts_with("# Context Compaction compact-1\n\n```json\n"));
        assert!(markdown.contains("\"status\": \"projection_committed\""));
        assert!(markdown.contains("\"source_start_event_seq\": 1"));
        assert!(markdown.contains("\"source_end_event_seq\": 8"));
        assert!(markdown.contains("\"completed_event_seq\": 9"));
        assert!(markdown.ends_with("```\n\nEarlier conversation summary"));
    }
}
