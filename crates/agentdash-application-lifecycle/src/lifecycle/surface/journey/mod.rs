//! Lifecycle journey 的业务投影层。
//!
//! 这里负责把 lifecycle run、session events 和 inline overlay 组合成稳定的
//! journey 视图；VFS provider 只负责把这些视图映射到 `lifecycle://...` 路径。

use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_agent_protocol::BackboneEvent;
use agentdash_agent_protocol::PlatformEvent;
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use agentdash_domain::workflow::{
    ExecutorRunRef, LifecycleRun, LifecycleRunStatus, RuntimeNodeState, RuntimeNodeStatus,
};
use serde::Serialize;
use uuid::Uuid;

use crate::lifecycle::execution_log::{RuntimeNodeArtifactScope, RuntimeNodePortArtifactRef};
use crate::lifecycle::{
    SessionToolResultCache, SessionToolResultCacheRead, SessionToolResultCacheStatus,
    SessionToolResultCacheStatusKind,
};
use agentdash_spi::{PersistedSessionEvent, SessionPersistence};

pub mod session_items;

pub use session_items::{
    SessionItemProjection, SessionItemSummary, SessionItemView, SessionLargeBodyStatus,
    SessionToolResultMetadata, filter_session_items, item_file_name, item_summary_for_view,
    render_item_content, session_item_projections, session_summary_archives,
    summary_archive_markdown, tool_result_metadata_for_projection,
    tool_result_metadata_for_projection_with_status,
};

pub const PORT_OUTPUTS_CONTAINER: &str = "port_outputs";
pub const SESSION_RECORDS_CONTAINER: &str = "session_records";
pub const JOURNEY_RECORDS_CONTAINER: &str = "journey_records";

pub type JourneyResult<T> = Result<T, LifecycleJourneyError>;

#[derive(Debug)]
pub enum LifecycleJourneyError {
    NotFound(String),
    OperationFailed(String),
}

impl std::fmt::Display for LifecycleJourneyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::OperationFailed(message) => write!(f, "operation failed: {message}"),
        }
    }
}

impl std::error::Error for LifecycleJourneyError {}

pub struct LifecycleJourneyProjection {
    inline_file_repo: Arc<dyn InlineFileRepository>,
    session_persistence: Arc<dyn SessionPersistence>,
    tool_result_cache: Arc<SessionToolResultCache>,
}

impl LifecycleJourneyProjection {
    pub fn new(
        inline_file_repo: Arc<dyn InlineFileRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        Self::new_with_tool_result_cache(
            inline_file_repo,
            session_persistence,
            SessionToolResultCache::new(),
        )
    }

    pub fn new_with_tool_result_cache(
        inline_file_repo: Arc<dyn InlineFileRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
        tool_result_cache: Arc<SessionToolResultCache>,
    ) -> Self {
        Self {
            inline_file_repo,
            session_persistence,
            tool_result_cache,
        }
    }

    pub fn session_persistence(&self) -> &dyn SessionPersistence {
        self.session_persistence.as_ref()
    }

    pub async fn session_events(
        &self,
        session_id: &str,
    ) -> JourneyResult<Vec<PersistedSessionEvent>> {
        self.session_persistence
            .list_all_events(session_id)
            .await
            .map_err(|e| {
                LifecycleJourneyError::OperationFailed(format!("读取 session events 失败: {e}"))
            })
    }

