//! `lifecycle_vfs` mount: expose AgentRun session evidence and runtime node projections.

use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::{
    ExecutorRunRef, LifecycleRun, LifecycleRunRepository, OrchestrationInstance, RuntimeNodeState,
};
use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use crate::lifecycle::SessionToolResultCache;
use crate::lifecycle::execution_log::{RuntimeNodeArtifactScope, encode_node_path_segment};
use crate::lifecycle::surface::journey::{
    LifecycleJourneyError, LifecycleJourneyProjection, SessionItemView,
    SessionToolResultCacheReader, filter_session_items, group_events_into_turn_summaries,
    item_file_name, session_summary_archives, to_json_pretty, tool_result_metadata_for_projection,
};
use crate::lifecycle::vfs_catalog::lifecycle_root_entries;
use agentdash_application_vfs::mount::PROVIDER_LIFECYCLE_VFS;
use agentdash_application_vfs::mount_inline::list_inline_entries;
use agentdash_application_vfs::mount_skill_asset::{
    lifecycle_mount_has_skill_asset_projection, list_lifecycle_skill_asset_projection,
    read_lifecycle_skill_asset_projection, search_lifecycle_skill_asset_projection,
};
use agentdash_application_vfs::path::normalize_mount_relative_path;
use agentdash_application_vfs::provider::{
    MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery, SearchResult,
};
use agentdash_application_vfs::types::{ListOptions, ListResult, ReadResult};
use agentdash_domain::common::Mount;
use agentdash_spi::platform::mount::RuntimeFileEntry;
use agentdash_spi::{SessionCompactionStore, SessionEventStore, SessionMetaStore};

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
        session_meta_store: Arc<dyn SessionMetaStore>,
        session_event_store: Arc<dyn SessionEventStore>,
        session_compaction_store: Arc<dyn SessionCompactionStore>,
    ) -> Self {
        Self::new_with_tool_result_cache(
            lifecycle_run_repo,
            inline_file_repo,
            skill_asset_repo,
            session_meta_store,
            session_event_store,
            session_compaction_store,
            SessionToolResultCache::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_tool_result_cache(
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_meta_store: Arc<dyn SessionMetaStore>,
        session_event_store: Arc<dyn SessionEventStore>,
        session_compaction_store: Arc<dyn SessionCompactionStore>,
        tool_result_cache: Arc<dyn SessionToolResultCacheReader>,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            skill_asset_repo,
            journey: LifecycleJourneyProjection::new_with_tool_result_cache(
                inline_file_repo,
                session_meta_store,
                session_event_store,
                session_compaction_store,
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
            [] => agent_run_session_root_entries(
                lifecycle_mount_has_skill_asset_projection(mount),
                mount,
            ),
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
            return read_lifecycle_skill_asset_projection(
                self.skill_asset_repo.as_ref(),
                mount,
                &path_norm,
            )
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
                diag!(
                    Info,
                    Subsystem::Lifecycle,
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
            return list_lifecycle_skill_asset_projection(
                self.skill_asset_repo.as_ref(),
                mount,
                options,
            )
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
            [] => lifecycle_root_entries(lifecycle_mount_has_skill_asset_projection(mount)),
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
            return search_lifecycle_skill_asset_projection(
                self.skill_asset_repo.as_ref(),
                mount,
                query,
            )
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
    let entries = session_summary_archives(journey.session_compaction_store(), session_id)
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
