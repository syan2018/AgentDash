//! `lifecycle_vfs` mount: expose AgentRun session evidence and runtime node projections.

use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::{
    ExecutorRunRef, LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    OrchestrationInstance, RuntimeNodeState,
};
use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use crate::lifecycle::execution_log::{RuntimeNodeArtifactScope, encode_node_path_segment};
use crate::lifecycle::surface::journey::{
    AgentRunCompactionArchiveReader, AgentRunJournalReader, AgentRunJournalRef,
    LifecycleJourneyError, LifecycleJourneyProjection, SessionItemView, filter_session_items,
    group_events_into_turn_summaries, item_file_name, session_summary_archives, to_json_pretty,
    tool_result_metadata_for_projection,
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
use agentdash_platform_spi::platform::mount::RuntimeFileEntry;

pub struct LifecycleMountProvider {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    skill_asset_repo: Arc<dyn SkillAssetRepository>,
    journey: LifecycleJourneyProjection,
}

impl LifecycleMountProvider {
    pub fn new(
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        agent_run_journal_reader: Arc<dyn AgentRunJournalReader>,
        agent_run_compaction_archive_reader: Arc<dyn AgentRunCompactionArchiveReader>,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_agent_repo,
            skill_asset_repo,
            journey: LifecycleJourneyProjection::new(
                inline_file_repo,
                agent_run_journal_reader,
                agent_run_compaction_archive_reader,
            ),
        }
    }

    async fn read_agent_run_session_scope(
        &self,
        mount: &Mount,
        segs: &[&str],
    ) -> Result<String, MountError> {
        let run_ctx = load_run_context(&self.lifecycle_run_repo, mount).await?;
        let session_source = agent_run_session_event_source_from_mount(mount)?;
        let content = match segs {
            [] | ["state"] => to_json_pretty(&agent_run_session_overview(&run_ctx.run, mount)?)
                .map_err(map_journey_err)?,
            ["execution-log"] => {
                to_json_pretty(&run_ctx.run.execution_log).map_err(map_journey_err)?
            }
            ["session"] => self
                .journey
                .read_session_projection(&session_source, &["meta"])
                .await
                .map_err(map_journey_err)?,
            ["session", rest @ ..] => self
                .journey
                .read_session_projection(&session_source, rest)
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
        let session_source = agent_run_session_event_source_from_mount(mount)?;
        let entries = match segs {
            [] => agent_run_session_root_entries(
                lifecycle_mount_has_skill_asset_projection(mount),
                mount,
            ),
            ["session", rest @ ..] => {
                list_session_projection_entries(
                    &self.journey,
                    &session_source,
                    "session",
                    rest,
                    options.recursive,
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

    async fn read_agent_runs_scope(
        &self,
        mount: &Mount,
        segs: &[&str],
    ) -> Result<String, MountError> {
        let run_id = parse_run_id_from_metadata(mount)?;
        let content = match segs {
            ["agent-runs"] => {
                let agents = self.list_lifecycle_agents(run_id).await?;
                to_json_pretty(&serde_json::json!({
                    "run_id": run_id.to_string(),
                    "agents": agents
                        .iter()
                        .map(|agent| serde_json::json!({
                            "agent_id": agent.id.to_string(),
                            "path": format!("agent-runs/{}", agent.id),
                            "sessions_path": agent_run_sessions_path(agent.id),
                        }))
                        .collect::<Vec<_>>()
                }))
                .map_err(map_journey_err)?
            }
            ["agent-runs", agent_id] => {
                let source = self
                    .agent_run_journal_ref_for_agent(mount, agent_id)
                    .await?;
                to_json_pretty(&serde_json::json!({
                    "agent_id": source.agent_id.to_string(),
                    "sessions_path": agent_run_sessions_path(source.agent_id),
                }))
                .map_err(map_journey_err)?
            }
            ["agent-runs", agent_id, "sessions"] => {
                let source = self
                    .agent_run_journal_ref_for_agent(mount, agent_id)
                    .await?;
                self.journey
                    .read_session_projection(&source, &["meta"])
                    .await
                    .map_err(map_journey_err)?
            }
            ["agent-runs", agent_id, "sessions", rest @ ..] => {
                let source = self
                    .agent_run_journal_ref_for_agent(mount, agent_id)
                    .await?;
                self.journey
                    .read_session_projection(&source, rest)
                    .await
                    .map_err(map_journey_err)?
            }
            _ => {
                return Err(MountError::NotFound(format!(
                    "lifecycle_vfs 不支持的 AgentRun 路径: `{}`",
                    segs.join("/")
                )));
            }
        };
        Ok(content)
    }

    async fn list_agent_runs_scope(
        &self,
        mount: &Mount,
        path_norm: &str,
        options: &ListOptions,
        segs: &[&str],
    ) -> Result<Vec<RuntimeFileEntry>, MountError> {
        let run_id = parse_run_id_from_metadata(mount)?;
        let mut entries = match segs {
            ["agent-runs"] => self
                .list_lifecycle_agents(run_id)
                .await?
                .into_iter()
                .map(|agent| RuntimeFileEntry::dir(format!("agent-runs/{}", agent.id)).as_virtual())
                .collect(),
            ["agent-runs", agent_id] => {
                let source = self
                    .agent_run_journal_ref_for_agent(mount, agent_id)
                    .await?;
                vec![RuntimeFileEntry::dir(agent_run_sessions_path(source.agent_id)).as_virtual()]
            }
            ["agent-runs", agent_id, "sessions", rest @ ..] => {
                let source = self
                    .agent_run_journal_ref_for_agent(mount, agent_id)
                    .await?;
                list_session_projection_entries(
                    &self.journey,
                    &source,
                    &agent_run_sessions_path(source.agent_id),
                    rest,
                    options.recursive,
                )
                .await?
            }
            _ => Vec::new(),
        };

        retain_entries_matching_pattern(&mut entries, options.pattern.as_deref());
        if !path_norm.is_empty() && options.recursive {
            entries.retain(|entry| entry.path.starts_with(path_norm));
        }
        Ok(entries)
    }

    async fn list_lifecycle_agents(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<agentdash_domain::workflow::LifecycleAgent>, MountError> {
        let mut agents = self
            .lifecycle_agent_repo
            .list_by_run(run_id)
            .await
            .map_err(map_domain_err)?;
        agents.sort_by_key(|agent| agent.id);
        Ok(agents)
    }

    async fn agent_run_journal_ref_for_agent(
        &self,
        mount: &Mount,
        agent_id: &str,
    ) -> Result<AgentRunJournalRef, MountError> {
        let run_id = parse_run_id_from_metadata(mount)?;
        let agent_id = Uuid::parse_str(agent_id)
            .map_err(|_| MountError::NotFound(format!("Lifecycle AgentRun 不存在: {agent_id}")))?;
        let belongs_to_run = self
            .list_lifecycle_agents(run_id)
            .await?
            .into_iter()
            .any(|agent| agent.id == agent_id);
        if !belongs_to_run {
            return Err(MountError::NotFound(format!(
                "Lifecycle AgentRun 不属于当前 run: {agent_id}"
            )));
        }
        Ok(AgentRunJournalRef::new(run_id, agent_id))
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

fn optional_agent_id_from_metadata(mount: &Mount) -> Result<Option<Uuid>, MountError> {
    let Some(raw) = mount
        .metadata
        .get("agent_id")
        .and_then(|value| value.as_str())
    else {
        return Ok(None);
    };
    Uuid::parse_str(raw).map(Some).map_err(|error| {
        MountError::OperationFailed(format!("mount metadata agent_id 无效: {error}"))
    })
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

fn agent_run_session_event_source_from_mount(
    mount: &Mount,
) -> Result<AgentRunJournalRef, MountError> {
    Ok(AgentRunJournalRef::new(
        parse_run_id_from_metadata(mount)?,
        parse_agent_id_from_metadata(mount)?,
    ))
}

fn node_runtime_session_event_source_from_mount(
    mount: &Mount,
) -> Result<AgentRunJournalRef, MountError> {
    let agent_id = optional_agent_id_from_metadata(mount)?.ok_or_else(|| {
        MountError::OperationFailed(
            "node_runtime lifecycle_vfs mount 缺少 AgentRun agent_id，无法读取 AgentRun journal"
                .to_string(),
        )
    })?;
    Ok(AgentRunJournalRef::new(
        parse_run_id_from_metadata(mount)?,
        agent_id,
    ))
}

fn current_node_session_event_source(
    mount: &Mount,
    ctx: &LifecycleMountContext,
) -> Result<AgentRunJournalRef, MountError> {
    session_id_for_node(current_node(ctx)?)?;
    node_runtime_session_event_source_from_mount(mount)
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
        RuntimeFileEntry::dir("agent-runs").as_virtual(),
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

fn agent_run_sessions_path(agent_id: Uuid) -> String {
    format!("agent-runs/{agent_id}/sessions")
}

async fn list_session_projection_entries(
    journey: &LifecycleJourneyProjection,
    source: &AgentRunJournalRef,
    display_root: &str,
    rest: &[&str],
    recursive: bool,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    match rest {
        [] => {
            if recursive {
                list_session_recursive_entries(journey, source, display_root).await
            } else {
                Ok(session_root_entries(display_root))
            }
        }
        ["items"] => {
            list_session_item_entries(
                journey,
                source,
                &format!("{display_root}/items"),
                SessionItemView::Items,
            )
            .await
        }
        ["messages"] => {
            list_session_item_entries(
                journey,
                source,
                &format!("{display_root}/messages"),
                SessionItemView::Messages,
            )
            .await
        }
        ["tools"] => {
            list_session_item_entries(
                journey,
                source,
                &format!("{display_root}/tools"),
                SessionItemView::Tools,
            )
            .await
        }
        ["tool-results"] => {
            list_session_tool_result_entries(
                journey,
                source,
                &format!("{display_root}/tool-results"),
                ToolResultListScope::Root,
                recursive,
            )
            .await
        }
        ["tool-results", turn_alias] => {
            list_session_tool_result_entries(
                journey,
                source,
                &format!("{display_root}/tool-results"),
                ToolResultListScope::Turn { turn_alias },
                recursive,
            )
            .await
        }
        ["tool-results", turn_alias, body_alias] => {
            list_session_tool_result_entries(
                journey,
                source,
                &format!("{display_root}/tool-results"),
                ToolResultListScope::Body {
                    turn_alias,
                    body_alias,
                },
                recursive,
            )
            .await
        }
        ["writes"] => {
            list_session_item_entries(
                journey,
                source,
                &format!("{display_root}/writes"),
                SessionItemView::Writes,
            )
            .await
        }
        ["summaries"] => {
            list_session_summary_entries(journey, source, &format!("{display_root}/summaries"))
                .await
        }
        ["terminal"] => {
            list_session_terminal_entries(
                journey,
                source,
                &format!("{display_root}/terminal"),
                recursive,
            )
            .await
        }
        ["turns"] => {
            list_session_turn_entries(journey, source, &format!("{display_root}/turns"), recursive)
                .await
        }
        ["turns", turn_id] => {
            list_session_turn_entries_for_turn(
                journey,
                source,
                &format!("{display_root}/turns"),
                turn_id,
            )
            .await
        }
        _ => Ok(Vec::new()),
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

        if matches!(segs.as_slice(), ["skills", ..]) {
            return read_lifecycle_skill_asset_projection(
                self.skill_asset_repo.as_ref(),
                mount,
                &path_norm,
            )
            .await;
        }

        if matches!(segs.as_slice(), ["agent-runs", ..]) {
            let content = self.read_agent_runs_scope(mount, &segs).await?;
            return Ok(ReadResult::new(path_norm, content));
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
                let session_source = current_node_session_event_source(mount, &active)?;
                self.journey
                    .read_session_projection(&session_source, &["meta"])
                    .await
                    .map_err(map_journey_err)?
            }
            ["session", rest @ ..] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                self.journey
                    .read_session_projection(&session_source, rest)
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
        if matches!(segs.as_slice(), ["agent-runs", ..]) {
            return Ok(ListResult {
                entries: self
                    .list_agent_runs_scope(mount, &path_norm, options, &segs)
                    .await?,
            });
        }
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
                let session_source = current_node_session_event_source(mount, &active)?;
                if options.recursive {
                    list_session_recursive_entries(&self.journey, &session_source, "session")
                        .await?
                } else {
                    session_root_entries("session")
                }
            }
            ["session", "items"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_item_entries(
                    &self.journey,
                    &session_source,
                    "session/items",
                    SessionItemView::Items,
                )
                .await?
            }
            ["session", "messages"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_item_entries(
                    &self.journey,
                    &session_source,
                    "session/messages",
                    SessionItemView::Messages,
                )
                .await?
            }
            ["session", "tools"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_item_entries(
                    &self.journey,
                    &session_source,
                    "session/tools",
                    SessionItemView::Tools,
                )
                .await?
            }
            ["session", "tool-results"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_tool_result_entries(
                    &self.journey,
                    &session_source,
                    "session/tool-results",
                    ToolResultListScope::Root,
                    options.recursive,
                )
                .await?
            }
            ["session", "tool-results", turn_alias] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_tool_result_entries(
                    &self.journey,
                    &session_source,
                    "session/tool-results",
                    ToolResultListScope::Turn { turn_alias },
                    options.recursive,
                )
                .await?
            }
            ["session", "tool-results", turn_alias, body_alias] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_tool_result_entries(
                    &self.journey,
                    &session_source,
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
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_item_entries(
                    &self.journey,
                    &session_source,
                    "session/writes",
                    SessionItemView::Writes,
                )
                .await?
            }
            ["session", "summaries"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_summary_entries(&self.journey, &session_source, "session/summaries")
                    .await?
            }
            ["session", "terminal"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_terminal_entries(
                    &self.journey,
                    &session_source,
                    "session/terminal",
                    options.recursive,
                )
                .await?
            }
            ["session", "turns"] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_turn_entries(
                    &self.journey,
                    &session_source,
                    "session/turns",
                    options.recursive,
                )
                .await?
            }
            ["session", "turns", turn_id] => {
                let active = load_active_context(&self.lifecycle_run_repo, mount).await?;
                let session_source = current_node_session_event_source(mount, &active)?;
                list_session_turn_entries_for_turn(
                    &self.journey,
                    &session_source,
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
    source: &AgentRunJournalRef,
    display_root: &str,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let mut entries = session_root_entries(display_root);
    entries.extend(
        list_session_item_entries(
            journey,
            source,
            &format!("{display_root}/items"),
            SessionItemView::Items,
        )
        .await?,
    );
    entries.extend(
        list_session_item_entries(
            journey,
            source,
            &format!("{display_root}/messages"),
            SessionItemView::Messages,
        )
        .await?,
    );
    entries.extend(
        list_session_item_entries(
            journey,
            source,
            &format!("{display_root}/tools"),
            SessionItemView::Tools,
        )
        .await?,
    );
    entries.extend(
        list_session_tool_result_entries(
            journey,
            source,
            &format!("{display_root}/tool-results"),
            ToolResultListScope::Root,
            true,
        )
        .await?,
    );
    entries.extend(
        list_session_item_entries(
            journey,
            source,
            &format!("{display_root}/writes"),
            SessionItemView::Writes,
        )
        .await?,
    );
    entries.extend(
        list_session_summary_entries(journey, source, &format!("{display_root}/summaries")).await?,
    );
    entries.extend(
        list_session_terminal_entries(journey, source, &format!("{display_root}/terminal"), true)
            .await?,
    );
    entries.extend(
        list_session_turn_entries(journey, source, &format!("{display_root}/turns"), true).await?,
    );
    Ok(entries)
}

async fn list_session_item_entries(
    journey: &LifecycleJourneyProjection,
    source: &AgentRunJournalRef,
    display_root: &str,
    view: SessionItemView,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let projections = journey
        .session_item_projections(source)
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
    source: &AgentRunJournalRef,
    display_root: &str,
    scope: ToolResultListScope<'_>,
    recursive: bool,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let projection_session_id = source.projection_session_id();
    let projections = journey
        .session_item_projections(source)
        .await
        .map_err(map_journey_err)?;
    let mut metadata = filter_session_items(&projections, SessionItemView::Tools)
        .iter()
        .filter_map(|projection| {
            tool_result_metadata_for_projection(&projection_session_id, projection)
        })
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
        entries.push(
            RuntimeFileEntry::file(rebase_session_path(display_root, &entry.metadata_path))
                .as_virtual(),
        );
        entries.push(
            RuntimeFileEntry::file(rebase_session_path(display_root, &entry.result_path))
                .as_virtual(),
        );
    }
    dirs.into_iter()
        .map(|path| RuntimeFileEntry::dir(path).as_virtual())
        .chain(entries)
        .collect()
}

fn rebase_session_path(display_root: &str, path: &str) -> String {
    path.strip_prefix("session/tool-results")
        .map(|suffix| format!("{display_root}{suffix}"))
        .unwrap_or_else(|| path.to_string())
}

async fn list_session_terminal_entries(
    journey: &LifecycleJourneyProjection,
    source: &AgentRunJournalRef,
    display_root: &str,
    _recursive: bool,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let mut metadata = journey
        .terminal_metadata_entries(source)
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
    source: &AgentRunJournalRef,
    display_root: &str,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let entries = session_summary_archives(
        journey
            .compaction_archives(source)
            .await
            .map_err(map_journey_err)?,
    );
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
    source: &AgentRunJournalRef,
    display_root: &str,
    recursive: bool,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let events = journey
        .journal_events(source)
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
    source: &AgentRunJournalRef,
    display_root: &str,
    turn_id: &str,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let events = journey
        .journal_events(source)
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
    use super::*;
    use crate::lifecycle::surface::journey::{
        AgentRunJournalProjection, JourneyResult, SessionCompactionArchive,
        SessionCompactionArchiveStatus,
    };
    use agentdash_agent_protocol::backbone::item::ItemCompletedNotification;
    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use agentdash_agent_protocol::{
        AgentDashThreadItem, BackboneEnvelope, BackboneEvent, SourceInfo, TraceInfo,
    };
    use agentdash_domain::common::MountCapability;
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind};
    use agentdash_domain::skill_asset::SkillAsset;
    use agentdash_domain::workflow::{AgentSource, LifecycleAgent};
    use agentdash_platform_spi::PersistedSessionEvent;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FixtureRunRepo {
        runs: Mutex<HashMap<Uuid, LifecycleRun>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for FixtureRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().insert(run.id, run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self.runs.lock().unwrap().get(&id).cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            let runs = self.runs.lock().unwrap();
            Ok(ids.iter().filter_map(|id| runs.get(id).cloned()).collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .values()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().insert(run.id, run.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().unwrap().remove(&id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FixtureAgentRepo {
        agents: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
    impl LifecycleAgentRepository for FixtureAgentRepo {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.agents.lock().unwrap().push(agent.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .unwrap()
                .iter()
                .find(|agent| agent.id == id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .unwrap()
                .iter()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut agents = self.agents.lock().unwrap();
            if let Some(existing) = agents.iter_mut().find(|existing| existing.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
    }

    struct EmptyInlineRepo;

    #[async_trait]
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

    struct EmptySkillRepo;

    #[async_trait]
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

    struct EmptyAgentRunJournalReader;

    #[async_trait]
    impl AgentRunJournalReader for EmptyAgentRunJournalReader {
        async fn visible_journal(
            &self,
            _reference: AgentRunJournalRef,
        ) -> JourneyResult<AgentRunJournalProjection> {
            Err(LifecycleJourneyError::NotFound(
                "no journal in fixture".to_string(),
            ))
        }
    }

    struct EmptyAgentRunCompactionArchiveReader;

    #[async_trait]
    impl AgentRunCompactionArchiveReader for EmptyAgentRunCompactionArchiveReader {
        async fn list_archives(
            &self,
            _reference: AgentRunJournalRef,
        ) -> JourneyResult<Vec<SessionCompactionArchive>> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone)]
    struct FixtureAgentRunJournalReader {
        projection: AgentRunJournalProjection,
    }

    #[async_trait]
    impl AgentRunJournalReader for FixtureAgentRunJournalReader {
        async fn visible_journal(
            &self,
            _reference: AgentRunJournalRef,
        ) -> JourneyResult<AgentRunJournalProjection> {
            Ok(self.projection.clone())
        }
    }

    #[derive(Clone)]
    struct FixtureAgentRunCompactionArchiveReader {
        archives: Vec<SessionCompactionArchive>,
    }

    #[async_trait]
    impl AgentRunCompactionArchiveReader for FixtureAgentRunCompactionArchiveReader {
        async fn list_archives(
            &self,
            _reference: AgentRunJournalRef,
        ) -> JourneyResult<Vec<SessionCompactionArchive>> {
            Ok(self.archives.clone())
        }
    }

    fn item_completed_event(
        session_id: &str,
        event_seq: u64,
        item: AgentDashThreadItem,
    ) -> PersistedSessionEvent {
        let envelope = BackboneEnvelope::new(
            BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                item,
                session_id.to_string(),
                "turn-1".to_string(),
            )),
            session_id,
            SourceInfo {
                connector_id: "fixture".to_string(),
                connector_type: "managed_runtime".to_string(),
                executor_id: None,
            },
        )
        .with_trace(TraceInfo {
            turn_id: Some("turn-1".to_string()),
            entry_index: Some(0),
        });
        PersistedSessionEvent {
            session_id: session_id.to_string(),
            event_seq,
            occurred_at_ms: event_seq as i64,
            committed_at_ms: event_seq as i64,
            session_update_type: "item_completed".to_string(),
            turn_id: Some("turn-1".to_string()),
            entry_index: Some(0),
            tool_call_id: None,
            ephemeral: false,
            notification: serde_json::to_value(envelope).expect("fixture envelope"),
        }
    }

    fn provider_fixture_with_journey(
        run: LifecycleRun,
        agents: Vec<LifecycleAgent>,
        journal_reader: Arc<dyn AgentRunJournalReader>,
        archive_reader: Arc<dyn AgentRunCompactionArchiveReader>,
    ) -> (LifecycleMountProvider, Mount) {
        let mount_agent_id = agents
            .first()
            .map(|agent| agent.id)
            .unwrap_or_else(Uuid::new_v4);
        let run_repo = Arc::new(FixtureRunRepo::default());
        run_repo.runs.lock().unwrap().insert(run.id, run.clone());
        let agent_repo = Arc::new(FixtureAgentRepo::default());
        *agent_repo.agents.lock().unwrap() = agents;
        let provider = LifecycleMountProvider::new(
            run_repo,
            agent_repo,
            Arc::new(EmptyInlineRepo),
            Arc::new(EmptySkillRepo),
            journal_reader,
            archive_reader,
        );
        let mount = Mount {
            id: "lifecycle".to_string(),
            provider: PROVIDER_LIFECYCLE_VFS.to_string(),
            backend_id: "backend".to_string(),
            root_ref: format!("lifecycle://run/{}/session", run.id),
            capabilities: vec![MountCapability::Read, MountCapability::List],
            default_write: false,
            display_name: "Lifecycle".to_string(),
            metadata: serde_json::json!({
                "scope": "agent_run_session",
                "run_id": run.id.to_string(),
                "agent_id": mount_agent_id.to_string(),
                "runtime_session_id": "runtime-session",
                "launch_frame_id": Uuid::new_v4().to_string(),
            }),
        };
        (provider, mount)
    }

    fn provider_fixture(
        run: LifecycleRun,
        agents: Vec<LifecycleAgent>,
    ) -> (LifecycleMountProvider, Mount) {
        provider_fixture_with_journey(
            run,
            agents,
            Arc::new(EmptyAgentRunJournalReader),
            Arc::new(EmptyAgentRunCompactionArchiveReader),
        )
    }

    #[tokio::test]
    async fn lifecycle_mount_lists_agent_run_sessions_index() {
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let parent_agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
        let child_agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::Subagent);
        let child_agent_id = child_agent.id;
        let (provider, mount) = provider_fixture(run, vec![parent_agent, child_agent]);

        let root = provider
            .list(
                &mount,
                &ListOptions {
                    path: "".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("root list");
        assert!(root.entries.iter().any(|entry| entry.path == "session"));
        assert!(root.entries.iter().any(|entry| entry.path == "agent-runs"));

        let agent_runs = provider
            .list(
                &mount,
                &ListOptions {
                    path: "agent-runs".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("agent-runs list");
        assert!(
            agent_runs.entries.iter().any(|entry| {
                entry.path == format!("agent-runs/{child_agent_id}") && entry.is_dir
            })
        );

        let sessions = provider
            .list(
                &mount,
                &ListOptions {
                    path: format!("agent-runs/{child_agent_id}/sessions"),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("agent sessions list");
        let paths = sessions
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();
        assert!(
            paths
                .iter()
                .any(|path| *path == format!("agent-runs/{child_agent_id}/sessions/messages"))
        );
        assert!(
            paths
                .iter()
                .any(|path| *path == format!("agent-runs/{child_agent_id}/sessions/events.json"))
        );
        assert!(
            paths
                .iter()
                .any(|path| *path == format!("agent-runs/{child_agent_id}/sessions/tool-results"))
        );
    }

    #[tokio::test]
    async fn lifecycle_mount_lists_and_reads_messages_and_compaction_summaries() {
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
        let projection_session_id = format!("agentrun:{}:{}", run.id, agent.id);
        let message_item: AgentDashThreadItem = codex::ThreadItem::AgentMessage {
            id: "turn-1:message-1".to_string(),
            text: "完整的助手消息正文".to_string(),
            phase: None,
            memory_citation: None,
        }
        .into();
        let journal_reader = Arc::new(FixtureAgentRunJournalReader {
            projection: AgentRunJournalProjection {
                delivery_runtime_session_id: "actual-runtime-thread".to_string(),
                events: vec![item_completed_event(
                    &projection_session_id,
                    1,
                    message_item,
                )],
            },
        });
        let archive_reader = Arc::new(FixtureAgentRunCompactionArchiveReader {
            archives: vec![SessionCompactionArchive {
                id: "compact-1".to_string(),
                lifecycle_item_id: "item-compact-1".to_string(),
                projection_version: 1,
                completed_event_seq: 2,
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(1),
                summary: "压缩前会话摘要".to_string(),
                trigger: None,
                phase: None,
                strategy: None,
                token_stats_json: serde_json::json!({"tokens_before": 42}),
                diagnostics_json: serde_json::Value::Null,
                turn_id: Some("turn-1".to_string()),
                entry_index: Some(1),
                status: SessionCompactionArchiveStatus::ProjectionCommitted,
            }],
        });
        let (provider, mount) =
            provider_fixture_with_journey(run, vec![agent], journal_reader, archive_reader);

        let messages = provider
            .list(
                &mount,
                &ListOptions {
                    path: "session/messages".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("messages list");
        let message_path = messages
            .entries
            .iter()
            .find(|entry| !entry.is_dir)
            .expect("message projection file")
            .path
            .clone();
        let message = provider
            .read_text(&mount, &message_path, &MountOperationContext::default())
            .await
            .expect("message read");
        assert!(message.content.contains("完整的助手消息正文"));

        let summaries = provider
            .list(
                &mount,
                &ListOptions {
                    path: "session/summaries".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("summaries list");
        let summary_path = summaries
            .entries
            .iter()
            .find(|entry| !entry.is_dir)
            .expect("summary projection file")
            .path
            .clone();
        let summary = provider
            .read_text(&mount, &summary_path, &MountOperationContext::default())
            .await
            .expect("summary read");
        assert!(summary.content.contains("压缩前会话摘要"));
        assert!(summary.content.contains("\"trigger\": null"));
        assert!(summary.content.contains("\"strategy\": null"));
    }

    #[tokio::test]
    async fn lifecycle_mount_lists_and_reads_canonical_tool_result_bodies() {
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
        let projection_session_id = format!("agentrun:{}:{}", run.id, agent.id);
        let items = vec![
            codex::ThreadItem::CommandExecution {
                aggregated_output: Some(Some("command-full-output".to_string())),
                command: "echo command".to_string(),
                command_actions: Vec::new(),
                cwd: codex::LegacyAppPathString("D:/workspace".to_string()),
                duration_ms: Some(Some(10)),
                exit_code: Some(Some(0)),
                id: "turn-1:command".to_string(),
                process_id: None,
                source: codex::CommandExecutionSource::Agent,
                status: codex::CommandExecutionStatus::Completed,
            },
            codex::ThreadItem::DynamicToolCall {
                arguments: serde_json::json!({"path": "README.md"}),
                content_items: Some(Some(vec![
                    codex::DynamicToolCallOutputContentItem::InputText {
                        text: "dynamic-full-output".to_string(),
                    },
                ])),
                duration_ms: Some(Some(12)),
                id: "turn-1:dynamic".to_string(),
                namespace: None,
                status: codex::DynamicToolCallStatus::Completed,
                success: Some(Some(true)),
                tool: "read_file".to_string(),
            },
            codex::ThreadItem::McpToolCall {
                app_context: None,
                arguments: serde_json::json!({"query": "fixture"}),
                duration_ms: Some(Some(14)),
                error: None,
                id: "turn-1:mcp".to_string(),
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: Some(Some(codex::McpToolCallResult {
                    content: vec![serde_json::json!({
                        "type": "text",
                        "text": "mcp-full-output"
                    })],
                    meta: None,
                    structured_content: None,
                })),
                server: "fixture-server".to_string(),
                status: codex::McpToolCallStatus::Completed,
                tool: "fixture-tool".to_string(),
            },
        ];
        let events = items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                item_completed_event(
                    &projection_session_id,
                    index as u64 + 1,
                    AgentDashThreadItem::Codex(item),
                )
            })
            .collect();
        let journal_reader = Arc::new(FixtureAgentRunJournalReader {
            projection: AgentRunJournalProjection {
                delivery_runtime_session_id: "actual-runtime-thread".to_string(),
                events,
            },
        });
        let (provider, mount) = provider_fixture_with_journey(
            run,
            vec![agent],
            journal_reader,
            Arc::new(FixtureAgentRunCompactionArchiveReader {
                archives: Vec::new(),
            }),
        );

        let index = provider
            .read_text(
                &mount,
                "session/tool-results",
                &MountOperationContext::default(),
            )
            .await
            .expect("tool result index");
        let index_json: serde_json::Value =
            serde_json::from_str(&index.content).expect("tool result index json");
        let entries = index_json.as_array().expect("tool result metadata array");
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|entry| {
            entry
                .pointer("/body_status/status")
                .and_then(|value| value.as_str())
                == Some("available")
        }));

        for (body_alias, expected) in [
            ("command", "command-full-output"),
            ("dynamic", "dynamic-full-output"),
            ("mcp", "mcp-full-output"),
        ] {
            let body = provider
                .read_text(
                    &mount,
                    &format!("session/tool-results/turn-1/{body_alias}/result.txt"),
                    &MountOperationContext::default(),
                )
                .await
                .unwrap_or_else(|error| panic!("{body_alias} tool result read: {error}"));
            assert_eq!(body.content, expected);
        }
    }

    #[tokio::test]
    async fn current_lifecycle_vfs_matches_pinned_main_observable_capture() {
        let project_id = Uuid::nil();
        let mut run = LifecycleRun::new_plain(project_id);
        run.id = Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();
        let mut agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
        agent.id = Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap();
        let projection_session_id = format!("agentrun:{}:{}", run.id, agent.id);
        let mcp_result = codex::McpToolCallResult {
            content: vec![serde_json::json!({
                "type": "text",
                "text": "mcp retained body"
            })],
            meta: Some(serde_json::json!({
                "truncation": {"policy": "head_tail", "originalBytes": 4096}
            })),
            structured_content: Some(serde_json::Value::Null),
        };
        let events = vec![
            item_completed_event(
                &projection_session_id,
                1,
                codex::ThreadItem::AgentMessage {
                    id: "turn-1:message".to_string(),
                    text: "canonical assistant body".to_string(),
                    phase: None,
                    memory_citation: None,
                }
                .into(),
            ),
            item_completed_event(
                &projection_session_id,
                2,
                codex::ThreadItem::CommandExecution {
                    aggregated_output: Some(Some("command complete body\n".to_string())),
                    command: "echo complete".to_string(),
                    command_actions: Vec::new(),
                    cwd: codex::LegacyAppPathString("D:/workspace".to_string()),
                    duration_ms: Some(Some(10)),
                    exit_code: Some(Some(0)),
                    id: "turn-1:command".to_string(),
                    process_id: None,
                    source: codex::CommandExecutionSource::Agent,
                    status: codex::CommandExecutionStatus::Completed,
                }
                .into(),
            ),
            item_completed_event(
                &projection_session_id,
                3,
                codex::ThreadItem::McpToolCall {
                    app_context: None,
                    arguments: serde_json::json!({"path": "large.log"}),
                    duration_ms: Some(Some(20)),
                    error: None,
                    id: "turn-1:mcp".to_string(),
                    mcp_app_resource_uri: None,
                    plugin_id: None,
                    result: Some(Some(mcp_result)),
                    server: "fixture-server".to_string(),
                    status: codex::McpToolCallStatus::Completed,
                    tool: "read".to_string(),
                }
                .into(),
            ),
        ];
        let journal_reader = Arc::new(FixtureAgentRunJournalReader {
            projection: AgentRunJournalProjection {
                delivery_runtime_session_id: "actual-runtime-thread".to_string(),
                events,
            },
        });
        let archive_reader = Arc::new(FixtureAgentRunCompactionArchiveReader {
            archives: vec![SessionCompactionArchive {
                id: "compact-1".to_string(),
                lifecycle_item_id: "item-compact-1".to_string(),
                projection_version: 1,
                completed_event_seq: 4,
                source_start_event_seq: Some(1),
                source_end_event_seq: Some(3),
                summary: "canonical compacted summary".to_string(),
                trigger: None,
                phase: None,
                strategy: None,
                token_stats_json: serde_json::json!({"tokens_before": 42}),
                diagnostics_json: serde_json::Value::Null,
                turn_id: Some("turn-1".to_string()),
                entry_index: Some(3),
                status: SessionCompactionArchiveStatus::ProjectionCommitted,
            }],
        });
        let (provider, mount) =
            provider_fixture_with_journey(run, vec![agent], journal_reader, archive_reader);

        let message_list = provider
            .list(
                &mount,
                &ListOptions {
                    path: "session/messages".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await
            .unwrap();
        let message_paths = message_list
            .entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let message_body = provider
            .read_text(
                &mount,
                message_paths.first().expect("message path"),
                &MountOperationContext::default(),
            )
            .await
            .unwrap()
            .content;

        let tool_list = provider
            .list(
                &mount,
                &ListOptions {
                    path: "session/tool-results".to_string(),
                    pattern: None,
                    recursive: true,
                },
                &MountOperationContext::default(),
            )
            .await
            .unwrap();
        let tool_paths = tool_list
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let tool_index = provider
            .read_text(
                &mount,
                "session/tool-results",
                &MountOperationContext::default(),
            )
            .await
            .unwrap();
        let tool_metadata: Vec<serde_json::Value> =
            serde_json::from_str(&tool_index.content).unwrap();
        let mut tool_reads = Vec::new();
        for metadata in tool_metadata {
            let result_path = metadata["result_path"].as_str().unwrap();
            let body = provider
                .read_text(&mount, result_path, &MountOperationContext::default())
                .await
                .unwrap()
                .content;
            tool_reads.push(serde_json::json!({
                "metadata": metadata,
                "body": body,
            }));
        }
        let summary_list = provider
            .list(
                &mount,
                &ListOptions {
                    path: "session/summaries".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await
            .unwrap();
        let summary_paths = summary_list
            .entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let summary_markdown = provider
            .read_text(
                &mount,
                summary_paths.first().expect("summary path"),
                &MountOperationContext::default(),
            )
            .await
            .unwrap()
            .content;
        let capture = serde_json::json!({
            "messages": {
                "list_paths": message_paths,
                "reads": [{"path": message_paths[0], "body": message_body}],
            },
            "tool_results": {
                "list_paths": tool_paths,
                "reads": tool_reads,
            },
            "summaries": {
                "list_paths": summary_paths,
                "reads": [{
                    "path": summary_paths[0],
                    "summary": "canonical compacted summary",
                    "trigger": serde_json::Value::Null,
                    "strategy": serde_json::Value::Null,
                    "contains_summary": summary_markdown.ends_with("canonical compacted summary"),
                    "contains_null_trigger": summary_markdown.contains("\"trigger\": null"),
                    "contains_null_strategy": summary_markdown.contains("\"strategy\": null"),
                }],
            },
        });
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../agentdash-agent-runtime-test-support/fixtures/session-parity/main/lifecycle-vfs-observables.json"
        ))
        .unwrap();
        assert_eq!(capture, expected["protected_observables"]);
    }

    #[tokio::test]
    async fn lifecycle_mount_rejects_agent_outside_current_run() {
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let parent_agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
        let outside_agent_id = Uuid::new_v4();
        let (provider, mount) = provider_fixture(run, vec![parent_agent]);

        let result = provider
            .list(
                &mount,
                &ListOptions {
                    path: format!("agent-runs/{outside_agent_id}/sessions"),
                    pattern: None,
                    recursive: false,
                },
                &MountOperationContext::default(),
            )
            .await;

        assert!(matches!(result, Err(MountError::NotFound(_))));
    }
}
