//! `lifecycle_vfs` mount: expose AgentRun session evidence and runtime node projections.

use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::{
    ExecutorRunRef, LifecycleRun, LifecycleRunRepository, OrchestrationInstance, RuntimeNodeState,
};
use async_trait::async_trait;
use serde::Serialize;
use tracing::info;
use uuid::Uuid;

use crate::lifecycle::execution_log::{RuntimeNodeArtifactScope, encode_node_path_segment};
use crate::lifecycle::surface::journey::{
    LifecycleJourneyError, LifecycleJourneyProjection, SessionItemView, filter_session_items,
    group_events_into_turn_summaries, item_file_name, session_summary_archives, to_json_pretty,
    tool_result_metadata_for_projection,
};
use crate::runtime::{Mount, RuntimeFileEntry};
use crate::session::{SessionPersistence, SessionToolResultCache};
use crate::vfs::lifecycle_catalog::lifecycle_root_entries;
use crate::vfs::mount::PROVIDER_LIFECYCLE_VFS;
use crate::vfs::mount_inline::list_inline_entries;
use crate::vfs::path::normalize_mount_relative_path;
use crate::vfs::provider::{
    MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery, SearchResult,
};
use crate::vfs::provider_skill_asset::{
    list_projected_skill_files, parse_skill_asset_mount_metadata, read_projected_skill_file,
    search_projected_skill_files,
};
use crate::vfs::types::{ListOptions, ListResult, ReadResult};

pub struct LifecycleMountProvider {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    skill_asset_repo: Arc<dyn SkillAssetRepository>,
    journey: LifecycleJourneyProjection,
}

impl LifecycleMountProvider {
    #[cfg(test)]
    pub fn new(
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        Self::new_with_tool_result_cache(
            lifecycle_run_repo,
            inline_file_repo,
            skill_asset_repo,
            session_persistence,
            SessionToolResultCache::new(),
        )
    }

    pub fn new_with_tool_result_cache(
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_persistence: Arc<dyn SessionPersistence>,
        tool_result_cache: Arc<SessionToolResultCache>,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            skill_asset_repo,
            journey: LifecycleJourneyProjection::new_with_tool_result_cache(
                inline_file_repo,
                session_persistence,
                tool_result_cache,
            ),
        }
    }

    async fn read_agent_run_session_scope(
        &self,
        mount: &Mount,
        segs: &[&str],
    ) -> Result<String, MountError> {
        let run_ctx = load_run_context(&self.lifecycle_run_repo, mount).await?;
        let session_id = parse_runtime_session_id_from_metadata(mount)?;
        let content = match segs {
            [] | ["state"] => to_json_pretty(&agent_run_session_overview(&run_ctx.run, mount)?)
                .map_err(map_journey_err)?,
            ["execution-log"] => {
                to_json_pretty(&run_ctx.run.execution_log).map_err(map_journey_err)?
            }
            ["session"] => self
                .journey
                .read_session_projection(&session_id, &["meta"])
                .await
                .map_err(map_journey_err)?,
            ["session", rest @ ..] => self
                .journey
                .read_session_projection(&session_id, rest)
                .await
                .map_err(map_journey_err)?,
            ["node"] | ["node", "state"] => {
                let active = load_agent_run_node_context(&self.lifecycle_run_repo, mount).await?;
                to_json_pretty(current_node(&active)?).map_err(map_journey_err)?
            }
            ["node", "artifacts"] => {
                let scope = runtime_scope_from_mount(mount)?;
                let map = self
                    .journey
                    .list_scoped_port_outputs(&scope)
                    .await
                    .map_err(map_journey_err)?;
                to_json_pretty(&map).map_err(map_journey_err)?
            }
            ["node", "artifacts", port_key] => {
                let artifact_ref = runtime_scope_from_mount(mount)?.port_ref(*port_key);
                self.journey
                    .read_scoped_port_output(&artifact_ref)
                    .await
                    .map_err(map_journey_err)?
            }
            ["node", "records"] => {
                let active = load_agent_run_node_context(&self.lifecycle_run_repo, mount).await?;
                self.journey
                    .read_records_map(active.run.id, &records_prefix(&active.node_path))
                    .await
                    .map_err(map_journey_err)?
            }
            ["node", "records", rest @ ..] => {
                let active = load_agent_run_node_context(&self.lifecycle_run_repo, mount).await?;
                self.journey
                    .read_record(active.run.id, &records_prefix(&active.node_path), rest)
                    .await
                    .map_err(map_journey_err)?
            }
            ["orchestration"] | ["orchestration", "state"] => {
                let active = load_agent_run_node_context(&self.lifecycle_run_repo, mount).await?;
                to_json_pretty(&active.orchestration).map_err(map_journey_err)?
            }
            _ => {
                return Err(MountError::NotFound(format!(
                    "agent_run_session lifecycle_vfs 不支持的路径: `{}`",
                    segs.join("/")
                )));
            }
        };
        Ok(content)
    }

    async fn list_agent_run_session_scope(
        &self,
        mount: &Mount,
        path_norm: &str,
        options: &ListOptions,
        segs: &[&str],
    ) -> Result<Vec<RuntimeFileEntry>, MountError> {
        let session_id = parse_runtime_session_id_from_metadata(mount)?;
        let entries = match segs {
            [] => agent_run_session_root_entries(lifecycle_mount_has_skills(mount), mount),
            ["session"] => {
                if options.recursive {
                    list_session_recursive_entries(&self.journey, &session_id, "session").await?
                } else {
                    session_root_entries("session")
                }
            }
            ["session", "items"] => {
                list_session_item_entries(
                    &self.journey,
                    &session_id,
                    "session/items",
                    SessionItemView::Items,
                )
                .await?
            }
            ["session", "messages"] => {
                list_session_item_entries(
                    &self.journey,
                    &session_id,
                    "session/messages",
                    SessionItemView::Messages,
                )
                .await?
            }
            ["session", "tools"] => {
                list_session_item_entries(
                    &self.journey,
                    &session_id,
                    "session/tools",
                    SessionItemView::Tools,
                )
                .await?
            }
            ["session", "tool-results"] => {
                list_session_tool_result_entries(
                    &self.journey,
                    &session_id,
                    "session/tool-results",
                    ToolResultListScope::Root,
                    options.recursive,
                )
                .await?
            }
            ["session", "tool-results", turn_alias] => {
                list_session_tool_result_entries(
                    &self.journey,
                    &session_id,
                    "session/tool-results",
                    ToolResultListScope::Turn { turn_alias },
                    options.recursive,
                )
                .await?
            }
            ["session", "tool-results", turn_alias, body_alias] => {
                list_session_tool_result_entries(
                    &self.journey,
                    &session_id,
                    "session/tool-results",
                    ToolResultListScope::Body {
                        turn_alias,
                        body_alias,
                    },
                    options.recursive,
                )
                .await?
            }
            ["session", "writes"] => {
                list_session_item_entries(
                    &self.journey,
                    &session_id,
                    "session/writes",
                    SessionItemView::Writes,
                )
                .await?
            }
            ["session", "summaries"] => {
                list_session_summary_entries(&self.journey, &session_id, "session/summaries")
                    .await?
            }
            ["session", "terminal"] => {
                list_session_terminal_entries(
                    &self.journey,
                    &session_id,
                    "session/terminal",
                    options.recursive,
                )
                .await?
            }
            ["session", "turns"] => {
                list_session_turn_entries(
                    &self.journey,
                    &session_id,
                    "session/turns",
                    options.recursive,
                )
                .await?
            }
            ["session", "turns", turn_id] => {
                list_session_turn_entries_for_turn(
                    &self.journey,
                    &session_id,
                    "session/turns",
                    turn_id,
                )
                .await?
            }
            ["node"] => {
                require_node_scope(mount)?;
                node_log_entries("node")
            }
            ["node", "artifacts"] => self
                .journey
                .list_scoped_port_outputs(&runtime_scope_from_mount(mount)?)
                .await
                .map_err(map_journey_err)?
                .into_keys()
                .map(|key| RuntimeFileEntry::file(format!("node/artifacts/{key}")).as_virtual())
                .collect(),
            ["node", "records"] => {
                let active = load_agent_run_node_context(&self.lifecycle_run_repo, mount).await?;
                let map = self
                    .journey
                    .records_map(active.run.id, &records_prefix(&active.node_path))
                    .await
                    .map_err(map_journey_err)?;
                list_inline_entries(&map, "", options.pattern.as_deref(), options.recursive)
                    .into_iter()
                    .map(|mut entry| {
                        entry.path = format!("node/records/{}", entry.path);
                        entry
                    })
                    .collect()
            }
            ["orchestration"] => {
                require_node_scope(mount)?;
                vec![RuntimeFileEntry::file("orchestration/state").as_virtual()]
            }
            _ => Vec::new(),
        };

        let mut entries = entries;
        retain_entries_matching_pattern(&mut entries, options.pattern.as_deref());
        if !path_norm.is_empty() && options.recursive {
            entries.retain(|entry| entry.path.starts_with(path_norm));
        }
        Ok(entries)
    }
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

