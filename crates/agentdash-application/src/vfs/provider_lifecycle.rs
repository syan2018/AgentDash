//! `lifecycle_vfs` mount: 通过 `LifecycleRunRepository` 暴露当前 lifecycle run 的虚拟文件视图。

use std::collections::BTreeMap;
use std::sync::Arc;

use super::mount::{PROVIDER_LIFECYCLE_VFS, list_inline_entries};
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountError, MountOperationContext, MountProvider, SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::{Mount, RuntimeFileEntry};
use crate::session::{PersistedSessionEvent, SessionPersistence};
use agentdash_agent_protocol::BackboneEvent;
use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use agentdash_domain::workflow::{
    LifecycleRun, LifecycleRunRepository, LifecycleRunStatus, LifecycleStepState,
};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};
use tracing::info;
use uuid::Uuid;

const PORT_OUTPUTS_CONTAINER: &str = "port_outputs";
const SESSION_RECORDS_CONTAINER: &str = "session_records";
const JOURNEY_RECORDS_CONTAINER: &str = "journey_records";

pub struct LifecycleMountProvider {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    inline_file_repo: Arc<dyn InlineFileRepository>,
    session_persistence: Arc<dyn SessionPersistence>,
}

impl LifecycleMountProvider {
    pub fn new(
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            inline_file_repo,
            session_persistence,
        }
    }

    async fn session_events(
        &self,
        session_id: &str,
    ) -> Result<Vec<PersistedSessionEvent>, MountError> {
        self.session_persistence
            .list_all_events(session_id)
            .await
            .map_err(|e| MountError::OperationFailed(format!("读取 session events 失败: {e}")))
    }

    async fn read_session_projection(
        &self,
        session_id: &str,
        rest: &[&str],
    ) -> Result<String, MountError> {
        match rest {
            ["meta"] => {
                let meta = self
                    .session_persistence
                    .get_session_meta(session_id)
                    .await
                    .map_err(|e| {
                        MountError::OperationFailed(format!("读取 session meta 失败: {e}"))
                    })?
                    .ok_or_else(|| MountError::NotFound(format!("session 不存在: {session_id}")))?;
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
            ["turns"] => {
                let events = self.session_events(session_id).await?;
                let summaries = group_events_into_turn_summaries(&events);
                to_json_pretty(&summaries)
            }
            ["turns", turn_id] | ["turns", turn_id, "events.json"] => {
                let events = self.session_events(session_id).await?;
                let turn_events: Vec<&PersistedSessionEvent> = events
                    .iter()
                    .filter(|e| e.turn_id.as_deref() == Some(*turn_id))
                    .collect();
                if turn_events.is_empty() {
                    return Err(MountError::NotFound(format!("turn 不存在: {turn_id}")));
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
                    return Err(MountError::NotFound(
                        "session 没有 terminal 输出".to_string(),
                    ));
                }
                Ok(output)
            }
            _ => Err(MountError::NotFound(format!(
                "session projection 不支持的路径: {}",
                rest.join("/")
            ))),
        }
    }

    async fn read_tool_calls_projection(
        &self,
        session_id: &str,
        rest: &[&str],
    ) -> Result<String, MountError> {
        let events = self.session_events(session_id).await?;
        let projections = tool_call_projections(&events);
        match rest {
            [] => {
                let summaries: Vec<_> = projections.iter().map(|p| &p.summary).collect();
                to_json_pretty(&summaries)
            }
            [tool_call_id, "raw.json"] => {
                let projection = find_tool_projection(&projections, tool_call_id)?;
                to_json_pretty(&projection.raw_events)
            }
            [tool_call_id, "request.json"] => {
                let projection = find_tool_projection(&projections, tool_call_id)?;
                let request = projection.request.as_ref().ok_or_else(|| {
                    MountError::NotFound(format!("tool call `{tool_call_id}` 没有 request"))
                })?;
                to_json_pretty(request)
            }
            [tool_call_id, "result.json"] => {
                let projection = find_tool_projection(&projections, tool_call_id)?;
                let result = projection.result.as_ref().ok_or_else(|| {
                    MountError::NotFound(format!("tool call `{tool_call_id}` 没有 result"))
                })?;
                to_json_pretty(result)
            }
            [tool_call_id, "stdout.txt"] => {
                let projection = find_tool_projection(&projections, tool_call_id)?;
                if projection.stdout.is_empty() {
                    return Err(MountError::NotFound(format!(
                        "tool call `{tool_call_id}` 没有 stdout"
                    )));
                }
                Ok(projection.stdout.clone())
            }
            _ => Err(MountError::NotFound(format!(
                "tool-calls projection 不支持的路径: {}",
                rest.join("/")
            ))),
        }
    }

    async fn read_writes_projection(&self, session_id: &str) -> Result<String, MountError> {
        let events = self.session_events(session_id).await?;
        let writes = tool_call_projections(&events)
            .into_iter()
            .filter(is_write_projection)
            .map(|projection| projection.summary)
            .collect::<Vec<_>>();
        to_json_pretty(&writes)
    }

    async fn read_records_map(&self, run_id: Uuid, step_key: &str) -> Result<String, MountError> {
        let files = self
            .inline_file_repo
            .list_files(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                JOURNEY_RECORDS_CONTAINER,
            )
            .await
            .map_err(map_domain_err)?;
        let prefix = format!("{step_key}/");
        let map: BTreeMap<String, String> = files
            .into_iter()
            .filter_map(|file| {
                file.path
                    .strip_prefix(&prefix)
                    .map(|path| (path.to_string(), file.content))
            })
            .collect();
        to_json_pretty(&map)
    }

    async fn read_record(
        &self,
        run_id: Uuid,
        step_key: &str,
        rest: &[&str],
    ) -> Result<String, MountError> {
        let name = join_rest(rest)?;
        let path = format!("{step_key}/{name}");
        self.inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                JOURNEY_RECORDS_CONTAINER,
                &path,
            )
            .await
            .map_err(map_domain_err)?
            .map(|file| file.content)
            .ok_or_else(|| MountError::NotFound(format!("record 不存在: {name}")))
    }

    async fn read_node_summary(
        &self,
        run_id: Uuid,
        step: &LifecycleStepState,
    ) -> Result<String, MountError> {
        if let Ok(Some(file)) = self
            .inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                SESSION_RECORDS_CONTAINER,
                &format!("{}/summary", step.step_key),
            )
            .await
        {
            return Ok(file.content);
        }

        step.summary
            .clone()
            .ok_or_else(|| MountError::NotFound(format!("node `{}` 没有 summary", step.step_key)))
    }

    async fn read_node_conclusions(
        &self,
        run_id: Uuid,
        step_key: &str,
    ) -> Result<String, MountError> {
        self.inline_file_repo
            .get_file(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                SESSION_RECORDS_CONTAINER,
                &format!("{step_key}/conclusions"),
            )
            .await
            .map_err(map_domain_err)?
            .map(|file| file.content)
            .ok_or_else(|| MountError::NotFound(format!("node `{step_key}` 没有 conclusions")))
    }

    async fn list_record_entries(
        &self,
        run_id: Uuid,
        step_key: &str,
        display_root: &str,
        base_path: &str,
        options: &ListOptions,
    ) -> Result<Vec<RuntimeFileEntry>, MountError> {
        let files = self
            .inline_file_repo
            .list_files(
                InlineFileOwnerKind::LifecycleRun,
                run_id,
                JOURNEY_RECORDS_CONTAINER,
            )
            .await
            .map_err(map_domain_err)?;
        let prefix = format!("{step_key}/");
        let projected_files = files
            .into_iter()
            .filter_map(|file| {
                file.path.strip_prefix(&prefix).map(|relative| {
                    (
                        format!("{}/{}", display_root.trim_matches('/'), relative),
                        file.content,
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        Ok(list_inline_entries(
            &projected_files,
            base_path,
            options.pattern.as_deref(),
            options.recursive,
        ))
    }
}

fn find_tool_projection<'a>(
    projections: &'a [ToolCallProjection],
    tool_call_id: &str,
) -> Result<&'a ToolCallProjection, MountError> {
    projections
        .iter()
        .find(|projection| projection.summary.tool_call_id == tool_call_id)
        .ok_or_else(|| MountError::NotFound(format!("tool call 不存在: {tool_call_id}")))
}

#[derive(Serialize)]
struct LifecycleRunOverview<'a> {
    id: Uuid,
    project_id: Uuid,
    lifecycle_id: Uuid,
    session_id: &'a str,
    status: &'a LifecycleRunStatus,
    current_step_key: Option<&'a str>,
    step_count: usize,
    log_count: usize,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_activity_at: chrono::DateTime<chrono::Utc>,
}

fn run_overview(run: &LifecycleRun) -> LifecycleRunOverview<'_> {
    LifecycleRunOverview {
        id: run.id,
        project_id: run.project_id,
        lifecycle_id: run.lifecycle_id,
        session_id: &run.session_id,
        status: &run.status,
        current_step_key: run.current_step_key(),
        step_count: run.step_states.len(),
        log_count: run.execution_log.len(),
        created_at: run.created_at,
        updated_at: run.updated_at,
        last_activity_at: run.last_activity_at,
    }
}

