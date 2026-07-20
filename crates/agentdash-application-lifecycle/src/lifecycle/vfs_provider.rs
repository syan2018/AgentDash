use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeItem, ManagedRuntimeItemBody, ManagedRuntimePresentationContentBlock,
};
use agentdash_application_vfs::{
    ListOptions, ListResult, MountError, MountOperationContext, MountProvider,
    PROVIDER_LIFECYCLE_VFS, ReadResult, RuntimeFileEntry, SearchMatch, SearchQuery, SearchResult,
    lifecycle_mount_has_skill_asset_projection, list_lifecycle_skill_asset_projection,
    normalize_mount_relative_path, read_lifecycle_skill_asset_projection,
    search_lifecycle_skill_asset_projection,
};
use agentdash_domain::{
    agent_run_target::AgentRunTarget,
    common::Mount,
    inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository},
    skill_asset::SkillAssetRepository,
    workflow::{
        LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository, OrchestrationInstance,
        RuntimeNodeState,
    },
};
use async_trait::async_trait;
use uuid::Uuid;

use super::{
    execution_log::{
        RuntimeNodeArtifactScope, encode_node_path_segment, load_scoped_port_output_map,
    },
    history_projection::{LifecycleHistoryProjection, LifecycleHistoryQueryPort},
    vfs_catalog::lifecycle_root_entries,
};

pub struct LifecycleMountProvider {
    lifecycle_runs: Arc<dyn LifecycleRunRepository>,
    lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
    inline_files: Arc<dyn InlineFileRepository>,
    skill_assets: Arc<dyn SkillAssetRepository>,
    history: Arc<dyn LifecycleHistoryQueryPort>,
}

impl LifecycleMountProvider {
    pub fn new(
        lifecycle_runs: Arc<dyn LifecycleRunRepository>,
        lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
        inline_files: Arc<dyn InlineFileRepository>,
        skill_assets: Arc<dyn SkillAssetRepository>,
        history: Arc<dyn LifecycleHistoryQueryPort>,
    ) -> Self {
        Self {
            lifecycle_runs,
            lifecycle_agents,
            inline_files,
            skill_assets,
            history,
        }
    }

    async fn load_run(&self, mount: &Mount) -> Result<LifecycleRun, MountError> {
        let run_id = uuid_metadata(mount, "run_id")?;
        self.lifecycle_runs
            .get_by_id(run_id)
            .await
            .map_err(domain_error)?
            .ok_or_else(|| MountError::NotFound(format!("LifecycleRun 不存在: {run_id}")))
    }

    async fn load_history(
        &self,
        target: &AgentRunTarget,
    ) -> Result<LifecycleHistoryProjection, MountError> {
        self.history
            .load(target)
            .await
            .map_err(|error| MountError::OperationFailed(error.to_string()))
    }

    async fn mount_history(&self, mount: &Mount) -> Result<LifecycleHistoryProjection, MountError> {
        self.load_history(&AgentRunTarget {
            run_id: uuid_metadata(mount, "run_id")?,
            agent_id: uuid_metadata(mount, "agent_id")?,
        })
        .await
    }

    async fn history_for_agent(
        &self,
        mount: &Mount,
        agent_id: &str,
    ) -> Result<LifecycleHistoryProjection, MountError> {
        let run_id = uuid_metadata(mount, "run_id")?;
        let agent_id = Uuid::parse_str(agent_id)
            .map_err(|_| MountError::NotFound(format!("Lifecycle Agent 不存在: {agent_id}")))?;
        let belongs_to_run = self
            .lifecycle_agents
            .list_by_run(run_id)
            .await
            .map_err(domain_error)?
            .into_iter()
            .any(|agent| agent.id == agent_id);
        if !belongs_to_run {
            return Err(MountError::NotFound(format!(
                "Lifecycle Agent 不属于当前 run: {agent_id}"
            )));
        }
        self.load_history(&AgentRunTarget { run_id, agent_id })
            .await
    }

