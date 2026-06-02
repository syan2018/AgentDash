//! Lifecycle journey 的业务投影层。
//!
//! 这里负责把 lifecycle run、session events 和 inline overlay 组合成稳定的
//! journey 视图；VFS provider 只负责把这些视图映射到 `lifecycle://...` 路径。

use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_agent_protocol::BackboneEvent;
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use agentdash_domain::workflow::{
    ActivityAttemptState, ActivityAttemptStatus, ExecutorRunRef, LifecycleRun, LifecycleRunStatus,
    WorkflowGraphInstance,
};
use serde::Serialize;
use uuid::Uuid;

use crate::session::{PersistedSessionEvent, SessionPersistence};

pub mod session_items;

pub use session_items::{
    SessionItemProjection, SessionItemSummary, SessionItemView, filter_session_items,
    item_file_name, item_summary_for_view, render_item_content, session_item_projections,
    session_summary_archives, summary_archive_markdown,
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
}

impl LifecycleJourneyProjection {
    pub fn new(
        inline_file_repo: Arc<dyn InlineFileRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        Self {
            inline_file_repo,
            session_persistence,
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
                    "status": meta.last_execution_status,
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

    pub async fn list_port_outputs(&self, run_id: Uuid) -> JourneyResult<BTreeMap<String, String>> {
        let files = self
            .inline_file_repo
            .list_files(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                PORT_OUTPUTS_CONTAINER,
            )
            .await
            .map_err(map_domain_err)?;
        Ok(files
            .into_iter()
            .filter_map(|file| {
                let path = file.path.clone();
                file.into_text_content().map(|content| (path, content))
            })
            .collect())
    }

    pub async fn read_port_output(&self, run_id: Uuid, port_key: &str) -> JourneyResult<String> {
        self.inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                PORT_OUTPUTS_CONTAINER,
                port_key,
            )
            .await
            .map_err(map_domain_err)?
            .and_then(|file| file.into_text_content())
            .ok_or_else(|| {
                LifecycleJourneyError::NotFound(format!("port output 不存在: {port_key}"))
            })
    }

    /// 读取 activity attempt 级别的 scoped port output。
    pub async fn read_scoped_port_output(
        &self,
        run_id: Uuid,
        scoped_path: &str,
    ) -> JourneyResult<String> {
        self.inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                PORT_OUTPUTS_CONTAINER,
                scoped_path,
            )
            .await
            .map_err(map_domain_err)?
            .and_then(|file| file.into_text_content())
            .ok_or_else(|| {
                LifecycleJourneyError::NotFound(format!("port output 不存在: {scoped_path}"))
            })
    }

    pub async fn write_port_output(
        &self,
        run_id: Uuid,
        port_key: &str,
        content: &str,
    ) -> JourneyResult<()> {
        let file = InlineFile::new(
            InlineFileOwnerKind::LifecycleRun,
            run_id,
            PORT_OUTPUTS_CONTAINER,
            port_key.to_string(),
            content.to_string(),
        );
        self.inline_file_repo
            .upsert_file(&file)
            .await
            .map_err(map_domain_err)
    }

    /// 写入 activity attempt 级别的 scoped port output。
    pub async fn write_scoped_port_output(
        &self,
        run_id: Uuid,
        scoped_path: &str,
        content: &str,
    ) -> JourneyResult<()> {
        let file = InlineFile::new(
            InlineFileOwnerKind::LifecycleRun,
            run_id,
            PORT_OUTPUTS_CONTAINER,
            scoped_path.to_string(),
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
        step: &ActivityAttemptState,
    ) -> JourneyResult<String> {
        if let Ok(Some(file)) = self
            .inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                SESSION_RECORDS_CONTAINER,
                &format!("{}/summary", step.activity_key),
            )
            .await
            && let Some(content) = file.into_text_content()
        {
            return Ok(content);
        }

        step.summary.clone().ok_or_else(|| {
            LifecycleJourneyError::NotFound(format!("node `{}` 没有 summary", step.activity_key))
        })
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
    root_graph_id: Uuid,
    status: &'a LifecycleRunStatus,
    current_activity_key: Option<String>,
    step_count: usize,
    log_count: usize,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_activity_at: chrono::DateTime<chrono::Utc>,
}

pub fn run_overview<'a>(
    run: &'a LifecycleRun,
    graph_instances: &[WorkflowGraphInstance],
) -> LifecycleRunOverview<'a> {
    LifecycleRunOverview {
        id: run.id,
        project_id: run.project_id,
        root_graph_id: run.root_graph_id,
        status: &run.status,
        current_activity_key: run.active_node_keys.first().cloned(),
        step_count: graph_instances
            .iter()
            .filter_map(|instance| instance.activity_state.as_ref())
            .map(|state| state.attempts.len())
            .sum(),
        log_count: run.execution_log.len(),
        created_at: run.created_at,
        updated_at: run.updated_at,
        last_activity_at: run.last_activity_at,
    }
}

