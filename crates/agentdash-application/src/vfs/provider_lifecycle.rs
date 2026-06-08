//! `lifecycle_vfs` mount: expose lifecycle run and orchestration node projections.

use std::sync::Arc;

use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::{
    ExecutorRunRef, LifecycleRun, LifecycleRunRepository, OrchestrationInstance,
    OrchestrationSourceRef, RuntimeNodeState, RuntimeNodeStatus,
};
use async_trait::async_trait;
use serde::Serialize;
use tracing::info;
use uuid::Uuid;

use super::lifecycle_catalog::{lifecycle_active_entries, lifecycle_root_entries};
use super::mount::{PROVIDER_LIFECYCLE_VFS, list_inline_entries};
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery, SearchResult,
};
use super::provider_skill_asset::{
    list_projected_skill_files, parse_skill_asset_mount_metadata, read_projected_skill_file,
    search_projected_skill_files,
};
use super::types::{ListOptions, ListResult, ReadResult};
use crate::runtime::{Mount, RuntimeFileEntry};
use crate::session::SessionPersistence;
use crate::workflow::execution_log::RuntimeNodeArtifactScope;
use crate::workflow::lifecycle::journey::{
    LifecycleJourneyError, LifecycleJourneyProjection, SessionItemView, filter_session_items,
    item_file_name, session_summary_archives, to_json_pretty,
};

pub struct LifecycleMountProvider {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    skill_asset_repo: Arc<dyn SkillAssetRepository>,
    journey: LifecycleJourneyProjection,
}

impl LifecycleMountProvider {
    pub fn new(
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            skill_asset_repo,
            journey: LifecycleJourneyProjection::new(inline_file_repo, session_persistence),
        }
    }
}

#[derive(Debug, Serialize)]
struct LifecycleRunOverview<'a> {
    id: Uuid,
    project_id: Uuid,
    status: &'a agentdash_domain::workflow::LifecycleRunStatus,
    orchestration_count: usize,
    active_node_refs: Vec<String>,
    log_count: usize,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_activity_at: chrono::DateTime<chrono::Utc>,
}

struct LifecycleMountContext {
    run: LifecycleRun,
    orchestration: OrchestrationInstance,
    node_path: String,
    attempt: u32,
}

fn map_domain_err(error: agentdash_domain::common::error::DomainError) -> MountError {
    MountError::OperationFailed(error.to_string())
}

fn map_journey_err(error: LifecycleJourneyError) -> MountError {
    match error {
        LifecycleJourneyError::NotFound(message) => MountError::NotFound(message),
        LifecycleJourneyError::OperationFailed(message) => MountError::OperationFailed(message),
    }
}

fn parse_uuid_metadata(mount: &Mount, key: &str) -> Result<Uuid, MountError> {
    let raw = mount
        .metadata
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| MountError::OperationFailed(format!("mount metadata 缺少 {key}")))?;
    Uuid::parse_str(raw)
        .map_err(|error| MountError::OperationFailed(format!("mount metadata {key} 无效: {error}")))
}

fn parse_run_id_from_metadata(mount: &Mount) -> Result<Uuid, MountError> {
    parse_uuid_metadata(mount, "run_id")
}

fn parse_orchestration_id_from_metadata(mount: &Mount) -> Result<Uuid, MountError> {
    parse_uuid_metadata(mount, "orchestration_id")
}

fn parse_node_path_from_metadata(mount: &Mount) -> Result<String, MountError> {
    mount
        .metadata
        .get("node_path")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| MountError::OperationFailed("mount metadata 缺少 node_path".to_string()))
}

fn parse_attempt_from_metadata(mount: &Mount) -> Result<u32, MountError> {
    mount
        .metadata
        .get("attempt")
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| MountError::OperationFailed("mount metadata 缺少 attempt".to_string()))
}