    async fn read_conversation(
        &self,
        projection: &LifecycleHistoryProjection,
        relative: &[&str],
    ) -> Result<String, MountError> {
        match relative {
            [] | ["meta"] => pretty_json(&serde_json::json!({
                "run_id": projection.target.run_id,
                "agent_id": projection.target.agent_id,
                "runtime_thread_id": projection.runtime_thread_id,
                "projection_revision": projection.projection_revision,
                "captured_at_ms": projection.captured_at_ms,
                "lifecycle": projection.lifecycle,
                "active_turn_id": projection.active_turn_id,
                "thread_name": projection.thread_name,
                "authority": projection.authority,
                "fidelity": projection.fidelity,
            })),
            ["summary"] => pretty_json(&serde_json::json!({
                "turns": projection.turns.len(),
                "items": projection.items.len(),
                "messages": projection.message_items().count(),
                "tools": projection.tool_items().count(),
                "compactions": projection.compaction_items().count(),
                "interactions": projection.interactions.len(),
                "active_turn_id": projection.active_turn_id,
            })),
            ["conclusions"] => Ok(last_agent_message(projection).unwrap_or_default()),
            ["events.json"] => pretty_json(&serde_json::json!({
                "target": projection.target,
                "runtime_thread_id": projection.runtime_thread_id,
                "projection_revision": projection.projection_revision,
                "records": projection.conversation_history,
            })),
            ["items", file] => {
                let item = find_item_file(projection.items.iter(), file, "json")?;
                pretty_json(item)
            }
            ["messages", file] => {
                let item = find_item_file(projection.message_items(), file, "md")?;
                Ok(render_message(item))
            }
            ["tools", file] => {
                let item = find_item_file(projection.tool_items(), file, "json")?;
                pretty_json(item)
            }
            ["writes", file] => {
                let item = find_item_file(projection.write_items(), file, "json")?;
                pretty_json(item)
            }
            ["summaries", file] => {
                let item = find_item_file(projection.compaction_items(), file, "md")?;
                Ok(render_compaction(item))
            }
            ["terminal"] => pretty_json(
                &projection
                    .terminal_control_items()
                    .collect::<Vec<&ManagedRuntimeItem>>(),
            ),
            ["turns", turn_id, "events.json"] => {
                let turn_id = projection
                    .turns
                    .iter()
                    .find(|turn| safe_segment(turn.id.as_str()) == *turn_id)
                    .map(|turn| &turn.id)
                    .ok_or_else(|| {
                        MountError::NotFound(format!("Runtime turn 不存在: {turn_id}"))
                    })?;
                let turn = projection
                    .turns
                    .iter()
                    .find(|turn| &turn.id == turn_id)
                    .expect("turn was resolved above");
                let items = projection.items_for_turn(turn_id).collect::<Vec<_>>();
                let interactions = projection
                    .interactions
                    .iter()
                    .filter(|interaction| &interaction.turn_id == turn_id)
                    .collect::<Vec<_>>();
                pretty_json(&serde_json::json!({
                    "turn": turn,
                    "items": items,
                    "interactions": interactions,
                }))
            }
            _ => Err(MountError::NotFound(format!(
                "Lifecycle conversation path 不存在: {}",
                relative.join("/")
            ))),
        }
    }

    async fn read_node_projection(
        &self,
        mount: &Mount,
        run: &LifecycleRun,
        segments: &[&str],
    ) -> Result<String, MountError> {
        let (orchestration, node, scope) = node_context(mount, run)?;
        match segments {
            ["node"] | ["node", "state"] => pretty_json(node),
            ["node", "artifacts"] => {
                let values = load_scoped_port_output_map(self.inline_files.as_ref(), &scope).await;
                pretty_json(&values)
            }
            ["node", "artifacts", port_key] => {
                let values = load_scoped_port_output_map(self.inline_files.as_ref(), &scope).await;
                values.get(*port_key).cloned().ok_or_else(|| {
                    MountError::NotFound(format!("node artifact 不存在: {port_key}"))
                })
            }
            ["node", "records"] => {
                let records = self.node_records(run.id, &scope.node_path).await?;
                pretty_json(&records)
            }
            ["node", "records", rest @ ..] => {
                let path = rest.join("/");
                self.node_records(run.id, &scope.node_path)
                    .await?
                    .into_iter()
                    .find_map(|(record_path, content)| (record_path == path).then_some(content))
                    .ok_or_else(|| MountError::NotFound(format!("node record 不存在: {path}")))
            }
            ["orchestration"] | ["orchestration", "state"] => pretty_json(orchestration),
            _ => Err(MountError::NotFound(format!(
                "Lifecycle node path 不存在: {}",
                segments.join("/")
            ))),
        }
    }