fn map_domain_err(e: agentdash_domain::common::error::DomainError) -> MountError {
    MountError::OperationFailed(e.to_string())
}

fn parse_run_id_from_metadata(mount: &Mount) -> Result<Uuid, MountError> {
    let run_id_str = mount
        .metadata
        .get("run_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MountError::OperationFailed("mount metadata 缺少 run_id".to_string()))?;
    Uuid::parse_str(run_id_str)
        .map_err(|e| MountError::OperationFailed(format!("run_id 无效: {e}")))
}

fn resolve_session_id_for_runs(_mount: &Mount, active_run: &LifecycleRun) -> String {
    active_run.session_id.clone()
}

async fn load_active_run(
    repo: &Arc<dyn LifecycleRunRepository>,
    mount: &Mount,
) -> Result<LifecycleRun, MountError> {
    let run_id = parse_run_id_from_metadata(mount)?;
    let run = repo
        .get_by_id(run_id)
        .await
        .map_err(map_domain_err)?
        .ok_or_else(|| MountError::NotFound(format!("lifecycle run 不存在: {run_id}")))?;
    Ok(run)
}

fn to_json_pretty<T: Serialize>(v: &T) -> Result<String, MountError> {
    serde_json::to_string_pretty(v).map_err(|e| MountError::OperationFailed(e.to_string()))
}

fn segments_from_path(path: &str) -> Vec<&str> {
    if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect()
    }
}

fn find_step<'a>(run: &'a LifecycleRun, key: &str) -> Result<&'a LifecycleStepState, MountError> {
    run.step_states
        .iter()
        .find(|s| s.step_key == key)
        .ok_or_else(|| MountError::NotFound(format!("node 不存在: {key}")))
}

fn current_step<'a>(run: &'a LifecycleRun) -> Result<&'a LifecycleStepState, MountError> {
    let key = run
        .current_step_key()
        .ok_or_else(|| MountError::NotFound("当前 lifecycle run 没有活跃 node".to_string()))?;
    find_step(run, key)
}

fn step_session_id<'a>(run: &'a LifecycleRun, key: &str) -> Result<&'a str, MountError> {
    find_step(run, key)?
        .session_id
        .as_deref()
        .ok_or_else(|| MountError::NotFound(format!("node `{key}` 没有关联 session")))
}

fn current_step_session_id(run: &LifecycleRun) -> Result<(&str, &str), MountError> {
    let step = current_step(run)?;
    let session_id = step.session_id.as_deref().ok_or_else(|| {
        MountError::NotFound(format!("node `{}` 没有关联 session", step.step_key))
    })?;
    Ok((step.step_key.as_str(), session_id))
}

fn join_rest(rest: &[&str]) -> Result<String, MountError> {
    let joined = rest.join("/");
    if joined.trim().is_empty() {
        Err(MountError::OperationFailed("路径不能为空".to_string()))
    } else {
        Ok(joined)
    }
}

fn event_tool_call_id(event: &PersistedSessionEvent) -> Option<String> {
    if let Some(id) = event
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(id.to_string());
    }

    match &event.notification.event {
        BackboneEvent::ItemStarted(n) => tool_item_id(&n.item),
        BackboneEvent::ItemCompleted(n) => tool_item_id(&n.item),
        BackboneEvent::CommandOutputDelta(n) => Some(n.item_id.clone()),
        BackboneEvent::FileChangeDelta(n) => Some(n.item_id.clone()),
        BackboneEvent::McpToolCallProgress(n) => Some(n.item_id.clone()),
        _ => None,
    }
}

fn tool_item_id(item: &codex::ThreadItem) -> Option<String> {
    match item {
        codex::ThreadItem::DynamicToolCall { id, .. }
        | codex::ThreadItem::CommandExecution { id, .. }
        | codex::ThreadItem::McpToolCall { id, .. }
        | codex::ThreadItem::FileChange { id, .. }
        | codex::ThreadItem::CollabAgentToolCall { id, .. } => Some(id.clone()),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize)]
struct ToolCallSummary {
    tool_call_id: String,
    kind: String,
    name: String,
    provider: Option<String>,
    status: Option<String>,
    turn_id: Option<String>,
    first_event_seq: u64,
    last_event_seq: u64,
    event_count: usize,
    has_request: bool,
    has_result: bool,
    has_stdout: bool,
    is_error: bool,
}

#[derive(Debug, Clone)]
struct ToolCallProjection {
    summary: ToolCallSummary,
    request: Option<Value>,
    result: Option<Value>,
    stdout: String,
    raw_events: Vec<PersistedSessionEvent>,
}

#[derive(Debug, Clone)]
struct ToolSnapshot {
    kind: String,
    name: String,
    provider: Option<String>,
    status: Option<String>,
    request: Option<Value>,
    result: Option<Value>,
    stdout: Option<String>,
    is_error: bool,
}

