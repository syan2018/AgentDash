//! `lifecycle_vfs` mount: 通过 `LifecycleRunRepository` 暴露当前 lifecycle run 的虚拟文件视图。

use std::sync::Arc;

use super::mount::PROVIDER_LIFECYCLE_VFS;
use super::path::normalize_mount_relative_path;
use super::provider::{MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery, SearchResult};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::{RuntimeFileEntry, Mount};
use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository, LifecycleRunStatus};
use agentdash_domain::workflow::WorkflowTargetKind;
use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

pub struct LifecycleMountProvider {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
}

impl LifecycleMountProvider {
    pub fn new(lifecycle_run_repo: Arc<dyn LifecycleRunRepository>) -> Self {
        Self { lifecycle_run_repo }
    }
}

#[derive(Serialize)]
struct LifecycleRunOverview<'a> {
    id: Uuid,
    project_id: Uuid,
    lifecycle_id: Uuid,
    target_kind: WorkflowTargetKind,
    target_id: Uuid,
    status: &'a LifecycleRunStatus,
    current_step_key: Option<&'a str>,
    step_count: usize,
    artifact_count: usize,
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
        target_kind: run.target_kind,
        target_id: run.target_id,
        status: &run.status,
        current_step_key: run.current_step_key.as_deref(),
        step_count: run.step_states.len(),
        artifact_count: run.record_artifacts.len(),
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
        .ok_or_else(|| {
            MountError::OperationFailed("mount metadata 缺少 run_id".to_string())
        })?;
    Uuid::parse_str(run_id_str)
        .map_err(|e| MountError::OperationFailed(format!("run_id 无效: {e}")))
}

#[derive(Debug, Clone, Copy)]
enum LifecycleRoot {
    Target {
        kind: WorkflowTargetKind,
        target_id: Uuid,
    },
    RunAnchor,
    Unknown,
}

fn parse_lifecycle_root(root_ref: &str) -> LifecycleRoot {
    let s = root_ref.trim();
    let prefix = "lifecycle://";
    if !s.starts_with(prefix) {
        return LifecycleRoot::Unknown;
    }
    let rest = &s[prefix.len()..];
    let mut parts = rest.split('/').filter(|p| !p.is_empty());
    let Some(kind) = parts.next() else {
        return LifecycleRoot::Unknown;
    };
    match kind {
        "target" => {
            let Some(k) = parts.next() else {
                return LifecycleRoot::Unknown;
            };
            let Some(id_str) = parts.next() else {
                return LifecycleRoot::Unknown;
            };
            let Ok(target_id) = Uuid::parse_str(id_str) else {
                return LifecycleRoot::Unknown;
            };
            let tk = match k {
                "project" => WorkflowTargetKind::Project,
                "story" => WorkflowTargetKind::Story,
                "task" => WorkflowTargetKind::Task,
                _ => return LifecycleRoot::Unknown,
            };
            LifecycleRoot::Target {
                kind: tk,
                target_id,
            }
        }
        "run" => {
            let Some(id_str) = parts.next() else {
                return LifecycleRoot::Unknown;
            };
            if Uuid::parse_str(id_str).is_err() {
                return LifecycleRoot::Unknown;
            }
            LifecycleRoot::RunAnchor
        }
        _ => LifecycleRoot::Unknown,
    }
}