    async fn node_records(
        &self,
        run_id: Uuid,
        node_path: &str,
    ) -> Result<Vec<(String, String)>, MountError> {
        let prefix = format!("{}/", encode_node_path_segment(node_path));
        Ok(self
            .inline_files
            .list_files(InlineFileOwnerKind::LifecycleRun, run_id, "session_records")
            .await
            .map_err(domain_error)?
            .into_iter()
            .filter_map(|file| {
                let path = file.path.strip_prefix(&prefix)?.to_string();
                let content = file.into_text_content()?;
                Some((path, content))
            })
            .collect())
    }

    async fn all_entries(
        &self,
        mount: &Mount,
        base_path: &str,
        recursive: bool,
    ) -> Result<Vec<RuntimeFileEntry>, MountError> {
        if base_path == "skills" || base_path.starts_with("skills/") {
            return Ok(list_lifecycle_skill_asset_projection(
                self.skill_assets.as_ref(),
                mount,
                &ListOptions {
                    path: base_path.to_string(),
                    pattern: None,
                    recursive,
                },
            )
            .await?
            .entries);
        }

        let run = self.load_run(mount).await?;
        let mut entries = lifecycle_root_entries(lifecycle_mount_has_skill_asset_projection(mount));
        if mount.metadata.get("agent_id").is_none() {
            entries.retain(|entry| entry.path != "session");
        }
        entries.push(RuntimeFileEntry::file("execution-log").as_virtual());

        if mount.metadata.get("agent_id").is_some() {
            let history = self.mount_history(mount).await?;
            entries.extend(conversation_entries("session", &history));
        }

        let agents = self
            .lifecycle_agents
            .list_by_run(run.id)
            .await
            .map_err(domain_error)?;
        for agent in agents {
            let prefix = format!("agent-runs/{}", agent.id);
            entries.push(RuntimeFileEntry::dir(prefix.clone()).as_virtual());
            entries.push(RuntimeFileEntry::dir(format!("{prefix}/sessions")).as_virtual());
            if recursive
                || base_path == format!("{prefix}/sessions")
                || base_path.starts_with(&format!("{prefix}/sessions/"))
            {
                let history = self
                    .load_history(&AgentRunTarget {
                        run_id: run.id,
                        agent_id: agent.id,
                    })
                    .await?;
                entries.extend(conversation_entries(
                    &format!("{prefix}/sessions"),
                    &history,
                ));
            }
        }

        if mount_has_node_scope(mount) {
            entries.extend([
                RuntimeFileEntry::dir("node").as_virtual(),
                RuntimeFileEntry::file("node/state").as_virtual(),
                RuntimeFileEntry::dir("node/artifacts").as_virtual(),
                RuntimeFileEntry::dir("node/records").as_virtual(),
                RuntimeFileEntry::dir("orchestration").as_virtual(),
                RuntimeFileEntry::file("orchestration/state").as_virtual(),
            ]);
            let (_, _, scope) = node_context(mount, &run)?;
            entries.extend(
                load_scoped_port_output_map(self.inline_files.as_ref(), &scope)
                    .await
                    .into_keys()
                    .map(|key| {
                        RuntimeFileEntry::file(format!("node/artifacts/{key}")).as_virtual()
                    }),
            );
            entries.extend(
                self.node_records(run.id, &scope.node_path)
                    .await?
                    .into_iter()
                    .map(|(path, _)| {
                        RuntimeFileEntry::file(format!("node/records/{path}")).as_virtual()
                    }),
            );
        }
        Ok(entries)
    }
}