fn runtime_scope_from_mount(mount: &Mount) -> Result<RuntimeNodeArtifactScope, MountError> {
    Ok(RuntimeNodeArtifactScope {
        run_id: parse_run_id_from_metadata(mount)?,
        orchestration_id: parse_orchestration_id_from_metadata(mount)?,
        node_path: parse_node_path_from_metadata(mount)?,
        attempt: parse_attempt_from_metadata(mount)?,
    })
}

async fn load_active_context(
    run_repo: &Arc<dyn LifecycleRunRepository>,
    mount: &Mount,
) -> Result<LifecycleMountContext, MountError> {
    let run_id = parse_run_id_from_metadata(mount)?;
    let orchestration_id = parse_orchestration_id_from_metadata(mount)?;
    let node_path = parse_node_path_from_metadata(mount)?;
    let attempt = parse_attempt_from_metadata(mount)?;
    let run = run_repo
        .get_by_id(run_id)
        .await
        .map_err(map_domain_err)?
        .ok_or_else(|| MountError::NotFound(format!("lifecycle run 不存在: {run_id}")))?;
    let orchestration = run
        .orchestrations
        .iter()
        .find(|item| item.orchestration_id == orchestration_id)
        .cloned()
        .ok_or_else(|| MountError::NotFound(format!("orchestration 不存在: {orchestration_id}")))?;
    Ok(LifecycleMountContext {
        run,
        orchestration,
        node_path,
        attempt,
    })
}

fn segments_from_path(path: &str) -> Vec<&str> {
    if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect()
    }
}

fn lifecycle_mount_has_skills(mount: &Mount) -> bool {
    parse_skill_asset_mount_metadata(mount)
        .map(|(_, keys)| !keys.is_empty())
        .unwrap_or(false)
}

fn flatten_nodes<'a>(nodes: &'a [RuntimeNodeState], out: &mut Vec<&'a RuntimeNodeState>) {
    for node in nodes {
        out.push(node);
        flatten_nodes(&node.children, out);
    }
}

fn all_nodes(orchestration: &OrchestrationInstance) -> Vec<&RuntimeNodeState> {
    let mut nodes = Vec::new();
    flatten_nodes(&orchestration.node_tree, &mut nodes);
    nodes
}

fn find_node<'a>(
    orchestration: &'a OrchestrationInstance,
    node_path: &str,
    attempt: Option<u32>,
) -> Result<&'a RuntimeNodeState, MountError> {
    all_nodes(orchestration)
        .into_iter()
        .find(|node| {
            node.node_path == node_path && attempt.is_none_or(|value| node.attempt == value)
        })
        .ok_or_else(|| MountError::NotFound(format!("runtime node 不存在: {node_path}")))
}

fn current_node(ctx: &LifecycleMountContext) -> Result<&RuntimeNodeState, MountError> {
    find_node(&ctx.orchestration, &ctx.node_path, Some(ctx.attempt))
}

fn node_session_id(node: &RuntimeNodeState) -> Option<String> {
    match &node.executor_run_ref {
        Some(ExecutorRunRef::RuntimeSession { session_id }) => Some(session_id.clone()),
        _ => None,
    }
}

fn session_id_for_node(node: &RuntimeNodeState) -> Result<String, MountError> {
    node_session_id(node)
        .ok_or_else(|| MountError::NotFound(format!("node `{}` 没有关联 session", node.node_path)))
}

fn run_overview(run: &LifecycleRun) -> LifecycleRunOverview<'_> {
    let active_node_refs = run
        .orchestrations
        .iter()
        .flat_map(|orchestration| {
            all_nodes(orchestration)
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
                        orchestration.orchestration_id, node.node_path, node.attempt, node.status
                    )
                })
        })
        .collect();
    LifecycleRunOverview {
        id: run.id,
        project_id: run.project_id,
        status: &run.status,
        orchestration_count: run.orchestrations.len(),
        active_node_refs,
        log_count: run.execution_log.len(),
        created_at: run.created_at,
        updated_at: run.updated_at,
        last_activity_at: run.last_activity_at,
    }
}

