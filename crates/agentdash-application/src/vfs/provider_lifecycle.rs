//! `lifecycle_vfs` mount: 通过 `LifecycleRunRepository` 暴露当前 lifecycle run 的虚拟文件视图。

use std::sync::Arc;

use super::mount::PROVIDER_LIFECYCLE_VFS;
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountError, MountOperationContext, MountProvider, SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::{Mount, RuntimeFileEntry};
use crate::session::{PersistedSessionEvent, SessionPersistence};
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository, LifecycleRunStatus};
use async_trait::async_trait;
use serde::Serialize;
use tracing::info;
use uuid::Uuid;

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
                    .list_files(InlineFileOwnerKind::LifecycleRun, run_id, "port_outputs")
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
                        "port_outputs",
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
                match segs.as_slice() {
                    [] | ["active"] => to_json_pretty(&run_overview(&active))?,
                    ["active", "steps"] => to_json_pretty(&active.step_states)?,
                    ["active", "steps", key] => {
                        let step = active
                            .step_states
                            .iter()
                            .find(|s| s.step_key == *key)
                            .ok_or_else(|| MountError::NotFound(format!("step 不存在: {key}")))?;
                        to_json_pretty(step)?
                    }
                    ["active", "log"] => to_json_pretty(&active.execution_log)?,
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
                    // ── nodes/ 路径族 ──────────────────────────────────
                    ["nodes", key, "state"] => {
                        let step = active
                            .step_states
                            .iter()
                            .find(|s| s.step_key == *key)
                            .ok_or_else(|| MountError::NotFound(format!("node 不存在: {key}")))?;
                        to_json_pretty(step)?
                    }
                    // ── nodes/{key}/session/* 路径族：session 虚拟投影 ──
                    ["nodes", key, "session", "meta"] => {
                        let step = active
                            .step_states
                            .iter()
                            .find(|s| s.step_key == *key)
                            .ok_or_else(|| MountError::NotFound(format!("node 不存在: {key}")))?;
                        let session_id = step.session_id.as_deref().ok_or_else(|| {
                            MountError::NotFound(format!("node `{key}` 没有关联 session"))
                        })?;
                        let meta = self
                            .session_persistence
                            .get_session_meta(session_id)
                            .await
                            .map_err(|e| {
                                MountError::OperationFailed(format!("读取 session meta 失败: {e}"))
                            })?
                            .ok_or_else(|| {
                                MountError::NotFound(format!("session 不存在: {session_id}"))
                            })?;
                        let meta_json = serde_json::json!({
                            "session_id": session_id,
                            "title": meta.title,
                            "status": meta.last_execution_status,
                            "last_event_seq": meta.last_event_seq,
                            "created_at": meta.created_at,
                            "updated_at": meta.updated_at,
                        });
                        to_json_pretty(&meta_json)?
                    }
                    ["nodes", key, "session", "turns"] => {
                        let step = active
                            .step_states
                            .iter()
                            .find(|s| s.step_key == *key)
                            .ok_or_else(|| MountError::NotFound(format!("node 不存在: {key}")))?;
                        let session_id = step.session_id.as_deref().ok_or_else(|| {
                            MountError::NotFound(format!("node `{key}` 没有关联 session"))
                        })?;
                        let events = self
                            .session_persistence
                            .list_all_events(session_id)
                            .await
                            .map_err(|e| {
                                MountError::OperationFailed(format!(
                                    "读取 session events 失败: {e}"
                                ))
                            })?;
                        let summaries = group_events_into_turn_summaries(&events);
                        to_json_pretty(&summaries)?
                    }
                    ["nodes", key, "session", "turns", turn_id] => {
                        let step = active
                            .step_states
                            .iter()
                            .find(|s| s.step_key == *key)
                            .ok_or_else(|| MountError::NotFound(format!("node 不存在: {key}")))?;
                        let session_id = step.session_id.as_deref().ok_or_else(|| {
                            MountError::NotFound(format!("node `{key}` 没有关联 session"))
                        })?;
                        let events = self
                            .session_persistence
                            .list_all_events(session_id)
                            .await
                            .map_err(|e| {
                                MountError::OperationFailed(format!(
                                    "读取 session events 失败: {e}"
                                ))
                            })?;
                        let turn_events: Vec<&PersistedSessionEvent> = events
                            .iter()
                            .filter(|e| e.turn_id.as_deref() == Some(*turn_id))
                            .collect();
                        if turn_events.is_empty() {
                            return Err(MountError::NotFound(format!("turn 不存在: {turn_id}")));
                        }
                        to_json_pretty(&turn_events)?
                    }
                    ["nodes", key, "session", "summary"] => {
                        let step = active
                            .step_states
                            .iter()
                            .find(|s| s.step_key == *key)
                            .ok_or_else(|| MountError::NotFound(format!("node 不存在: {key}")))?;
                        // 混合读取：先查 inline_fs 物化副本，再 fallback
                        let run_id = parse_run_id_from_metadata(mount)?;
                        if let Ok(Some(file)) = self
                            .inline_file_repo
                            .get_file(
                                InlineFileOwnerKind::LifecycleRun,
                                run_id,
                                "session_records",
                                &format!("{key}/summary"),
                            )
                            .await
                        {
                            file.content
                        } else {
                            // Fallback: step_state.summary
                            step.summary.clone().ok_or_else(|| {
                                MountError::NotFound(format!("node `{key}` 没有 summary"))
                            })?
                        }
                    }
                    ["nodes", key, "session", "conclusions"] => {
                        let run_id = parse_run_id_from_metadata(mount)?;
                        if !active.step_states.iter().any(|s| s.step_key == *key) {
                            return Err(MountError::NotFound(format!("node 不存在: {key}")));
                        }
                        self.inline_file_repo
                            .get_file(
                                InlineFileOwnerKind::LifecycleRun,
                                run_id,
                                "session_records",
                                &format!("{key}/conclusions"),
                            )
                            .await
                            .map_err(map_domain_err)?
                            .map(|f| f.content)
                            .ok_or_else(|| {
                                MountError::NotFound(format!("node `{key}` 没有 conclusions"))
                            })?
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

                if !allowed_keys.is_empty() && !allowed_keys.contains(port_key) {
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
                    "port_outputs",
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
            _ => Err(MountError::NotSupported(format!(
                "lifecycle_vfs 仅支持写入 artifacts/{{port_key}} 路径，收到: `{path_norm}`"
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
            // ── artifacts/: port output 文件列表（从 inline_fs_files 查询）──
            ["artifacts"] => {
                let run_id = parse_run_id_from_metadata(mount)?;
                let files = self
                    .inline_file_repo
                    .list_files(InlineFileOwnerKind::LifecycleRun, run_id, "port_outputs")
                    .await
                    .map_err(map_domain_err)?;
                files
                    .iter()
                    .map(|f| {
                        RuntimeFileEntry::file(format!("artifacts/{}", f.path))
                            .with_size(f.content.len() as u64)
                    })
                    .collect()
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
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/turns")).as_virtual(),
                    ]
                }
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