#[async_trait]
impl MountProvider for LifecycleMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_LIFECYCLE_VFS
    }

    fn display_name(&self) -> &str {
        "Lifecycle 执行记录"
    }

    fn root_ref_hint(&self) -> &str {
        "lifecycle://run/{run_id}/agent/{agent_id}"
    }

    fn supported_capabilities(&self) -> Vec<&str> {
        vec!["read", "list", "search"]
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path =
            normalize_mount_relative_path(path, true).map_err(MountError::OperationFailed)?;
        if path == "skills" || path.starts_with("skills/") {
            return read_lifecycle_skill_asset_projection(self.skill_assets.as_ref(), mount, &path)
                .await;
        }
        let segments = path_segments(&path);
        let run = self.load_run(mount).await?;
        let content = match segments.as_slice() {
            [] | ["state"] => {
                let history = if mount.metadata.get("agent_id").is_some() {
                    Some(self.mount_history(mount).await?)
                } else {
                    None
                };
                pretty_json(&serde_json::json!({
                    "run": &run,
                    "history": history,
                }))?
            }
            ["execution-log"] => pretty_json(&run.execution_log)?,
            ["session", rest @ ..] => {
                let projection = self.mount_history(mount).await?;
                self.read_conversation(&projection, rest).await?
            }
            ["agent-runs"] => {
                let agents = self
                    .lifecycle_agents
                    .list_by_run(run.id)
                    .await
                    .map_err(domain_error)?;
                pretty_json(&serde_json::json!({
                    "run_id": run.id,
                    "agents": agents,
                }))?
            }
            ["agent-runs", agent_id] => {
                let projection = self.history_for_agent(mount, agent_id).await?;
                pretty_json(&serde_json::json!({
                    "agent_id": projection.target.agent_id,
                    "runtime_thread_id": projection.runtime_thread_id,
                    "sessions_path": format!("agent-runs/{agent_id}/sessions"),
                }))?
            }
            ["agent-runs", agent_id, "sessions", rest @ ..] => {
                let projection = self.history_for_agent(mount, agent_id).await?;
                self.read_conversation(&projection, rest).await?
            }
            ["node", ..] | ["orchestration", ..] => {
                self.read_node_projection(mount, &run, &segments).await?
            }
            _ => {
                return Err(MountError::NotFound(format!(
                    "Lifecycle path 不存在: {path}"
                )));
            }
        };
        let revision = if mount.metadata.get("agent_id").is_some() {
            let history = self.mount_history(mount).await?;
            format!("runtime:{}", history.projection_revision.0)
        } else {
            format!("lifecycle:{}", run.revision)
        };
        Ok(ReadResult::new(path, content)
            .with_version_token(revision)
            .with_modified_at(run.updated_at.timestamp_millis()))
    }

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let path =
            normalize_mount_relative_path(path, false).map_err(MountError::OperationFailed)?;
        if mount
            .metadata
            .get("scope")
            .and_then(serde_json::Value::as_str)
            != Some("node_runtime")
            || !mount_has_node_scope(mount)
        {
            return Err(MountError::NotSupported(
                "Lifecycle canonical conversation/history projection 是只读视图".to_string(),
            ));
        }
        let run = self.load_run(mount).await?;
        let (_, _, scope) = node_context(mount, &run)?;
        let segments = path_segments(&path);
        let file = match segments.as_slice() {
            ["node", "artifacts", port_key] => {
                let allowed = mount
                    .metadata
                    .get("writable_port_keys")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str)
                    .any(|candidate| candidate == *port_key);
                if !allowed {
                    return Err(MountError::OperationFailed(format!(
                        "当前 node 未声明 output port `{port_key}`"
                    )));
                }
                InlineFile::new(
                    InlineFileOwnerKind::LifecycleRun,
                    run.id,
                    "port_outputs",
                    scope.port_ref(*port_key).inline_path(),
                    content,
                )
            }
            ["node", "records", rest @ ..] if !rest.is_empty() => InlineFile::new(
                InlineFileOwnerKind::LifecycleRun,
                run.id,
                "session_records",
                format!(
                    "{}/{}",
                    encode_node_path_segment(&scope.node_path),
                    rest.join("/")
                ),
                content,
            ),
            _ => {
                return Err(MountError::NotSupported(format!(
                    "Lifecycle node mount 不支持写入路径: {path}"
                )));
            }
        };
        self.inline_files
            .upsert_file(&file)
            .await
            .map_err(domain_error)
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let base_path = normalize_mount_relative_path(&options.path, true)
            .map_err(MountError::OperationFailed)?;
        let entries = self
            .all_entries(mount, &base_path, options.recursive)
            .await?;
        Ok(ListResult {
            entries: filter_entries(
                entries,
                &base_path,
                options.pattern.as_deref(),
                options.recursive,
            )?,
        })
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
                self.skill_assets.as_ref(),
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
        let max_results = query.max_results.unwrap_or(usize::MAX);
        let mut matches = Vec::new();
        for entry in listing.entries.into_iter().filter(|entry| !entry.is_dir) {
            let read = self.read_text(mount, &entry.path, ctx).await?;
            for (index, line) in read.content.lines().enumerate() {
                let haystack = if query.case_sensitive {
                    line.to_string()
                } else {
                    line.to_lowercase()
                };
                if !haystack.contains(&needle) {
                    continue;
                }
                matches.push(SearchMatch {
                    path: entry.path.clone(),
                    line: Some((index + 1) as u32),
                    content: line.trim().to_string(),
                });
                if matches.len() >= max_results {
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

fn pretty_json(value: &impl serde::Serialize) -> Result<String, MountError> {
    serde_json::to_string_pretty(value)
        .map_err(|error| MountError::OperationFailed(error.to_string()))
}

fn path_segments(path: &str) -> Vec<&str> {
    if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect()
    }
}

fn uuid_metadata(mount: &Mount, key: &str) -> Result<Uuid, MountError> {
    let value = mount
        .metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| MountError::OperationFailed(format!("mount metadata 缺少 {key}")))?;
    Uuid::parse_str(value)
        .map_err(|error| MountError::OperationFailed(format!("mount metadata {key} 无效: {error}")))
}

fn u32_metadata(mount: &Mount, key: &str) -> Result<u32, MountError> {
    mount
        .metadata
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| MountError::OperationFailed(format!("mount metadata 缺少或无效: {key}")))
}

