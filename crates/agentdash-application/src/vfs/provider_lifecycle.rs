//! `lifecycle_vfs` mount: 将 lifecycle journey 投影适配为 VFS 访问面。

use std::collections::BTreeMap;
use std::sync::Arc;

use super::lifecycle_catalog::{lifecycle_active_entries, lifecycle_root_entries};
use super::mount::{PROVIDER_LIFECYCLE_VFS, list_inline_entries};
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountError, MountOperationContext, MountProvider, SearchQuery, SearchResult,
};
use super::provider_skill_asset::{
    list_projected_skill_files, parse_skill_asset_mount_metadata, read_projected_skill_file,
    search_projected_skill_files,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::{Mount, RuntimeFileEntry};
use crate::session::SessionPersistence;
use crate::workflow::lifecycle::journey::{
    LifecycleJourneyError, LifecycleJourneyProjection, SessionItemView, current_step,
    current_step_session_id, filter_session_items, find_step, group_events_into_turn_summaries,
    item_file_name, run_overview, session_summary_archives, step_session_id, to_json_pretty,
};
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository};
use async_trait::async_trait;
use tracing::info;
use uuid::Uuid;

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

fn lifecycle_mount_has_skills(mount: &Mount) -> bool {
    parse_skill_asset_mount_metadata(mount)
        .map(|(_, keys)| !keys.is_empty())
        .unwrap_or(false)
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

fn parse_run_id_from_metadata(mount: &Mount) -> Result<Uuid, MountError> {
    let run_id_str = mount
        .metadata
        .get("run_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| MountError::OperationFailed("mount metadata 缺少 run_id".to_string()))?;
    Uuid::parse_str(run_id_str)
        .map_err(|error| MountError::OperationFailed(format!("run_id 无效: {error}")))
}

fn resolve_session_id_for_runs(_mount: &Mount, active_run: &LifecycleRun) -> String {
    active_run.session_id.clone()
}

async fn load_active_run(
    repo: &Arc<dyn LifecycleRunRepository>,
    mount: &Mount,
) -> Result<LifecycleRun, MountError> {
    let run_id = parse_run_id_from_metadata(mount)?;
    repo.get_by_id(run_id)
        .await
        .map_err(map_domain_err)?
        .ok_or_else(|| MountError::NotFound(format!("lifecycle run 不存在: {run_id}")))
}

fn segments_from_path(path: &str) -> Vec<&str> {
    if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect()
    }
}

fn list_projected_entries(
    files: BTreeMap<String, String>,
    display_root: &str,
    base_path: &str,
    options: &ListOptions,
) -> Vec<RuntimeFileEntry> {
    let display_root = display_root.trim_matches('/');
    let projected = files
        .into_iter()
        .map(|(path, content)| {
            (
                format!("{display_root}/{}", path.trim_matches('/')),
                content,
            )
        })
        .collect::<BTreeMap<_, _>>();
    list_inline_entries(
        &projected,
        base_path,
        options.pattern.as_deref(),
        options.recursive,
    )
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
                let run_id = parse_run_id_from_metadata(mount)?;
                let map = self
                    .journey
                    .list_port_outputs(run_id)
                    .await
                    .map_err(map_journey_err)?;
                to_json_pretty(&map).map_err(map_journey_err)?
            }
            ["artifacts", port_key] | ["active", "artifacts", port_key] => {
                let run_id = parse_run_id_from_metadata(mount)?;
                self.journey
                    .read_port_output(run_id, port_key)
                    .await
                    .map_err(map_journey_err)?
            }
            _ => {
                let active = load_active_run(&self.lifecycle_run_repo, mount).await?;
                let run_id = parse_run_id_from_metadata(mount)?;
                match segs.as_slice() {
                    [] | ["active"] => {
                        to_json_pretty(&run_overview(&active)).map_err(map_journey_err)?
                    }
                    ["active", "steps"] => {
                        to_json_pretty(&active.step_states).map_err(map_journey_err)?
                    }
                    ["active", "steps", key] => {
                        let step = find_step(&active, key).map_err(map_journey_err)?;
                        to_json_pretty(step).map_err(map_journey_err)?
                    }
                    ["active", "log"] => {
                        to_json_pretty(&active.execution_log).map_err(map_journey_err)?
                    }
                    ["state"] => {
                        let step = current_step(&active).map_err(map_journey_err)?;
                        to_json_pretty(step).map_err(map_journey_err)?
                    }
                    ["session", "summary"] => {
                        let step = current_step(&active).map_err(map_journey_err)?;
                        self.journey
                            .read_node_summary(run_id, step)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["session", "conclusions"] => {
                        let step = current_step(&active).map_err(map_journey_err)?;
                        self.journey
                            .read_node_conclusions(run_id, &step.step_key)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["session", rest @ ..] => {
                        let (_, session_id) =
                            current_step_session_id(&active).map_err(map_journey_err)?;
                        self.journey
                            .read_session_projection(session_id, rest)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["records"] => {
                        let step = current_step(&active).map_err(map_journey_err)?;
                        self.journey
                            .read_records_map(run_id, &step.step_key)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["records", rest @ ..] => {
                        let step = current_step(&active).map_err(map_journey_err)?;
                        self.journey
                            .read_record(run_id, &step.step_key, rest)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["runs"] => {
                        let session_id = resolve_session_id_for_runs(mount, &active);
                        let runs = self
                            .lifecycle_run_repo
                            .list_by_session(&session_id)
                            .await
                            .map_err(map_domain_err)?;
                        let summaries = runs.iter().map(run_overview).collect::<Vec<_>>();
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
                    ["nodes", key, "state"] => {
                        let step = find_step(&active, key).map_err(map_journey_err)?;
                        to_json_pretty(step).map_err(map_journey_err)?
                    }
                    ["nodes", key, "records"] => {
                        find_step(&active, key).map_err(map_journey_err)?;
                        self.journey
                            .read_records_map(run_id, key)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["nodes", key, "records", rest @ ..] => {
                        find_step(&active, key).map_err(map_journey_err)?;
                        self.journey
                            .read_record(run_id, key, rest)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["nodes", key, "session", "summary"] => {
                        let step = find_step(&active, key).map_err(map_journey_err)?;
                        self.journey
                            .read_node_summary(run_id, step)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["nodes", key, "session", "conclusions"] => {
                        find_step(&active, key).map_err(map_journey_err)?;
                        self.journey
                            .read_node_conclusions(run_id, key)
                            .await
                            .map_err(map_journey_err)?
                    }
                    ["nodes", key, "session", rest @ ..] => {
                        let session_id = step_session_id(&active, key).map_err(map_journey_err)?;
                        self.journey
                            .read_session_projection(session_id, rest)
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

                let run_id = parse_run_id_from_metadata(mount)?;
                self.journey
                    .write_port_output(run_id, port_key, content)
                    .await
                    .map_err(map_journey_err)?;
                info!(
                    run_id = %run_id,
                    port_key = %port_key,
                    content_len = content.len(),
                    "lifecycle VFS: wrote port output"
                );
                Ok(())
            }
            ["records", rest @ ..] => {
                let active = load_active_run(&self.lifecycle_run_repo, mount).await?;
                let step = current_step(&active).map_err(map_journey_err)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let name = self
                    .journey
                    .write_record(run_id, &step.step_key, rest, content)
                    .await
                    .map_err(map_journey_err)?;
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
                find_step(&active, key).map_err(map_journey_err)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let name = self
                    .journey
                    .write_record(run_id, key, rest, content)
                    .await
                    .map_err(map_journey_err)?;
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

        if matches!(segs.as_slice(), ["skills", ..]) {
            return list_projected_skill_files(self.skill_asset_repo.as_ref(), mount, options)
                .await;
        }

        let active = load_active_run(&self.lifecycle_run_repo, mount).await?;

        let entries = match segs.as_slice() {
            [] => lifecycle_root_entries(lifecycle_mount_has_skills(mount)),
            ["active"] => lifecycle_active_entries(
                serde_json::to_string(&active.execution_log)
                    .map(|content| content.len() as u64)
                    .unwrap_or(0),
            ),
            ["active", "steps"] => active
                .step_states
                .iter()
                .map(|step| {
                    RuntimeFileEntry::file(format!("active/steps/{}", step.step_key)).as_virtual()
                })
                .collect(),
            ["artifacts"] | ["active", "artifacts"] => {
                let run_id = parse_run_id_from_metadata(mount)?;
                let files = self
                    .journey
                    .list_port_outputs(run_id)
                    .await
                    .map_err(map_journey_err)?;
                let display_root = if matches!(segs.as_slice(), ["active", "artifacts"]) {
                    "active/artifacts"
                } else {
                    "artifacts"
                };
                list_projected_entries(files, display_root, display_root, options)
            }
            ["session"] => {
                if current_step_session_id(&active).is_ok() {
                    vec![
                        RuntimeFileEntry::file("session/meta").as_virtual(),
                        RuntimeFileEntry::file("session/summary").as_virtual(),
                        RuntimeFileEntry::file("session/conclusions").as_virtual(),
                        RuntimeFileEntry::file("session/events.json").as_virtual(),
                        RuntimeFileEntry::file("session/terminal").as_virtual(),
                        RuntimeFileEntry::dir("session/items").as_virtual(),
                        RuntimeFileEntry::dir("session/messages").as_virtual(),
                        RuntimeFileEntry::dir("session/tools").as_virtual(),
                        RuntimeFileEntry::dir("session/writes").as_virtual(),
                        RuntimeFileEntry::dir("session/summaries").as_virtual(),
                    ]
                } else {
                    vec![
                        RuntimeFileEntry::file("session/summary").as_virtual(),
                        RuntimeFileEntry::file("session/conclusions").as_virtual(),
                    ]
                }
            }
            ["session", "items"] => {
                let (_, session_id) = current_step_session_id(&active).map_err(map_journey_err)?;
                list_session_item_entries(
                    &self.journey,
                    session_id,
                    "session/items",
                    SessionItemView::Items,
                )
                .await?
            }
            ["session", "messages"] => {
                let (_, session_id) = current_step_session_id(&active).map_err(map_journey_err)?;
                list_session_item_entries(
                    &self.journey,
                    session_id,
                    "session/messages",
                    SessionItemView::Messages,
                )
                .await?
            }
            ["session", "tools"] => {
                let (_, session_id) = current_step_session_id(&active).map_err(map_journey_err)?;
                list_session_item_entries(
                    &self.journey,
                    session_id,
                    "session/tools",
                    SessionItemView::Tools,
                )
                .await?
            }
            ["session", "writes"] => {
                let (_, session_id) = current_step_session_id(&active).map_err(map_journey_err)?;
                list_session_item_entries(
                    &self.journey,
                    session_id,
                    "session/writes",
                    SessionItemView::Writes,
                )
                .await?
            }
            ["session", "summaries"] => {
                let (_, session_id) = current_step_session_id(&active).map_err(map_journey_err)?;
                list_session_summary_entries(&self.journey, session_id, "session/summaries").await?
            }
            ["session", "turns"] => {
                let (_, session_id) = current_step_session_id(&active).map_err(map_journey_err)?;
                let events = self
                    .journey
                    .session_events(session_id)
                    .await
                    .map_err(map_journey_err)?;
                group_events_into_turn_summaries(&events)
                    .into_iter()
                    .map(|turn| {
                        RuntimeFileEntry::dir(format!("session/turns/{}", turn.turn_id))
                            .as_virtual()
                    })
                    .collect()
            }
            ["session", "turns", turn_id] => {
                let (_, session_id) = current_step_session_id(&active).map_err(map_journey_err)?;
                let events = self
                    .journey
                    .session_events(session_id)
                    .await
                    .map_err(map_journey_err)?;
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
            ["records"] => {
                let step = current_step(&active).map_err(map_journey_err)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let files = self
                    .journey
                    .records_map(run_id, &step.step_key)
                    .await
                    .map_err(map_journey_err)?;
                list_projected_entries(files, "records", "records", options)
            }
            ["records", rest @ ..] => {
                let step = current_step(&active).map_err(map_journey_err)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let files = self
                    .journey
                    .records_map(run_id, &step.step_key)
                    .await
                    .map_err(map_journey_err)?;
                let display_base = format!("records/{}", rest.join("/"));
                list_projected_entries(files, "records", &display_base, options)
            }
            ["nodes"] => active
                .step_states
                .iter()
                .map(|step| RuntimeFileEntry::dir(format!("nodes/{}", step.step_key)).as_virtual())
                .collect(),
            ["nodes", key] => {
                if let Some(step) = active.step_states.iter().find(|step| step.step_key == *key) {
                    let mut entries =
                        vec![RuntimeFileEntry::file(format!("nodes/{key}/state")).as_virtual()];
                    if step.session_id.is_some() {
                        entries.push(
                            RuntimeFileEntry::dir(format!("nodes/{key}/session")).as_virtual(),
                        );
                    }
                    entries.push(RuntimeFileEntry::dir(format!("nodes/{key}/records")));
                    entries
                } else {
                    Vec::new()
                }
            }
            ["nodes", key, "session"] => {
                let step = active.step_states.iter().find(|step| step.step_key == *key);
                if step.and_then(|step| step.session_id.as_ref()).is_none() {
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
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/items")).as_virtual(),
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/messages")).as_virtual(),
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/tools")).as_virtual(),
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/writes")).as_virtual(),
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/summaries"))
                            .as_virtual(),
                    ]
                }
            }
            ["nodes", key, "session", "items"] => {
                let session_id = step_session_id(&active, key).map_err(map_journey_err)?;
                list_session_item_entries(
                    &self.journey,
                    session_id,
                    &format!("nodes/{key}/session/items"),
                    SessionItemView::Items,
                )
                .await?
            }
            ["nodes", key, "session", "messages"] => {
                let session_id = step_session_id(&active, key).map_err(map_journey_err)?;
                list_session_item_entries(
                    &self.journey,
                    session_id,
                    &format!("nodes/{key}/session/messages"),
                    SessionItemView::Messages,
                )
                .await?
            }
            ["nodes", key, "session", "tools"] => {
                let session_id = step_session_id(&active, key).map_err(map_journey_err)?;
                list_session_item_entries(
                    &self.journey,
                    session_id,
                    &format!("nodes/{key}/session/tools"),
                    SessionItemView::Tools,
                )
                .await?
            }
            ["nodes", key, "session", "writes"] => {
                let session_id = step_session_id(&active, key).map_err(map_journey_err)?;
                list_session_item_entries(
                    &self.journey,
                    session_id,
                    &format!("nodes/{key}/session/writes"),
                    SessionItemView::Writes,
                )
                .await?
            }
            ["nodes", key, "session", "summaries"] => {
                let session_id = step_session_id(&active, key).map_err(map_journey_err)?;
                list_session_summary_entries(
                    &self.journey,
                    session_id,
                    &format!("nodes/{key}/session/summaries"),
                )
                .await?
            }
            ["nodes", key, "session", "turns"] => {
                let session_id = step_session_id(&active, key).map_err(map_journey_err)?;
                let events = self
                    .journey
                    .session_events(session_id)
                    .await
                    .map_err(map_journey_err)?;
                group_events_into_turn_summaries(&events)
                    .into_iter()
                    .map(|turn| {
                        RuntimeFileEntry::dir(format!("nodes/{key}/session/turns/{}", turn.turn_id))
                            .as_virtual()
                    })
                    .collect()
            }
            ["nodes", key, "session", "turns", turn_id] => {
                let session_id = step_session_id(&active, key).map_err(map_journey_err)?;
                let events = self
                    .journey
                    .session_events(session_id)
                    .await
                    .map_err(map_journey_err)?;
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
            ["nodes", key, "records"] => {
                find_step(&active, key).map_err(map_journey_err)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let files = self
                    .journey
                    .records_map(run_id, key)
                    .await
                    .map_err(map_journey_err)?;
                let display_root = format!("nodes/{key}/records");
                list_projected_entries(files, &display_root, &display_root, options)
            }
            ["nodes", key, "records", rest @ ..] => {
                find_step(&active, key).map_err(map_journey_err)?;
                let run_id = parse_run_id_from_metadata(mount)?;
                let files = self
                    .journey
                    .records_map(run_id, key)
                    .await
                    .map_err(map_journey_err)?;
                let display_root = format!("nodes/{key}/records");
                let display_base = format!("nodes/{key}/records/{}", rest.join("/"));
                list_projected_entries(files, &display_root, &display_base, options)
            }
            ["runs"] => {
                let session_id = resolve_session_id_for_runs(mount, &active);
                let runs = self
                    .lifecycle_run_repo
                    .list_by_session(&session_id)
                    .await
                    .map_err(map_domain_err)?;
                runs.iter()
                    .map(|run| RuntimeFileEntry::file(format!("runs/{}", run.id)).as_virtual())
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
        // 仅 skills 子树上提供 native substring search（直接读 skill_asset 表）。
        // virtual projection 路径（nodes/ session/ items/ records/ ...）通过
        // SPI `grep_text` 默认实现的 list+read+regex 通用算法覆盖；
        // 那条路径才是 agent 从 journey 中精确定位被截断信息的主战场，
        // substring 通用搜索在 lifecycle 上不公开。
        if query
            .path
            .as_deref()
            .map(|path| path.trim_matches('/').starts_with("skills"))
            .unwrap_or(false)
        {
            return search_projected_skill_files(self.skill_asset_repo.as_ref(), mount, query)
                .await;
        }
        Err(MountError::NotSupported(
            "lifecycle_vfs 仅在 skills 子树支持通用 substring search_text；\
             virtual projection 请用 fs_grep（走 grep_text 路径）"
                .to_string(),
        ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{
        ExecutionStatus, MemorySessionPersistence, SessionBootstrapState, SessionEventStore,
        SessionMeta, SessionMetaStore, SessionProjectionStore, TitleSource,
    };
    use agentdash_agent_protocol::codex_app_server_protocol as codex;
    use agentdash_agent_protocol::{
        BackboneEnvelope, BackboneEvent, ItemCompletedNotification, ItemStartedNotification,
        PlatformEvent, SourceInfo, TraceInfo,
    };
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind};
    use agentdash_domain::skill_asset::{SkillAsset, SkillAssetRepository};
    use agentdash_domain::workflow::{LifecycleStepDefinition, LifecycleStepExecutionStatus};
    use agentdash_spi::{
        NewCompactionProjectionCommit, SESSION_PROJECTION_KIND_MODEL_CONTEXT,
        SessionCompactionRecord, SessionCompactionStatus, SessionProjectionHeadRecord,
        SessionProjectionSegmentRecord,
    };
    use std::sync::Mutex;

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

    #[derive(Default)]
    struct EmptySkillAssetRepo;

    #[async_trait::async_trait]
    impl SkillAssetRepository for EmptySkillAssetRepo {
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
            tab_layout: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Plain,
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
            namespace: None,
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
            plugin_id: None,
            tool: "lookup".to_string(),
            status: codex::McpToolCallStatus::Completed,
            arguments: serde_json::json!({ "query": "lifecycle" }),
            mcp_app_resource_uri: None,
            result: Some(Box::new(codex::McpToolCallResult {
                content: vec![serde_json::json!({ "type": "text", "text": "mcp result" })],
                structured_content: Some(serde_json::json!({ "answer": 42 })),
                meta: None,
            })),
            error: None,
            duration_ms: Some(7),
        }
    }

    async fn fixture() -> (LifecycleMountProvider, Mount, MemorySessionPersistence) {
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
                    BackboneEvent::ItemStarted(ItemStartedNotification::new(
                        dynamic_tool_item(
                            "tool-1",
                            "read_file",
                            codex::DynamicToolCallStatus::InProgress,
                            None,
                        ),
                        session_id.to_string(),
                        "t-1".to_string(),
                    )),
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
                    BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                        dynamic_tool_item(
                            "tool-1",
                            "read_file",
                            codex::DynamicToolCallStatus::Completed,
                            Some("file contents"),
                        ),
                        session_id.to_string(),
                        "t-1".to_string(),
                    )),
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
                    BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                        dynamic_tool_item(
                            "patch-1",
                            "fs_apply_patch",
                            codex::DynamicToolCallStatus::Completed,
                            Some("patched"),
                        ),
                        session_id.to_string(),
                        "t-1".to_string(),
                    )),
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
                    BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                        mcp_tool_item("mcp-1"),
                        session_id.to_string(),
                        "t-1".to_string(),
                    )),
                ),
            )
            .await
            .expect("append mcp");

        persistence
            .commit_compaction_projection(
                session_id,
                NewCompactionProjectionCommit {
                    completed_event: envelope(
                        session_id,
                        "t-1",
                        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                            key: "context_compacted".to_string(),
                            value: serde_json::json!({
                                "lifecycle_item_id": "compact-item-1",
                                "summary": "保留原文回看索引的压缩摘要",
                                "compacted_until_ref": { "turn_id": "t-1", "entry_index": 2 },
                                "first_kept_ref": null,
                            }),
                        }),
                    ),
                    compaction: SessionCompactionRecord {
                        id: "compact-1".to_string(),
                        session_id: session_id.to_string(),
                        projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                        projection_version: 1,
                        lifecycle_item_id: "compact-item-1".to_string(),
                        start_event_seq: 1,
                        completed_event_seq: Some(5),
                        failed_event_seq: None,
                        status: SessionCompactionStatus::ProjectionCommitted,
                        trigger: "unit_test".to_string(),
                        reason: None,
                        phase: None,
                        strategy: "summarize".to_string(),
                        budget_scope: None,
                        base_head_event_seq: None,
                        source_start_event_seq: Some(1),
                        source_end_event_seq: Some(2),
                        first_kept_event_seq: None,
                        summary: "保留原文回看索引的压缩摘要".to_string(),
                        replacement_projection_json: serde_json::json!({}),
                        token_stats_json: serde_json::json!({}),
                        diagnostics_json: serde_json::json!({
                            "summary_format": "markdown_with_recall_index_v1"
                        }),
                        created_by: None,
                        created_at_ms: 1,
                        completed_at_ms: Some(2),
                    },
                    segments: vec![SessionProjectionSegmentRecord {
                        id: "segment-1".to_string(),
                        session_id: session_id.to_string(),
                        projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                        projection_version: 1,
                        sort_order: 0,
                        segment_type: "summary_chunk".to_string(),
                        origin: "projection".to_string(),
                        synthetic: true,
                        source_start_event_seq: Some(1),
                        source_end_event_seq: Some(2),
                        source_refs_json: serde_json::json!({}),
                        generated_by_compaction_id: Some("compact-1".to_string()),
                        content_json: serde_json::json!({
                            "content": "保留原文回看索引的压缩摘要"
                        }),
                        token_estimate: Some(12),
                        created_at_ms: 1,
                    }],
                    head: SessionProjectionHeadRecord {
                        session_id: session_id.to_string(),
                        projection_kind: SESSION_PROJECTION_KIND_MODEL_CONTEXT.to_string(),
                        projection_version: 1,
                        head_event_seq: 5,
                        active_compaction_id: Some("compact-1".to_string()),
                        updated_by_event_seq: Some(5),
                        updated_at_ms: 2,
                    },
                },
            )
            .await
            .expect("commit compaction");

        let mount = crate::vfs::build_lifecycle_mount_with_ports(
            run.id,
            "test-lifecycle",
            &["report".into()],
        );
        let provider = LifecycleMountProvider::new(
            run_repo,
            inline_repo.clone(),
            Arc::new(EmptySkillAssetRepo),
            Arc::new(persistence.clone()),
        );
        (provider, mount, persistence)
    }

    #[tokio::test]
    async fn lifecycle_vfs_projects_current_node_session_items_and_tools() {
        let (provider, mount, _persistence) = fixture().await;
        let ctx = MountOperationContext::default();

        let item_index = provider
            .read_text(&mount, "session/items", &ctx)
            .await
            .expect("item index");
        assert!(item_index.content.contains("\"item_id\": \"tool-1\""));
        assert!(item_index.content.contains("\"item_kind\": \"tool\""));

        let tool_entries = provider
            .list(
                &mount,
                &ListOptions {
                    path: "session/tools".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &ctx,
            )
            .await
            .expect("list tools")
            .entries;
        let tool_path = tool_entries
            .iter()
            .find(|entry| entry.path.contains("tool-1") && entry.path.contains("read_file"))
            .map(|entry| entry.path.clone())
            .expect("tool-1 file");

        let tool_index = provider
            .read_text(&mount, "session/tools", &ctx)
            .await
            .expect("tool index");
        assert!(tool_index.content.contains(&tool_path));
        assert!(tool_index.content.contains("\"item_id\": \"mcp-1\""));

        let tool_item = provider
            .read_text(&mount, &tool_path, &ctx)
            .await
            .expect("tool item");
        assert!(tool_item.content.contains("\"path\": \"src/lib.rs\""));
        assert!(tool_item.content.contains("file contents"));

        let write_entries = provider
            .list(
                &mount,
                &ListOptions {
                    path: "session/writes".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &ctx,
            )
            .await
            .expect("list writes")
            .entries;
        let write_path = write_entries
            .iter()
            .find(|entry| entry.path.contains("patch-1") && entry.path.contains("fs_apply_patch"))
            .map(|entry| entry.path.clone())
            .expect("patch write file");

        let writes = provider
            .read_text(&mount, "session/writes", &ctx)
            .await
            .expect("writes");
        assert!(writes.content.contains(&write_path));

        let summary_entries = provider
            .list(
                &mount,
                &ListOptions {
                    path: "session/summaries".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &ctx,
            )
            .await
            .expect("list summaries")
            .entries;
        let summary_path = summary_entries
            .iter()
            .find(|entry| entry.path.contains("compact-1"))
            .map(|entry| entry.path.clone())
            .expect("compaction summary file");
        let summary = provider
            .read_text(&mount, &summary_path, &ctx)
            .await
            .expect("summary file");
        assert!(summary.content.contains("保留原文回看索引"));

        let node_tools = provider
            .list(
                &mount,
                &ListOptions {
                    path: "nodes/analyze/session/tools".to_string(),
                    pattern: None,
                    recursive: false,
                },
                &ctx,
            )
            .await
            .expect("list node tools")
            .entries;
        assert!(node_tools.iter().any(|entry| entry.path.contains("tool-1")));

        let removed_tool_calls = provider.read_text(&mount, "tool-calls", &ctx).await;
        assert!(matches!(removed_tool_calls, Err(MountError::NotFound(_))));
    }

    #[tokio::test]
    async fn lifecycle_vfs_records_and_artifacts_are_writable_by_path_rules() {
        let (provider, mount, _persistence) = fixture().await;
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
            .read_text(&mount, "active/artifacts/report", &ctx)
            .await
            .expect("read artifact through active alias");
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
        let (provider, mount, _persistence) = fixture().await;
        let mut registry = crate::vfs::MountProviderRegistry::new();
        registry.register(Arc::new(provider));
        let service = crate::vfs::VfsService::new(Arc::new(registry));
        let vfs = agentdash_spi::Vfs {
            mounts: vec![mount],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let target = crate::vfs::parse_mount_uri(
            "lifecycle://session/tools/0001__tool-1__read_file__src_lib_rs.json",
            &vfs,
        )
        .expect("URI should parse");

        let read = service
            .read_text(&vfs, &target, None, None)
            .await
            .expect("standard VFS service should read lifecycle URI");

        assert!(read.content.contains("file contents"));
    }
}
