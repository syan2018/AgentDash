use std::collections::BTreeMap;

use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{AgentDashNativeThreadItem, AgentDashThreadItem, BackboneEvent};
use agentdash_spi::{
    PersistedSessionEvent, SESSION_PROJECTION_KIND_MODEL_CONTEXT, SessionCompactionRecord,
    SessionCompactionStatus, SessionPersistence,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::session::{lifecycle_path_for_tool_result, readable_aliases_from_item_id};

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionLargeBodyStatus {
    pub status: String,
    pub message: String,
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

pub fn tool_result_metadata_for_projection(
    session_id: &str,
    projection: &SessionItemProjection,
) -> Option<SessionToolResultMetadata> {
    tool_result_metadata_for_projection_with_status(
        session_id,
        projection,
        cache_miss_status("result body"),
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

pub fn tool_result_cache_miss_text(session_id: &str, item_id: &str) -> String {
    let (turn_alias, body_alias) = readable_aliases_from_item_id(item_id);
    let lifecycle_path = lifecycle_path_for_tool_result(&turn_alias, &body_alias);
    format!(
        "[tool result cache missing]\nsession_id: {session_id}\nitem_id: {item_id}\nlifecycle_path: {lifecycle_path}\nThe original tool result body is not available from the session cache."
    )
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
            ) => {
                if key == "context_compacted" {
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
            AgentDashNativeThreadItem::ShellExec { command, cwd, .. } => cwd
                .as_ref()
                .map(|cwd| format!("{cwd} {command}"))
                .unwrap_or_else(|| command.clone()),
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
            .and_then(|items| content_items_preview(items)),
        AgentDashThreadItem::Codex(codex::ThreadItem::CommandExecution {
            aggregated_output,
            ..
        }) => aggregated_output.clone(),
        AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ShellExec {
            aggregated_output,
            ..
        }) => aggregated_output.clone(),
        AgentDashThreadItem::AgentDash(item) => item
            .content_items()
            .and_then(|items| content_items_preview(items)),
        _ => None,
    }
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

pub fn cache_miss_status(label: &str) -> SessionLargeBodyStatus {
    SessionLargeBodyStatus {
        status: "cache_miss".to_string(),
        message: format!("{label} is not available from the current session cache."),
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
}