fn string_metadata(mount: &Mount, key: &str) -> Result<String, MountError> {
    mount
        .metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| MountError::OperationFailed(format!("mount metadata 缺少 {key}")))
}

fn mount_has_node_scope(mount: &Mount) -> bool {
    mount.metadata.get("orchestration_id").is_some()
        && mount.metadata.get("node_path").is_some()
        && mount.metadata.get("attempt").is_some()
}

fn node_context<'a>(
    mount: &Mount,
    run: &'a LifecycleRun,
) -> Result<
    (
        &'a OrchestrationInstance,
        &'a RuntimeNodeState,
        RuntimeNodeArtifactScope,
    ),
    MountError,
> {
    let orchestration_id = uuid_metadata(mount, "orchestration_id")?;
    let node_path = string_metadata(mount, "node_path")?;
    let attempt = u32_metadata(mount, "attempt")?;
    let orchestration = run
        .orchestrations
        .iter()
        .find(|item| item.orchestration_id == orchestration_id)
        .ok_or_else(|| MountError::NotFound(format!("orchestration 不存在: {orchestration_id}")))?;
    let node = find_node(&orchestration.node_tree, &node_path, attempt).ok_or_else(|| {
        MountError::NotFound(format!("runtime node 不存在: {node_path}#{attempt}"))
    })?;
    Ok((
        orchestration,
        node,
        RuntimeNodeArtifactScope {
            run_id: run.id,
            orchestration_id,
            node_path,
            attempt,
        },
    ))
}