    pub async fn read_session_projection(
        &self,
        session_id: &str,
        rest: &[&str],
    ) -> JourneyResult<String> {
        match rest {
            ["meta"] => {
                let meta = self
                    .session_persistence
                    .get_session_meta(session_id)
                    .await
                    .map_err(|e| {
                        LifecycleJourneyError::OperationFailed(format!(
                            "读取 session meta 失败: {e}"
                        ))
                    })?
                    .ok_or_else(|| {
                        LifecycleJourneyError::NotFound(format!("session 不存在: {session_id}"))
                    })?;
                let meta_json = serde_json::json!({
                    "session_id": session_id,
                    "title": meta.title,
                    "status": meta.last_delivery_status,
                    "last_event_seq": meta.last_event_seq,
                    "created_at": meta.created_at,
                    "updated_at": meta.updated_at,
                });
                to_json_pretty(&meta_json)
            }
            ["events.json"] => {
                let events = self.session_events(session_id).await?;
                to_json_pretty(&events)
            }
            ["items"] => {
                self.read_items_index(session_id, SessionItemView::Items)
                    .await
            }
            ["items", rest @ ..] => {
                self.read_item_file(session_id, SessionItemView::Items, rest)
                    .await
            }
            ["messages"] => {
                self.read_items_index(session_id, SessionItemView::Messages)
                    .await
            }
            ["messages", rest @ ..] => {
                self.read_item_file(session_id, SessionItemView::Messages, rest)
                    .await
            }
            ["tools"] => {
                self.read_items_index(session_id, SessionItemView::Tools)
                    .await
            }
            ["tools", rest @ ..] => {
                self.read_item_file(session_id, SessionItemView::Tools, rest)
                    .await
            }
            ["tool-results"] => self.read_tool_results_index(session_id).await,
            ["tool-results", turn_alias] => {
                self.read_tool_results_turn_index(session_id, turn_alias)
                    .await
            }
            ["tool-results", turn_alias, body_alias, "metadata.json"] => {
                let item_id = tool_result_item_id(turn_alias, body_alias);
                self.read_tool_result_metadata(session_id, &item_id).await
            }
            ["tool-results", turn_alias, body_alias, "result.txt"] => {
                let item_id = tool_result_item_id(turn_alias, body_alias);
                self.read_tool_result_body_status(session_id, &item_id)
                    .await
            }
            ["writes"] => {
                self.read_items_index(session_id, SessionItemView::Writes)
                    .await
            }
            ["writes", rest @ ..] => {
                self.read_item_file(session_id, SessionItemView::Writes, rest)
                    .await
            }
            ["summaries"] => self.read_compaction_summary_index(session_id).await,
            ["summaries", rest @ ..] => self.read_compaction_summary(session_id, rest).await,
            ["turns"] => {
                let events = self.session_events(session_id).await?;
                let summaries = group_events_into_turn_summaries(&events);
                to_json_pretty(&summaries)
            }
            ["turns", turn_id] | ["turns", turn_id, "events.json"] => {
                let events = self.session_events(session_id).await?;
                let turn_events: Vec<&PersistedSessionEvent> = events
                    .iter()
                    .filter(|event| event.turn_id.as_deref() == Some(*turn_id))
                    .collect();
                if turn_events.is_empty() {
                    return Err(LifecycleJourneyError::NotFound(format!(
                        "turn 不存在: {turn_id}"
                    )));
                }
                to_json_pretty(&turn_events)
            }
            ["terminal", file_name] if file_name.ends_with(".metadata.json") => {
                let terminal_alias = file_name
                    .strip_suffix(".metadata.json")
                    .unwrap_or(file_name);
                self.read_terminal_metadata(session_id, terminal_alias)
                    .await
            }
            ["terminal", file_name] if file_name.ends_with(".log") => {
                let terminal_alias = file_name.strip_suffix(".log").unwrap_or(file_name);
                self.read_terminal_log_status(session_id, terminal_alias)
                    .await
            }
            ["terminal"] => {
                let events = self.session_events(session_id).await?;
                let output = events
                    .iter()
                    .filter_map(|event| match &event.notification.event {
                        BackboneEvent::CommandOutputDelta(delta) => Some(delta.delta.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if output.is_empty() {
                    return Err(LifecycleJourneyError::NotFound(
                        "session 没有 terminal 输出".to_string(),
                    ));
                }
                Ok(output)
            }
            _ => Err(LifecycleJourneyError::NotFound(format!(
                "session projection 不支持的路径: {}",
                rest.join("/")
            ))),
        }
    }

    pub async fn read_tool_results_index(&self, session_id: &str) -> JourneyResult<String> {
        let projections = self.session_item_projections(session_id).await?;
        let metadata = filter_session_items(&projections, SessionItemView::Tools)
            .iter()
            .filter_map(|projection| {
                let item_id = projection.summary.item_id.as_str();
                tool_result_metadata_for_projection_with_status(
                    session_id,
                    projection,
                    self.tool_result_body_status(session_id, item_id),
                )
            })
            .collect::<Vec<_>>();
        to_json_pretty(&metadata)
    }

    pub async fn read_tool_results_turn_index(
        &self,
        session_id: &str,
        turn_alias: &str,
    ) -> JourneyResult<String> {
        let projections = self.session_item_projections(session_id).await?;
        let metadata = filter_session_items(&projections, SessionItemView::Tools)
            .iter()
            .filter_map(|projection| {
                let item_id = projection.summary.item_id.as_str();
                tool_result_metadata_for_projection_with_status(
                    session_id,
                    projection,
                    self.tool_result_body_status(session_id, item_id),
                )
            })
            .filter(|entry| entry.turn_alias == turn_alias)
            .collect::<Vec<_>>();
        if metadata.is_empty() {
            return Err(LifecycleJourneyError::NotFound(format!(
                "tool result turn 不存在: {turn_alias}"
            )));
        }
        to_json_pretty(&metadata)
    }

    pub async fn read_tool_result_metadata(
        &self,
        session_id: &str,
        item_id: &str,
    ) -> JourneyResult<String> {
        let projections = self.session_item_projections(session_id).await?;
        let metadata = filter_session_items(&projections, SessionItemView::Tools)
            .iter()
            .find(|projection| projection.summary.item_id == item_id)
            .and_then(|projection| {
                tool_result_metadata_for_projection_with_status(
                    session_id,
                    projection,
                    self.tool_result_body_status(session_id, item_id),
                )
            })
            .ok_or_else(|| {
                LifecycleJourneyError::NotFound(format!("tool result 不存在: {item_id}"))
            })?;
        to_json_pretty(&metadata)
    }

    pub async fn read_tool_result_body_status(
        &self,
        session_id: &str,
        item_id: &str,
    ) -> JourneyResult<String> {
        let projections = self.session_item_projections(session_id).await?;
        let exists = filter_session_items(&projections, SessionItemView::Tools)
            .iter()
            .any(|projection| projection.summary.item_id == item_id);
        if !exists {
            return Err(LifecycleJourneyError::NotFound(format!(
                "tool result 不存在: {item_id}"
            )));
        }
        match self.tool_result_cache.read_text(session_id, item_id) {
            SessionToolResultCacheRead::Available { text, .. } => Ok(text),
            SessionToolResultCacheRead::Unavailable(status) => Ok(status.message),
        }
    }

    fn tool_result_body_status(&self, session_id: &str, item_id: &str) -> SessionLargeBodyStatus {
        match self.tool_result_cache.read_text(session_id, item_id) {
            SessionToolResultCacheRead::Available { metadata, .. } => SessionLargeBodyStatus {
                status: "available".to_string(),
                message: format!(
                    "result body is available from the current session cache ({} bytes stored, {} original bytes).",
                    metadata.stored_bytes, metadata.original_bytes
                ),
            },
            SessionToolResultCacheRead::Unavailable(status) => tool_result_cache_status(status),
        }
    }

    pub async fn terminal_metadata_entries(
        &self,
        session_id: &str,
    ) -> JourneyResult<Vec<SessionTerminalMetadata>> {
        let events = self.session_events(session_id).await?;
        let mut entries: BTreeMap<String, SessionTerminalMetadataBuilder> = BTreeMap::new();
        for event in &events {
            match &event.notification.event {
                BackboneEvent::Platform(PlatformEvent::TerminalOutput { terminal_id, data }) => {
                    let next_index = entries.len() + 1;
                    entries
                        .entry(terminal_id.clone())
                        .or_insert_with(|| {
                            SessionTerminalMetadataBuilder::new(
                                session_id,
                                terminal_id,
                                format_readable_alias("term", next_index),
                            )
                        })
                        .apply_output(event, data);
                }
                BackboneEvent::Platform(PlatformEvent::TerminalStateChanged {
                    terminal_id,
                    state,
                    exit_code,
                    message,
                }) => {
                    let next_index = entries.len() + 1;
                    entries
                        .entry(terminal_id.clone())
                        .or_insert_with(|| {
                            SessionTerminalMetadataBuilder::new(
                                session_id,
                                terminal_id,
                                format_readable_alias("term", next_index),
                            )
                        })
                        .apply_state(event, state, *exit_code, message.as_deref());
                }
                _ => {}
            }
        }
        Ok(entries
            .into_values()
            .map(SessionTerminalMetadataBuilder::finish)
            .collect())
    }

    pub async fn read_terminal_metadata(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> JourneyResult<String> {
        let metadata = self
            .terminal_metadata_entries(session_id)
            .await?
            .into_iter()
            .find(|entry| entry.terminal_id == terminal_id)
            .ok_or_else(|| {
                LifecycleJourneyError::NotFound(format!("terminal 不存在: {terminal_id}"))
            })?;
        to_json_pretty(&metadata)
    }

    pub async fn read_terminal_log_status(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> JourneyResult<String> {
        let metadata = self
            .terminal_metadata_entries(session_id)
            .await?
            .into_iter()
            .find(|entry| entry.terminal_id == terminal_id)
            .ok_or_else(|| {
                LifecycleJourneyError::NotFound(format!("terminal 不存在: {terminal_id}"))
            })?;
        Ok(format!(
            "[terminal log cache missing]\nsession_id: {session_id}\nterminal_id: {}\nlifecycle_path: {}\nThe original terminal log is not available from the retained output cache.",
            metadata.terminal_id, metadata.lifecycle_path
        ))
    }

    pub async fn session_item_projections(
        &self,
        session_id: &str,
    ) -> JourneyResult<Vec<SessionItemProjection>> {
        let events = self.session_events(session_id).await?;
        Ok(session_item_projections(&events))
    }

    pub async fn read_items_index(
        &self,
        session_id: &str,
        view: SessionItemView,
    ) -> JourneyResult<String> {
        let projections = self.session_item_projections(session_id).await?;
        let summaries = filter_session_items(&projections, view)
            .iter()
            .map(|projection| item_summary_for_view(projection, view))
            .collect::<Vec<SessionItemSummary>>();
        to_json_pretty(&summaries)
    }

    pub async fn read_item_file(
        &self,
        session_id: &str,
        view: SessionItemView,
        rest: &[&str],
    ) -> JourneyResult<String> {
        let name = join_rest(rest)?;
        let projections = self.session_item_projections(session_id).await?;
        let projection = filter_session_items(&projections, view)
            .into_iter()
            .find(|projection| item_file_name(projection, view) == name)
            .ok_or_else(|| {
                LifecycleJourneyError::NotFound(format!("session item 不存在: {name}"))
            })?;
        render_item_content(&projection, view)
    }

    pub async fn read_compaction_summary_index(&self, session_id: &str) -> JourneyResult<String> {
        let entries = session_summary_archives(self.session_persistence.as_ref(), session_id)
            .await?
            .into_iter()
            .map(|(entry, _)| entry)
            .collect::<Vec<_>>();
        to_json_pretty(&entries)
    }

    pub async fn read_compaction_summary(
        &self,
        session_id: &str,
        rest: &[&str],
    ) -> JourneyResult<String> {
        let name = join_rest(rest)?;
        let entries =
            session_summary_archives(self.session_persistence.as_ref(), session_id).await?;
        let (_, compaction) = entries
            .into_iter()
            .find(|(entry, _)| {
                entry
                    .path
                    .strip_prefix("session/summaries/")
                    .is_some_and(|path| path == name)
            })
            .ok_or_else(|| {
                LifecycleJourneyError::NotFound(format!("compaction summary 不存在: {name}"))
            })?;
        Ok(summary_archive_markdown(&compaction))
    }

    pub async fn list_scoped_port_outputs(
        &self,
        scope: &RuntimeNodeArtifactScope,
    ) -> JourneyResult<BTreeMap<String, String>> {
        let prefix = scope.path_prefix();
        let files = self
            .inline_file_repo
            .list_files(
                InlineFileOwnerKind::LifecycleRun,
                scope.run_id,
                PORT_OUTPUTS_CONTAINER,
            )
            .await
            .map_err(map_domain_err)?;
        Ok(files
            .into_iter()
            .filter_map(|file| {
                let port_key = file.path.strip_prefix(&prefix)?.to_string();
                if port_key.is_empty() || port_key.contains('/') {
                    return None;
                }
                file.into_text_content().map(|content| (port_key, content))
            })
            .collect())
    }

    pub async fn read_scoped_port_output(
        &self,
        artifact_ref: &RuntimeNodePortArtifactRef,
    ) -> JourneyResult<String> {
        self.inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                artifact_ref.run_id,
                PORT_OUTPUTS_CONTAINER,
                &artifact_ref.inline_path(),
            )
            .await
            .map_err(map_domain_err)?
            .and_then(|file| file.into_text_content())
            .ok_or_else(|| {
                LifecycleJourneyError::NotFound(format!(
                    "port output 不存在: {}",
                    artifact_ref.inline_path()
                ))
            })
    }

    pub async fn write_scoped_port_output(
        &self,
        artifact_ref: &RuntimeNodePortArtifactRef,
        content: &str,
    ) -> JourneyResult<()> {
        let file = InlineFile::new(
            InlineFileOwnerKind::LifecycleRun,
            artifact_ref.run_id,
            PORT_OUTPUTS_CONTAINER,
            artifact_ref.inline_path(),
            content.to_string(),
        );
        self.inline_file_repo
            .upsert_file(&file)
            .await
            .map_err(map_domain_err)
    }

    pub async fn records_map(
        &self,
        run_id: Uuid,
        activity_key: &str,
    ) -> JourneyResult<BTreeMap<String, String>> {
        let files = self
            .inline_file_repo
            .list_files(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                JOURNEY_RECORDS_CONTAINER,
            )
            .await
            .map_err(map_domain_err)?;
        let prefix = format!("{activity_key}/");
        Ok(files
            .into_iter()
            .filter_map(|file| {
                let content = file.clone().into_text_content()?;
                file.path
                    .strip_prefix(&prefix)
                    .map(|path| (path.to_string(), content))
            })
            .collect())
    }

    pub async fn read_records_map(
        &self,
        run_id: Uuid,
        activity_key: &str,
    ) -> JourneyResult<String> {
        let map = self.records_map(run_id, activity_key).await?;
        to_json_pretty(&map)
    }

    pub async fn read_record(
        &self,
        run_id: Uuid,
        activity_key: &str,
        rest: &[&str],
    ) -> JourneyResult<String> {
        let name = join_rest(rest)?;
        let path = format!("{activity_key}/{name}");
        self.inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                JOURNEY_RECORDS_CONTAINER,
                &path,
            )
            .await
            .map_err(map_domain_err)?
            .and_then(|file| file.into_text_content())
            .ok_or_else(|| LifecycleJourneyError::NotFound(format!("record 不存在: {name}")))
    }

    pub async fn write_record(
        &self,
        run_id: Uuid,
        activity_key: &str,
        rest: &[&str],
        content: &str,
    ) -> JourneyResult<String> {
        let name = join_rest(rest)?;
        let path = format!("{activity_key}/{name}");
        let file = InlineFile::new(
            InlineFileOwnerKind::LifecycleRun,
            run_id,
            JOURNEY_RECORDS_CONTAINER,
            path,
            content.to_string(),
        );
        self.inline_file_repo
            .upsert_file(&file)
            .await
            .map_err(map_domain_err)?;
        Ok(name)
    }

    pub async fn read_node_summary(
        &self,
        run_id: Uuid,
        step: &RuntimeNodeState,
    ) -> JourneyResult<String> {
        if let Ok(Some(file)) = self
            .inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                SESSION_RECORDS_CONTAINER,
                &format!("{}/summary", step.node_path),
            )
            .await
            && let Some(content) = file.into_text_content()
        {
            return Ok(content);
        }

        Err(LifecycleJourneyError::NotFound(format!(
            "node `{}` 没有 summary",
            step.node_path
        )))
    }

    pub async fn read_node_conclusions(
        &self,
        run_id: Uuid,
        activity_key: &str,
    ) -> JourneyResult<String> {
        self.inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                SESSION_RECORDS_CONTAINER,
                &format!("{activity_key}/conclusions"),
            )
            .await
            .map_err(map_domain_err)?
            .and_then(|file| file.into_text_content())
            .ok_or_else(|| {
                LifecycleJourneyError::NotFound(format!("node `{activity_key}` 没有 conclusions"))
            })
    }
}

#[derive(Serialize)]
pub struct LifecycleRunOverview<'a> {
    id: Uuid,
    project_id: Uuid,
    status: &'a LifecycleRunStatus,
    active_runtime_node_refs: Vec<String>,
    step_count: usize,
    log_count: usize,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_activity_at: chrono::DateTime<chrono::Utc>,
}