fn tool_snapshot_from_item(item: &codex::ThreadItem) -> Option<ToolSnapshot> {
    match item {
        codex::ThreadItem::DynamicToolCall {
            tool,
            arguments,
            status,
            content_items,
            success,
            ..
        } => {
            let result = content_items
                .as_ref()
                .and_then(|items| serde_json::to_value(items).ok());
            Some(ToolSnapshot {
                kind: "dynamic_tool_call".to_string(),
                name: tool.clone(),
                provider: None,
                status: Some(dynamic_tool_call_status_str(status).to_string()),
                request: Some(arguments.clone()),
                stdout: content_items
                    .as_ref()
                    .map(|items| dynamic_content_items_text(items)),
                result,
                is_error: success == &Some(false)
                    || matches!(status, codex::DynamicToolCallStatus::Failed),
            })
        }
        codex::ThreadItem::McpToolCall {
            server,
            tool,
            arguments,
            status,
            result,
            error,
            ..
        } => {
            let result = result
                .as_ref()
                .and_then(|value| serde_json::to_value(value).ok())
                .or_else(|| error.as_ref().map(|e| json!({ "error": e.message })));
            Some(ToolSnapshot {
                kind: "mcp_tool_call".to_string(),
                name: tool.clone(),
                provider: Some(server.clone()),
                status: Some(mcp_tool_call_status_str(status).to_string()),
                request: Some(arguments.clone()),
                stdout: error.as_ref().map(|e| e.message.clone()),
                result,
                is_error: error.is_some() || matches!(status, codex::McpToolCallStatus::Failed),
            })
        }
        codex::ThreadItem::CommandExecution {
            command,
            cwd,
            status,
            aggregated_output,
            exit_code,
            ..
        } => {
            let result = aggregated_output.as_ref().map(|output| {
                json!({
                    "output": output,
                    "exit_code": exit_code,
                })
            });
            Some(ToolSnapshot {
                kind: "command_execution".to_string(),
                name: "command_execution".to_string(),
                provider: None,
                status: Some(command_execution_status_str(status).to_string()),
                request: Some(json!({
                    "command": command,
                    "cwd": cwd,
                })),
                stdout: aggregated_output.clone(),
                result,
                is_error: exit_code.is_some_and(|code| code != 0)
                    || matches!(status, codex::CommandExecutionStatus::Failed),
            })
        }
        codex::ThreadItem::FileChange {
            changes, status, ..
        } => {
            let result = serde_json::to_value(changes).ok();
            Some(ToolSnapshot {
                kind: "file_change".to_string(),
                name: "file_change".to_string(),
                provider: None,
                status: Some(patch_apply_status_str(status).to_string()),
                request: None,
                stdout: None,
                result,
                is_error: matches!(status, codex::PatchApplyStatus::Failed),
            })
        }
        item @ codex::ThreadItem::CollabAgentToolCall { tool, status, .. } => Some(ToolSnapshot {
            kind: "collab_agent_tool_call".to_string(),
            name: serde_json::to_value(tool)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "collab_agent_tool".to_string()),
            provider: None,
            status: serde_json::to_value(status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned)),
            request: serde_json::to_value(item).ok(),
            result: None,
            stdout: None,
            is_error: false,
        }),
        _ => None,
    }
}

fn dynamic_content_items_text(items: &[codex::DynamicToolCallOutputContentItem]) -> String {
    items
        .iter()
        .filter_map(|item| match item {
            codex::DynamicToolCallOutputContentItem::InputText { text } => Some(text.as_str()),
            codex::DynamicToolCallOutputContentItem::InputImage { .. } => Some("[image output]"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn dynamic_tool_call_status_str(status: &codex::DynamicToolCallStatus) -> &'static str {
    match status {
        codex::DynamicToolCallStatus::InProgress => "in_progress",
        codex::DynamicToolCallStatus::Completed => "completed",
        codex::DynamicToolCallStatus::Failed => "failed",
    }
}

fn mcp_tool_call_status_str(status: &codex::McpToolCallStatus) -> &'static str {
    match status {
        codex::McpToolCallStatus::InProgress => "in_progress",
        codex::McpToolCallStatus::Completed => "completed",
        codex::McpToolCallStatus::Failed => "failed",
    }
}

fn command_execution_status_str(status: &codex::CommandExecutionStatus) -> &'static str {
    match status {
        codex::CommandExecutionStatus::InProgress => "in_progress",
        codex::CommandExecutionStatus::Completed => "completed",
        codex::CommandExecutionStatus::Failed => "failed",
        codex::CommandExecutionStatus::Declined => "declined",
    }
}

fn patch_apply_status_str(status: &codex::PatchApplyStatus) -> &'static str {
    match status {
        codex::PatchApplyStatus::InProgress => "in_progress",
        codex::PatchApplyStatus::Completed => "completed",
        codex::PatchApplyStatus::Failed => "failed",
        codex::PatchApplyStatus::Declined => "declined",
    }
}

fn tool_call_projections(events: &[PersistedSessionEvent]) -> Vec<ToolCallProjection> {
    let mut grouped: BTreeMap<String, Vec<PersistedSessionEvent>> = BTreeMap::new();
    for event in events {
        if let Some(id) = event_tool_call_id(event) {
            grouped.entry(id).or_default().push(event.clone());
        }
    }

    grouped
        .into_iter()
        .filter_map(|(tool_call_id, mut raw_events)| {
            raw_events.sort_by_key(|event| event.event_seq);
            build_tool_call_projection(tool_call_id, raw_events)
        })
        .collect()
}