#[derive(Serialize)]
pub struct TurnSummary {
    pub turn_id: String,
    event_count: usize,
    first_event_type: String,
    first_occurred_at_ms: i64,
    last_occurred_at_ms: i64,
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

/// 取展示用的 Activity attempt 列表（投影 node 列表用）。
pub fn step_states_from_graph_instance(
    graph_instance: &WorkflowGraphInstance,
) -> JourneyResult<Vec<ActivityAttemptState>> {
    Ok(graph_instance_state(graph_instance)?.attempts.clone())
}

pub fn find_step(
    graph_instance: &WorkflowGraphInstance,
    key: &str,
) -> JourneyResult<ActivityAttemptState> {
    graph_instance_state(graph_instance)?
        .attempts
        .iter()
        .find(|attempt| attempt.activity_key == key)
        .cloned()
        .ok_or_else(|| LifecycleJourneyError::NotFound(format!("node 不存在: {key}")))
}

pub fn current_step(graph_instance: &WorkflowGraphInstance) -> JourneyResult<ActivityAttemptState> {
    graph_instance_state(graph_instance)?
        .attempts
        .iter()
        .find(|attempt| {
            matches!(
                attempt.status,
                ActivityAttemptStatus::Ready
                    | ActivityAttemptStatus::Claiming
                    | ActivityAttemptStatus::Running
            )
        })
        .cloned()
        .ok_or_else(|| {
            LifecycleJourneyError::NotFound("当前 graph instance 没有活跃 node".to_string())
        })
}

pub fn attempt_session_id(attempt: &ActivityAttemptState) -> Option<String> {
    match &attempt.executor_run {
        Some(ExecutorRunRef::RuntimeSession { session_id }) => Some(session_id.clone()),
        _ => None,
    }
}

pub fn step_session_id(graph_instance: &WorkflowGraphInstance, key: &str) -> JourneyResult<String> {
    attempt_session_id(&find_step(graph_instance, key)?)
        .ok_or_else(|| LifecycleJourneyError::NotFound(format!("node `{key}` 没有关联 session")))
}

pub fn current_step_session_id(
    graph_instance: &WorkflowGraphInstance,
) -> JourneyResult<(String, String)> {
    let attempt = current_step(graph_instance)?;
    let session_id = attempt_session_id(&attempt).ok_or_else(|| {
        LifecycleJourneyError::NotFound(format!("node `{}` 没有关联 session", attempt.activity_key))
    })?;
    Ok((attempt.activity_key, session_id))
}

fn graph_instance_state(
    graph_instance: &WorkflowGraphInstance,
) -> JourneyResult<&agentdash_domain::workflow::ActivityLifecycleRunState> {
    graph_instance.activity_state.as_ref().ok_or_else(|| {
        LifecycleJourneyError::NotFound(format!(
            "graph instance {} 缺少 activity state",
            graph_instance.id
        ))
    })
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

pub fn to_json_pretty<T: Serialize>(value: &T) -> JourneyResult<String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| LifecycleJourneyError::OperationFailed(error.to_string()))
}

fn map_domain_err(error: agentdash_domain::common::error::DomainError) -> LifecycleJourneyError {
    LifecycleJourneyError::OperationFailed(error.to_string())
}