fn find_node<'a>(
    nodes: &'a [RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a RuntimeNodeState> {
    nodes.iter().find_map(|node| {
        if node.node_path == node_path && node.attempt == attempt {
            Some(node)
        } else {
            find_node(&node.children, node_path, attempt)
        }
    })
}

fn domain_error(error: agentdash_domain::DomainError) -> MountError {
    MountError::OperationFailed(error.to_string())
}

fn conversation_entries(
    prefix: &str,
    projection: &LifecycleHistoryProjection,
) -> Vec<RuntimeFileEntry> {
    let mut entries = vec![
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
    ];
    entries.extend(projection.items.iter().enumerate().map(|(index, item)| {
        RuntimeFileEntry::file(format!(
            "{prefix}/items/{}",
            item_file_name(index, item, "json")
        ))
        .as_virtual()
    }));
    entries.extend(projection.message_items().enumerate().map(|(index, item)| {
        RuntimeFileEntry::file(format!(
            "{prefix}/messages/{}",
            item_file_name(index, item, "md")
        ))
        .as_virtual()
    }));
    entries.extend(projection.tool_items().enumerate().map(|(index, item)| {
        RuntimeFileEntry::file(format!(
            "{prefix}/tools/{}",
            item_file_name(index, item, "json")
        ))
        .as_virtual()
    }));
    entries.extend(projection.write_items().enumerate().map(|(index, item)| {
        RuntimeFileEntry::file(format!(
            "{prefix}/writes/{}",
            item_file_name(index, item, "json")
        ))
        .as_virtual()
    }));
    entries.extend(
        projection
            .compaction_items()
            .enumerate()
            .map(|(index, item)| {
                RuntimeFileEntry::file(format!(
                    "{prefix}/summaries/{}",
                    item_file_name(index, item, "md")
                ))
                .as_virtual()
            }),
    );
    for turn in &projection.turns {
        let turn_path = format!("{prefix}/turns/{}", safe_segment(turn.id.as_str()));
        entries.push(RuntimeFileEntry::dir(turn_path.clone()).as_virtual());
        entries.push(RuntimeFileEntry::file(format!("{turn_path}/events.json")).as_virtual());
    }
    entries
}

fn item_file_name(index: usize, item: &ManagedRuntimeItem, extension: &str) -> String {
    format!(
        "{index:06}-{}.{}",
        safe_segment(item.id.as_str()),
        extension
    )
}

fn find_item_file<'a>(
    items: impl Iterator<Item = &'a ManagedRuntimeItem>,
    file: &str,
    extension: &str,
) -> Result<&'a ManagedRuntimeItem, MountError> {
    items
        .enumerate()
        .find_map(|(index, item)| (item_file_name(index, item, extension) == file).then_some(item))
        .ok_or_else(|| MountError::NotFound(format!("conversation item 不存在: {file}")))
}

fn safe_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn render_message(item: &ManagedRuntimeItem) -> String {
    match &item.presentation.body {
        ManagedRuntimeItemBody::UserMessage { content }
        | ManagedRuntimeItemBody::AgentMessage { content, .. } => render_blocks(content),
        ManagedRuntimeItemBody::HookPrompt {
            hook_point,
            content,
        } => format!("## Hook: {hook_point}\n\n{}", render_blocks(content)),
        _ => String::new(),
    }
}

fn last_agent_message(projection: &LifecycleHistoryProjection) -> Option<String> {
    projection.items.iter().rev().find_map(|item| {
        if matches!(
            item.presentation.body,
            ManagedRuntimeItemBody::AgentMessage { .. }
        ) {
            Some(render_message(item))
        } else {
            None
        }
    })
}