fn build_tool_call_projection(
    tool_call_id: String,
    raw_events: Vec<PersistedSessionEvent>,
) -> Option<ToolCallProjection> {
    let first = raw_events.first()?;
    let last = raw_events.last()?;
    let mut kind = "tool_call".to_string();
    let mut name = "tool_call".to_string();
    let mut provider = None;
    let mut status = None;
    let mut request = None;
    let mut result = None;
    let mut stdout_parts = Vec::new();
    let mut is_error = false;

    for event in &raw_events {
        match &event.notification.event {
            BackboneEvent::ItemStarted(n) => {
                if let Some(snapshot) = tool_snapshot_from_item(&n.item) {
                    kind = snapshot.kind;
                    name = snapshot.name;
                    provider = snapshot.provider.or(provider);
                    status = snapshot.status.or(status);
                    if request.is_none() {
                        request = snapshot.request;
                    }
                    if snapshot.result.is_some() {
                        result = snapshot.result;
                    }
                    if let Some(stdout) = snapshot
                        .stdout
                        .map(|s| s.trim_end_matches('\n').to_string())
                        .filter(|s| !s.is_empty())
                    {
                        stdout_parts.push(stdout);
                    }
                    is_error |= snapshot.is_error;
                }
            }
            BackboneEvent::ItemCompleted(n) => {
                if let Some(snapshot) = tool_snapshot_from_item(&n.item) {
                    kind = snapshot.kind;
                    name = snapshot.name;
                    provider = snapshot.provider.or(provider);
                    status = snapshot.status.or(status);
                    if request.is_none() {
                        request = snapshot.request;
                    }
                    if snapshot.result.is_some() {
                        result = snapshot.result;
                    }
                    if let Some(stdout) = snapshot
                        .stdout
                        .map(|s| s.trim_end_matches('\n').to_string())
                        .filter(|s| !s.is_empty())
                    {
                        stdout_parts.push(stdout);
                    }
                    is_error |= snapshot.is_error;
                }
            }
            BackboneEvent::CommandOutputDelta(n) => {
                if !n.delta.is_empty() {
                    stdout_parts.push(n.delta.clone());
                }
            }
            BackboneEvent::FileChangeDelta(n) => {
                if !n.delta.is_empty() {
                    stdout_parts.push(n.delta.clone());
                }
            }
            BackboneEvent::McpToolCallProgress(n) => {
                if !n.message.is_empty() {
                    stdout_parts.push(n.message.clone());
                }
            }
            BackboneEvent::Error(_) => {
                is_error = true;
            }
            _ => {}
        }
    }

    let stdout = stdout_parts.join("");
    if result.is_none() && !stdout.is_empty() {
        result = Some(json!({ "output": stdout }));
    }

    let summary = ToolCallSummary {
        tool_call_id,
        kind,
        name,
        provider,
        status,
        turn_id: raw_events.iter().find_map(|event| event.turn_id.clone()),
        first_event_seq: first.event_seq,
        last_event_seq: last.event_seq,
        event_count: raw_events.len(),
        has_request: request.is_some(),
        has_result: result.is_some(),
        has_stdout: !stdout.is_empty(),
        is_error,
    };

    Some(ToolCallProjection {
        summary,
        request,
        result,
        stdout,
        raw_events,
    })
}

fn is_write_projection(projection: &ToolCallProjection) -> bool {
    if projection.summary.kind == "file_change" {
        return true;
    }
    let name = projection.summary.name.to_ascii_lowercase();
    name.contains("write")
        || name.contains("apply_patch")
        || name.contains("patch")
        || name.contains("edit")
}