pub fn run_overview<'a>(run: &'a LifecycleRun) -> LifecycleRunOverview<'a> {
    let active_runtime_node_refs = active_runtime_node_refs(run);
    LifecycleRunOverview {
        id: run.id,
        project_id: run.project_id,
        status: &run.status,
        active_runtime_node_refs,
        step_count: run
            .orchestrations
            .iter()
            .map(|instance| flatten_runtime_nodes(&instance.node_tree).len())
            .sum(),
        log_count: run.execution_log.len(),
        created_at: run.created_at,
        updated_at: run.updated_at,
        last_activity_at: run.last_activity_at,
    }
}

fn active_runtime_node_refs(run: &LifecycleRun) -> Vec<String> {
    run.orchestrations
        .iter()
        .flat_map(|instance| {
            flatten_runtime_nodes(&instance.node_tree)
                .into_iter()
                .filter(|node| {
                    matches!(
                        node.status,
                        RuntimeNodeStatus::Ready
                            | RuntimeNodeStatus::Claiming
                            | RuntimeNodeStatus::Running
                            | RuntimeNodeStatus::Blocked
                    )
                })
                .map(move |node| {
                    format!(
                        "{}:{}#{}:{:?}",
                        instance.orchestration_id, node.node_path, node.attempt, node.status
                    )
                })
        })
        .collect()
}