fn render_compaction(item: &ManagedRuntimeItem) -> String {
    match &item.presentation.body {
        ManagedRuntimeItemBody::ContextCompaction {
            summary: Some(summary),
            ..
        } => render_blocks(summary),
        ManagedRuntimeItemBody::ContextCompaction {
            summary: None,
            source_digest,
        } => source_digest
            .as_ref()
            .map(|digest| format!("Compaction source: `{digest}`"))
            .unwrap_or_else(|| "Compaction summary is not available.".to_string()),
        _ => String::new(),
    }
}

fn render_blocks(blocks: &[ManagedRuntimePresentationContentBlock]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            ManagedRuntimePresentationContentBlock::Text { text } => text.clone(),
            ManagedRuntimePresentationContentBlock::Image {
                media_type, source, ..
            } => format!("![{media_type}]({source})"),
            ManagedRuntimePresentationContentBlock::LocalResource { path, .. } => {
                format!("[Local resource]({path})")
            }
            ManagedRuntimePresentationContentBlock::ResourceLink { uri, title, .. } => {
                format!("[{}]({uri})", title.as_deref().unwrap_or(uri))
            }
            ManagedRuntimePresentationContentBlock::SkillReference { name, path } => path
                .as_ref()
                .map(|path| format!("Skill `{name}`: `{path}`"))
                .unwrap_or_else(|| format!("Skill `{name}`")),
            ManagedRuntimePresentationContentBlock::Mention { label, reference } => {
                format!("@{label} ({reference})")
            }
            ManagedRuntimePresentationContentBlock::Structured {
                schema,
                schema_version,
                value,
            } => format!(
                "```json\n{}\n```\n\nSchema: `{schema}@{schema_version}`",
                serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
            ),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn filter_entries(
    entries: Vec<RuntimeFileEntry>,
    base_path: &str,
    pattern: Option<&str>,
    recursive: bool,
) -> Result<Vec<RuntimeFileEntry>, MountError> {
    let matcher = pattern
        .map(globset::Glob::new)
        .transpose()
        .map_err(|error| MountError::OperationFailed(format!("无效 glob: {error}")))?
        .map(|glob| glob.compile_matcher());
    let base_prefix = (!base_path.is_empty()).then(|| format!("{base_path}/"));
    Ok(entries
        .into_iter()
        .filter(|entry| {
            let relative = match &base_prefix {
                Some(prefix) => match entry.path.strip_prefix(prefix) {
                    Some(relative) if !relative.is_empty() => relative,
                    _ => return false,
                },
                None => entry.path.as_str(),
            };
            if !recursive && relative.contains('/') {
                return false;
            }
            matcher.as_ref().is_none_or(|matcher| {
                matcher.is_match(&entry.path)
                    || entry
                        .path
                        .rsplit('/')
                        .next()
                        .is_some_and(|name| matcher.is_match(name))
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_protocol::{
        BackboneEnvelope, BackboneEvent, CanonicalConversationPresentation,
        CanonicalConversationRecord, PresentationDurability, SourceInfo, UserInputSource,
        UserInputSubmissionKind, UserInputSubmittedNotification, text_user_input_blocks,
    };
    use agentdash_agent_runtime_contract::{
        ManagedRuntimeLifecycleStatus, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, RuntimeProjectionRevision, RuntimeThreadId,
    };
    use agentdash_domain::workflow::{AgentSource, LifecycleAgent};
    use agentdash_test_support::{
        inline_file::MemoryInlineFileRepository,
        skill::MemorySkillAssetRepository,
        workflow::{MemoryLifecycleAgentRepository, MemoryLifecycleRunRepository},
    };

    #[derive(Clone)]
    struct StaticHistoryQuery {
        projection: LifecycleHistoryProjection,
    }

    #[async_trait]
    impl LifecycleHistoryQueryPort for StaticHistoryQuery {
        async fn load(
            &self,
            target: &AgentRunTarget,
        ) -> Result<
            LifecycleHistoryProjection,
            super::super::history_projection::LifecycleHistoryQueryError,
        > {
            assert_eq!(target, &self.projection.target);
            Ok(self.projection.clone())
        }
    }

    #[test]
    fn safe_segment_preserves_stable_identity_without_creating_paths() {
        assert_eq!(safe_segment("turn/a:b"), "turn_a_b");
    }

    #[test]
    fn non_recursive_listing_keeps_only_direct_children() {
        let entries = vec![
            RuntimeFileEntry::dir("session/messages").as_virtual(),
            RuntimeFileEntry::file("session/messages/000-a.md").as_virtual(),
            RuntimeFileEntry::file("session/meta").as_virtual(),
        ];
        let filtered =
            filter_entries(entries, "session", None, false).expect("valid listing filter");
        assert_eq!(filtered.len(), 2);
        assert!(
            filtered
                .iter()
                .any(|entry| entry.path == "session/messages")
        );
        assert!(filtered.iter().any(|entry| entry.path == "session/meta"));
    }

    #[tokio::test]
    async fn events_json_reads_exact_canonical_history_without_a_journal_store() {
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain(project_id);
        let agent = LifecycleAgent::new_root(run.id, project_id, AgentSource::ProjectAgent);
        let target = AgentRunTarget {
            run_id: run.id,
            agent_id: agent.id,
        };
        let record = CanonicalConversationRecord::new(
            "native:history:1",
            CanonicalConversationPresentation::new(
                PresentationDurability::Durable,
                BackboneEnvelope::new(
                    BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                        "thread-1",
                        "turn-1",
                        "item-1",
                        UserInputSubmissionKind::Prompt,
                        UserInputSource::core_composer(),
                        text_user_input_blocks("hello"),
                    )),
                    "thread-1",
                    SourceInfo {
                        connector_id: "native".to_string(),
                        connector_type: "native".to_string(),
                        executor_id: None,
                    },
                ),
            ),
        );
        let expected_record =
            serde_json::to_value(&record).expect("serialize canonical history fixture");
        let projection = LifecycleHistoryProjection {
            target: target.clone(),
            runtime_thread_id: RuntimeThreadId::new("runtime-thread-1").expect("thread id"),
            projection_revision: RuntimeProjectionRevision(7),
            captured_at_ms: 17,
            lifecycle: ManagedRuntimeLifecycleStatus::Active,
            active_turn_id: None,
            thread_name: None,
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            turns: Vec::new(),
            items: Vec::new(),
            interactions: Vec::new(),
            conversation_history: vec![record],
        };

        let run_repo = Arc::new(MemoryLifecycleRunRepository::default());
        run_repo.create(&run).await.expect("seed run");
        let agent_repo = Arc::new(MemoryLifecycleAgentRepository::default());
        agent_repo.create(&agent).await.expect("seed agent");
        let provider = LifecycleMountProvider::new(
            run_repo,
            agent_repo,
            Arc::new(MemoryInlineFileRepository::default()),
            Arc::new(MemorySkillAssetRepository::default()),
            Arc::new(StaticHistoryQuery { projection }),
        );
        let mount = Mount {
            id: "lifecycle".to_string(),
            provider: PROVIDER_LIFECYCLE_VFS.to_string(),
            backend_id: String::new(),
            root_ref: format!(
                "lifecycle://run/{}/agent/{}/thread/runtime-thread-1",
                run.id, agent.id
            ),
            capabilities: Vec::new(),
            default_write: false,
            display_name: "Lifecycle".to_string(),
            metadata: serde_json::json!({
                "run_id": run.id,
                "agent_id": agent.id,
                "scope": "agent_run_history",
            }),
        };

        let read = provider
            .read_text(
                &mount,
                "session/events.json",
                &MountOperationContext::default(),
            )
            .await
            .expect("read canonical history");
        let value: serde_json::Value = serde_json::from_str(&read.content).expect("events JSON");
        assert_eq!(value["projection_revision"], "7");
        assert_eq!(value["records"][0], expected_record);
        assert_eq!(read.version_token.as_deref(), Some("runtime:7"));
    }
}