#[async_trait]
impl MountProvider for LifecycleMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_LIFECYCLE_VFS
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path_norm =
            normalize_mount_relative_path(path, true).map_err(MountError::OperationFailed)?;
        let segs = segments_from_path(&path_norm);

        // ── artifacts 路径族：直接查 inline_fs_files，不加载整个 LifecycleRun ──
        let content = match segs.as_slice() {
            ["artifacts"] => {
                let run_id = parse_run_id_from_metadata(mount)?;
                let files = self
                    .inline_file_repo
                    .list_files(
                        InlineFileOwnerKind::LifecycleRun,
                        run_id,
                        PORT_OUTPUTS_CONTAINER,
                    )
                    .await
                    .map_err(map_domain_err)?;
                let map: std::collections::BTreeMap<String, String> =
                    files.into_iter().map(|f| (f.path, f.content)).collect();
                to_json_pretty(&map)?
            }
            ["artifacts", port_key] => {
                let run_id = parse_run_id_from_metadata(mount)?;
                self.inline_file_repo
                    .get_file(
                        InlineFileOwnerKind::LifecycleRun,
                        run_id,
                        PORT_OUTPUTS_CONTAINER,
                        port_key,
                    )
                    .await
                    .map_err(map_domain_err)?
                    .map(|f| f.content)
                    .ok_or_else(|| {
                        MountError::NotFound(format!("port output 不存在: {port_key}"))
                    })?
            }
            // ── 其它路径需要加载完整的 LifecycleRun ──
            _ => {
                let active = load_active_run(&self.lifecycle_run_repo, mount).await?;
                let run_id = parse_run_id_from_metadata(mount)?;
                match segs.as_slice() {
                    [] | ["active"] => to_json_pretty(&run_overview(&active))?,
                    ["active", "steps"] => to_json_pretty(&active.step_states)?,
                    ["active", "steps", key] => {
                        let step = find_step(&active, key)?;
                        to_json_pretty(step)?
                    }
                    ["active", "log"] => to_json_pretty(&active.execution_log)?,
                    ["state"] => {
                        let step = current_step(&active)?;
                        to_json_pretty(step)?
                    }
                    ["session", "summary"] => {
                        let step = current_step(&active)?;
                        self.read_node_summary(run_id, step).await?
                    }
                    ["session", "conclusions"] => {
                        let step = current_step(&active)?;
                        self.read_node_conclusions(run_id, &step.step_key).await?
                    }
                    ["session", rest @ ..] => {
                        let (_, session_id) = current_step_session_id(&active)?;
                        self.read_session_projection(session_id, rest).await?
                    }
                    ["tool-calls"] => {
                        let (_, session_id) = current_step_session_id(&active)?;
                        self.read_tool_calls_projection(session_id, &[]).await?
                    }
                    ["tool-calls", rest @ ..] => {
                        let (_, session_id) = current_step_session_id(&active)?;
                        self.read_tool_calls_projection(session_id, rest).await?
                    }
                    ["writes"] => {
                        let (_, session_id) = current_step_session_id(&active)?;
                        self.read_writes_projection(session_id).await?
                    }
                    ["records"] => {
                        let step = current_step(&active)?;
                        self.read_records_map(run_id, &step.step_key).await?
                    }
                    ["records", rest @ ..] => {
                        let step = current_step(&active)?;
                        self.read_record(run_id, &step.step_key, rest).await?
                    }
                    ["runs"] => {
                        let sid = resolve_session_id_for_runs(mount, &active);
                        let runs = self
                            .lifecycle_run_repo
                            .list_by_session(&sid)
                            .await
                            .map_err(map_domain_err)?;
                        let summaries: Vec<_> = runs.iter().map(run_overview).collect();
                        to_json_pretty(&summaries)?
                    }
                    ["runs", id_str] => {
                        let rid = Uuid::parse_str(id_str).map_err(|e| {
                            MountError::OperationFailed(format!("run id 无效: {e}"))
                        })?;
                        let run = self
                            .lifecycle_run_repo
                            .get_by_id(rid)
                            .await
                            .map_err(map_domain_err)?
                            .ok_or_else(|| MountError::NotFound(format!("run 不存在: {rid}")))?;
                        to_json_pretty(&run_overview(&run))?
                    }
                    ["nodes", key, "state"] => {
                        let step = find_step(&active, key)?;
                        to_json_pretty(step)?
                    }
                    ["nodes", key, "records"] => {
                        find_step(&active, key)?;
                        self.read_records_map(run_id, key).await?
                    }
                    ["nodes", key, "records", rest @ ..] => {
                        find_step(&active, key)?;
                        self.read_record(run_id, key, rest).await?
                    }
                    ["nodes", key, "session", "summary"] => {
                        let step = find_step(&active, key)?;
                        self.read_node_summary(run_id, step).await?
                    }
                    ["nodes", key, "session", "conclusions"] => {
                        find_step(&active, key)?;
                        self.read_node_conclusions(run_id, key).await?
                    }
                    ["nodes", key, "session", "tool-calls"] => {
                        let session_id = step_session_id(&active, key)?;
                        self.read_tool_calls_projection(session_id, &[]).await?
                    }
                    ["nodes", key, "session", "tool-calls", rest @ ..] => {
                        let session_id = step_session_id(&active, key)?;
                        self.read_tool_calls_projection(session_id, rest).await?
                    }
                    ["nodes", key, "session", "writes"] => {
                        let session_id = step_session_id(&active, key)?;
                        self.read_writes_projection(session_id).await?
                    }
                    ["nodes", key, "session", rest @ ..] => {
                        let session_id = step_session_id(&active, key)?;
                        self.read_session_projection(session_id, rest).await?
                    }
                    _ => {
                        return Err(MountError::NotFound(format!(
                            "lifecycle_vfs 不支持的路径: `{path_norm}`"
                        )));
                    }
                }
            }
        };

        Ok(ReadResult::new(path_norm, content))
    }

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let path_norm =
            normalize_mount_relative_path(path, true).map_err(MountError::OperationFailed)?;
        let segs = segments_from_path(&path_norm);

        match segs.as_slice() {
            ["artifacts", port_key] => {
                let allowed_keys = mount
                    .metadata
                    .get("writable_port_keys")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                    .unwrap_or_default();

                if !allowed_keys.contains(port_key) {
                    return Err(MountError::OperationFailed(format!(
                        "当前 node 没有名为 `{port_key}` 的 output port，可写 port: {:?}",
                        allowed_keys
                    )));
                }

                // 直接写入 inline_fs_files，不再加载整个 LifecycleRun 实体
                let run_id = parse_run_id_from_metadata(mount)?;
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
                    .map_err(map_domain_err)?;

                info!(
                    run_id = %run_id,
                    port_key = %port_key,
                    content_len = content.len(),
                    "lifecycle VFS: wrote port output to inline_fs_files"
                );
                Ok(())
            }
            ["records", rest @ ..] => {
                let active = load_active_run(&self.lifecycle_run_repo, mount).await?;
                let step = current_step(&active)?;
                let name = join_rest(rest)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let path = format!("{}/{}", step.step_key, name);
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

                info!(
                    run_id = %run_id,
                    step_key = %step.step_key,
                    record = %name,
                    content_len = content.len(),
                    "lifecycle VFS: wrote journey record"
                );
                Ok(())
            }
            ["nodes", key, "records", rest @ ..] => {
                let active = load_active_run(&self.lifecycle_run_repo, mount).await?;
                find_step(&active, key)?;
                let name = join_rest(rest)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let path = format!("{key}/{name}");
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

                info!(
                    run_id = %run_id,
                    step_key = %key,
                    record = %name,
                    content_len = content.len(),
                    "lifecycle VFS: wrote explicit node journey record"
                );
                Ok(())
            }
            _ => Err(MountError::NotSupported(format!(
                "lifecycle_vfs 仅支持写入 artifacts/{{port_key}} 或 records/{{name}} 路径，收到: `{path_norm}`"
            ))),
        }
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let base = normalize_mount_relative_path(&options.path, true)
            .map_err(MountError::OperationFailed)?;
        let segs = segments_from_path(&base);
        let active = load_active_run(&self.lifecycle_run_repo, mount).await?;

        let entries: Vec<RuntimeFileEntry> = match segs.as_slice() {
            [] => vec![
                RuntimeFileEntry::dir("active").as_virtual(),
                RuntimeFileEntry::dir("artifacts"),
                RuntimeFileEntry::file("state").as_virtual(),
                RuntimeFileEntry::dir("session").as_virtual(),
                RuntimeFileEntry::dir("tool-calls").as_virtual(),
                RuntimeFileEntry::file("writes").as_virtual(),
                RuntimeFileEntry::dir("records"),
                RuntimeFileEntry::dir("nodes").as_virtual(),
                RuntimeFileEntry::dir("runs").as_virtual(),
            ],
            ["active"] => vec![
                RuntimeFileEntry::dir("active/steps").as_virtual(),
                RuntimeFileEntry::file("active/log")
                    .with_size(
                        serde_json::to_string(&active.execution_log)
                            .map(|s| s.len() as u64)
                            .unwrap_or(0),
                    )
                    .as_virtual(),
            ],
            ["active", "steps"] => active
                .step_states
                .iter()
                .map(|s| {
                    RuntimeFileEntry::file(format!("active/steps/{}", s.step_key)).as_virtual()
                })
                .collect(),
            ["session"] => {
                if current_step_session_id(&active).is_ok() {
                    vec![
                        RuntimeFileEntry::file("session/meta").as_virtual(),
                        RuntimeFileEntry::file("session/summary").as_virtual(),
                        RuntimeFileEntry::file("session/conclusions").as_virtual(),
                        RuntimeFileEntry::file("session/events.json").as_virtual(),
                        RuntimeFileEntry::file("session/terminal").as_virtual(),
                        RuntimeFileEntry::dir("session/turns").as_virtual(),
                    ]
                } else {
                    vec![
                        RuntimeFileEntry::file("session/summary").as_virtual(),
                        RuntimeFileEntry::file("session/conclusions").as_virtual(),
                    ]
                }
            }
            ["session", "turns"] => {
                let (_, session_id) = current_step_session_id(&active)?;
                let events = self.session_events(session_id).await?;
                group_events_into_turn_summaries(&events)
                    .into_iter()
                    .map(|turn| {
                        RuntimeFileEntry::dir(format!("session/turns/{}", turn.turn_id))
                            .as_virtual()
                    })
                    .collect()
            }
            ["session", "turns", turn_id] => {
                let (_, session_id) = current_step_session_id(&active)?;
                let events = self.session_events(session_id).await?;
                if events
                    .iter()
                    .any(|event| event.turn_id.as_deref() == Some(*turn_id))
                {
                    vec![
                        RuntimeFileEntry::file(format!("session/turns/{turn_id}/events.json"))
                            .as_virtual(),
                    ]
                } else {
                    Vec::new()
                }
            }
            ["tool-calls"] => {
                let (_, session_id) = current_step_session_id(&active)?;
                let events = self.session_events(session_id).await?;
                tool_call_projections(&events)
                    .into_iter()
                    .map(|projection| {
                        RuntimeFileEntry::dir(format!(
                            "tool-calls/{}",
                            projection.summary.tool_call_id
                        ))
                        .as_virtual()
                    })
                    .collect()
            }
            ["tool-calls", tool_call_id] => {
                let (_, session_id) = current_step_session_id(&active)?;
                let events = self.session_events(session_id).await?;
                let projections = tool_call_projections(&events);
                let projection = find_tool_projection(&projections, tool_call_id)?;
                let mut entries = vec![
                    RuntimeFileEntry::file(format!("tool-calls/{tool_call_id}/raw.json"))
                        .as_virtual(),
                ];
                if projection.request.is_some() {
                    entries.push(
                        RuntimeFileEntry::file(format!("tool-calls/{tool_call_id}/request.json"))
                            .as_virtual(),
                    );
                }
                if projection.result.is_some() {
                    entries.push(
                        RuntimeFileEntry::file(format!("tool-calls/{tool_call_id}/result.json"))
                            .as_virtual(),
                    );
                }
                if !projection.stdout.is_empty() {
                    entries.push(
                        RuntimeFileEntry::file(format!("tool-calls/{tool_call_id}/stdout.txt"))
                            .as_virtual(),
                    );
                }
                entries
            }
            ["records"] => {
                let step = current_step(&active)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                self.list_record_entries(run_id, &step.step_key, "records", "records", options)
                    .await?
            }
            ["records", rest @ ..] => {
                let step = current_step(&active)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let display_base = format!("records/{}", rest.join("/"));
                self.list_record_entries(run_id, &step.step_key, "records", &display_base, options)
                    .await?
            }
            // ── artifacts/: port output 文件列表（从 inline_fs_files 查询）──
            ["artifacts"] => {
                let run_id = parse_run_id_from_metadata(mount)?;
                let files = self
                    .inline_file_repo
                    .list_files(
                        InlineFileOwnerKind::LifecycleRun,
                        run_id,
                        PORT_OUTPUTS_CONTAINER,
                    )
                    .await
                    .map_err(map_domain_err)?;
                let map = files
                    .into_iter()
                    .map(|file| (format!("artifacts/{}", file.path), file.content))
                    .collect::<BTreeMap<_, _>>();
                list_inline_entries(
                    &map,
                    "artifacts",
                    options.pattern.as_deref(),
                    options.recursive,
                )
            }
            // ── nodes/ 路径族 ──────────────────────────────────
            ["nodes"] => active
                .step_states
                .iter()
                .map(|s| RuntimeFileEntry::dir(format!("nodes/{}", s.step_key)).as_virtual())
                .collect(),
            ["nodes", key] => {
                let step = active.step_states.iter().find(|s| s.step_key == *key);
                if step.is_none() {
                    Vec::new()
                } else {
                    let step = step.unwrap();
                    let mut entries =
                        vec![RuntimeFileEntry::file(format!("nodes/{key}/state")).as_virtual()];
                    if step.session_id.is_some() {
                        entries.push(
                            RuntimeFileEntry::dir(format!("nodes/{key}/session")).as_virtual(),
                        );
                    }
                    entries.push(RuntimeFileEntry::dir(format!("nodes/{key}/records")));
                    entries
                }
            }
            ["nodes", key, "session"] => {
                let step = active.step_states.iter().find(|s| s.step_key == *key);
                if step.and_then(|s| s.session_id.as_ref()).is_none() {
                    Vec::new()
                } else {
                    vec![
                        RuntimeFileEntry::file(format!("nodes/{key}/session/meta")).as_virtual(),
                        RuntimeFileEntry::file(format!("nodes/{key}/session/summary")).as_virtual(),
                        RuntimeFileEntry::file(format!("nodes/{key}/session/conclusions"))
                            .as_virtual(),
                        RuntimeFileEntry::file(format!("nodes/{key}/session/events.json"))
                            .as_virtual(),
                        RuntimeFileEntry::file(format!("nodes/{key}/session/terminal"))
                            .as_virtual(),
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/turns")).as_virtual(),
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/tool-calls"))
                            .as_virtual(),
                        RuntimeFileEntry::file(format!("nodes/{key}/session/writes")).as_virtual(),
                    ]
                }
            }
            ["nodes", key, "session", "turns"] => {
                let session_id = step_session_id(&active, key)?;
                let events = self.session_events(session_id).await?;
                group_events_into_turn_summaries(&events)
                    .into_iter()
                    .map(|turn| {
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/turns/{}", turn.turn_id))
                            .as_virtual()
                    })
                    .collect()
            }
            ["nodes", key, "session", "turns", turn_id] => {
                let session_id = step_session_id(&active, key)?;
                let events = self.session_events(session_id).await?;
                if events
                    .iter()
                    .any(|event| event.turn_id.as_deref() == Some(*turn_id))
                {
                    vec![
                        RuntimeFileEntry::file(format!(
                            "nodes/{key}/session/turns/{turn_id}/events.json"
                        ))
                        .as_virtual(),
                    ]
                } else {
                    Vec::new()
                }
            }
            ["nodes", key, "session", "tool-calls"] => {
                let session_id = step_session_id(&active, key)?;
                let events = self.session_events(session_id).await?;
                tool_call_projections(&events)
                    .into_iter()
                    .map(|projection| {
                        RuntimeFileEntry::dir(format!(
                            "nodes/{key}/session/tool-calls/{}",
                            projection.summary.tool_call_id
                        ))
                        .as_virtual()
                    })
                    .collect()
            }
            ["nodes", key, "session", "tool-calls", tool_call_id] => {
                let session_id = step_session_id(&active, key)?;
                let events = self.session_events(session_id).await?;
                let projections = tool_call_projections(&events);
                let projection = find_tool_projection(&projections, tool_call_id)?;
                let mut entries = vec![
                    RuntimeFileEntry::file(format!(
                        "nodes/{key}/session/tool-calls/{tool_call_id}/raw.json"
                    ))
                    .as_virtual(),
                ];
                if projection.request.is_some() {
                    entries.push(
                        RuntimeFileEntry::file(format!(
                            "nodes/{key}/session/tool-calls/{tool_call_id}/request.json"
                        ))
                        .as_virtual(),
                    );
                }
                if projection.result.is_some() {
                    entries.push(
                        RuntimeFileEntry::file(format!(
                            "nodes/{key}/session/tool-calls/{tool_call_id}/result.json"
                        ))
                        .as_virtual(),
                    );
                }
                if !projection.stdout.is_empty() {
                    entries.push(
                        RuntimeFileEntry::file(format!(
                            "nodes/{key}/session/tool-calls/{tool_call_id}/stdout.txt"
                        ))
                        .as_virtual(),
                    );
                }
                entries
            }
            ["nodes", key, "records"] => {
                find_step(&active, key)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let display_root = format!("nodes/{key}/records");
                self.list_record_entries(run_id, key, &display_root, &display_root, options)
                    .await?
            }
            ["nodes", key, "records", rest @ ..] => {
                find_step(&active, key)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let display_root = format!("nodes/{key}/records");
                let display_base = format!("nodes/{key}/records/{}", rest.join("/"));
                self.list_record_entries(run_id, key, &display_root, &display_base, options)
                    .await?
            }
            ["runs"] => {
                let sid = resolve_session_id_for_runs(mount, &active);
                let runs = self
                    .lifecycle_run_repo
                    .list_by_session(&sid)
                    .await
                    .map_err(map_domain_err)?;
                runs.iter()
                    .map(|r| RuntimeFileEntry::file(format!("runs/{}", r.id)).as_virtual())
                    .collect()
            }
            _ => Vec::new(),
        };

        Ok(ListResult { entries })
    }

    async fn search_text(
        &self,
        _mount: &Mount,
        _query: &SearchQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        Ok(SearchResult { matches: vec![] })
    }

    async fn exec(
        &self,
        _mount: &Mount,
        _request: &ExecRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        Err(MountError::NotSupported(
            "lifecycle_vfs 不支持 exec".to_string(),
        ))
    }
}

