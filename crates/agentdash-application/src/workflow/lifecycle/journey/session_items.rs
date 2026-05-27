use std::collections::BTreeMap;

use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{AgentDashNativeThreadItem, AgentDashThreadItem, BackboneEvent};
use agentdash_spi::content_block_to_text;
use agentdash_spi::{
    PersistedSessionEvent, SESSION_PROJECTION_KIND_MODEL_CONTEXT, SessionCompactionRecord,
    SessionCompactionStatus, SessionPersistence,
};
use serde::Serialize;
use serde_json::{Value, json};

use super::{JourneyResult, LifecycleJourneyError, to_json_pretty};

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

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummaryArchiveEntry {
    pub item_index: usize,
    pub compaction_id: String,
    pub path: String,
    pub projection_version: u64,
    pub source_start_event_seq: Option<u64>,
    pub source_end_event_seq: Option<u64>,
    pub completed_event_seq: Option<u64>,
    pub status: SessionCompactionStatus,
    pub preview: String,
}

pub async fn session_summary_archives(
    persistence: &dyn SessionPersistence,
    session_id: &str,
) -> JourneyResult<Vec<(SessionSummaryArchiveEntry, SessionCompactionRecord)>> {
    let mut compactions = persistence
        .list_compactions(session_id, SESSION_PROJECTION_KIND_MODEL_CONTEXT)
        .await
        .map_err(|error| {
            LifecycleJourneyError::OperationFailed(format!(
                "读取 compaction summaries 失败: {error}"
            ))
        })?;
    compactions.sort_by_key(|record| {
        (
            record.completed_event_seq.unwrap_or(record.start_event_seq),
            record.projection_version,
        )
    });

    let mut entries = Vec::new();
    for (idx, compaction) in compactions.into_iter().enumerate() {
        if compaction.status != SessionCompactionStatus::ProjectionCommitted {
            continue;
        }
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
    Ok(entries)
}

pub fn summary_archive_markdown(compaction: &SessionCompactionRecord) -> String {
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
            BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value },
            ) => {
                if key == "user_message_chunk" {
                    let item_id = user_item_id(event);
                    let builder = builders
                        .entry(item_id.clone())
                        .or_insert_with(|| SessionItemBuilder::new(item_id, "user_message"));
                    builder.apply_event(event);
                    builder.role = Some("user");
                    builder.raw_blocks.push(value.clone());
                    if let Ok(block) = serde_json::from_value::<
                        agentdash_agent_protocol::ContentBlock,
                    >(value.clone())
                    {
                        if let Some(text) = content_block_to_text(&block) {
                            builder.text.push_str(&text);
                        }
                    } else {
                        builder.text.push_str(&json_preview(value));
                    }
                } else if key == "context_compacted" {
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
            BackboneEvent::ItemCompleted(notification) => {
                apply_thread_item_event(&mut builders, event, &notification.item);
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

fn user_item_id(event: &PersistedSessionEvent) -> String {
    event
        .turn_id
        .as_deref()
        .map(|turn_id| format!("user:{turn_id}"))
        .unwrap_or_else(|| format!("user:event:{}", event.event_seq))
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
        AgentDashThreadItem::Codex(item) => match item {
            codex::ThreadItem::DynamicToolCall { arguments, .. } => value_target(arguments),
            codex::ThreadItem::McpToolCall { arguments, .. } => value_target(arguments),
            codex::ThreadItem::CommandExecution { command, cwd, .. } => {
                format!("{} {}", cwd.to_string_lossy(), command)
            }
            codex::ThreadItem::FileChange { changes, .. } => changes
                .iter()
                .map(|change| change.path.to_string())
                .collect::<Vec<_>>()
                .join("_"),
            codex::ThreadItem::ContextCompaction { id } => id.clone(),
            _ => item.id().to_string(),
        },
        AgentDashThreadItem::AgentDash(item) => match item {
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
                matches!(status, codex::DynamicToolCallStatus::Completed) && success.unwrap_or(true)
            }
            codex::ThreadItem::McpToolCall { status, error, .. } => {
                matches!(status, codex::McpToolCallStatus::Completed) && error.is_none()
            }
            codex::ThreadItem::CommandExecution {
                status, exit_code, ..
            } => {
                matches!(status, codex::CommandExecutionStatus::Completed)
                    && exit_code.unwrap_or(0) == 0
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