#[derive(Serialize)]
pub struct TurnSummary {
    pub turn_id: String,
    event_count: usize,
    first_event_type: String,
    first_occurred_at_ms: i64,
    last_occurred_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionTerminalMetadata {
    pub session_id: String,
    pub terminal_id: String,
    pub raw_terminal_id: String,
    pub metadata_path: String,
    pub log_path: String,
    pub lifecycle_path: String,
    pub body_status: String,
    pub event_count: usize,
    pub output_bytes: usize,
    pub first_event_seq: Option<u64>,
    pub last_event_seq: Option<u64>,
    pub last_state: Option<String>,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
    pub preview: String,
}

struct SessionTerminalMetadataBuilder {
    session_id: String,
    raw_terminal_id: String,
    terminal_alias: String,
    event_count: usize,
    output_bytes: usize,
    first_event_seq: Option<u64>,
    last_event_seq: Option<u64>,
    last_state: Option<String>,
    exit_code: Option<i32>,
    message: Option<String>,
    preview: String,
}

impl SessionTerminalMetadataBuilder {
    fn new(session_id: &str, terminal_id: &str, terminal_alias: String) -> Self {
        Self {
            session_id: session_id.to_string(),
            raw_terminal_id: terminal_id.to_string(),
            terminal_alias,
            event_count: 0,
            output_bytes: 0,
            first_event_seq: None,
            last_event_seq: None,
            last_state: None,
            exit_code: None,
            message: None,
            preview: String::new(),
        }
    }