fn resolve_target_for_runs(mount: &Mount, active_run: &LifecycleRun) -> (WorkflowTargetKind, Uuid) {
    match parse_lifecycle_root(&mount.root_ref) {
        LifecycleRoot::Target { kind, target_id } => (kind, target_id),
        LifecycleRoot::RunAnchor | LifecycleRoot::Unknown => {
            (active_run.target_kind, active_run.target_id)
        }
    }
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
        let path_norm = normalize_mount_relative_path(path, true)
            .map_err(MountError::OperationFailed)?;
        let segs = segments_from_path(&path_norm);
        let active = load_active_run(&self.lifecycle_run_repo, mount).await?;

        let content = match segs.as_slice() {
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
            ["active", "artifacts"] => to_json_pretty(&active.record_artifacts)?,
            ["active", "artifacts", id_str] => {
                let aid = Uuid::parse_str(id_str)
                    .map_err(|e| MountError::OperationFailed(format!("artifact id 无效: {e}")))?;
                let art = active
                    .record_artifacts
                    .iter()
                    .find(|a| a.id == aid)
                    .ok_or_else(|| MountError::NotFound(format!("artifact 不存在: {aid}")))?;
                art.content.clone()
            }
            ["active", "log"] => to_json_pretty(&active.execution_log)?,
            ["runs"] => {
                let (tk, tid) = resolve_target_for_runs(mount, &active);
                let runs = self
                    .lifecycle_run_repo
                    .list_by_target(tk, tid)
                    .await
                    .map_err(map_domain_err)?;
                let summaries: Vec<_> = runs.iter().map(run_overview).collect();
                to_json_pretty(&summaries)?
            }
            ["runs", id_str] => {
                let rid = Uuid::parse_str(id_str)
                    .map_err(|e| MountError::OperationFailed(format!("run id 无效: {e}")))?;
                let run = self
                    .lifecycle_run_repo
                    .get_by_id(rid)
                    .await
                    .map_err(map_domain_err)?
                    .ok_or_else(|| MountError::NotFound(format!("run 不存在: {rid}")))?;
                to_json_pretty(&run_overview(&run))?
            }
            _ => {
                return Err(MountError::NotFound(format!(
                    "lifecycle_vfs 不支持的路径: `{path_norm}`"
                )));
            }
        };

        Ok(ReadResult {
            path: path_norm,
            content,
        })
    }

    async fn write_text(
        &self,
        _mount: &Mount,
        _path: &str,
        _content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        Err(MountError::NotSupported(
            "lifecycle_vfs 不支持写入".to_string(),
        ))
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
                RuntimeFileEntry {
                    path: "active".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: true,
                },
                RuntimeFileEntry {
                    path: "runs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: true,
                },
            ],
            ["active"] => vec![
                RuntimeFileEntry {
                    path: "active/steps".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: true,
                },
                RuntimeFileEntry {
                    path: "active/artifacts".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: true,
                },
                RuntimeFileEntry {
                    path: "active/log".to_string(),
                    size: Some(
                        serde_json::to_string(&active.execution_log)
                            .map(|s| s.len() as u64)
                            .unwrap_or(0),
                    ),
                    modified_at: None,
                    is_dir: false,
                },
            ],
            ["active", "steps"] => active
                .step_states
                .iter()
                .map(|s| RuntimeFileEntry {
                    path: format!("active/steps/{}", s.step_key),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                })
                .collect(),
            ["active", "artifacts"] => active
                .record_artifacts
                .iter()
                .map(|a| RuntimeFileEntry {
                    path: format!("active/artifacts/{}", a.id),
                    size: Some(a.content.len() as u64),
                    modified_at: None,
                    is_dir: false,
                })
                .collect(),
            ["runs"] => {
                let (tk, tid) = resolve_target_for_runs(mount, &active);
                let runs = self
                    .lifecycle_run_repo
                    .list_by_target(tk, tid)
                    .await
                    .map_err(map_domain_err)?;
                runs
                    .iter()
                    .map(|r| RuntimeFileEntry {
                        path: format!("runs/{}", r.id),
                        size: None,
                        modified_at: None,
                        is_dir: false,
                    })
                    .collect()
            }
            _ => Vec::new(),
        };

        Ok(ListResult { entries })
    }

    async fn search_text(
        &self,
        mount: &Mount,
        query: &SearchQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        let active = load_active_run(&self.lifecycle_run_repo, mount).await?;
        let base = query
            .path
            .as_deref()
            .map(|p| normalize_mount_relative_path(p, true))
            .transpose()
            .map_err(MountError::OperationFailed)?
            .unwrap_or_default();

        let pattern = &query.pattern;
        if pattern.is_empty() {
            return Ok(SearchResult { matches: vec![] });
        }

        let mut matches = Vec::new();
        let max = query.max_results.unwrap_or(50);

        for art in &active.record_artifacts {
            if matches.len() >= max {
                break;
            }
            let rel_path = format!("active/artifacts/{}", art.id);
            if !base.is_empty() && !rel_path.starts_with(&base) && base != "active/artifacts" {
                continue;
            }
            let haystack = &art.content;
            let found = if query.case_sensitive {
                haystack.contains(pattern.as_str())
            } else {
                let lower_h = haystack.to_lowercase();
                let lower_p = pattern.to_lowercase();
                lower_h.contains(&lower_p)
            };
            if found {
                matches.push(SearchMatch {
                    path: rel_path,
                    line: None,
                    content: art.content.chars().take(500).collect(),
                });
            }
        }

        Ok(SearchResult { matches })
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