// ── Session 投影 helper ──────────────────────────────────

#[derive(Serialize)]
struct TurnSummary {
    turn_id: String,
    event_count: usize,
    first_event_type: String,
    first_occurred_at_ms: i64,
    last_occurred_at_ms: i64,
}

fn group_events_into_turn_summaries(events: &[PersistedSessionEvent]) -> Vec<TurnSummary> {
    use std::collections::BTreeMap;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{
        ExecutionStatus, MemorySessionPersistence, SessionBootstrapState, SessionMeta, TitleSource,
    };
    use agentdash_agent_protocol::{BackboneEnvelope, SourceInfo, TraceInfo};
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::workflow::{LifecycleStepDefinition, LifecycleStepExecutionStatus};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct InMemoryLifecycleRunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait::async_trait]
    impl LifecycleRunRepository for InMemoryLifecycleRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_by_lifecycle(
            &self,
            lifecycle_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.lifecycle_id == lifecycle_id)
                .cloned()
                .collect())
        }

        async fn list_by_session(
            &self,
            session_id: &str,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.session_id == session_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut guard = self.runs.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|existing| existing.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().unwrap().retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryInlineFileRepo {
        files: Mutex<Vec<InlineFile>>,
    }

    #[async_trait::async_trait]
    impl InlineFileRepository for InMemoryInlineFileRepo {
        async fn get_file(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
            path: &str,
        ) -> Result<Option<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .find(|file| {
                    file.owner_kind == owner_kind
                        && file.owner_id == owner_id
                        && file.container_id == container_id
                        && file.path == path
                })
                .cloned())
        }

        async fn list_files(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<Vec<InlineFile>, DomainError> {
            let mut files = self
                .files
                .lock()
                .unwrap()
                .iter()
                .filter(|file| {
                    file.owner_kind == owner_kind
                        && file.owner_id == owner_id
                        && file.container_id == container_id
                })
                .cloned()
                .collect::<Vec<_>>();
            files.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(files)
        }

        async fn list_files_by_owner(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
        ) -> Result<Vec<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .filter(|file| file.owner_kind == owner_kind && file.owner_id == owner_id)
                .cloned()
                .collect())
        }

        async fn upsert_file(&self, file: &InlineFile) -> Result<(), DomainError> {
            let mut guard = self.files.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|existing| {
                existing.owner_kind == file.owner_kind
                    && existing.owner_id == file.owner_id
                    && existing.container_id == file.container_id
                    && existing.path == file.path
            }) {
                *existing = file.clone();
            } else {
                guard.push(file.clone());
            }
            Ok(())
        }

        async fn upsert_files(&self, files: &[InlineFile]) -> Result<(), DomainError> {
            for file in files {
                self.upsert_file(file).await?;
            }
            Ok(())
        }

        async fn delete_file(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
            path: &str,
        ) -> Result<(), DomainError> {
            self.files.lock().unwrap().retain(|file| {
                file.owner_kind != owner_kind
                    || file.owner_id != owner_id
                    || file.container_id != container_id
                    || file.path != path
            });
            Ok(())
        }

        async fn delete_by_container(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<(), DomainError> {
            self.files.lock().unwrap().retain(|file| {
                file.owner_kind != owner_kind
                    || file.owner_id != owner_id
                    || file.container_id != container_id
            });
            Ok(())
        }

        async fn delete_by_owner(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
        ) -> Result<(), DomainError> {
            self.files
                .lock()
                .unwrap()
                .retain(|file| file.owner_kind != owner_kind || file.owner_id != owner_id);
            Ok(())
        }

        async fn count_files(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<i64, DomainError> {
            Ok(self
                .list_files(owner_kind, owner_id, container_id)
                .await?
                .len() as i64)
        }
    }

    fn test_step(key: &str) -> LifecycleStepDefinition {
        LifecycleStepDefinition {
            key: key.to_string(),
            description: String::new(),
            workflow_key: None,
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
            capability_config: Default::default(),
        }
    }

    fn test_meta(session_id: &str) -> SessionMeta {
        SessionMeta {
            id: session_id.to_string(),
            title: "Lifecycle node".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_execution_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Plain,
            pending_capability_surface_transitions: Vec::new(),
        }
    }

    fn source() -> SourceInfo {
        SourceInfo {
            connector_id: "test".to_string(),
            connector_type: "unit".to_string(),
            executor_id: None,
        }
    }

    fn envelope(session_id: &str, turn_id: &str, event: BackboneEvent) -> BackboneEnvelope {
        BackboneEnvelope::new(event, session_id, source()).with_trace(TraceInfo {
            turn_id: Some(turn_id.to_string()),
            entry_index: None,
        })
    }

    fn dynamic_tool_item(
        id: &str,
        tool: &str,
        status: codex::DynamicToolCallStatus,
        content: Option<&str>,
    ) -> codex::ThreadItem {
        codex::ThreadItem::DynamicToolCall {
            id: id.to_string(),
            tool: tool.to_string(),
            arguments: serde_json::json!({ "path": "src/lib.rs" }),
            status,
            content_items: content.map(|text| {
                vec![codex::DynamicToolCallOutputContentItem::InputText {
                    text: text.to_string(),
                }]
            }),
            success: content.map(|_| true),
            duration_ms: Some(12),
        }
    }

    fn mcp_tool_item(id: &str) -> codex::ThreadItem {
        codex::ThreadItem::McpToolCall {
            id: id.to_string(),
            server: "memory".to_string(),
            tool: "lookup".to_string(),
            status: codex::McpToolCallStatus::Completed,
            arguments: serde_json::json!({ "query": "lifecycle" }),
            result: Some(codex::McpToolCallResult {
                content: vec![serde_json::json!({ "type": "text", "text": "mcp result" })],
                structured_content: Some(serde_json::json!({ "answer": 42 })),
                meta: None,
            }),
            error: None,
            duration_ms: Some(7),
        }
    }

    async fn fixture() -> (
        LifecycleMountProvider,
        Mount,
        Arc<InMemoryInlineFileRepo>,
        MemorySessionPersistence,
    ) {
        let run_repo = Arc::new(InMemoryLifecycleRunRepo::default());
        let inline_repo = Arc::new(InMemoryInlineFileRepo::default());
        let persistence = MemorySessionPersistence::default();
        let session_id = "sess-node";

        let steps = vec![test_step("analyze")];
        let mut run = LifecycleRun::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "sess-root",
            &steps,
            "analyze",
            &[],
        )
        .expect("run");
        run.activate_step("analyze").expect("activate");
        run.bind_step_session("analyze", session_id).expect("bind");
        run.step_states[0].status = LifecycleStepExecutionStatus::Running;
        run.step_states[0].summary = Some("节点摘要".to_string());
        run_repo.create(&run).await.expect("store run");

        persistence
            .create_session(&test_meta(session_id))
            .await
            .expect("create session");
        persistence
            .append_event(
                session_id,
                &envelope(
                    session_id,
                    "t-1",
                    BackboneEvent::ItemStarted(codex::ItemStartedNotification {
                        item: dynamic_tool_item(
                            "tool-1",
                            "read_file",
                            codex::DynamicToolCallStatus::InProgress,
                            None,
                        ),
                        thread_id: session_id.to_string(),
                        turn_id: "t-1".to_string(),
                    }),
                ),
            )
            .await
            .expect("append started");
        persistence
            .append_event(
                session_id,
                &envelope(
                    session_id,
                    "t-1",
                    BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                        item: dynamic_tool_item(
                            "tool-1",
                            "read_file",
                            codex::DynamicToolCallStatus::Completed,
                            Some("file contents"),
                        ),
                        thread_id: session_id.to_string(),
                        turn_id: "t-1".to_string(),
                    }),
                ),
            )
            .await
            .expect("append completed");
        persistence
            .append_event(
                session_id,
                &envelope(
                    session_id,
                    "t-1",
                    BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                        item: dynamic_tool_item(
                            "patch-1",
                            "fs_apply_patch",
                            codex::DynamicToolCallStatus::Completed,
                            Some("patched"),
                        ),
                        thread_id: session_id.to_string(),
                        turn_id: "t-1".to_string(),
                    }),
                ),
            )
            .await
            .expect("append patch");
        persistence
            .append_event(
                session_id,
                &envelope(
                    session_id,
                    "t-1",
                    BackboneEvent::ItemCompleted(codex::ItemCompletedNotification {
                        item: mcp_tool_item("mcp-1"),
                        thread_id: session_id.to_string(),
                        turn_id: "t-1".to_string(),
                    }),
                ),
            )
            .await
            .expect("append mcp");

        let mount = crate::vfs::build_lifecycle_mount_with_ports(
            run.id,
            "test-lifecycle",
            &["report".into()],
        );
        let provider = LifecycleMountProvider::new(
            run_repo,
            inline_repo.clone(),
            Arc::new(persistence.clone()),
        );
        (provider, mount, inline_repo, persistence)
    }

    #[tokio::test]
    async fn lifecycle_vfs_projects_current_node_session_and_tool_calls() {
        let (provider, mount, _inline_repo, _persistence) = fixture().await;
        let ctx = MountOperationContext::default();

        let turn = provider
            .read_text(&mount, "session/turns/t-1/events.json", &ctx)
            .await
            .expect("turn events");
        assert!(turn.content.contains("\"eventSeq\""));

        let node_turn = provider
            .read_text(&mount, "nodes/analyze/session/turns/t-1/events.json", &ctx)
            .await
            .expect("node turn events");
        assert_eq!(turn.content, node_turn.content);

        let tool_index = provider
            .read_text(&mount, "tool-calls", &ctx)
            .await
            .expect("tool index");
        assert!(tool_index.content.contains("\"tool_call_id\": \"tool-1\""));
        assert!(
            tool_index
                .content
                .contains("\"kind\": \"dynamic_tool_call\"")
        );
        assert!(tool_index.content.contains("\"tool_call_id\": \"mcp-1\""));
        assert!(tool_index.content.contains("\"kind\": \"mcp_tool_call\""));
        assert!(tool_index.content.contains("\"provider\": \"memory\""));

        let request = provider
            .read_text(&mount, "tool-calls/tool-1/request.json", &ctx)
            .await
            .expect("request");
        assert!(request.content.contains("\"path\": \"src/lib.rs\""));

        let result = provider
            .read_text(&mount, "tool-calls/tool-1/result.json", &ctx)
            .await
            .expect("result");
        assert!(result.content.contains("file contents"));

        let stdout = provider
            .read_text(&mount, "tool-calls/tool-1/stdout.txt", &ctx)
            .await
            .expect("stdout");
        assert!(stdout.content.contains("file contents"));

        let writes = provider
            .read_text(&mount, "writes", &ctx)
            .await
            .expect("writes");
        assert!(writes.content.contains("\"tool_call_id\": \"patch-1\""));

        let missing_mcp_calls = provider.read_text(&mount, "mcp-calls", &ctx).await;
        assert!(
            matches!(missing_mcp_calls, Err(MountError::NotFound(_))),
            "MCP 不应有独立 mcp-calls 路径族"
        );
    }

    #[tokio::test]
    async fn lifecycle_vfs_records_are_writable_without_opening_artifacts() {
        let (provider, mount, _inline_repo, _persistence) = fixture().await;
        let ctx = MountOperationContext::default();

        provider
            .write_text(&mount, "records/note.md", "hello record", &ctx)
            .await
            .expect("write current record");
        let record = provider
            .read_text(&mount, "records/note.md", &ctx)
            .await
            .expect("read current record");
        assert_eq!(record.content, "hello record");

        provider
            .write_text(
                &mount,
                "nodes/analyze/records/explicit.md",
                "explicit record",
                &ctx,
            )
            .await
            .expect("write explicit record");
        let explicit = provider
            .read_text(&mount, "nodes/analyze/records/explicit.md", &ctx)
            .await
            .expect("read explicit record");
        assert_eq!(explicit.content, "explicit record");

        let record_entries = provider
            .list(
                &mount,
                &ListOptions {
                    path: "records".to_string(),
                    pattern: None,
                    recursive: true,
                },
                &ctx,
            )
            .await
            .expect("list records")
            .entries;
        assert!(
            record_entries
                .iter()
                .any(|entry| entry.path == "records/note.md")
        );

        provider
            .write_text(&mount, "artifacts/report", "deliverable", &ctx)
            .await
            .expect("write allowed artifact");
        let artifact = provider
            .read_text(&mount, "artifacts/report", &ctx)
            .await
            .expect("read artifact");
        assert_eq!(artifact.content, "deliverable");

        let denied = provider
            .write_text(&mount, "artifacts/unknown", "nope", &ctx)
            .await;
        assert!(
            matches!(denied, Err(MountError::OperationFailed(_))),
            "未知 artifact port 必须被路径级白名单拒绝"
        );
    }

    #[tokio::test]
    async fn lifecycle_vfs_uri_reads_through_standard_service() {
        let (provider, mount, _inline_repo, _persistence) = fixture().await;
        let mut registry = crate::vfs::MountProviderRegistry::new();
        registry.register(Arc::new(provider));
        let service = crate::vfs::RelayVfsService::new(Arc::new(registry));
        let vfs = agentdash_spi::Vfs {
            mounts: vec![mount],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let target = crate::vfs::parse_mount_uri("lifecycle://tool-calls/tool-1/result.json", &vfs)
            .expect("URI should parse");

        let read = service
            .read_text(&vfs, &target, None, None)
            .await
            .expect("standard VFS service should read lifecycle URI");

        assert!(read.content.contains("file contents"));
    }
}