    fn apply_output(&mut self, event: &PersistedSessionEvent, data: &str) {
        self.apply_event_seq(event);
        self.output_bytes = self.output_bytes.saturating_add(data.len());
        if self.preview.len() < 2048 {
            self.preview.push_str(data);
            if self.preview.len() > 2048 {
                self.preview
                    .truncate(previous_char_boundary(&self.preview, 2048));
            }
        }
    }

    fn apply_state(
        &mut self,
        event: &PersistedSessionEvent,
        state: &str,
        exit_code: Option<i32>,
        message: Option<&str>,
    ) {
        self.apply_event_seq(event);
        self.last_state = Some(state.to_string());
        self.exit_code = exit_code;
        self.message = message.map(ToOwned::to_owned);
    }

    fn apply_event_seq(&mut self, event: &PersistedSessionEvent) {
        self.event_count += 1;
        self.first_event_seq = Some(
            self.first_event_seq
                .map_or(event.event_seq, |seq| seq.min(event.event_seq)),
        );
        self.last_event_seq = Some(
            self.last_event_seq
                .map_or(event.event_seq, |seq| seq.max(event.event_seq)),
        );
    }

    fn finish(self) -> SessionTerminalMetadata {
        let log_path = format!("session/terminal/{}.log", self.terminal_alias);
        SessionTerminalMetadata {
            session_id: self.session_id,
            terminal_id: self.terminal_alias.clone(),
            raw_terminal_id: self.raw_terminal_id,
            metadata_path: format!("session/terminal/{}.metadata.json", self.terminal_alias),
            lifecycle_path: format!("lifecycle://session/terminal/{}.log", self.terminal_alias),
            log_path,
            body_status: "cache_miss".to_string(),
            event_count: self.event_count,
            output_bytes: self.output_bytes,
            first_event_seq: self.first_event_seq,
            last_event_seq: self.last_event_seq,
            last_state: self.last_state,
            exit_code: self.exit_code,
            message: self.message,
            preview: if self.preview.is_empty() {
                "empty".to_string()
            } else {
                self.preview
            },
        }
    }
}

pub fn group_events_into_turn_summaries(events: &[PersistedSessionEvent]) -> Vec<TurnSummary> {
    let mut groups: BTreeMap<String, Vec<&PersistedSessionEvent>> = BTreeMap::new();
    for event in events {
        if let Some(turn_id) = event.turn_id.as_deref() {
            groups.entry(turn_id.to_string()).or_default().push(event);
        }
    }
    groups
        .into_iter()
        .map(|(turn_id, turn_events)| {
            let first = turn_events.first().unwrap();
            let last = turn_events.last().unwrap();
            TurnSummary {
                turn_id,
                event_count: turn_events.len(),
                first_event_type: first.session_update_type.clone(),
                first_occurred_at_ms: first.occurred_at_ms,
                last_occurred_at_ms: last.occurred_at_ms,
            }
        })
        .collect()
}

/// 取展示用的 runtime node 列表。
pub fn step_states_from_runtime_nodes(nodes: &[RuntimeNodeState]) -> Vec<RuntimeNodeState> {
    flatten_runtime_nodes(nodes).into_iter().cloned().collect()
}

pub fn find_step(nodes: &[RuntimeNodeState], key: &str) -> JourneyResult<RuntimeNodeState> {
    flatten_runtime_nodes(nodes)
        .into_iter()
        .find(|node| node.node_path == key || node.node_id == key)
        .cloned()
        .ok_or_else(|| LifecycleJourneyError::NotFound(format!("node 不存在: {key}")))
}

pub fn current_step(nodes: &[RuntimeNodeState]) -> JourneyResult<RuntimeNodeState> {
    flatten_runtime_nodes(nodes)
        .into_iter()
        .find(|node| {
            matches!(
                node.status,
                RuntimeNodeStatus::Ready
                    | RuntimeNodeStatus::Claiming
                    | RuntimeNodeStatus::Running
                    | RuntimeNodeStatus::Blocked
            )
        })
        .cloned()
        .ok_or_else(|| {
            LifecycleJourneyError::NotFound("当前 orchestration 没有活跃 node".to_string())
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JourneyNodeCoordinate {
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

pub fn attempt_coordinate(
    orchestration_id: Uuid,
    attempt: &RuntimeNodeState,
) -> JourneyNodeCoordinate {
    JourneyNodeCoordinate {
        orchestration_id,
        node_path: attempt.node_path.clone(),
        attempt: attempt.attempt,
    }
}

pub fn step_coordinate(
    nodes: &[RuntimeNodeState],
    orchestration_id: Uuid,
    key: &str,
) -> JourneyResult<JourneyNodeCoordinate> {
    Ok(attempt_coordinate(
        orchestration_id,
        &find_step(nodes, key)?,
    ))
}

pub fn current_step_coordinate(
    nodes: &[RuntimeNodeState],
    orchestration_id: Uuid,
) -> JourneyResult<JourneyNodeCoordinate> {
    Ok(attempt_coordinate(orchestration_id, &current_step(nodes)?))
}

/// Trace helper: 从 node executor ref 提取 delivery RuntimeSession，用于打开 transcript。
pub fn attempt_session_id(attempt: &RuntimeNodeState) -> Option<String> {
    match &attempt.executor_run_ref {
        Some(ExecutorRunRef::RuntimeSession { session_id }) => Some(session_id.clone()),
        _ => None,
    }
}

/// Trace helper: 查找 node 后返回 delivery RuntimeSession id，用于打开 transcript。
pub fn step_session_id(nodes: &[RuntimeNodeState], key: &str) -> JourneyResult<String> {
    attempt_session_id(&find_step(nodes, key)?)
        .ok_or_else(|| LifecycleJourneyError::NotFound(format!("node `{key}` 没有关联 session")))
}

/// Trace helper: 查找当前 active node 后返回 delivery RuntimeSession id。
pub fn current_step_session_id(nodes: &[RuntimeNodeState]) -> JourneyResult<(String, String)> {
    let attempt = current_step(nodes)?;
    let session_id = attempt_session_id(&attempt).ok_or_else(|| {
        LifecycleJourneyError::NotFound(format!("node `{}` 没有关联 session", attempt.node_path))
    })?;
    Ok((attempt.node_path, session_id))
}

fn flatten_runtime_nodes(nodes: &[RuntimeNodeState]) -> Vec<&RuntimeNodeState> {
    fn collect<'a>(node: &'a RuntimeNodeState, acc: &mut Vec<&'a RuntimeNodeState>) {
        acc.push(node);
        for child in &node.children {
            collect(child, acc);
        }
    }
    let mut flattened = Vec::new();
    for node in nodes {
        collect(node, &mut flattened);
    }
    flattened
}

pub fn join_rest(rest: &[&str]) -> JourneyResult<String> {
    let joined = rest.join("/");
    if joined.trim().is_empty() {
        Err(LifecycleJourneyError::OperationFailed(
            "路径不能为空".to_string(),
        ))
    } else {
        Ok(joined)
    }
}

fn tool_result_item_id(turn_alias: &str, body_alias: &str) -> String {
    format!("{turn_alias}:{body_alias}")
}

fn format_readable_alias(prefix: &str, index: usize) -> String {
    if index < 1000 {
        format!("{prefix}_{index:03}")
    } else {
        format!("{prefix}_{index}")
    }
}

pub fn to_json_pretty<T: Serialize>(value: &T) -> JourneyResult<String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| LifecycleJourneyError::OperationFailed(error.to_string()))
}

fn previous_char_boundary(value: &str, max_bytes: usize) -> usize {
    let mut boundary = max_bytes.min(value.len());
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

fn tool_result_cache_status(status: SessionToolResultCacheStatus) -> SessionLargeBodyStatus {
    let status_name = match status.status {
        SessionToolResultCacheStatusKind::Missing => "cache_miss",
        SessionToolResultCacheStatusKind::Expired => "expired",
    };
    SessionLargeBodyStatus {
        status: status_name.to_string(),
        message: status.message,
    }
}

fn map_domain_err(error: agentdash_domain::common::error::DomainError) -> LifecycleJourneyError {
    LifecycleJourneyError::OperationFailed(error.to_string())
}
