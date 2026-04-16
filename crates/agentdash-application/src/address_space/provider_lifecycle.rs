//! `lifecycle_vfs` mount: 通过 `LifecycleRunRepository` 暴露当前 lifecycle run 的虚拟文件视图。

use std::sync::Arc;

use super::mount::PROVIDER_LIFECYCLE_VFS;
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::{Mount, RuntimeFileEntry};
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use agentdash_domain::workflow::{
    LifecycleRun, LifecycleRunRepository, LifecycleRunStatus, WorkflowRecordArtifact,
    WorkflowRecordArtifactType,
};
use async_trait::async_trait;
use serde::Serialize;
use tracing::info;
use uuid::Uuid;

pub struct LifecycleMountProvider {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    inline_file_repo: Arc<dyn InlineFileRepository>,
}

impl LifecycleMountProvider {
    pub fn new(
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            inline_file_repo,
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
        session_id: &run.session_id,
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

/// 按 step_key 过滤 artifact
fn artifacts_for_node<'a>(
    run: &'a LifecycleRun,
    node_key: &str,
) -> Vec<&'a WorkflowRecordArtifact> {
    run.record_artifacts
        .iter()
        .filter(|a| a.step_key == node_key)
        .collect()
}

/// 解析 artifact_type snake_case 字符串
fn parse_artifact_type(s: &str) -> Option<WorkflowRecordArtifactType> {
    let quoted = format!("\"{}\"", s);
    serde_json::from_str(&quoted).ok()
}

/// 按 step_key + artifact_type 过滤，按创建时间降序
fn artifacts_by_type<'a>(
    run: &'a LifecycleRun,
    node_key: &str,
    type_str: &str,
) -> Result<Vec<&'a WorkflowRecordArtifact>, MountError> {
    let artifact_type = parse_artifact_type(type_str)
        .ok_or_else(|| MountError::NotFound(format!("未知 artifact_type: {type_str}")))?;
    let mut matched: Vec<&WorkflowRecordArtifact> = run
        .record_artifacts
        .iter()
        .filter(|a| a.step_key == node_key && a.artifact_type == artifact_type)
        .collect();
    matched.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(matched)
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
                            .ok_or_else(|| {
                                MountError::NotFound(format!("step 不存在: {key}"))
                            })?;
                        to_json_pretty(step)?
                    }
                    ["active", "artifacts"] => to_json_pretty(&active.record_artifacts)?,
                    ["active", "artifacts", id_str] => {
                        let aid = Uuid::parse_str(id_str).map_err(|e| {
                            MountError::OperationFailed(format!("artifact id 无效: {e}"))
                        })?;
                        let art = active
                            .record_artifacts
                            .iter()
                            .find(|a| a.id == aid)
                            .ok_or_else(|| {
                                MountError::NotFound(format!("artifact 不存在: {aid}"))
                            })?;
                        art.content.clone()
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
                            .ok_or_else(|| {
                                MountError::NotFound(format!("run 不存在: {rid}"))
                            })?;
                        to_json_pretty(&run_overview(&run))?
                    }
                    // ── nodes/ 路径族 ──────────────────────────────────
                    ["nodes", key, "state"] => {
                        let step = active
                            .step_states
                            .iter()
                            .find(|s| s.step_key == *key)
                            .ok_or_else(|| {
                                MountError::NotFound(format!("node 不存在: {key}"))
                            })?;
                        to_json_pretty(step)?
                    }
                    ["nodes", key, "artifacts"] => {
                        let arts = artifacts_for_node(&active, key);
                        to_json_pretty(&arts)?
                    }
                    ["nodes", key, "artifacts", "by-type", type_str] => {
                        let matched = artifacts_by_type(&active, key, type_str)?;
                        let latest = matched.first().ok_or_else(|| {
                            MountError::NotFound(format!(
                                "node `{key}` 无 `{type_str}` 类型 artifact"
                            ))
                        })?;
                        latest.content.clone()
                    }
                    ["nodes", key, "artifacts", "by-type", type_str, "list"] => {
                        let matched = artifacts_by_type(&active, key, type_str)?;
                        to_json_pretty(&matched)?
                    }
                    ["nodes", key, "artifacts", id_str] => {
                        let aid = Uuid::parse_str(id_str).map_err(|e| {
                            MountError::OperationFailed(format!("artifact id 无效: {e}"))
                        })?;
                        let art = active
                            .record_artifacts
                            .iter()
                            .find(|a| a.id == aid && a.step_key == *key)
                            .ok_or_else(|| {
                                MountError::NotFound(format!(
                                    "node `{key}` 下 artifact 不存在: {aid}"
                                ))
                            })?;
                        art.content.clone()
                    }
                    _ => {
                        return Err(MountError::NotFound(format!(
                            "lifecycle_vfs 不支持的路径: `{path_norm}`"
                        )));
                    }
                }
            }
        };

        Ok(ReadResult {
            path: path_norm,
            content,
        })
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
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                    })
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
                RuntimeFileEntry {
                    path: "active".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: true,
                },
                RuntimeFileEntry {
                    path: "artifacts".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: true,
                },
                RuntimeFileEntry {
                    path: "nodes".to_string(),
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
                    .map(|f| RuntimeFileEntry {
                        path: format!("artifacts/{}", f.path),
                        size: Some(f.content.len() as u64),
                        modified_at: None,
                        is_dir: false,
                    })
                    .collect()
            }
            // ── nodes/ 路径族 ──────────────────────────────────
            ["nodes"] => active
                .step_states
                .iter()
                .map(|s| RuntimeFileEntry {
                    path: format!("nodes/{}", s.step_key),
                    size: None,
                    modified_at: None,
                    is_dir: true,
                })
                .collect(),
            ["nodes", key] => {
                if !active.step_states.iter().any(|s| s.step_key == *key) {
                    Vec::new()
                } else {
                    vec![
                        RuntimeFileEntry {
                            path: format!("nodes/{key}/state"),
                            size: None,
                            modified_at: None,
                            is_dir: false,
                        },
                        RuntimeFileEntry {
                            path: format!("nodes/{key}/artifacts"),
                            size: None,
                            modified_at: None,
                            is_dir: true,
                        },
                    ]
                }
            }
            ["nodes", key, "artifacts"] => {
                let arts = artifacts_for_node(&active, key);
                arts.iter()
                    .map(|a| RuntimeFileEntry {
                        path: format!("nodes/{key}/artifacts/{}", a.id),
                        size: Some(a.content.len() as u64),
                        modified_at: None,
                        is_dir: false,
                    })
                    .collect()
            }
            ["runs"] => {
                let sid = resolve_session_id_for_runs(mount, &active);
                let runs = self
                    .lifecycle_run_repo
                    .list_by_session(&sid)
                    .await
                    .map_err(map_domain_err)?;
                runs.iter()
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