#[derive(Debug, Serialize)]
struct AgentRunLifecycleSessionOverview {
    run_id: Uuid,
    agent_id: Uuid,
    runtime_session_id: String,
    launch_frame_id: Uuid,
    orchestration_id: Option<Uuid>,
    node_path: Option<String>,
    attempt: Option<u32>,
    run_status: agentdash_domain::workflow::LifecycleRunStatus,
    execution_log_count: usize,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_activity_at: chrono::DateTime<chrono::Utc>,
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

fn parse_string_metadata(mount: &Mount, key: &str) -> Result<String, MountError> {
    mount
        .metadata
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| MountError::OperationFailed(format!("mount metadata 缺少 {key}")))
}

fn parse_run_id_from_metadata(mount: &Mount) -> Result<Uuid, MountError> {
    parse_uuid_metadata(mount, "run_id")
}

fn parse_agent_id_from_metadata(mount: &Mount) -> Result<Uuid, MountError> {
    parse_uuid_metadata(mount, "agent_id")
}

fn parse_launch_frame_id_from_metadata(mount: &Mount) -> Result<Uuid, MountError> {
    parse_uuid_metadata(mount, "launch_frame_id")
}

fn parse_runtime_session_id_from_metadata(mount: &Mount) -> Result<String, MountError> {
    parse_string_metadata(mount, "runtime_session_id")
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

fn mount_scope(mount: &Mount) -> Option<&str> {
    mount.metadata.get("scope").and_then(|value| value.as_str())
}

fn mount_is_agent_run_session_scope(mount: &Mount) -> bool {
    mount_scope(mount) == Some("agent_run_session")
}

fn mount_is_node_runtime_scope(mount: &Mount) -> bool {
    mount_scope(mount) == Some("node_runtime")
}

fn require_node_scope(mount: &Mount) -> Result<(), MountError> {
    if mount_has_node_scope(mount) {
        Ok(())
    } else {
        Err(MountError::NotFound(
            "当前 lifecycle_vfs mount 没有 orchestration node anchor".to_string(),
        ))
    }
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

async fn load_agent_run_node_context(
    run_repo: &Arc<dyn LifecycleRunRepository>,
    mount: &Mount,
) -> Result<LifecycleMountContext, MountError> {
    require_node_scope(mount)?;
    load_active_context(run_repo, mount).await
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

fn agent_run_session_overview(
    run: &LifecycleRun,
    mount: &Mount,
) -> Result<AgentRunLifecycleSessionOverview, MountError> {
    Ok(AgentRunLifecycleSessionOverview {
        run_id: run.id,
        agent_id: parse_agent_id_from_metadata(mount)?,
        runtime_session_id: parse_runtime_session_id_from_metadata(mount)?,
        launch_frame_id: parse_launch_frame_id_from_metadata(mount)?,
        orchestration_id: mount_has_node_scope(mount)
            .then(|| parse_orchestration_id_from_metadata(mount))
            .transpose()?,
        node_path: mount_has_node_scope(mount)
            .then(|| parse_node_path_from_metadata(mount))
            .transpose()?,
        attempt: mount_has_node_scope(mount)
            .then(|| parse_attempt_from_metadata(mount))
            .transpose()?,
        run_status: run.status,
        execution_log_count: run.execution_log.len(),
        created_at: run.created_at,
        updated_at: run.updated_at,
        last_activity_at: run.last_activity_at,
    })
}

fn records_prefix(node_path: &str) -> String {
    encode_node_path_segment(node_path)
}

fn agent_run_session_root_entries(include_skills: bool, mount: &Mount) -> Vec<RuntimeFileEntry> {
    let mut entries = vec![
        RuntimeFileEntry::file("state").as_virtual(),
        RuntimeFileEntry::dir("session").as_virtual(),
        RuntimeFileEntry::file("execution-log").as_virtual(),
    ];
    if mount_has_node_scope(mount) {
        entries.push(RuntimeFileEntry::dir("node").as_virtual());
        entries.push(RuntimeFileEntry::dir("orchestration").as_virtual());
    }
    if include_skills {
        entries.push(RuntimeFileEntry::dir("skills").as_virtual());
    }
    entries
}

fn node_log_entries(prefix: &str) -> Vec<RuntimeFileEntry> {
    vec![
        RuntimeFileEntry::file(format!("{prefix}/state")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/artifacts")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/records")).as_virtual(),
    ]
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
        RuntimeFileEntry::dir(format!("{prefix}/tool-results")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/writes")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/summaries")).as_virtual(),
        RuntimeFileEntry::dir(format!("{prefix}/terminal")).as_virtual(),
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

        if mount_is_agent_run_session_scope(mount) {
            let content = self.read_agent_run_session_scope(mount, &segs).await?;
            return Ok(ReadResult::new(path_norm, content));
        }

        if !mount_is_node_runtime_scope(mount) || !mount_has_node_scope(mount) {
            return Err(MountError::NotSupported(
                "lifecycle_vfs 只支持 agent_run_session 只读资源面或 node_runtime 执行期 mount"
                    .to_string(),
            ));
        }
        let content = match segs.as_slice() {
            [] | ["state"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                to_json_pretty(current_node(&active)?).map_err(map_journey_err)?
            }
            ["session"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_id = session_id_for_node(current_node(&active)?)?;
                self.journey
                    .read_session_projection(&session_id, &["meta"])
                    .await
                    .map_err(map_journey_err)?
            }
            ["session", rest @ ..] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_id = session_id_for_node(current_node(&active)?)?;
                self.journey
                    .read_session_projection(&session_id, rest)
                    .await
                    .map_err(map_journey_err)?
            }
            ["artifacts"] => {
                let scope = runtime_scope_from_mount(mount)?;
                let map = self
                    .journey
                    .list_scoped_port_outputs(&scope)
                    .await
                    .map_err(map_journey_err)?;
                to_json_pretty(&map).map_err(map_journey_err)?
            }
            ["artifacts", port_key] => {
                let artifact_ref = runtime_scope_from_mount(mount)?.port_ref(*port_key);
                self.journey
                    .read_scoped_port_output(&artifact_ref)
                    .await
                    .map_err(map_journey_err)?
            }
            ["records"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                self.journey
                    .read_records_map(active.run.id, &records_prefix(&active.node_path))
                    .await
                    .map_err(map_journey_err)?
            }
            ["records", rest @ ..] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                self.journey
                    .read_record(active.run.id, &records_prefix(&active.node_path), rest)
                    .await
                    .map_err(map_journey_err)?
            }
            _ => {
                return Err(MountError::NotFound(format!(
                    "lifecycle_vfs 不支持的路径: `{path_norm}`"
                )));
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
        if mount_is_agent_run_session_scope(mount) {
            return Err(MountError::NotSupported(
                "agent_run_session lifecycle_vfs 是只读执行证据面".to_string(),
            ));
        }
        if !mount_is_node_runtime_scope(mount) || !mount_has_node_scope(mount) {
            return Err(MountError::NotSupported(
                "lifecycle_vfs 写入只支持 node_runtime 执行期 mount".to_string(),
            ));
        }

        match segs.as_slice() {
            ["artifacts", port_key] => {
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
        if mount_is_agent_run_session_scope(mount) {
            return Ok(ListResult {
                entries: self
                    .list_agent_run_session_scope(mount, &path_norm, options, &segs)
                    .await?,
            });
        }
        if !mount_is_node_runtime_scope(mount) || !mount_has_node_scope(mount) {
            return Err(MountError::NotSupported(
                "lifecycle_vfs 只支持 agent_run_session 只读资源面或 node_runtime 执行期 mount"
                    .to_string(),
            ));
        }
        let mut entries = match segs.as_slice() {
            [] => lifecycle_root_entries(lifecycle_mount_has_skills(mount)),
            ["artifacts"] => self
                .journey
                .list_scoped_port_outputs(&runtime_scope_from_mount(mount)?)
                .await
                .map_err(map_journey_err)?
                .into_keys()
                .map(|key| RuntimeFileEntry::file(format!("{path_norm}/{key}")).as_virtual())
                .collect(),
            ["records"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
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
            ["session"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_id = session_id_for_node(current_node(&active)?)?;
                if options.recursive {
                    list_session_recursive_entries(&self.journey, &session_id, "session").await?
                } else {
                    session_root_entries("session")
                }
            }
            ["session", "items"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/items",
                    SessionItemView::Items,
                )
                .await?
            }
            ["session", "messages"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/messages",
                    SessionItemView::Messages,
                )
                .await?
            }
            ["session", "tools"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/tools",
                    SessionItemView::Tools,
                )
                .await?
            }
            ["session", "tool-results"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_tool_result_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/tool-results",
                    ToolResultListScope::Root,
                    options.recursive,
                )
                .await?
            }
            ["session", "tool-results", turn_alias] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_tool_result_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/tool-results",
                    ToolResultListScope::Turn { turn_alias },
                    options.recursive,
                )
                .await?
            }
            ["session", "tool-results", turn_alias, body_alias] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_tool_result_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/tool-results",
                    ToolResultListScope::Body {
                        turn_alias,
                        body_alias,
                    },
                    options.recursive,
                )
                .await?
            }
            ["session", "writes"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_item_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/writes",
                    SessionItemView::Writes,
                )
                .await?
            }
            ["session", "summaries"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_summary_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/summaries",
                )
                .await?
            }
            ["session", "terminal"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_terminal_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/terminal",
                    options.recursive,
                )
                .await?
            }
            ["session", "turns"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_turn_entries(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/turns",
                    options.recursive,
                )
                .await?
            }
            ["session", "turns", turn_id] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                list_session_turn_entries_for_turn(
                    &self.journey,
                    &session_id_for_node(current_node(&active)?)?,
                    "session/turns",
                    turn_id,
                )
                .await?
            }
            _ => Vec::new(),
        };
        retain_entries_matching_pattern(&mut entries, options.pattern.as_deref());
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
        for entry in listing
            .entries
            .into_iter()
            .filter(|entry| !entry.is_dir && !is_large_lifecycle_body_path(&entry.path))
        {
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

fn is_large_lifecycle_body_path(path: &str) -> bool {
    (path.starts_with("session/tool-results/") && path.ends_with("/result.txt"))
        || (path.starts_with("session/terminal/") && path.ends_with(".log"))
}

fn retain_entries_matching_pattern(entries: &mut Vec<RuntimeFileEntry>, pattern: Option<&str>) {
    let Some(pattern) = pattern.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    entries.retain(|entry| lifecycle_path_matches_pattern(&entry.path, pattern));
}

fn lifecycle_path_matches_pattern(path: &str, pattern: &str) -> bool {
    if pattern.contains('*')
        || pattern.contains('?')
        || pattern.contains('[')
        || pattern.contains('{')
    {
        globset::Glob::new(pattern)
            .ok()
            .map(|glob| glob.compile_matcher().is_match(path))
            .unwrap_or(false)
    } else {
        path.contains(pattern)
    }
}

async fn list_session_recursive_entries(
    journey: &LifecycleJourneyProjection,
    session_id: &str,
    display_root: &str,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let mut entries = session_root_entries(display_root);
    entries.extend(
        list_session_item_entries(
            journey,
            session_id,
            &format!("{display_root}/items"),
            SessionItemView::Items,
        )
        .await?,
    );
    entries.extend(
        list_session_item_entries(
            journey,
            session_id,
            &format!("{display_root}/messages"),
            SessionItemView::Messages,
        )
        .await?,
    );
    entries.extend(
        list_session_item_entries(
            journey,
            session_id,
            &format!("{display_root}/tools"),
            SessionItemView::Tools,
        )
        .await?,
    );
    entries.extend(
        list_session_tool_result_entries(
            journey,
            session_id,
            &format!("{display_root}/tool-results"),
            ToolResultListScope::Root,
            true,
        )
        .await?,
    );
    entries.extend(
        list_session_item_entries(
            journey,
            session_id,
            &format!("{display_root}/writes"),
            SessionItemView::Writes,
        )
        .await?,
    );
    entries.extend(
        list_session_summary_entries(journey, session_id, &format!("{display_root}/summaries"))
            .await?,
    );
    entries.extend(
        list_session_terminal_entries(
            journey,
            session_id,
            &format!("{display_root}/terminal"),
            true,
        )
        .await?,
    );
    entries.extend(
        list_session_turn_entries(journey, session_id, &format!("{display_root}/turns"), true)
            .await?,
    );
    Ok(entries)
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

#[derive(Debug, Clone, Copy)]
enum ToolResultListScope<'a> {
    Root,
    Turn {
        turn_alias: &'a str,
    },
    Body {
        turn_alias: &'a str,
        body_alias: &'a str,
    },
}

async fn list_session_tool_result_entries(
    journey: &LifecycleJourneyProjection,
    session_id: &str,
    display_root: &str,
    scope: ToolResultListScope<'_>,
    recursive: bool,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let projections = journey
        .session_item_projections(session_id)
        .await
        .map_err(map_journey_err)?;
    let mut metadata = filter_session_items(&projections, SessionItemView::Tools)
        .iter()
        .filter_map(|projection| tool_result_metadata_for_projection(session_id, projection))
        .collect::<Vec<_>>();
    metadata.sort_by(|left, right| left.item_id.cmp(&right.item_id));

    match scope {
        ToolResultListScope::Root => {
            if recursive {
                return Ok(tool_result_recursive_entries(display_root, &metadata));
            }
            let turn_aliases = metadata
                .iter()
                .map(|entry| entry.turn_alias.as_str())
                .collect::<BTreeSet<_>>();
            Ok(turn_aliases
                .into_iter()
                .map(|turn_alias| {
                    RuntimeFileEntry::dir(format!("{display_root}/{turn_alias}")).as_virtual()
                })
                .collect())
        }
        ToolResultListScope::Turn { turn_alias } => {
            let scoped = metadata
                .iter()
                .filter(|entry| entry.turn_alias == turn_alias)
                .collect::<Vec<_>>();
            if recursive {
                return Ok(tool_result_recursive_entries_for_refs(display_root, scoped));
            }
            let body_aliases = scoped
                .iter()
                .map(|entry| entry.body_alias.as_str())
                .collect::<BTreeSet<_>>();
            Ok(body_aliases
                .into_iter()
                .map(|body_alias| {
                    RuntimeFileEntry::dir(format!("{display_root}/{turn_alias}/{body_alias}"))
                        .as_virtual()
                })
                .collect())
        }
        ToolResultListScope::Body {
            turn_alias,
            body_alias,
        } => {
            let exists = metadata
                .iter()
                .any(|entry| entry.turn_alias == turn_alias && entry.body_alias == body_alias);
            if !exists {
                return Ok(Vec::new());
            }
            Ok(vec![
                RuntimeFileEntry::file(format!(
                    "{display_root}/{turn_alias}/{body_alias}/metadata.json"
                ))
                .as_virtual(),
                RuntimeFileEntry::file(format!(
                    "{display_root}/{turn_alias}/{body_alias}/result.txt"
                ))
                .as_virtual(),
            ])
        }
    }
}

fn tool_result_recursive_entries(
    display_root: &str,
    metadata: &[crate::lifecycle::surface::journey::SessionToolResultMetadata],
) -> Vec<RuntimeFileEntry> {
    tool_result_recursive_entries_for_refs(display_root, metadata.iter().collect())
}

fn tool_result_recursive_entries_for_refs(
    display_root: &str,
    metadata: Vec<&crate::lifecycle::surface::journey::SessionToolResultMetadata>,
) -> Vec<RuntimeFileEntry> {
    let mut dirs = BTreeSet::new();
    let mut entries = Vec::new();
    for entry in metadata {
        dirs.insert(format!("{display_root}/{}", entry.turn_alias));
        dirs.insert(format!(
            "{display_root}/{}/{}",
            entry.turn_alias, entry.body_alias
        ));
        entries.push(RuntimeFileEntry::file(entry.metadata_path.clone()).as_virtual());
        entries.push(RuntimeFileEntry::file(entry.result_path.clone()).as_virtual());
    }
    dirs.into_iter()
        .map(|path| RuntimeFileEntry::dir(path).as_virtual())
        .chain(entries)
        .collect()
}

async fn list_session_terminal_entries(
    journey: &LifecycleJourneyProjection,
    session_id: &str,
    display_root: &str,
    _recursive: bool,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let mut metadata = journey
        .terminal_metadata_entries(session_id)
        .await
        .map_err(map_journey_err)?;
    metadata.sort_by(|left, right| left.terminal_id.cmp(&right.terminal_id));
    Ok(metadata
        .into_iter()
        .flat_map(|entry| {
            let metadata_entry = RuntimeFileEntry::file(format!(
                "{display_root}/{}.metadata.json",
                entry.terminal_id
            ))
            .as_virtual();
            let log_entry =
                RuntimeFileEntry::file(format!("{display_root}/{}.log", entry.terminal_id))
                    .as_virtual();
            vec![metadata_entry, log_entry]
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

async fn list_session_turn_entries(
    journey: &LifecycleJourneyProjection,
    session_id: &str,
    display_root: &str,
    recursive: bool,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let events = journey
        .session_events(session_id)
        .await
        .map_err(map_journey_err)?;
    Ok(group_events_into_turn_summaries(&events)
        .into_iter()
        .flat_map(|summary| {
            if recursive {
                vec![
                    RuntimeFileEntry::dir(format!("{display_root}/{}", summary.turn_id))
                        .as_virtual(),
                    RuntimeFileEntry::file(format!(
                        "{display_root}/{}/events.json",
                        summary.turn_id
                    ))
                    .as_virtual(),
                ]
            } else {
                vec![
                    RuntimeFileEntry::dir(format!("{display_root}/{}", summary.turn_id))
                        .as_virtual(),
                ]
            }
        })
        .collect())
}

async fn list_session_turn_entries_for_turn(
    journey: &LifecycleJourneyProjection,
    session_id: &str,
    display_root: &str,
    turn_id: &str,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let events = journey
        .session_events(session_id)
        .await
        .map_err(map_journey_err)?;
    if events
        .iter()
        .any(|event| event.turn_id.as_deref() == Some(turn_id))
    {
        Ok(vec![
            RuntimeFileEntry::file(format!("{display_root}/{turn_id}/events.json")).as_virtual(),
        ])
    } else {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_agent_protocol::backbone::item::ItemCompletedNotification;
    use agentdash_agent_protocol::{
        BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo,
        codex_app_server_protocol as codex,
    };
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
    use crate::session::{
        ExecutionStatus, SessionEventStore, SessionMeta, SessionMetaStore, TitleSource,
    };
    use crate::vfs::{
        build_agent_run_session_lifecycle_mount, build_lifecycle_mount_with_node_scope,
    };

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

    fn provider_for_agent_run_session(run: LifecycleRun) -> (LifecycleMountProvider, Mount) {
        provider_for_agent_run_session_with_persistence(
            run,
            Arc::new(MemorySessionPersistence::default()),
        )
    }

    fn provider_for_agent_run_session_with_persistence(
        run: LifecycleRun,
        persistence: Arc<MemorySessionPersistence>,
    ) -> (LifecycleMountProvider, Mount) {
        provider_for_agent_run_session_with_persistence_and_cache(
            run,
            persistence,
            SessionToolResultCache::new(),
        )
    }

    fn provider_for_agent_run_session_with_persistence_and_cache(
        run: LifecycleRun,
        persistence: Arc<MemorySessionPersistence>,
        tool_result_cache: Arc<SessionToolResultCache>,
    ) -> (LifecycleMountProvider, Mount) {
        let mount = build_agent_run_session_lifecycle_mount(
            run.id,
            Uuid::new_v4(),
            "session-1",
            Uuid::new_v4(),
            None,
            None,
            None,
        );
        let provider = LifecycleMountProvider::new_with_tool_result_cache(
            Arc::new(RunRepo::with_run(run)),
            Arc::new(EmptyInlineRepo),
            Arc::new(EmptySkillRepo),
            persistence,
            tool_result_cache,
        );
        (provider, mount)
    }

    fn provider_for_agent_run_node_session(
        run: LifecycleRun,
        orchestration_id: Uuid,
        node_path: &str,
    ) -> (LifecycleMountProvider, Mount) {
        let mount = build_agent_run_session_lifecycle_mount(
            run.id,
            Uuid::new_v4(),
            "session-1",
            Uuid::new_v4(),
            Some(orchestration_id),
            Some(node_path),
            Some(1),
        );
        let provider = LifecycleMountProvider::new(
            Arc::new(RunRepo::with_run(run)),
            Arc::new(EmptyInlineRepo),
            Arc::new(EmptySkillRepo),
            Arc::new(MemorySessionPersistence::default()),
        );
        (provider, mount)
    }

    fn provider_for_node_runtime(
        run: LifecycleRun,
        orchestration_id: Uuid,
        node_path: &str,
    ) -> (LifecycleMountProvider, Mount) {
        let mount = build_lifecycle_mount_with_node_scope(
            run.id,
            orchestration_id,
            node_path,
            "test-lifecycle",
            &["report".to_string()],
            Some(1),
        );
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

    fn glob_list_options(path: &str, pattern: &str) -> ListOptions {
        ListOptions {
            path: path.to_string(),
            pattern: Some(pattern.to_string()),
            recursive: pattern.contains("**"),
        }
    }

    fn entry_paths(result: ListResult) -> Vec<String> {
        result.entries.into_iter().map(|entry| entry.path).collect()
    }

    fn session_meta(session_id: &str) -> SessionMeta {
        SessionMeta {
            id: session_id.to_string(),
            title: "test session".to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_session_id: None,
        }
    }

    fn source_info() -> SourceInfo {
        SourceInfo {
            connector_id: "test".to_string(),
            connector_type: "pi_agent".to_string(),
            executor_id: None,
        }
    }

    fn tool_result_aliases(item_id: &str) -> (String, String) {
        item_id
            .split_once(':')
            .map(|(turn_alias, body_alias)| (turn_alias.to_string(), body_alias.to_string()))
            .unwrap_or_else(|| ("turn_unknown".to_string(), item_id.to_string()))
    }

    fn tool_result_turn_dir(item_id: &str) -> String {
        let (turn_alias, _) = tool_result_aliases(item_id);
        format!("session/tool-results/{turn_alias}")
    }

    fn tool_result_metadata_path(item_id: &str) -> String {
        let (turn_alias, body_alias) = tool_result_aliases(item_id);
        format!("session/tool-results/{turn_alias}/{body_alias}/metadata.json")
    }

    fn tool_result_body_path(item_id: &str) -> String {
        let (turn_alias, body_alias) = tool_result_aliases(item_id);
        format!("session/tool-results/{turn_alias}/{body_alias}/result.txt")
    }

    fn tool_result_lifecycle_path(item_id: &str) -> String {
        format!("lifecycle://{}", tool_result_body_path(item_id))
    }

    fn dynamic_tool_completed_envelope(session_id: &str, item_id: &str) -> BackboneEnvelope {
        let lifecycle_path = tool_result_lifecycle_path(item_id);
        let item = codex::ThreadItem::DynamicToolCall {
            id: item_id.to_string(),
            namespace: None,
            tool: "dynamic_tool".to_string(),
            arguments: serde_json::json!({ "query": "large output" }),
            status: codex::DynamicToolCallStatus::Completed,
            content_items: Some(vec![codex::DynamicToolCallOutputContentItem::InputText {
                text: format!(
                    "[tool result truncated]\nlifecycle_path: {lifecycle_path}\npolicy: head_tail\n\npreview body"
                ),
            }]),
            success: Some(true),
            duration_ms: None,
        };
        BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item,
                session_id.to_string(),
                "turn-1".to_string(),
            )),
            session_id,
            source_info(),
        )
        .with_turn_id("turn-1")
        .with_entry_index(0)
    }

    fn terminal_output_envelope(session_id: &str, terminal_id: &str) -> BackboneEnvelope {
        BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::TerminalOutput {
                terminal_id: terminal_id.to_string(),
                data: "terminal preview".to_string(),
            }),
            session_id,
            source_info(),
        )
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

    fn run_with_orchestration() -> (LifecycleRun, Uuid) {
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
        (run, orchestration_id)
    }

    #[tokio::test]
    async fn agent_run_session_mount_lists_plain_session_log_surface() {
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let (provider, mount) = provider_for_agent_run_session(run);

        let paths = entry_paths(
            provider
                .list(&mount, &list_options(""), &MountOperationContext::default())
                .await
                .expect("list root"),
        );

        assert!(paths.contains(&"state".to_string()));
        assert!(paths.contains(&"session".to_string()));
        assert!(paths.contains(&"execution-log".to_string()));
        assert!(!paths.contains(&"runs".to_string()));
        assert!(!paths.contains(&"orchestrations".to_string()));

        let session_paths = entry_paths(
            provider
                .list(
                    &mount,
                    &list_options("session"),
                    &MountOperationContext::default(),
                )
                .await
                .expect("list session"),
        );
        assert!(session_paths.contains(&"session/events.json".to_string()));
        assert!(session_paths.contains(&"session/messages".to_string()));
        assert!(session_paths.contains(&"session/tools".to_string()));

        let read = provider
            .read_text(&mount, "state", &MountOperationContext::default())
            .await
            .expect("read state");
        assert!(
            read.content
                .contains("\"runtime_session_id\": \"session-1\"")
        );
    }

    #[tokio::test]
    async fn agent_run_session_mount_glob_lists_session_tree() {
        let session_id = "session-1";
        let persistence = Arc::new(MemorySessionPersistence::default());
        persistence
            .create_session(&session_meta(session_id))
            .await
            .expect("create session");
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let (provider, mount) = provider_for_agent_run_session_with_persistence(run, persistence);

        let paths = entry_paths(
            provider
                .list(
                    &mount,
                    &glob_list_options("session", "**/*"),
                    &MountOperationContext::default(),
                )
                .await
                .expect("glob session recursively"),
        );

        assert!(paths.contains(&"session/events.json".to_string()));
        assert!(paths.contains(&"session/items".to_string()));
        assert!(!paths.is_empty());
    }

    #[tokio::test]
    async fn agent_run_session_mount_exposes_anchor_node_without_project_wide_orchestration() {
        let (run, orchestration_id) = run_with_orchestration();
        let (provider, mount) =
            provider_for_agent_run_node_session(run, orchestration_id, "phase/plan");

        let root_paths = entry_paths(
            provider
                .list(&mount, &list_options(""), &MountOperationContext::default())
                .await
                .expect("list root"),
        );
        assert!(root_paths.contains(&"node".to_string()));
        assert!(root_paths.contains(&"orchestration".to_string()));
        assert!(!root_paths.contains(&"runs".to_string()));

        let node_paths = entry_paths(
            provider
                .list(
                    &mount,
                    &list_options("node"),
                    &MountOperationContext::default(),
                )
                .await
                .expect("list node"),
        );
        assert!(node_paths.contains(&"node/state".to_string()));
        assert!(node_paths.contains(&"node/artifacts".to_string()));
        assert!(node_paths.contains(&"node/records".to_string()));

        let read = provider
            .read_text(&mount, "node/state", &MountOperationContext::default())
            .await
            .expect("read node state");
        assert!(read.content.contains("\"node_path\": \"phase/plan\""));
    }

    #[tokio::test]
    async fn agent_run_session_mount_rejects_direct_writes() {
        let (run, orchestration_id) = run_with_orchestration();
        let (provider, mount) =
            provider_for_agent_run_node_session(run, orchestration_id, "phase/plan");

        let error = provider
            .write_text(
                &mount,
                "node/records/note.md",
                "note",
                &MountOperationContext::default(),
            )
            .await
            .expect_err("agent run session mount is read-only");

        assert!(matches!(error, MountError::NotSupported(_)));
    }

    #[tokio::test]
    async fn agent_run_session_mount_exposes_tool_result_metadata_and_miss_body() {
        let session_id = "session-1";
        let item_id = "turn_001:tool_001";
        let persistence = Arc::new(MemorySessionPersistence::default());
        persistence
            .create_session(&session_meta(session_id))
            .await
            .expect("create session");
        persistence
            .append_event(
                session_id,
                &dynamic_tool_completed_envelope(session_id, item_id),
            )
            .await
            .expect("append tool event");

        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let (provider, mount) = provider_for_agent_run_session_with_persistence(run, persistence);

        let paths = entry_paths(
            provider
                .list(
                    &mount,
                    &list_options("session/tool-results"),
                    &MountOperationContext::default(),
                )
                .await
                .expect("list tool results"),
        );
        assert!(paths.contains(&tool_result_turn_dir(item_id)));

        let metadata = provider
            .read_text(
                &mount,
                &tool_result_metadata_path(item_id),
                &MountOperationContext::default(),
            )
            .await
            .expect("read tool result metadata");
        assert!(metadata.content.contains("\"body_status\""));
        assert!(metadata.content.contains("\"cache_miss\""));
        assert!(metadata.content.contains("preview body"));

        let result = provider
            .read_text(
                &mount,
                &tool_result_body_path(item_id),
                &MountOperationContext::default(),
            )
            .await
            .expect("read tool result miss body");
        assert!(result.content.contains("[tool result cache missing]"));
        assert!(!result.content.contains("preview body"));

        let search = provider
            .search_text(
                &mount,
                &SearchQuery {
                    pattern: "preview body".to_string(),
                    path: Some("session/tool-results".to_string()),
                    case_sensitive: true,
                    max_results: None,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("search tool result metadata");
        assert_eq!(search.matches.len(), 1);
        assert_eq!(search.matches[0].path, tool_result_metadata_path(item_id));
    }

    #[tokio::test]
    async fn agent_run_session_mount_reads_tool_result_cache_body() {
        let session_id = "session-1";
        let item_id = "turn_001:tool_001";
        let persistence = Arc::new(MemorySessionPersistence::default());
        persistence
            .create_session(&session_meta(session_id))
            .await
            .expect("create session");
        persistence
            .append_event(
                session_id,
                &dynamic_tool_completed_envelope(session_id, item_id),
            )
            .await
            .expect("append tool event");

        let cache = SessionToolResultCache::new();
        cache.put_text(
            session_id,
            item_id,
            "complete cached body\nAGENTDASH_CACHE_BODY_SENTINEL",
            47,
        );
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let (provider, mount) =
            provider_for_agent_run_session_with_persistence_and_cache(run, persistence, cache);

        let metadata = provider
            .read_text(
                &mount,
                &tool_result_metadata_path(item_id),
                &MountOperationContext::default(),
            )
            .await
            .expect("read tool result metadata");
        assert!(metadata.content.contains("\"status\": \"available\""));

        let result = provider
            .read_text(
                &mount,
                &tool_result_body_path(item_id),
                &MountOperationContext::default(),
            )
            .await
            .expect("read cached tool result body");
        assert!(result.content.contains("AGENTDASH_CACHE_BODY_SENTINEL"));
    }

    #[tokio::test]
    async fn agent_run_session_mount_returns_expired_tool_result_status() {
        let session_id = "session-1";
        let item_id = "turn_001:tool_001";
        let persistence = Arc::new(MemorySessionPersistence::default());
        persistence
            .create_session(&session_meta(session_id))
            .await
            .expect("create session");
        persistence
            .append_event(
                session_id,
                &dynamic_tool_completed_envelope(session_id, item_id),
            )
            .await
            .expect("append tool event");

        let cache = SessionToolResultCache::new();
        cache.put_text_with_ttl(
            session_id,
            item_id,
            "expired cache body AGENTDASH_EXPIRED_BODY_SENTINEL",
            48,
            Some(std::time::Duration::from_millis(0)),
        );
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let (provider, mount) =
            provider_for_agent_run_session_with_persistence_and_cache(run, persistence, cache);

        let metadata = provider
            .read_text(
                &mount,
                &tool_result_metadata_path(item_id),
                &MountOperationContext::default(),
            )
            .await
            .expect("read tool result metadata");
        assert!(metadata.content.contains("\"status\": \"expired\""));
        assert!(!metadata.content.contains("AGENTDASH_EXPIRED_BODY_SENTINEL"));

        let result = provider
            .read_text(
                &mount,
                &tool_result_body_path(item_id),
                &MountOperationContext::default(),
            )
            .await
            .expect("read expired tool result body");
        assert!(result.content.contains("[tool result cache expired]"));
        assert!(!result.content.contains("AGENTDASH_EXPIRED_BODY_SENTINEL"));
    }

    #[tokio::test]
    async fn agent_run_session_mount_exposes_terminal_metadata_and_miss_log() {
        let session_id = "session-1";
        let terminal_id = "terminal-1";
        let persistence = Arc::new(MemorySessionPersistence::default());
        persistence
            .create_session(&session_meta(session_id))
            .await
            .expect("create session");
        persistence
            .append_event(
                session_id,
                &terminal_output_envelope(session_id, terminal_id),
            )
            .await
            .expect("append terminal event");

        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let (provider, mount) = provider_for_agent_run_session_with_persistence(run, persistence);

        let paths = entry_paths(
            provider
                .list(
                    &mount,
                    &list_options("session/terminal"),
                    &MountOperationContext::default(),
                )
                .await
                .expect("list terminal"),
        );
        assert!(paths.contains(&"session/terminal/term_001.metadata.json".to_string()));
        assert!(paths.contains(&"session/terminal/term_001.log".to_string()));

        let metadata = provider
            .read_text(
                &mount,
                "session/terminal/term_001.metadata.json",
                &MountOperationContext::default(),
            )
            .await
            .expect("read terminal metadata");
        assert!(metadata.content.contains("\"terminal_id\": \"term_001\""));
        assert!(
            metadata
                .content
                .contains("\"raw_terminal_id\": \"terminal-1\"")
        );
        assert!(metadata.content.contains("\"cache_miss\""));

        let log = provider
            .read_text(
                &mount,
                "session/terminal/term_001.log",
                &MountOperationContext::default(),
            )
            .await
            .expect("read terminal miss log");
        assert!(log.content.contains("[terminal log cache missing]"));
        assert!(log.content.contains("terminal_id: term_001"));
        assert!(!log.content.contains(terminal_id));
        assert!(!log.content.contains("terminal preview"));
    }

    #[tokio::test]
    async fn node_runtime_mount_exposes_only_current_node_writable_surface() {
        let (run, orchestration_id) = run_with_orchestration();
        let (provider, mount) = provider_for_node_runtime(run, orchestration_id, "phase/plan");

        let paths = entry_paths(
            provider
                .list(&mount, &list_options(""), &MountOperationContext::default())
                .await
                .expect("list node runtime root"),
        );

        assert!(paths.contains(&"state".to_string()));
        assert!(paths.contains(&"session".to_string()));
        assert!(paths.contains(&"artifacts".to_string()));
        assert!(paths.contains(&"records".to_string()));
        assert!(!paths.contains(&"nodes".to_string()));
        assert!(!paths.contains(&"runs".to_string()));
        assert!(!paths.contains(&"orchestrations".to_string()));

        provider
            .write_text(
                &mount,
                "artifacts/report",
                "done",
                &MountOperationContext::default(),
            )
            .await
            .expect("node runtime artifact is writable");

        provider
            .write_text(
                &mount,
                "records/note.md",
                "note",
                &MountOperationContext::default(),
            )
            .await
            .expect("node runtime records are writable");

        let error = provider
            .write_text(
                &mount,
                "nodes/phase%2Fplan/records/note.md",
                "legacy",
                &MountOperationContext::default(),
            )
            .await
            .expect_err("node runtime does not expose cross-node writes");
        assert!(matches!(error, MountError::NotSupported(_)));
    }
}
