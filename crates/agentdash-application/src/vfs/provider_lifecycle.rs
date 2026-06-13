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
use crate::workflow::execution_log::{
    RuntimeNodeArtifactScope, decode_node_path_segment, encode_node_path_segment,
};
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

struct LifecycleRunMountContext {
    run: LifecycleRun,
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

fn mount_has_node_scope(mount: &Mount) -> bool {
    mount.metadata.get("orchestration_id").is_some()
        && mount.metadata.get("node_path").is_some()
        && mount.metadata.get("attempt").is_some()
}

fn runtime_scope_from_mount(mount: &Mount) -> Result<RuntimeNodeArtifactScope, MountError> {
    Ok(RuntimeNodeArtifactScope {
        run_id: parse_run_id_from_metadata(mount)?,
        orchestration_id: parse_orchestration_id_from_metadata(mount)?,
        node_path: parse_node_path_from_metadata(mount)?,
        attempt: parse_attempt_from_metadata(mount)?,
    })
}

async fn load_run_context(
    run_repo: &Arc<dyn LifecycleRunRepository>,
    mount: &Mount,
) -> Result<LifecycleRunMountContext, MountError> {
    let run_id = parse_run_id_from_metadata(mount)?;
    let run = run_repo
        .get_by_id(run_id)
        .await
        .map_err(map_domain_err)?
        .ok_or_else(|| MountError::NotFound(format!("lifecycle run 不存在: {run_id}")))?;
    Ok(LifecycleRunMountContext { run })
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

fn find_orchestration(
    run: &LifecycleRun,
    orchestration_id: Uuid,
) -> Result<&OrchestrationInstance, MountError> {
    run.orchestrations
        .iter()
        .find(|item| item.orchestration_id == orchestration_id)
        .ok_or_else(|| MountError::NotFound(format!("orchestration 不存在: {orchestration_id}")))
}

fn active_orchestration(run: &LifecycleRun) -> Result<&OrchestrationInstance, MountError> {
    run.orchestrations
        .iter()
        .find(|orchestration| {
            matches!(
                orchestration.status,
                agentdash_domain::workflow::OrchestrationStatus::Running
                    | agentdash_domain::workflow::OrchestrationStatus::Paused
                    | agentdash_domain::workflow::OrchestrationStatus::Pending
            )
        })
        .or_else(|| run.orchestrations.last())
        .ok_or_else(|| MountError::NotFound("lifecycle run 没有 orchestration".to_string()))
}

fn active_node(orchestration: &OrchestrationInstance) -> Option<&RuntimeNodeState> {
    all_nodes(orchestration)
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
        .or_else(|| all_nodes(orchestration).into_iter().last())
}

fn active_context_from_run(run: LifecycleRun) -> Result<LifecycleMountContext, MountError> {
    let orchestration = active_orchestration(&run)?.clone();
    let node = active_node(&orchestration).ok_or_else(|| {
        MountError::NotFound("active orchestration 没有 runtime node".to_string())
    })?;
    let node_path = node.node_path.clone();
    let attempt = node.attempt;
    Ok(LifecycleMountContext {
        run,
        orchestration,
        node_path,
        attempt,
    })
}

async fn load_active_or_run_context(
    run_repo: &Arc<dyn LifecycleRunRepository>,
    mount: &Mount,
) -> Result<LifecycleMountContext, MountError> {
    if mount_has_node_scope(mount) {
        return load_active_context(run_repo, mount).await;
    }
    let run_ctx = load_run_context(run_repo, mount).await?;
    active_context_from_run(run_ctx.run)
}

fn decode_node_key(value: &str) -> Result<String, MountError> {
    decode_node_path_segment(value).map_err(MountError::OperationFailed)
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

fn find_node_by_segment<'a>(
    orchestration: &'a OrchestrationInstance,
    node_key: &str,
) -> Result<&'a RuntimeNodeState, MountError> {
    let node_path = decode_node_key(node_key)?;
    find_node(orchestration, &node_path, None)
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
    encode_node_path_segment(node_path)
}

fn lifecycle_root_entries_for_scope(
    include_skills: bool,
    node_scoped: bool,
    run: &LifecycleRun,
) -> Vec<RuntimeFileEntry> {
    if node_scoped {
        return lifecycle_root_entries(include_skills);
    }
    let mut entries = vec![
        RuntimeFileEntry::file("state").as_virtual(),
        RuntimeFileEntry::file("context").as_virtual(),
        RuntimeFileEntry::dir("orchestrations").as_virtual(),
        RuntimeFileEntry::dir("runs").as_virtual(),
    ];
    if !run.orchestrations.is_empty() {
        entries.push(RuntimeFileEntry::dir("active").as_virtual());
    }
    if run.orchestrations.len() == 1 {
        entries.push(RuntimeFileEntry::dir("nodes").as_virtual());
    }
    if include_skills {
        entries.push(RuntimeFileEntry::dir("skills").as_virtual());
    }
    entries
}

fn orchestration_entries(run: &LifecycleRun) -> Vec<RuntimeFileEntry> {
    run.orchestrations
        .iter()
        .map(|orchestration| {
            RuntimeFileEntry::dir(format!("orchestrations/{}", orchestration.orchestration_id))
                .as_virtual()
        })
        .collect()
}

fn orchestration_root_entries(orchestration_id: Uuid) -> Vec<RuntimeFileEntry> {
    vec![
        RuntimeFileEntry::file(format!("orchestrations/{orchestration_id}/state")).as_virtual(),
        RuntimeFileEntry::dir(format!("orchestrations/{orchestration_id}/nodes")).as_virtual(),
        RuntimeFileEntry::file(format!("orchestrations/{orchestration_id}/log")).as_virtual(),
    ]
}

fn node_root_entries(prefix: &str, node: &RuntimeNodeState) -> Vec<RuntimeFileEntry> {
    let mut entries = vec![
        RuntimeFileEntry::file(format!("{prefix}/state")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/records")),
    ];
    if node_session_id(node).is_some() {
        entries.push(RuntimeFileEntry::dir(format!("{prefix}/session")).as_virtual());
    }
    entries
}

fn node_entries(prefix: &str, orchestration: &OrchestrationInstance) -> Vec<RuntimeFileEntry> {
    all_nodes(orchestration)
        .into_iter()
        .map(|node| {
            RuntimeFileEntry::dir(format!(
                "{prefix}/{}",
                encode_node_path_segment(&node.node_path)
            ))
            .as_virtual()
        })
        .collect()
}

fn session_root_entries(prefix: &str) -> Vec<RuntimeFileEntry> {
    vec![
        RuntimeFileEntry::file(format!("{prefix}/meta")).as_virtual(),
        RuntimeFileEntry::file(format!("{prefix}/summary")).as_virtual(),
        RuntimeFileEntry::file(format!("{prefix}/conclusions")).as_virtual(),
        RuntimeFileEntry::file(format!("{prefix}/events.json")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/items")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/messages")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/tools")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/writes")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/summaries")).as_virtual(),
        RuntimeFileEntry::file(format!("{prefix}/terminal")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/turns")).as_virtual(),
    ]
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

        let run_ctx = load_run_context(&self.lifecycle_run_repo, mount).await?;
        let node_scoped = mount_has_node_scope(mount);
        let content = match segs.as_slice() {
            [] => to_json_pretty(&run_overview(&run_ctx.run)).map_err(map_journey_err)?,
            ["state"] if !node_scoped => {
                to_json_pretty(&run_overview(&run_ctx.run)).map_err(map_journey_err)?
            }
            ["context"] => to_json_pretty(&run_ctx.run.context).map_err(map_journey_err)?,
            ["orchestrations"] => {
                to_json_pretty(&run_ctx.run.orchestrations).map_err(map_journey_err)?
            }
            ["orchestrations", orchestration_id]
            | ["orchestrations", orchestration_id, "state"] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                to_json_pretty(orchestration).map_err(map_journey_err)?
            }
            ["orchestrations", orchestration_id, "nodes"] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let nodes = all_nodes(orchestration);
                to_json_pretty(&nodes).map_err(map_journey_err)?
            }
            ["orchestrations", orchestration_id, "nodes", node_key]
            | [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "state",
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                to_json_pretty(node).map_err(map_journey_err)?
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "session",
                rest @ ..,
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                let session_id = session_id_for_node(node)?;
                self.journey
                    .read_session_projection(&session_id, rest)
                    .await
                    .map_err(map_journey_err)?
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "records",
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                self.journey
                    .read_records_map(run_ctx.run.id, &records_prefix(&node.node_path))
                    .await
                    .map_err(map_journey_err)?
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "records",
                rest @ ..,
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                self.journey
                    .read_record(run_ctx.run.id, &records_prefix(&node.node_path), rest)
                    .await
                    .map_err(map_journey_err)?
            }
            ["orchestrations", orchestration_id, "log"] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                find_orchestration(&run_ctx.run, orchestration_id)?;
                to_json_pretty(&run_ctx.run.execution_log).map_err(map_journey_err)?
            }
            ["runs"] => {
                let source_ref = if node_scoped {
                    Some(
                        load_active_context(&self.lifecycle_run_repo, mount)
                            .await?
                            .orchestration
                            .source_ref,
                    )
                } else {
                    None
                };
                let runs = self
                    .lifecycle_run_repo
                    .list_by_project(run_ctx.run.project_id)
                    .await
                    .map_err(map_domain_err)?;
                let summaries = runs
                    .iter()
                    .filter(|run| {
                        source_ref
                            .as_ref()
                            .is_none_or(|source| run_has_source_family(run, source))
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
            ["artifacts"] | ["active", "artifacts"] => {
                let scope = load_active_or_run_context(&self.lifecycle_run_repo, mount)
                    .await
                    .map(|active| RuntimeNodeArtifactScope {
                        run_id: active.run.id,
                        orchestration_id: active.orchestration.orchestration_id,
                        node_path: active.node_path,
                        attempt: active.attempt,
                    })?;
                let map = self
                    .journey
                    .list_scoped_port_outputs(&scope)
                    .await
                    .map_err(map_journey_err)?;
                to_json_pretty(&map).map_err(map_journey_err)?
            }
            ["artifacts", port_key] | ["active", "artifacts", port_key] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let artifact_ref = RuntimeNodeArtifactScope {
                    run_id: active.run.id,
                    orchestration_id: active.orchestration.orchestration_id,
                    node_path: active.node_path,
                    attempt: active.attempt,
                }
                .port_ref(*port_key);
                self.journey
                    .read_scoped_port_output(&artifact_ref)
                    .await
                    .map_err(map_journey_err)?
            }
            _ => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
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
                        let node = find_node_by_segment(&active.orchestration, key)?;
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
                    ["nodes", key, "records"] => {
                        let node = find_node_by_segment(&active.orchestration, key)?;
                        self.journey
                            .read_records_map(run_id, &records_prefix(&node.node_path))
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["nodes", key, "records", rest @ ..] => {
                        let node = find_node_by_segment(&active.orchestration, key)?;
                        self.journey
                            .read_record(run_id, &records_prefix(&node.node_path), rest)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["nodes", key, "session", rest @ ..] => {
                        let node = find_node_by_segment(&active.orchestration, key)?;
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
        if !mount_has_node_scope(mount) {
            return Err(MountError::NotSupported(
                "run-scoped lifecycle_vfs 只支持只读浏览；写入需要 node-scoped runtime mount"
                    .to_string(),
            ));
        }

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
                let node = find_node_by_segment(&active.orchestration, key)?;
                self.journey
                    .write_record(
                        active.run.id,
                        &records_prefix(&node.node_path),
                        rest,
                        content,
                    )
                    .await
                    .map_err(map_journey_err)?;
                Ok(())
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "records",
                rest @ ..,
            ] => {
                let run_ctx = load_run_context(&self.lifecycle_run_repo, mount).await?;
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                self.journey
                    .write_record(
                        run_ctx.run.id,
                        &records_prefix(&node.node_path),
                        rest,
                        content,
                    )
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
        let node_scoped = mount_has_node_scope(mount);
        let run_ctx = load_run_context(&self.lifecycle_run_repo, mount).await?;
        let mut entries = match segs.as_slice() {
            [] => lifecycle_root_entries_for_scope(
                lifecycle_mount_has_skills(mount),
                node_scoped,
                &run_ctx.run,
            ),
            ["orchestrations"] => orchestration_entries(&run_ctx.run),
            ["orchestrations", orchestration_id] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                find_orchestration(&run_ctx.run, orchestration_id)?;
                orchestration_root_entries(orchestration_id)
            }
            ["orchestrations", orchestration_id, "nodes"] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                node_entries(
                    &format!("orchestrations/{orchestration_id}/nodes"),
                    orchestration,
                )
            }
            ["orchestrations", orchestration_id, "nodes", node_key] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                node_root_entries(
                    &format!("orchestrations/{orchestration_id}/nodes/{node_key}"),
                    node,
                )
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "session",
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                session_id_for_node(node)?;
                session_root_entries(&format!(
                    "orchestrations/{orchestration_id}/nodes/{node_key}/session"
                ))
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "records",
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                let map = self
                    .journey
                    .records_map(run_ctx.run.id, &records_prefix(&node.node_path))
                    .await
                    .map_err(map_journey_err)?;
                let prefix = format!("orchestrations/{orchestration_id}/nodes/{node_key}/records");
                list_inline_entries(&map, "", options.pattern.as_deref(), options.recursive)
                    .into_iter()
                    .map(|mut entry| {
                        entry.path = format!("{prefix}/{}", entry.path);
                        entry
                    })
                    .collect()
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "session",
                "items",
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!("orchestrations/{orchestration_id}/nodes/{node_key}/session/items"),
                    SessionItemView::Items,
                )
                .await?
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "session",
                "messages",
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!("orchestrations/{orchestration_id}/nodes/{node_key}/session/messages"),
                    SessionItemView::Messages,
                )
                .await?
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "session",
                "tools",
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!("orchestrations/{orchestration_id}/nodes/{node_key}/session/tools"),
                    SessionItemView::Tools,
                )
                .await?
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "session",
                "writes",
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!("orchestrations/{orchestration_id}/nodes/{node_key}/session/writes"),
                    SessionItemView::Writes,
                )
                .await?
            }
            [
                "orchestrations",
                orchestration_id,
                "nodes",
                node_key,
                "session",
                "summaries",
            ] => {
                let orchestration_id = Uuid::parse_str(orchestration_id).map_err(|error| {
                    MountError::OperationFailed(format!("orchestration id 无效: {error}"))
                })?;
                let orchestration = find_orchestration(&run_ctx.run, orchestration_id)?;
                let node = find_node_by_segment(orchestration, node_key)?;
                list_session_summary_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!(
                        "orchestrations/{orchestration_id}/nodes/{node_key}/session/summaries"
                    ),
                )
                .await?
            }
            ["runs"] => self
                .lifecycle_run_repo
                .list_by_project(run_ctx.run.project_id)
                .await
                .map_err(map_domain_err)?
                .into_iter()
                .map(|run| RuntimeFileEntry::file(format!("runs/{}", run.id)).as_virtual())
                .collect(),
            ["active"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                lifecycle_active_entries(active.run.execution_log.len() as u64)
            }
            ["active", "steps"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                all_nodes(&active.orchestration)
                    .into_iter()
                    .map(|node| {
                        RuntimeFileEntry::file(format!(
                            "active/steps/{}",
                            encode_node_path_segment(&node.node_path)
                        ))
                        .as_virtual()
                    })
                    .collect()
            }
            ["nodes"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                node_entries("nodes", &active.orchestration)
            }
            ["nodes", node_key] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let node = find_node_by_segment(&active.orchestration, node_key)?;
                node_root_entries(&format!("nodes/{node_key}"), node)
            }
            ["artifacts"] | ["active", "artifacts"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let scope = RuntimeNodeArtifactScope {
                    run_id: active.run.id,
                    orchestration_id: active.orchestration.orchestration_id,
                    node_path: active.node_path,
                    attempt: active.attempt,
                };
                self.journey
                    .list_scoped_port_outputs(&scope)
                    .await
                    .map_err(map_journey_err)?
                    .into_keys()
                    .map(|key| RuntimeFileEntry::file(format!("{path_norm}/{key}")).as_virtual())
                    .collect()
            }
            ["records"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
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
            ["nodes", node_key, "records"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let node = find_node_by_segment(&active.orchestration, node_key)?;
                let map = self
                    .journey
                    .records_map(active.run.id, &records_prefix(&node.node_path))
                    .await
                    .map_err(map_journey_err)?;
                let prefix = format!("nodes/{node_key}/records");
                list_inline_entries(&map, "", options.pattern.as_deref(), options.recursive)
                    .into_iter()
                    .map(|mut entry| {
                        entry.path = format!("{prefix}/{}", entry.path);
                        entry
                    })
                    .collect()
            }
            ["session"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                session_id_for_node(current_node(&active)?)?;
                session_root_entries("session")
            }
            ["nodes", node_key, "session"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let node = find_node_by_segment(&active.orchestration, node_key)?;
                session_id_for_node(node)?;
                session_root_entries(&format!("nodes/{node_key}/session"))
            }
            ["session", "items"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/items",
                    SessionItemView::Items,
                )
                .await?
            }
            ["session", "messages"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/messages",
                    SessionItemView::Messages,
                )
                .await?
            }
            ["session", "tools"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/tools",
                    SessionItemView::Tools,
                )
                .await?
            }
            ["session", "writes"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/writes",
                    SessionItemView::Writes,
                )
                .await?
            }
            ["session", "summaries"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                list_session_summary_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/summaries",
                )
                .await?
            }
            ["nodes", node_key, "session", "items"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let node = find_node_by_segment(&active.orchestration, node_key)?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!("nodes/{node_key}/session/items"),
                    SessionItemView::Items,
                )
                .await?
            }
            ["nodes", node_key, "session", "messages"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let node = find_node_by_segment(&active.orchestration, node_key)?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!("nodes/{node_key}/session/messages"),
                    SessionItemView::Messages,
                )
                .await?
            }
            ["nodes", node_key, "session", "tools"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let node = find_node_by_segment(&active.orchestration, node_key)?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!("nodes/{node_key}/session/tools"),
                    SessionItemView::Tools,
                )
                .await?
            }
            ["nodes", node_key, "session", "writes"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let node = find_node_by_segment(&active.orchestration, node_key)?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!("nodes/{node_key}/session/writes"),
                    SessionItemView::Writes,
                )
                .await?
            }
            ["nodes", node_key, "session", "summaries"] => {
                let active = load_active_or_run_context(&self.lifecycle_run_repo, mount).await?;
                let node = find_node_by_segment(&active.orchestration, node_key)?;
                list_session_summary_entries(
                    &self.journey,
                    &session_id_for_node(node)?,
                    &format!("nodes/{node_key}/session/summaries"),
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind};
    use agentdash_domain::skill_asset::{SkillAsset, SkillAssetRepository};
    use agentdash_domain::workflow::{
        LifecycleRunRepository, OrchestrationPlanSnapshot, OrchestrationSourceRef,
        OrchestrationStatus, PlanNode, PlanNodeKind, RuntimeNodeState, RuntimeNodeStatus,
    };
    use chrono::Utc;

    use super::*;
    use crate::session::MemorySessionPersistence;
    use crate::vfs::build_lifecycle_run_mount;

    #[derive(Default)]
    struct RunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    impl RunRepo {
        fn with_run(run: LifecycleRun) -> Self {
            Self {
                runs: Mutex::new(vec![run]),
            }
        }
    }

    #[async_trait::async_trait]
    impl LifecycleRunRepository for RunRepo {
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

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
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

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut runs = self.runs.lock().unwrap();
            if let Some(current) = runs.iter_mut().find(|current| current.id == run.id) {
                *current = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().unwrap().retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct EmptyInlineRepo;

    #[async_trait::async_trait]
    impl InlineFileRepository for EmptyInlineRepo {
        async fn get_file(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
            _path: &str,
        ) -> Result<Option<InlineFile>, DomainError> {
            Ok(None)
        }

        async fn list_files(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
        ) -> Result<Vec<InlineFile>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_files_by_owner(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
        ) -> Result<Vec<InlineFile>, DomainError> {
            Ok(Vec::new())
        }

        async fn upsert_file(&self, _file: &InlineFile) -> Result<(), DomainError> {
            Ok(())
        }

        async fn upsert_files(&self, _files: &[InlineFile]) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_file(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
            _path: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_by_container(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete_by_owner(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn count_files(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: Uuid,
            _container_id: &str,
        ) -> Result<i64, DomainError> {
            Ok(0)
        }
    }

    #[derive(Default)]
    struct EmptySkillRepo;

    #[async_trait::async_trait]
    impl SkillAssetRepository for EmptySkillRepo {
        async fn create(&self, _asset: &SkillAsset) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, _id: Uuid) -> Result<Option<SkillAsset>, DomainError> {
            Ok(None)
        }

        async fn get_by_project_and_key(
            &self,
            _project_id: Uuid,
            _key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            Ok(None)
        }

        async fn get_by_project_and_builtin_key(
            &self,
            _project_id: Uuid,
            _builtin_key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            Ok(None)
        }

        async fn list_by_project(&self, _project_id: Uuid) -> Result<Vec<SkillAsset>, DomainError> {
            Ok(Vec::new())
        }

        async fn update(&self, _asset: &SkillAsset) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn provider_for_run(run: LifecycleRun) -> (LifecycleMountProvider, Mount) {
        let mount = build_lifecycle_run_mount(run.id);
        let provider = LifecycleMountProvider::new(
            Arc::new(RunRepo::with_run(run)),
            Arc::new(EmptyInlineRepo),
            Arc::new(EmptySkillRepo),
            Arc::new(MemorySessionPersistence::default()),
        );
        (provider, mount)
    }

    fn list_options(path: &str) -> ListOptions {
        ListOptions {
            path: path.to_string(),
            pattern: None,
            recursive: false,
        }
    }

    fn entry_paths(result: ListResult) -> Vec<String> {
        result.entries.into_iter().map(|entry| entry.path).collect()
    }

    fn runtime_node(node_path: &str) -> RuntimeNodeState {
        RuntimeNodeState {
            node_id: "plan".to_string(),
            node_path: node_path.to_string(),
            kind: PlanNodeKind::AgentCall,
            status: RuntimeNodeStatus::Running,
            attempt: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            executor_run_ref: Some(ExecutorRunRef::RuntimeSession {
                session_id: "session-1".to_string(),
            }),
            children: Vec::new(),
            phase_path: Vec::new(),
            started_at: None,
            completed_at: None,
            error: None,
            trace_refs: Vec::new(),
            cache: None,
        }
    }

    fn run_with_orchestration() -> (LifecycleRun, Uuid, String) {
        let project_id = Uuid::new_v4();
        let source_ref = OrchestrationSourceRef::Inline {
            source_digest: "test".to_string(),
        };
        let plan_snapshot = OrchestrationPlanSnapshot {
            plan_digest: "digest".to_string(),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![PlanNode {
                node_id: "plan".to_string(),
                node_path: "phase/plan".to_string(),
                parent_node_id: None,
                kind: PlanNodeKind::AgentCall,
                label: None,
                executor: None,
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec!["plan".to_string()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: Default::default(),
            metadata: None,
            created_at: Utc::now(),
        };
        let mut orchestration = OrchestrationInstance::new("root", source_ref, plan_snapshot);
        orchestration.status = OrchestrationStatus::Running;
        orchestration.node_tree = vec![runtime_node("phase/plan")];
        let orchestration_id = orchestration.orchestration_id;
        let mut run = LifecycleRun::new_control(project_id);
        assert!(run.add_orchestration(orchestration));
        (run, orchestration_id, "phase%2Fplan".to_string())
    }

    #[tokio::test]
    async fn run_scoped_mount_lists_graphless_run_surface() {
        let run = LifecycleRun::new_graphless(Uuid::new_v4());
        let (provider, mount) = provider_for_run(run);

        let paths = entry_paths(
            provider
                .list(&mount, &list_options(""), &MountOperationContext::default())
                .await
                .expect("list root"),
        );

        assert!(paths.contains(&"state".to_string()));
        assert!(paths.contains(&"context".to_string()));
        assert!(paths.contains(&"orchestrations".to_string()));
        assert!(paths.contains(&"runs".to_string()));
        assert!(!paths.contains(&"active".to_string()));
        assert!(!paths.contains(&"nodes".to_string()));
    }

    #[tokio::test]
    async fn run_scoped_mount_lists_orchestration_nodes_with_stable_encoded_paths() {
        let (run, orchestration_id, encoded_node) = run_with_orchestration();
        let (provider, mount) = provider_for_run(run);

        let root_paths = entry_paths(
            provider
                .list(
                    &mount,
                    &list_options("orchestrations"),
                    &MountOperationContext::default(),
                )
                .await
                .expect("list orchestrations"),
        );
        assert!(root_paths.contains(&format!("orchestrations/{orchestration_id}")));

        let node_paths = entry_paths(
            provider
                .list(
                    &mount,
                    &list_options(&format!("orchestrations/{orchestration_id}/nodes")),
                    &MountOperationContext::default(),
                )
                .await
                .expect("list nodes"),
        );
        assert!(node_paths.contains(&format!(
            "orchestrations/{orchestration_id}/nodes/{encoded_node}"
        )));

        let read = provider
            .read_text(
                &mount,
                &format!("orchestrations/{orchestration_id}/nodes/{encoded_node}/state"),
                &MountOperationContext::default(),
            )
            .await
            .expect("read node state");
        assert!(read.content.contains("\"node_path\": \"phase/plan\""));
    }

    #[tokio::test]
    async fn run_scoped_mount_rejects_direct_writes() {
        let (run, orchestration_id, encoded_node) = run_with_orchestration();
        let (provider, mount) = provider_for_run(run);

        let error = provider
            .write_text(
                &mount,
                &format!("orchestrations/{orchestration_id}/nodes/{encoded_node}/records/note.md"),
                "note",
                &MountOperationContext::default(),
            )
            .await
            .expect_err("run mount is read-only");

        assert!(matches!(error, MountError::NotSupported(_)));
    }
}