fn same_source_family(left: &OrchestrationSourceRef, right: &OrchestrationSourceRef) -> bool {
    match (left, right) {
        (
            OrchestrationSourceRef::WorkflowGraph { graph_id: left, .. },
            OrchestrationSourceRef::WorkflowGraph {
                graph_id: right, ..
            },
        ) => left == right,
        (
            OrchestrationSourceRef::RunScriptArtifact {
                artifact_id: left, ..
            },
            OrchestrationSourceRef::RunScriptArtifact {
                artifact_id: right, ..
            },
        ) => left == right,
        (
            OrchestrationSourceRef::WorkflowScript {
                script_id: left, ..
            },
            OrchestrationSourceRef::WorkflowScript {
                script_id: right, ..
            },
        ) => left == right,
        (
            OrchestrationSourceRef::Inline {
                source_digest: left,
            },
            OrchestrationSourceRef::Inline {
                source_digest: right,
            },
        ) => left == right,
        _ => false,
    }
}

fn run_has_source_family(run: &LifecycleRun, source_ref: &OrchestrationSourceRef) -> bool {
    run.orchestrations
        .iter()
        .any(|orchestration| same_source_family(&orchestration.source_ref, source_ref))
}

fn records_prefix(node_path: &str) -> String {
    crate::workflow::execution_log::encode_node_path_segment(node_path)
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

        if matches!(segs.as_slice(), ["skills", ..]) {
            return read_projected_skill_file(self.skill_asset_repo.as_ref(), mount, &path_norm)
                .await;
        }

        let content = match segs.as_slice() {
            ["artifacts"] | ["active", "artifacts"] => {
                let scope = runtime_scope_from_mount(mount)?;
                let map = self
                    .journey
                    .list_scoped_port_outputs(&scope)
                    .await
                    .map_err(map_journey_err)?;
                to_json_pretty(&map).map_err(map_journey_err)?
            }
            ["artifacts", port_key] | ["active", "artifacts", port_key] => {
                let artifact_ref = runtime_scope_from_mount(mount)?.port_ref(*port_key);
                self.journey
                    .read_scoped_port_output(&artifact_ref)
                    .await
                    .map_err(map_journey_err)?
            }
            _ => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let run_id = active.run.id;
                match segs.as_slice() {
                    [] | ["active"] => {
                        to_json_pretty(&run_overview(&active.run)).map_err(map_journey_err)?
                    }
                    ["active", "steps"] => {
                        let nodes = all_nodes(&active.orchestration);
                        to_json_pretty(&nodes).map_err(map_journey_err)?
                    }
                    ["active", "steps", key] | ["nodes", key, "state"] => {
                        let node = find_node(&active.orchestration, key, None)?;
                        to_json_pretty(node).map_err(map_journey_err)?
                    }
                    ["active", "log"] => {
                        to_json_pretty(&active.run.execution_log).map_err(map_journey_err)?
                    }
                    ["state"] => {
                        let node = current_node(&active)?;
                        to_json_pretty(node).map_err(map_journey_err)?
                    }
                    ["session", rest @ ..] => {
                        let session_id = session_id_for_node(current_node(&active)?)?;
                        self.journey
                            .read_session_projection(&session_id, rest)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["records"] => self
                        .journey
                        .read_records_map(run_id, &records_prefix(&active.node_path))
                        .await
                        .map_err(map_journey_err)?,
                    ["records", rest @ ..] => self
                        .journey
                        .read_record(run_id, &records_prefix(&active.node_path), rest)
                        .await
                        .map_err(map_journey_err)?,
                    ["runs"] => {
                        let runs = self
                            .lifecycle_run_repo
                            .list_by_project(active.run.project_id)
                            .await
                            .map_err(map_domain_err)?;
                        let summaries = runs
                            .iter()
                            .filter(|run| {
                                run_has_source_family(run, &active.orchestration.source_ref)
                            })
                            .map(run_overview)
                            .collect::<Vec<_>>();
                        to_json_pretty(&summaries).map_err(map_journey_err)?
                    }
                    ["runs", id_str] => {
                        let run_id = Uuid::parse_str(id_str).map_err(|error| {
                            MountError::OperationFailed(format!("run id 无效: {error}"))
                        })?;
                        let run = self
                            .lifecycle_run_repo
                            .get_by_id(run_id)
                            .await
                            .map_err(map_domain_err)?
                            .ok_or_else(|| MountError::NotFound(format!("run 不存在: {run_id}")))?;
                        to_json_pretty(&run_overview(&run)).map_err(map_journey_err)?
                    }
                    ["nodes", key, "records"] => {
                        find_node(&active.orchestration, key, None)?;
                        self.journey
                            .read_records_map(run_id, &records_prefix(key))
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["nodes", key, "records", rest @ ..] => {
                        find_node(&active.orchestration, key, None)?;
                        self.journey
                            .read_record(run_id, &records_prefix(key), rest)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["nodes", key, "session", rest @ ..] => {
                        let node = find_node(&active.orchestration, key, None)?;
                        let session_id = session_id_for_node(node)?;
                        self.journey
                            .read_session_projection(&session_id, rest)
                            .await
                            .map_err(map_journey_err)?
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
            ["artifacts", port_key] | ["active", "artifacts", port_key] => {
                let allowed_keys = mount
                    .metadata
                    .get("writable_port_keys")
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !allowed_keys.contains(port_key) {
                    return Err(MountError::OperationFailed(format!(
                        "当前 node 没有名为 `{port_key}` 的 output port，可写 port: {:?}",
                        allowed_keys
                    )));
                }
                let artifact_ref = runtime_scope_from_mount(mount)?.port_ref(*port_key);
                self.journey
                    .write_scoped_port_output(&artifact_ref, content)
                    .await
                    .map_err(map_journey_err)?;
                info!(
                    run_id = %artifact_ref.run_id,
                    orchestration_id = %artifact_ref.orchestration_id,
                    node_path = %artifact_ref.node_path,
                    attempt = artifact_ref.attempt,
                    port_key = %port_key,
                    scoped_path = %artifact_ref.inline_path(),
                    content_len = content.len(),
                    "lifecycle VFS: wrote scoped port output"
                );
                Ok(())
            }
            ["records", rest @ ..] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                self.journey
                    .write_record(
                        active.run.id,
                        &records_prefix(&active.node_path),
                        rest,
                        content,
                    )
                    .await
                    .map_err(map_journey_err)?;
                Ok(())
            }
            ["nodes", key, "records", rest @ ..] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                find_node(&active.orchestration, key, None)?;
                self.journey
                    .write_record(active.run.id, &records_prefix(key), rest, content)
                    .await
                    .map_err(map_journey_err)?;
                Ok(())
            }
            _ => Err(MountError::NotSupported(format!(
                "lifecycle_vfs 不支持写入路径: `{path_norm}`"
            ))),
        }
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let path_norm = normalize_mount_relative_path(&options.path, true)
            .map_err(MountError::OperationFailed)?;
        if path_norm == "skills" || path_norm.starts_with("skills/") {
            return list_projected_skill_files(self.skill_asset_repo.as_ref(), mount, options)
                .await;
        }
        let segs = segments_from_path(&path_norm);
        let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
        let mut entries = match segs.as_slice() {
            [] => lifecycle_root_entries(lifecycle_mount_has_skills(mount)),
            ["active"] => lifecycle_active_entries(active.run.execution_log.len() as u64),
            ["active", "steps"] | ["nodes"] => all_nodes(&active.orchestration)
                .into_iter()
                .map(|node| RuntimeFileEntry::file(node.node_path.clone()).as_virtual())
                .collect(),
            ["artifacts"] | ["active", "artifacts"] => {
                let scope = runtime_scope_from_mount(mount)?;
                self.journey
                    .list_scoped_port_outputs(&scope)
                    .await
                    .map_err(map_journey_err)?
                    .into_keys()
                    .map(|key| RuntimeFileEntry::file(format!("{path_norm}/{key}")).as_virtual())
                    .collect()
            }
            ["records"] => {
                let map = self
                    .journey
                    .records_map(active.run.id, &records_prefix(&active.node_path))
                    .await
                    .map_err(map_journey_err)?;
                list_inline_entries(&map, "", options.pattern.as_deref(), options.recursive)
                    .into_iter()
                    .map(|mut entry| {
                        entry.path = format!("records/{}", entry.path);
                        entry
                    })
                    .collect()
            }
            ["session", "items"] => {
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/items",
                    SessionItemView::Items,
                )
                .await?
            }
            ["session", "messages"] => {
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/messages",
                    SessionItemView::Messages,
                )
                .await?
            }
            ["session", "tools"] => {
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/tools",
                    SessionItemView::Tools,
                )
                .await?
            }
            ["session", "writes"] => {
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/writes",
                    SessionItemView::Writes,
                )
                .await?
            }
            ["session", "summaries"] => {
                list_session_summary_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/summaries",
                )
                .await?
            }
            _ => Vec::new(),
        };
        if let Some(pattern) = options.pattern.as_deref().filter(|value| !value.is_empty()) {
            entries.retain(|entry| entry.path.contains(pattern));
        }
        Ok(ListResult { entries })
    }

    async fn search_text(
        &self,
        mount: &Mount,
        query: &SearchQuery,
        ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        if query
            .path
            .as_deref()
            .is_some_and(|path| path == "skills" || path.starts_with("skills/"))
        {
            return search_projected_skill_files(self.skill_asset_repo.as_ref(), mount, query)
                .await;
        }
        let listing = self
            .list(
                mount,
                &ListOptions {
                    path: query.path.clone().unwrap_or_default(),
                    pattern: None,
                    recursive: true,
                },
                ctx,
            )
            .await?;
        let needle = if query.case_sensitive {
            query.pattern.clone()
        } else {
            query.pattern.to_lowercase()
        };
        let max = query.max_results.unwrap_or(usize::MAX);
        let mut matches = Vec::new();
        for entry in listing.entries.into_iter().filter(|entry| !entry.is_dir) {
            let Ok(read) = self.read_text(mount, &entry.path, ctx).await else {
                continue;
            };
            for (idx, line) in read.content.lines().enumerate() {
                let haystack = if query.case_sensitive {
                    line.to_string()
                } else {
                    line.to_lowercase()
                };
                if !haystack.contains(&needle) {
                    continue;
                }
                matches.push(SearchMatch {
                    path: read.path.clone(),
                    line: Some((idx + 1) as u32),
                    content: line.trim().to_string(),
                });
                if matches.len() >= max {
                    return Ok(SearchResult {
                        matches,
                        truncated: true,
                    });
                }
            }
        }
        Ok(SearchResult {
            matches,
            truncated: false,
        })
    }
}

async fn list_session_item_entries(
    journey: &LifecycleJourneyProjection,
    session_id: &str,
    display_root: &str,
    view: SessionItemView,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let projections = journey
        .session_item_projections(session_id)
        .await
        .map_err(map_journey_err)?;
    Ok(filter_session_items(&projections, view)
        .iter()
        .map(|projection| {
            RuntimeFileEntry::file(format!(
                "{display_root}/{}",
                item_file_name(projection, view)
            ))
            .as_virtual()
        })
        .collect())
}

async fn list_session_summary_entries(
    journey: &LifecycleJourneyProjection,
    session_id: &str,
    display_root: &str,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let entries = session_summary_archives(journey.session_persistence(), session_id)
        .await
        .map_err(map_journey_err)?;
    Ok(entries
        .into_iter()
        .filter_map(|(entry, _)| {
            entry
                .path
                .strip_prefix("session/summaries/")
                .map(|name| RuntimeFileEntry::file(format!("{display_root}/{name}")).as_virtual())
        })
        .collect())
}
