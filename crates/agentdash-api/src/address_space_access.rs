/// Address Space 访问层 — Relay 传输实现与 Runtime 工具
///
/// 值类型、路径工具和 Mount 推导逻辑已迁移到 `agentdash_application::address_space`。
use std::sync::Arc;

use agent_client_protocol::{
    McpServer, SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate,
};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};
use agentdash_agent::tools::schema_value;
use agentdash_agent::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolUpdateCallback,
};
use agentdash_application::workflow::{
    AppendWorkflowPhaseArtifactsCommand, WorkflowRecordArtifactDraft, WorkflowRunService,
};
use agentdash_domain::session_binding::{
    SessionBinding, SessionBindingRepository, SessionOwnerType,
};
use agentdash_domain::workflow::{WorkflowDefinitionRepository, WorkflowRunRepository};
use agentdash_executor::{
    AgentDashExecutorConfig, CompanionSessionContext, ConnectorError, ExecutionAddressSpace,
    ExecutionContext, ExecutionMountCapability, ExecutorHub, HookEvaluationQuery,
    HookPendingAction, HookPendingActionResolutionKind, HookTraceEntry, HookTrigger,
    PromptSessionRequest, RuntimeToolProvider, SessionHookRefreshQuery,
    build_hook_trace_notification,
};
use agentdash_relay::{
    RelayMessage, ToolFileListPayload, ToolFileReadPayload, ToolFileWritePayload,
    ToolSearchPayload, ToolShellExecPayload,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub use agentdash_application::address_space::*;

use crate::relay::registry::BackendRegistry;

const MAX_SEARCH_FILE_BYTES: u64 = 256 * 1024;

// ─── Inline Content Persistence ─────────────────────────────

/// 内联文件写入持久化接口。
/// 实现方负责将 inline_fs mount 的文件修改写回到对应的
/// Project/Story container 配置中。
#[async_trait]
pub trait InlineContentPersister: Send + Sync {
    /// 将文件内容持久化到归属的 container 定义。
    /// `source_project_id` / `source_story_id` 标识来源 owner，
    /// `container_id` 从 mount.root_ref 中解析（`context://inline/{id}`），
    /// `path` 为归一化后的文件路径。
    async fn persist_write(
        &self,
        source_project_id: &str,
        source_story_id: Option<&str>,
        container_id: &str,
        path: &str,
        content: &str,
    ) -> Result<(), String>;
}

/// Per-session 的内联文件写入覆盖层。
///
/// 设计目标：
/// - 同一 session 内 write 后立即可 read（write-through cache）
/// - 写入同时通过 `InlineContentPersister` 持久化到 DB
/// - 多个 Agent 工具共享同一个 overlay（`Arc<InlineContentOverlay>`）
pub struct InlineContentOverlay {
    overrides: tokio::sync::RwLock<std::collections::HashMap<(String, String), String>>,
    persister: Arc<dyn InlineContentPersister>,
}

impl InlineContentOverlay {
    pub fn new(persister: Arc<dyn InlineContentPersister>) -> Self {
        Self {
            overrides: Default::default(),
            persister,
        }
    }

    pub async fn read(&self, mount_id: &str, path: &str) -> Option<String> {
        self.overrides
            .read()
            .await
            .get(&(mount_id.to_string(), path.to_string()))
            .cloned()
    }

    pub async fn has_override(&self, mount_id: &str, path: &str) -> bool {
        self.overrides
            .read()
            .await
            .contains_key(&(mount_id.to_string(), path.to_string()))
    }

    /// 返回指定 mount 下所有被覆盖的文件（用于 list 时合并新增文件）
    pub async fn overridden_files(
        &self,
        mount_id: &str,
    ) -> std::collections::HashMap<String, String> {
        self.overrides
            .read()
            .await
            .iter()
            .filter(|((mid, _), _)| mid == mount_id)
            .map(|((_, path), content)| (path.clone(), content.clone()))
            .collect()
    }

    pub async fn write(
        &self,
        address_space: &ExecutionAddressSpace,
        mount: &agentdash_executor::ExecutionMount,
        path: &str,
        content: &str,
    ) -> Result<(), String> {
        let container_id = mount
            .root_ref
            .strip_prefix("context://inline/")
            .ok_or_else(|| format!("无法从 root_ref 解析 container_id: {}", mount.root_ref))?;

        let project_id = address_space
            .source_project_id
            .as_deref()
            .ok_or("address space 缺少 source_project_id，无法持久化 inline 写入")?;

        // 1. 写入本地覆盖缓存（立即可读）
        self.overrides
            .write()
            .await
            .insert((mount.id.clone(), path.to_string()), content.to_string());

        // 2. 持久化到 DB
        self.persister
            .persist_write(
                project_id,
                address_space.source_story_id.as_deref(),
                container_id,
                path,
                content,
            )
            .await
    }
}

// ─── DB Inline Content Persister ────────────────────────────

/// 基于 Project / Story Repository 的 InlineContentPersister 实现。
///
/// 将 inline_fs 的文件写入持久化到对应的 ContextContainerDefinition
/// (project.config.context_containers 或 story.context.context_containers)。
pub struct DbInlineContentPersister {
    project_repo: Arc<dyn agentdash_domain::project::ProjectRepository>,
    story_repo: Arc<dyn agentdash_domain::story::StoryRepository>,
}

impl DbInlineContentPersister {
    pub fn new(
        project_repo: Arc<dyn agentdash_domain::project::ProjectRepository>,
        story_repo: Arc<dyn agentdash_domain::story::StoryRepository>,
    ) -> Self {
        Self {
            project_repo,
            story_repo,
        }
    }

    fn upsert_inline_file(
        containers: &mut Vec<agentdash_domain::context_container::ContextContainerDefinition>,
        container_id: &str,
        path: &str,
        content: &str,
    ) -> Result<(), String> {
        let container = containers
            .iter_mut()
            .find(|c| c.id.trim() == container_id)
            .ok_or_else(|| format!("容器 {} 不存在", container_id))?;

        match &mut container.provider {
            agentdash_domain::context_container::ContextContainerProvider::InlineFiles {
                files,
            } => {
                if let Some(file) = files.iter_mut().find(|f| {
                    normalize_mount_relative_path(&f.path, false).unwrap_or_default() == path
                }) {
                    file.content = content.to_string();
                } else {
                    files.push(agentdash_domain::context_container::ContextContainerFile {
                        path: path.to_string(),
                        content: content.to_string(),
                    });
                }
                Ok(())
            }
            _ => Err(format!("容器 {} 不是 inline_files 类型", container_id)),
        }
    }
}

#[async_trait]
impl InlineContentPersister for DbInlineContentPersister {
    async fn persist_write(
        &self,
        source_project_id: &str,
        source_story_id: Option<&str>,
        container_id: &str,
        path: &str,
        content: &str,
    ) -> Result<(), String> {
        let project_uuid = uuid::Uuid::parse_str(source_project_id)
            .map_err(|e| format!("无效的 project_id: {e}"))?;

        // 优先尝试在 story 中查找（story 级 container 覆盖 project 级）
        if let Some(story_id_str) = source_story_id {
            let story_uuid =
                uuid::Uuid::parse_str(story_id_str).map_err(|e| format!("无效的 story_id: {e}"))?;
            if let Some(mut story) = self
                .story_repo
                .get_by_id(story_uuid)
                .await
                .map_err(|e| format!("加载 story 失败: {e}"))?
            {
                if story
                    .context
                    .context_containers
                    .iter()
                    .any(|c| c.id.trim() == container_id)
                {
                    Self::upsert_inline_file(
                        &mut story.context.context_containers,
                        container_id,
                        path,
                        content,
                    )?;
                    self.story_repo
                        .update(&story)
                        .await
                        .map_err(|e| format!("保存 story 失败: {e}"))?;
                    return Ok(());
                }
            }
        }

        // 回退到 project
        let mut project = self
            .project_repo
            .get_by_id(project_uuid)
            .await
            .map_err(|e| format!("加载 project 失败: {e}"))?
            .ok_or_else(|| format!("project {} 不存在", source_project_id))?;

        Self::upsert_inline_file(
            &mut project.config.context_containers,
            container_id,
            path,
            content,
        )?;
        self.project_repo
            .update(&project)
            .await
            .map_err(|e| format!("保存 project 失败: {e}"))?;

        Ok(())
    }
}

// ─── Service ────────────────────────────────────────────────

#[derive(Clone)]
pub struct RelayAddressSpaceService {
    backend_registry: Arc<BackendRegistry>,
}

impl RelayAddressSpaceService {
    pub fn new(backend_registry: Arc<BackendRegistry>) -> Self {
        Self { backend_registry }
    }

    pub fn session_for_workspace(
        &self,
        workspace: &agentdash_domain::workspace::Workspace,
    ) -> Result<ExecutionAddressSpace, String> {
        build_workspace_address_space(workspace)
    }

    pub fn build_task_address_space(
        &self,
        project: &agentdash_domain::project::Project,
        story: &agentdash_domain::story::Story,
        workspace: Option<&agentdash_domain::workspace::Workspace>,
        agent_type: Option<&str>,
    ) -> Result<ExecutionAddressSpace, String> {
        build_derived_address_space(
            project,
            Some(story),
            workspace,
            agent_type,
            SessionMountTarget::Task,
        )
    }

    pub fn build_project_address_space(
        &self,
        project: &agentdash_domain::project::Project,
        workspace: Option<&agentdash_domain::workspace::Workspace>,
        agent_type: Option<&str>,
    ) -> Result<ExecutionAddressSpace, String> {
        build_derived_address_space(
            project,
            None,
            workspace,
            agent_type,
            SessionMountTarget::Project,
        )
    }

    pub fn build_story_address_space(
        &self,
        project: &agentdash_domain::project::Project,
        story: &agentdash_domain::story::Story,
        workspace: Option<&agentdash_domain::workspace::Workspace>,
        agent_type: Option<&str>,
    ) -> Result<ExecutionAddressSpace, String> {
        build_derived_address_space(
            project,
            Some(story),
            workspace,
            agent_type,
            SessionMountTarget::Story,
        )
    }

    /// 预览模式：不指定 agent_type，生成最大可见范围的 address space
    pub fn build_preview_address_space(
        &self,
        project: &agentdash_domain::project::Project,
        story: Option<&agentdash_domain::story::Story>,
        workspace: Option<&agentdash_domain::workspace::Workspace>,
        target: SessionMountTarget,
    ) -> Result<ExecutionAddressSpace, String> {
        build_derived_address_space(project, story, workspace, None, target)
    }

    pub fn list_mounts(
        &self,
        address_space: &ExecutionAddressSpace,
    ) -> Vec<agentdash_executor::ExecutionMount> {
        address_space.mounts.clone()
    }

    pub async fn read_text(
        &self,
        address_space: &ExecutionAddressSpace,
        target: &ResourceRef,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<ReadResult, String> {
        let mount = resolve_mount(
            address_space,
            &target.mount_id,
            ExecutionMountCapability::Read,
        )?;
        let path = normalize_mount_relative_path(&target.path, false)?;
        if mount.provider == PROVIDER_INLINE_FS {
            // overlay 优先（session 内写入立即可读）
            if let Some(ov) = overlay {
                if let Some(content) = ov.read(&mount.id, &path).await {
                    return Ok(ReadResult { path, content });
                }
            }
            let files = inline_files_from_mount(mount)?;
            let content = files
                .get(&path)
                .cloned()
                .ok_or_else(|| format!("文件不存在: {}", path))?;
            return Ok(ReadResult { path, content });
        }
        let response = self
            .backend_registry
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileRead {
                    id: RelayMessage::new_id("addr-read"),
                    payload: ToolFileReadPayload {
                        call_id: RelayMessage::new_id("call"),
                        path: path.clone(),
                        workspace_root: mount.root_ref.clone(),
                    },
                },
            )
            .await
            .map_err(|error| format!("relay file_read 失败: {error}"))?;

        match response {
            RelayMessage::ResponseToolFileRead {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(ReadResult {
                path,
                content: payload.content,
            }),
            RelayMessage::ResponseToolFileRead {
                error: Some(error), ..
            } => Err(error.message),
            other => Err(format!("file_read 返回意外响应: {}", other.id())),
        }
    }

    pub async fn write_text(
        &self,
        address_space: &ExecutionAddressSpace,
        target: &ResourceRef,
        content: &str,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<(), String> {
        let mount = resolve_mount(
            address_space,
            &target.mount_id,
            ExecutionMountCapability::Write,
        )?;
        let path = normalize_mount_relative_path(&target.path, false)?;
        if mount.provider == PROVIDER_INLINE_FS {
            let ov = overlay.ok_or_else(|| {
                format!(
                    "mount `{}` 是内联容器，需要 InlineContentOverlay 才能写入",
                    mount.id
                )
            })?;
            return ov.write(address_space, mount, &path, content).await;
        }
        let response = self
            .backend_registry
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileWrite {
                    id: RelayMessage::new_id("addr-write"),
                    payload: ToolFileWritePayload {
                        call_id: RelayMessage::new_id("call"),
                        path,
                        content: content.to_string(),
                        workspace_root: mount.root_ref.clone(),
                    },
                },
            )
            .await
            .map_err(|error| format!("relay file_write 失败: {error}"))?;

        match response {
            RelayMessage::ResponseToolFileWrite { error: None, .. } => Ok(()),
            RelayMessage::ResponseToolFileWrite {
                error: Some(error), ..
            } => Err(error.message),
            other => Err(format!("file_write 返回意外响应: {}", other.id())),
        }
    }

    pub async fn list(
        &self,
        address_space: &ExecutionAddressSpace,
        mount_id: &str,
        options: ListOptions,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<ListResult, String> {
        let mount = resolve_mount(address_space, mount_id, ExecutionMountCapability::List)?;
        let path = normalize_mount_relative_path(&options.path, true)?;
        if mount.provider == PROVIDER_INLINE_FS {
            let mut files = inline_files_from_mount(mount)?;
            // 合并 overlay 中的新增/修改文件
            if let Some(ov) = overlay {
                for (p, c) in ov.overridden_files(&mount.id).await {
                    files.insert(p, c);
                }
            }
            return Ok(ListResult {
                entries: list_inline_entries(
                    &files,
                    &path,
                    options.pattern.as_deref(),
                    options.recursive,
                ),
            });
        }
        let response = self
            .backend_registry
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolFileList {
                    id: RelayMessage::new_id("addr-list"),
                    payload: ToolFileListPayload {
                        call_id: RelayMessage::new_id("call"),
                        path,
                        workspace_root: mount.root_ref.clone(),
                        pattern: options.pattern,
                        recursive: options.recursive,
                    },
                },
            )
            .await
            .map_err(|error| format!("relay file_list 失败: {error}"))?;

        match response {
            RelayMessage::ResponseToolFileList {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(ListResult {
                entries: payload.entries,
            }),
            RelayMessage::ResponseToolFileList {
                error: Some(error), ..
            } => Err(error.message),
            other => Err(format!("file_list 返回意外响应: {}", other.id())),
        }
    }

    pub async fn exec(
        &self,
        address_space: &ExecutionAddressSpace,
        request: &ExecRequest,
    ) -> Result<ExecResult, String> {
        let mount = resolve_mount(
            address_space,
            &request.mount_id,
            ExecutionMountCapability::Exec,
        )?;
        let cwd = normalize_mount_relative_path(&request.cwd, true)?;
        if mount.provider == PROVIDER_INLINE_FS {
            return Err(format!("mount `{}` 不支持 exec", mount.id));
        }
        let response = self
            .backend_registry
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolShellExec {
                    id: RelayMessage::new_id("addr-exec"),
                    payload: ToolShellExecPayload {
                        call_id: RelayMessage::new_id("call"),
                        command: request.command.clone(),
                        workspace_root: join_root_ref(&mount.root_ref, &cwd),
                        timeout_ms: request.timeout_ms,
                    },
                },
            )
            .await
            .map_err(|error| format!("relay shell_exec 失败: {error}"))?;

        match response {
            RelayMessage::ResponseToolShellExec {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(ExecResult {
                exit_code: payload.exit_code,
                stdout: payload.stdout,
                stderr: payload.stderr,
            }),
            RelayMessage::ResponseToolShellExec {
                error: Some(error), ..
            } => Err(error.message),
            other => Err(format!("shell_exec 返回意外响应: {}", other.id())),
        }
    }

    pub async fn search_text(
        &self,
        address_space: &ExecutionAddressSpace,
        mount_id: &str,
        path: &str,
        query: &str,
        max_results: usize,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<Vec<String>, String> {
        self.search_text_extended(
            address_space,
            mount_id,
            path,
            query,
            false,
            None,
            max_results,
            0,
            overlay,
        )
        .await
        .map(|(hits, _truncated)| hits)
    }

    /// 扩展搜索接口 — 支持正则、glob 过滤、上下文行
    pub async fn search_text_extended(
        &self,
        address_space: &ExecutionAddressSpace,
        mount_id: &str,
        path: &str,
        query: &str,
        is_regex: bool,
        include_glob: Option<&str>,
        max_results: usize,
        context_lines: usize,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<(Vec<String>, bool), String> {
        let mount = resolve_mount(address_space, mount_id, ExecutionMountCapability::Search)?;
        let base_path = normalize_mount_relative_path(path, true)?;

        if mount.provider == PROVIDER_INLINE_FS {
            return self
                .search_inline(mount, &base_path, query, is_regex, max_results, context_lines, overlay)
                .await;
        }

        let response = self
            .backend_registry
            .send_command(
                &mount.backend_id,
                RelayMessage::CommandToolSearch {
                    id: RelayMessage::new_id("addr-search"),
                    payload: ToolSearchPayload {
                        call_id: RelayMessage::new_id("call"),
                        workspace_root: join_root_ref(&mount.root_ref, &base_path),
                        query: query.to_string(),
                        path: None,
                        is_regex,
                        include_glob: include_glob.map(String::from),
                        max_results,
                        context_lines,
                    },
                },
            )
            .await
            .map_err(|error| format!("relay search 失败: {error}"))?;

        match response {
            RelayMessage::ResponseToolSearch {
                payload: Some(payload),
                error: None,
                ..
            } => {
                let hits: Vec<String> = payload
                    .hits
                    .iter()
                    .map(|hit| {
                        let mut line = format!("{}:{}: {}", hit.path, hit.line_number, hit.content);
                        if context_lines > 0 {
                            if !hit.context_before.is_empty() {
                                let before = hit
                                    .context_before
                                    .iter()
                                    .enumerate()
                                    .map(|(i, c)| {
                                        format!(
                                            "{}:{}- {}",
                                            hit.path,
                                            hit.line_number - hit.context_before.len() + i,
                                            c
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                line = format!("{}\n{}", before, line);
                            }
                            if !hit.context_after.is_empty() {
                                let after = hit
                                    .context_after
                                    .iter()
                                    .enumerate()
                                    .map(|(i, c)| {
                                        format!("{}:{}- {}", hit.path, hit.line_number + 1 + i, c)
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                line = format!("{}\n{}", line, after);
                            }
                        }
                        line
                    })
                    .collect();
                Ok((hits, payload.truncated))
            }
            RelayMessage::ResponseToolSearch {
                error: Some(error), ..
            } => Err(error.message),
            other => Err(format!("search 返回意外响应: {}", other.id())),
        }
    }

    async fn search_inline(
        &self,
        mount: &agentdash_executor::ExecutionMount,
        base_path: &str,
        query: &str,
        is_regex: bool,
        max_results: usize,
        context_lines: usize,
        overlay: Option<&InlineContentOverlay>,
    ) -> Result<(Vec<String>, bool), String> {
        let mut files = inline_files_from_mount(mount)?;
        if let Some(ov) = overlay {
            for (p, c) in ov.overridden_files(&mount.id).await {
                files.insert(p, c);
            }
        }

        let re = if is_regex {
            Some(regex::Regex::new(query).map_err(|e| format!("无效正则: {e}"))?)
        } else {
            None
        };

        let mut hits = Vec::new();
        let mut truncated = false;

        for (file_path, content) in &files {
            if !file_path.starts_with(base_path.trim_start_matches("./").trim_start_matches('/'))
                && !base_path.is_empty()
                && base_path != "."
            {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            for (idx, line) in lines.iter().enumerate() {
                let matched = match &re {
                    Some(re) => re.is_match(line),
                    None => line.contains(query),
                };
                if matched {
                    let mut formatted = format!("{}:{}: {}", file_path, idx + 1, line.trim());
                    if context_lines > 0 {
                        let start = idx.saturating_sub(context_lines);
                        let end = (idx + 1 + context_lines).min(lines.len());
                        if start < idx {
                            let before: Vec<String> = (start..idx)
                                .map(|i| format!("{}:{}- {}", file_path, i + 1, lines[i].trim()))
                                .collect();
                            formatted = format!("{}\n{}", before.join("\n"), formatted);
                        }
                        if idx + 1 < end {
                            let after: Vec<String> = (idx + 1..end)
                                .map(|i| format!("{}:{}- {}", file_path, i + 1, lines[i].trim()))
                                .collect();
                            formatted = format!("{}\n{}", formatted, after.join("\n"));
                        }
                    }
                    hits.push(formatted);
                    if hits.len() >= max_results {
                        truncated = true;
                        return Ok((hits, truncated));
                    }
                }
            }
        }

        Ok((hits, truncated))
    }
}

// ─── Runtime Tool Provider ──────────────────────────────────

#[derive(Clone)]
pub struct RelayRuntimeToolProvider {
    service: Arc<RelayAddressSpaceService>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    workflow_run_repo: Arc<dyn WorkflowRunRepository>,
    executor_hub_handle: SharedExecutorHubHandle,
    inline_persister: Option<Arc<dyn InlineContentPersister>>,
}

impl RelayRuntimeToolProvider {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        workflow_run_repo: Arc<dyn WorkflowRunRepository>,
        executor_hub_handle: SharedExecutorHubHandle,
        inline_persister: Option<Arc<dyn InlineContentPersister>>,
    ) -> Self {
        Self {
            service,
            session_binding_repo,
            workflow_definition_repo,
            workflow_run_repo,
            executor_hub_handle,
            inline_persister,
        }
    }
}

#[derive(Clone, Default)]
pub struct SharedExecutorHubHandle {
    inner: Arc<RwLock<Option<ExecutorHub>>>,
}

impl SharedExecutorHubHandle {
    pub async fn set(&self, hub: ExecutorHub) {
        let mut guard = self.inner.write().await;
        *guard = Some(hub);
    }

    pub async fn get(&self) -> Option<ExecutorHub> {
        self.inner.read().await.clone()
    }
}

#[async_trait]
impl RuntimeToolProvider for RelayRuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError> {
        let address_space = context.address_space.clone().ok_or_else(|| {
            ConnectorError::InvalidConfig("缺少 address_space，无法构建统一访问工具".to_string())
        })?;

        let overlay: Option<Arc<InlineContentOverlay>> = self
            .inline_persister
            .as_ref()
            .map(|p| Arc::new(InlineContentOverlay::new(p.clone())));

        let mut tools: Vec<DynAgentTool> = vec![
            Arc::new(MountsListTool::new(
                self.service.clone(),
                address_space.clone(),
            )),
            Arc::new(FsReadTool::new(
                self.service.clone(),
                address_space.clone(),
                overlay.clone(),
            )),
            Arc::new(FsWriteTool::new(
                self.service.clone(),
                address_space.clone(),
                overlay.clone(),
            )),
            Arc::new(FsListTool::new(
                self.service.clone(),
                address_space.clone(),
                overlay.clone(),
            )),
            Arc::new(FsSearchTool::new(
                self.service.clone(),
                address_space.clone(),
                overlay.clone(),
            )),
            Arc::new(ShellExecTool::new(self.service.clone(), address_space)),
        ];

        let caps = &context.flow_capabilities;
        if caps.workflow_artifact {
            tools.push(Arc::new(WorkflowArtifactReportTool::new(
                self.workflow_definition_repo.clone(),
                self.workflow_run_repo.clone(),
                context,
            )));
        }
        if caps.companion_dispatch {
            tools.push(Arc::new(CompanionDispatchTool::new(
                self.session_binding_repo.clone(),
                self.executor_hub_handle.clone(),
                context,
            )));
        }
        if caps.companion_complete {
            tools.push(Arc::new(CompanionCompleteTool::new(
                self.executor_hub_handle.clone(),
                context,
            )));
        }
        if caps.resolve_hook_action {
            tools.push(Arc::new(ResolveHookActionTool::new(
                self.executor_hub_handle.clone(),
                context,
            )));
        }

        Ok(tools)
    }
}

fn ok_text(text: String) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(text)],
        is_error: false,
        details: None,
    }
}

// ─── Tool Implementations ───────────────────────────────────

#[derive(Clone)]
struct WorkflowArtifactReportTool {
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    workflow_run_repo: Arc<dyn WorkflowRunRepository>,
    current_session_id: Option<String>,
    current_turn_id: String,
    hook_session: Option<Arc<agentdash_executor::HookSessionRuntime>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WorkflowArtifactReportParams {
    pub content: String,
    pub artifact_type: Option<String>,
    pub title: Option<String>,
}

struct ActiveWorkflowLocator {
    run_id: Uuid,
    phase_key: String,
}

#[derive(Clone)]
struct CompanionDispatchTool {
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    executor_hub_handle: SharedExecutorHubHandle,
    current_session_id: Option<String>,
    current_turn_id: String,
    current_executor_config: AgentDashExecutorConfig,
    workspace_root: std::path::PathBuf,
    working_dir: String,
    address_space: Option<ExecutionAddressSpace>,
    mcp_servers: Vec<agent_client_protocol::McpServer>,
    hook_session: Option<Arc<agentdash_executor::HookSessionRuntime>>,
}

impl WorkflowArtifactReportTool {
    fn new(
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        workflow_run_repo: Arc<dyn WorkflowRunRepository>,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            workflow_definition_repo,
            workflow_run_repo,
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
            hook_session: context.hook_session.clone(),
        }
    }
}

impl CompanionDispatchTool {
    fn new(
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        executor_hub_handle: SharedExecutorHubHandle,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            session_binding_repo,
            executor_hub_handle,
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
            current_executor_config: context.executor_config.clone(),
            workspace_root: context.workspace_root.clone(),
            working_dir: relative_working_dir(context),
            address_space: context.address_space.clone(),
            mcp_servers: context.mcp_servers.clone(),
            hook_session: context.hook_session.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum CompanionSliceMode {
    #[default]
    Compact,
    Full,
    WorkflowOnly,
    ConstraintsOnly,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum CompanionAdoptionMode {
    #[default]
    Suggestion,
    FollowUpRequired,
    BlockingReview,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CompanionDispatchParams {
    pub prompt: String,
    pub companion_label: Option<String>,
    pub title: Option<String>,
    pub auto_create: Option<bool>,
    pub wait_for_completion: Option<bool>,
    pub slice_mode: Option<CompanionSliceMode>,
    pub adoption_mode: Option<CompanionAdoptionMode>,
    pub max_fragments: Option<usize>,
    pub max_constraints: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct CompanionDispatchSlice {
    mode: CompanionSliceMode,
    fragments: Vec<agentdash_executor::HookContextFragment>,
    constraints: Vec<agentdash_executor::HookConstraint>,
    inherited_fragment_labels: Vec<String>,
    inherited_constraint_keys: Vec<String>,
    omitted_fragment_count: usize,
    omitted_constraint_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct CompanionDispatchPlan {
    dispatch_id: String,
    companion_label: String,
    parent_session_id: String,
    parent_turn_id: String,
    adoption_mode: CompanionAdoptionMode,
    slice: CompanionDispatchSlice,
}

#[derive(Debug, Clone)]
struct CompanionExecutionSlice {
    address_space: Option<ExecutionAddressSpace>,
    mcp_servers: Vec<McpServer>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CompanionCompleteParams {
    pub summary: String,
    pub status: Option<String>,
    #[schemars(default)]
    pub findings: Vec<String>,
    #[schemars(default)]
    pub follow_ups: Vec<String>,
    #[schemars(default)]
    pub artifact_refs: Vec<String>,
}

#[derive(Clone)]
struct CompanionCompleteTool {
    executor_hub_handle: SharedExecutorHubHandle,
    current_session_id: Option<String>,
    current_turn_id: String,
}

impl CompanionCompleteTool {
    fn new(executor_hub_handle: SharedExecutorHubHandle, context: &ExecutionContext) -> Self {
        Self {
            executor_hub_handle,
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum HookActionResolutionMode {
    Adopted,
    Rejected,
    Completed,
    Superseded,
    UserDismissed,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ResolveHookActionParams {
    pub action_id: String,
    pub resolution_kind: HookActionResolutionMode,
    pub note: Option<String>,
}

#[derive(Clone)]
struct ResolveHookActionTool {
    current_session_id: Option<String>,
    current_turn_id: String,
    hook_session: Option<Arc<agentdash_executor::HookSessionRuntime>>,
    executor_hub_handle: SharedExecutorHubHandle,
}

impl ResolveHookActionTool {
    fn new(executor_hub_handle: SharedExecutorHubHandle, context: &ExecutionContext) -> Self {
        Self {
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
            hook_session: context.hook_session.clone(),
            executor_hub_handle,
        }
    }
}

#[async_trait]
impl AgentTool for WorkflowArtifactReportTool {
    fn name(&self) -> &str {
        "report_workflow_artifact"
    }

    fn description(&self) -> &str {
        "向当前 active workflow phase 追加结构化记录产物。支持 `phase_note` / `checklist_evidence` / `session_summary` / `journal_update` / `archive_suggestion`；当 phase 使用 checklist_passed 时，优先写入 `checklist_evidence`。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<WorkflowArtifactReportParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: WorkflowArtifactReportParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let content = params.content.trim();
        if content.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "content 不能为空".to_string(),
            ));
        }

        let hook_session = self.hook_session.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有 hook runtime，无法写入 workflow 记录产物".to_string(),
            )
        })?;
        let locator =
            active_workflow_locator_from_snapshot(&hook_session.snapshot()).ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "当前 session 没有关联 active workflow，无法写入 workflow 记录产物".to_string(),
                )
            })?;

        let artifact_type =
            normalize_workflow_record_artifact_type(params.artifact_type.as_deref())?;
        let title = params
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                active_workflow_default_artifact_title_from_snapshot(&hook_session.snapshot())
            })
            .unwrap_or_else(|| format!("{} 阶段记录", locator.phase_key));

        let service = WorkflowRunService::new(
            self.workflow_definition_repo.as_ref(),
            self.workflow_run_repo.as_ref(),
        );
        let run = service
            .append_phase_artifacts(AppendWorkflowPhaseArtifactsCommand {
                run_id: locator.run_id,
                phase_key: locator.phase_key.clone(),
                artifacts: vec![WorkflowRecordArtifactDraft {
                    artifact_type,
                    title: title.clone(),
                    content: content.to_string(),
                }],
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        hook_session
            .refresh(SessionHookRefreshQuery {
                session_id: hook_session.session_id().to_string(),
                turn_id: Some(self.current_turn_id.clone()),
                reason: Some("tool:report_workflow_artifact".to_string()),
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已写入 workflow 记录产物。\n- run_id: {}\n- phase_key: {}\n- artifact_type: {}\n- title: {}",
                run.id,
                locator.phase_key,
                workflow_record_artifact_type_key(artifact_type),
                title
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "session_id": self.current_session_id.clone(),
                "turn_id": self.current_turn_id.clone(),
                "run_id": run.id,
                "phase_key": locator.phase_key,
                "artifact_type": workflow_record_artifact_type_key(artifact_type),
                "title": title,
            })),
        })
    }
}

#[async_trait]
impl AgentTool for CompanionDispatchTool {
    fn name(&self) -> &str {
        "companion_dispatch"
    }

    fn description(&self) -> &str {
        "把一个子任务派发到当前 owner 关联的 companion/subagent session，并返回派发结果"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompanionDispatchParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let mut params: CompanionDispatchParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        if params.prompt.trim().is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "prompt 不能为空".to_string(),
            ));
        }
        if params.wait_for_completion.unwrap_or(false) {
            return Err(AgentToolError::InvalidArguments(
                "当前 companion_dispatch 仅支持异步派发，不支持 wait_for_completion=true"
                    .to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的 hook runtime，无法执行 companion dispatch".to_string(),
            )
        })?;
        let hook_session = self.hook_session.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前缺少 hook runtime，无法生成 companion dispatch 上下文".to_string(),
            )
        })?;
        let companion_label = params
            .companion_label
            .clone()
            .unwrap_or_else(|| "companion".to_string());
        let slice_mode = params.slice_mode.unwrap_or_default();
        let adoption_mode = params.adoption_mode.unwrap_or_default();

        let before_resolution = evaluate_subagent_hook(
            hook_session.as_ref(),
            HookTrigger::BeforeSubagentDispatch,
            Some(self.current_turn_id.clone()),
            &companion_label,
            Some(serde_json::json!({
                "prompt": params.prompt,
                "companion_label": companion_label,
                "auto_create": params.auto_create.unwrap_or(true),
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
            })),
        )
        .await
        .map_err(AgentToolError::ExecutionFailed)?;

        let executor_hub = self.executor_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "ExecutorHub 尚未完成初始化，无法执行 companion dispatch".to_string(),
            )
        })?;

        if let Some(reason) = before_resolution.block_reason.clone() {
            record_subagent_trace(
                hook_session.as_ref(),
                Some(&executor_hub),
                Some(self.current_turn_id.as_str()),
                HookTrigger::BeforeSubagentDispatch,
                "deny",
                &companion_label,
                &before_resolution,
            )
            .await;
            return Err(AgentToolError::ExecutionFailed(reason));
        }

        let dispatch_plan = build_companion_dispatch_plan(
            hook_session.as_ref(),
            &before_resolution,
            &current_session_id,
            &self.current_turn_id,
            &companion_label,
            slice_mode,
            adoption_mode,
            params.max_fragments,
            params.max_constraints,
        );
        record_subagent_trace(
            hook_session.as_ref(),
            Some(&executor_hub),
            Some(self.current_turn_id.as_str()),
            HookTrigger::BeforeSubagentDispatch,
            "allow",
            &companion_label,
            &before_resolution,
        )
        .await;

        let target_binding = self
            .resolve_or_create_companion_binding(
                hook_session.as_ref(),
                &companion_label,
                params.auto_create.unwrap_or(true),
                params.title.take(),
            )
            .await?;
        if target_binding.session_id == current_session_id {
            return Err(AgentToolError::ExecutionFailed(
                "当前会话已经是目标 companion session，暂不允许向自身再次派发 companion"
                    .to_string(),
            ));
        }

        let companion_context = CompanionSessionContext {
            dispatch_id: dispatch_plan.dispatch_id.clone(),
            parent_session_id: current_session_id.clone(),
            parent_turn_id: self.current_turn_id.clone(),
            companion_label: companion_label.clone(),
            slice_mode: companion_slice_mode_key(slice_mode).to_string(),
            adoption_mode: companion_adoption_mode_key(adoption_mode).to_string(),
            inherited_fragment_labels: dispatch_plan.slice.inherited_fragment_labels.clone(),
            inherited_constraint_keys: dispatch_plan.slice.inherited_constraint_keys.clone(),
        };
        let _ = executor_hub
            .update_session_meta(&target_binding.session_id, |meta| {
                meta.companion_context = Some(companion_context.clone());
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let final_prompt = build_companion_dispatch_prompt(&dispatch_plan, &params.prompt);
        let execution_slice = build_companion_execution_slice(
            self.address_space.as_ref(),
            &self.mcp_servers,
            slice_mode,
        );
        let turn_id = executor_hub
            .start_prompt_with_follow_up(
                &target_binding.session_id,
                None,
                PromptSessionRequest {
                    prompt: Some(final_prompt),
                    prompt_blocks: None,
                    working_dir: Some(self.working_dir.clone()),
                    env: std::collections::HashMap::new(),
                    executor_config: Some(self.current_executor_config.clone()),
                    mcp_servers: execution_slice.mcp_servers.clone(),
                    workspace_root: Some(self.workspace_root.clone()),
                    address_space: execution_slice.address_space.clone(),
                    flow_capabilities: None,
                    system_context: None,
                },
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let child_notification = build_companion_event_notification(
            &target_binding.session_id,
            &turn_id,
            "companion_dispatch_registered",
            format!("收到来自主 session 的 `{companion_label}` 派发任务"),
            serde_json::json!({
                "dispatch_id": dispatch_plan.dispatch_id,
                "parent_session_id": current_session_id,
                "parent_turn_id": self.current_turn_id,
                "companion_label": companion_label,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "inherited_fragment_labels": dispatch_plan.slice.inherited_fragment_labels,
                "inherited_constraint_keys": dispatch_plan.slice.inherited_constraint_keys,
                "inherited_mount_ids": execution_slice.address_space.as_ref().map(|space| {
                    space.mounts.iter().map(|mount| mount.id.clone()).collect::<Vec<_>>()
                }).unwrap_or_default(),
                "mcp_server_count": execution_slice.mcp_servers.len(),
            }),
        );
        let _ = executor_hub
            .inject_notification(&target_binding.session_id, child_notification)
            .await;

        let after_resolution = evaluate_subagent_hook(
            hook_session.as_ref(),
            HookTrigger::AfterSubagentDispatch,
            Some(self.current_turn_id.clone()),
            &companion_label,
            Some(serde_json::json!({
                "dispatch_id": dispatch_plan.dispatch_id,
                "companion_session_id": target_binding.session_id,
                "turn_id": turn_id,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "fragment_count": dispatch_plan.slice.fragments.len(),
                "constraint_count": dispatch_plan.slice.constraints.len(),
            })),
        )
        .await
        .map_err(AgentToolError::ExecutionFailed)?;
        record_subagent_trace(
            hook_session.as_ref(),
            Some(&executor_hub),
            Some(self.current_turn_id.as_str()),
            HookTrigger::AfterSubagentDispatch,
            "dispatched",
            &companion_label,
            &after_resolution,
        )
        .await;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已派发到 companion session。\n- label: {}\n- session_id: {}\n- turn_id: {}\n- slice_mode: {:?}\n- adoption_mode: {:?}\n- 当前为异步执行，可在对应会话中继续观察结果，并要求其通过 companion_complete 回传结果。",
                companion_label, target_binding.session_id, turn_id, slice_mode, adoption_mode
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "companion_label": companion_label,
                "companion_session_id": target_binding.session_id,
                "turn_id": turn_id,
                "dispatch_id": dispatch_plan.dispatch_id,
                "slice_mode": slice_mode,
                "adoption_mode": adoption_mode,
                "inherited_fragment_labels": dispatch_plan.slice.inherited_fragment_labels,
                "inherited_constraint_keys": dispatch_plan.slice.inherited_constraint_keys,
                "inherited_mount_ids": execution_slice.address_space.as_ref().map(|space| {
                    space.mounts.iter().map(|mount| mount.id.clone()).collect::<Vec<_>>()
                }).unwrap_or_default(),
                "mcp_server_count": execution_slice.mcp_servers.len(),
                "matched_rule_keys": after_resolution.matched_rule_keys,
            })),
        })
    }
}

impl CompanionDispatchTool {
    async fn resolve_or_create_companion_binding(
        &self,
        hook_session: &agentdash_executor::HookSessionRuntime,
        label: &str,
        auto_create: bool,
        title: Option<String>,
    ) -> Result<SessionBinding, AgentToolError> {
        let snapshot = hook_session.snapshot();
        let candidates = companion_owner_candidates(&snapshot)?;
        for (owner_type, owner_id, _) in &candidates {
            if let Some(binding) = self
                .session_binding_repo
                .find_by_owner_and_label(*owner_type, *owner_id, label)
                .await
                .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            {
                return Ok(binding);
            }
        }

        if !auto_create {
            return Err(AgentToolError::ExecutionFailed(format!(
                "当前 owner 还没有 label=`{label}` 的 companion session，且 auto_create=false"
            )));
        }

        let (owner_type, owner_id, owner_title) = candidates.first().cloned().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有关联 owner，无法创建 companion session".to_string(),
            )
        })?;
        let project_id = companion_project_id_for_owner(&snapshot, owner_type, owner_id)?;
        let executor_hub = self.executor_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "ExecutorHub 尚未完成初始化，无法创建 companion session".to_string(),
            )
        })?;
        let meta = executor_hub
            .create_session(
                title
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| owner_title.as_deref().unwrap_or("Companion Session")),
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        let binding =
            SessionBinding::new(project_id, meta.id, owner_type, owner_id, label.to_string());
        self.session_binding_repo
            .create(&binding)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        Ok(binding)
    }
}

#[async_trait]
impl AgentTool for CompanionCompleteTool {
    fn name(&self) -> &str {
        "companion_complete"
    }

    fn description(&self) -> &str {
        "把当前 companion session 的结构化结果回传给主 session，供主 Agent 采纳或继续推进"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompanionCompleteParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: CompanionCompleteParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        if params.summary.trim().is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "summary 不能为空".to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的上下文，无法回传 companion 结果".to_string(),
            )
        })?;
        let executor_hub = self.executor_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "ExecutorHub 尚未完成初始化，无法回传 companion 结果".to_string(),
            )
        })?;
        let session_meta = executor_hub
            .get_session_meta(&current_session_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "当前 session 不存在，无法回传 companion 结果".to_string(),
                )
            })?;
        let companion_context = session_meta.companion_context.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 不是通过 companion_dispatch 建立的上下文，无法使用 companion_complete".to_string(),
            )
        })?;

        let status = normalize_companion_result_status(params.status.as_deref())?;
        let payload = serde_json::json!({
            "dispatch_id": companion_context.dispatch_id,
            "companion_label": companion_context.companion_label,
            "companion_session_id": current_session_id,
            "companion_turn_id": self.current_turn_id,
            "parent_session_id": companion_context.parent_session_id,
            "parent_turn_id": companion_context.parent_turn_id,
            "slice_mode": companion_context.slice_mode,
            "adoption_mode": companion_context.adoption_mode,
            "status": status,
            "summary": params.summary.trim(),
            "findings": params.findings,
            "follow_ups": params.follow_ups,
            "artifact_refs": params.artifact_refs,
        });

        if let Some(parent_hook_session) = executor_hub
            .ensure_hook_session_runtime(
                &companion_context.parent_session_id,
                Some(companion_context.parent_turn_id.as_str()),
            )
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
        {
            let resolution = evaluate_subagent_hook(
                parent_hook_session.as_ref(),
                HookTrigger::SubagentResult,
                Some(companion_context.parent_turn_id.clone()),
                &companion_context.companion_label,
                Some(payload.clone()),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
            if let Some(action) = build_subagent_pending_action(
                &companion_context.parent_turn_id,
                &companion_context.companion_label,
                &payload,
                &resolution,
            ) {
                parent_hook_session.enqueue_pending_action(action);
            }
            record_subagent_trace(
                parent_hook_session.as_ref(),
                Some(&executor_hub),
                Some(companion_context.parent_turn_id.as_str()),
                HookTrigger::SubagentResult,
                "result_returned",
                &companion_context.companion_label,
                &resolution,
            )
            .await;
        }

        let parent_notification = build_companion_event_notification(
            &companion_context.parent_session_id,
            &companion_context.parent_turn_id,
            "companion_result_available",
            format!(
                "Companion `{}` 已回传结果，等待主 session 采纳",
                companion_context.companion_label
            ),
            payload.clone(),
        );
        executor_hub
            .inject_notification(&companion_context.parent_session_id, parent_notification)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let child_notification = build_companion_event_notification(
            &current_session_id,
            &self.current_turn_id,
            "companion_result_returned",
            "已将当前 companion 结果回传到主 session".to_string(),
            payload.clone(),
        );
        let _ = executor_hub
            .inject_notification(&current_session_id, child_notification)
            .await;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已把 companion 结果回传到主 session。\n- parent_session_id: {}\n- dispatch_id: {}\n- status: {}",
                companion_context.parent_session_id, companion_context.dispatch_id, status
            ))],
            is_error: false,
            details: Some(payload),
        })
    }
}

#[async_trait]
impl AgentTool for ResolveHookActionTool {
    fn name(&self) -> &str {
        "resolve_hook_action"
    }

    fn description(&self) -> &str {
        "把当前 session 中的 hook pending action 显式标记为 adopted/rejected/completed/superseded 等已结案状态"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<ResolveHookActionParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ResolveHookActionParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let action_id = params.action_id.trim();
        if action_id.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "action_id 不能为空".to_string(),
            ));
        }

        let current_session_id = self.current_session_id.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有可识别的 hook runtime，无法结案 hook action".to_string(),
            )
        })?;
        let hook_session = self.hook_session.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有 hook runtime，无法结案 hook action".to_string(),
            )
        })?;
        let resolution_kind = map_hook_action_resolution_kind(params.resolution_kind);
        let note = params
            .note
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let action = hook_session
            .resolve_pending_action(
                action_id,
                resolution_kind,
                note.clone(),
                Some(self.current_turn_id.clone()),
            )
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "当前 session 中不存在 action_id=`{action_id}` 的 hook action"
                ))
            })?;

        if let Some(executor_hub) = self.executor_hub_handle.get().await {
            let notification = build_hook_action_resolved_notification(
                &current_session_id,
                &self.current_turn_id,
                &action,
            );
            let _ = executor_hub
                .inject_notification(&current_session_id, notification)
                .await;
        }

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已更新 hook action 结案状态。\n- action_id: {}\n- status: {}\n- resolution_kind: {}",
                action.id,
                hook_action_status_key(action.status),
                action
                    .resolution_kind
                    .map(hook_action_resolution_key)
                    .unwrap_or("unknown")
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "session_id": current_session_id,
                "turn_id": self.current_turn_id,
                "action": action,
            })),
        })
    }
}

fn relative_working_dir(context: &ExecutionContext) -> String {
    context
        .working_directory
        .strip_prefix(&context.workspace_root)
        .ok()
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                ".".to_string()
            } else {
                relative.to_string_lossy().replace('\\', "/")
            }
        })
        .unwrap_or_else(|| ".".to_string())
}

async fn evaluate_subagent_hook(
    hook_session: &agentdash_executor::HookSessionRuntime,
    trigger: HookTrigger,
    turn_id: Option<String>,
    subagent_type: &str,
    payload: Option<serde_json::Value>,
) -> Result<agentdash_executor::HookResolution, String> {
    let resolution = hook_session
        .evaluate(HookEvaluationQuery {
            session_id: hook_session.session_id().to_string(),
            trigger: trigger.clone(),
            turn_id: turn_id.clone(),
            tool_name: None,
            tool_call_id: None,
            subagent_type: Some(subagent_type.to_string()),
            snapshot: Some(hook_session.snapshot()),
            payload,
        })
        .await
        .map_err(|error| error.to_string())?;

    if resolution.refresh_snapshot {
        hook_session
            .refresh(SessionHookRefreshQuery {
                session_id: hook_session.session_id().to_string(),
                turn_id,
                reason: Some(format!("trigger:{trigger:?}:{subagent_type}")),
            })
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(resolution)
}

async fn record_subagent_trace(
    hook_session: &agentdash_executor::HookSessionRuntime,
    executor_hub: Option<&ExecutorHub>,
    turn_id: Option<&str>,
    trigger: HookTrigger,
    decision: &str,
    subagent_type: &str,
    resolution: &agentdash_executor::HookResolution,
) {
    let trace = HookTraceEntry {
        sequence: hook_session.next_trace_sequence(),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        revision: hook_session.revision(),
        trigger,
        decision: decision.to_string(),
        tool_name: None,
        tool_call_id: None,
        subagent_type: Some(subagent_type.to_string()),
        matched_rule_keys: resolution.matched_rule_keys.clone(),
        refresh_snapshot: resolution.refresh_snapshot,
        block_reason: resolution.block_reason.clone(),
        completion: resolution.completion.clone(),
        diagnostics: resolution.diagnostics.clone(),
    };
    hook_session.append_trace(trace.clone());

    if let (Some(executor_hub), Some(turn_id)) = (executor_hub, turn_id) {
        if let Some(notification) = build_hook_trace_notification(
            hook_session.session_id(),
            Some(turn_id),
            hook_trace_source(),
            &trace,
        ) {
            let _ = executor_hub
                .inject_notification(hook_session.session_id(), notification)
                .await;
        }
    }
}

fn build_subagent_pending_action(
    parent_turn_id: &str,
    companion_label: &str,
    payload: &serde_json::Value,
    resolution: &agentdash_executor::HookResolution,
) -> Option<HookPendingAction> {
    if resolution.context_fragments.is_empty() && resolution.constraints.is_empty() {
        return None;
    }

    let adoption_mode = payload
        .get("adoption_mode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("suggestion")
        .trim()
        .to_string();
    if adoption_mode.is_empty() || adoption_mode == "suggestion" {
        return None;
    }

    let summary = payload
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Companion 已回流结果")
        .trim()
        .to_string();
    let status = payload
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("completed")
        .trim()
        .to_string();
    let dispatch_id = payload
        .get("dispatch_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");

    Some(HookPendingAction {
        id: format!("{adoption_mode}:{dispatch_id}:{parent_turn_id}"),
        created_at_ms: chrono::Utc::now().timestamp_millis(),
        title: if adoption_mode == "blocking_review" {
            format!("Companion `{companion_label}` 结果需要阻塞式 review")
        } else {
            format!("Companion `{companion_label}` 结果需要主 session 跟进")
        },
        summary: format!("status={status}, dispatch_id={dispatch_id}, summary={summary}"),
        action_type: adoption_mode,
        turn_id: Some(parent_turn_id.to_string()),
        source_trigger: HookTrigger::SubagentResult,
        status: agentdash_executor::HookPendingActionStatus::Pending,
        last_injected_at_ms: None,
        resolved_at_ms: None,
        resolution_kind: None,
        resolution_note: None,
        resolution_turn_id: None,
        context_fragments: resolution.context_fragments.clone(),
        constraints: resolution.constraints.clone(),
    })
}

fn hook_trace_source() -> AgentDashSourceV1 {
    let mut source = AgentDashSourceV1::new("pi-agent", "runtime_tool");
    source.executor_id = Some("PI_AGENT".to_string());
    source
}

fn active_workflow_locator_from_snapshot(
    snapshot: &agentdash_executor::SessionHookSnapshot,
) -> Option<ActiveWorkflowLocator> {
    let active_workflow = snapshot.metadata.as_ref()?.get("active_workflow")?;
    let run_id = active_workflow
        .get("run_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())?;
    let phase_key = active_workflow
        .get("phase_key")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    Some(ActiveWorkflowLocator { run_id, phase_key })
}

fn active_workflow_default_artifact_title_from_snapshot(
    snapshot: &agentdash_executor::SessionHookSnapshot,
) -> Option<String> {
    snapshot
        .metadata
        .as_ref()
        .and_then(|value| value.get("active_workflow"))
        .and_then(|value| value.get("default_artifact_title"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_workflow_record_artifact_type(
    value: Option<&str>,
) -> Result<agentdash_domain::workflow::WorkflowRecordArtifactType, AgentToolError> {
    match value.unwrap_or("phase_note").trim() {
        "" | "phase_note" => Ok(agentdash_domain::workflow::WorkflowRecordArtifactType::PhaseNote),
        "checklist_evidence" => {
            Ok(agentdash_domain::workflow::WorkflowRecordArtifactType::ChecklistEvidence)
        }
        "session_summary" => {
            Ok(agentdash_domain::workflow::WorkflowRecordArtifactType::SessionSummary)
        }
        "journal_update" => {
            Ok(agentdash_domain::workflow::WorkflowRecordArtifactType::JournalUpdate)
        }
        "archive_suggestion" => {
            Ok(agentdash_domain::workflow::WorkflowRecordArtifactType::ArchiveSuggestion)
        }
        other => Err(AgentToolError::InvalidArguments(format!(
            "artifact_type 不支持 `{other}`"
        ))),
    }
}

fn workflow_record_artifact_type_key(
    artifact_type: agentdash_domain::workflow::WorkflowRecordArtifactType,
) -> &'static str {
    match artifact_type {
        agentdash_domain::workflow::WorkflowRecordArtifactType::SessionSummary => "session_summary",
        agentdash_domain::workflow::WorkflowRecordArtifactType::JournalUpdate => "journal_update",
        agentdash_domain::workflow::WorkflowRecordArtifactType::ArchiveSuggestion => {
            "archive_suggestion"
        }
        agentdash_domain::workflow::WorkflowRecordArtifactType::PhaseNote => "phase_note",
        agentdash_domain::workflow::WorkflowRecordArtifactType::ChecklistEvidence => {
            "checklist_evidence"
        }
    }
}

fn build_companion_dispatch_prompt(plan: &CompanionDispatchPlan, user_prompt: &str) -> String {
    let mut sections = vec!["[Companion Dispatch Context]".to_string()];

    sections.push(format!(
        "## Dispatch Metadata\n- dispatch_id: {}\n- companion_label: {}\n- slice_mode: {:?}\n- adoption_mode: {:?}",
        plan.dispatch_id, plan.companion_label, plan.slice.mode, plan.adoption_mode
    ));

    if !plan.slice.fragments.is_empty() {
        sections.push(format!(
            "## 继承上下文\n{}",
            plan.slice
                .fragments
                .iter()
                .map(|fragment| format!("### {}\n{}", fragment.label, fragment.content.trim()))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }

    if !plan.slice.constraints.is_empty() {
        sections.push(format!(
            "## 继承约束\n{}",
            plan.slice
                .constraints
                .iter()
                .map(|constraint| format!("- {}", constraint.description))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    if plan.slice.omitted_fragment_count > 0 || plan.slice.omitted_constraint_count > 0 {
        sections.push(format!(
            "## 切片说明\n- omitted_fragments: {}\n- omitted_constraints: {}",
            plan.slice.omitted_fragment_count, plan.slice.omitted_constraint_count
        ));
    }

    sections.push(format!("## 派发任务\n{}", user_prompt.trim()));
    sections.push(
        "## 回流要求\n- 完成后请调用 `companion_complete`。\n- 必填 summary。\n- 如有关键发现请写入 findings。\n- 如需要主 session 后续行动请写入 follow_ups。".to_string(),
    );
    sections.join("\n\n")
}

fn build_companion_dispatch_plan(
    hook_session: &agentdash_executor::HookSessionRuntime,
    resolution: &agentdash_executor::HookResolution,
    parent_session_id: &str,
    parent_turn_id: &str,
    companion_label: &str,
    slice_mode: CompanionSliceMode,
    adoption_mode: CompanionAdoptionMode,
    max_fragments: Option<usize>,
    max_constraints: Option<usize>,
) -> CompanionDispatchPlan {
    let dispatch_id = format!("dispatch-{}", uuid::Uuid::new_v4().simple());
    let slice = build_companion_dispatch_slice(
        &hook_session.snapshot(),
        resolution,
        slice_mode,
        max_fragments.unwrap_or(3),
        max_constraints.unwrap_or(4),
    );
    CompanionDispatchPlan {
        dispatch_id,
        companion_label: companion_label.to_string(),
        parent_session_id: parent_session_id.to_string(),
        parent_turn_id: parent_turn_id.to_string(),
        adoption_mode,
        slice,
    }
}

fn build_companion_dispatch_slice(
    snapshot: &agentdash_executor::SessionHookSnapshot,
    resolution: &agentdash_executor::HookResolution,
    mode: CompanionSliceMode,
    max_fragments: usize,
    max_constraints: usize,
) -> CompanionDispatchSlice {
    let all_fragments = match mode {
        CompanionSliceMode::Full => resolution.context_fragments.clone(),
        CompanionSliceMode::WorkflowOnly => resolution
            .context_fragments
            .iter()
            .filter(|fragment| {
                fragment.slot == "workflow"
                    || fragment.label.contains("workflow")
                    || fragment
                        .source_refs
                        .iter()
                        .any(|source| source.layer == agentdash_executor::HookSourceLayer::Workflow)
            })
            .cloned()
            .collect(),
        CompanionSliceMode::ConstraintsOnly => Vec::new(),
        CompanionSliceMode::Compact => {
            let mut compact = Vec::new();
            if let Some(owner_summary) = build_companion_owner_summary(snapshot) {
                compact.push(agentdash_executor::HookContextFragment {
                    slot: "companion".to_string(),
                    label: "owner_summary".to_string(),
                    content: owner_summary,
                    source_summary: vec!["session:owner_summary".to_string()],
                    source_refs: Vec::new(),
                });
            }
            compact.extend(
                resolution
                    .context_fragments
                    .iter()
                    .filter(|fragment| {
                        fragment.slot == "workflow" || fragment.label.contains("workflow")
                    })
                    .take(1)
                    .cloned(),
            );
            compact.extend(
                resolution
                    .context_fragments
                    .iter()
                    .filter(|fragment| fragment.slot == "instruction_append")
                    .take(1)
                    .cloned(),
            );
            compact
        }
    };

    let all_constraints = match mode {
        CompanionSliceMode::ConstraintsOnly
        | CompanionSliceMode::Full
        | CompanionSliceMode::Compact => resolution.constraints.clone(),
        CompanionSliceMode::WorkflowOnly => resolution
            .constraints
            .iter()
            .filter(|constraint| {
                constraint
                    .source_refs
                    .iter()
                    .any(|source| source.layer == agentdash_executor::HookSourceLayer::Workflow)
            })
            .cloned()
            .collect(),
    };

    let fragments = all_fragments
        .iter()
        .take(max_fragments.max(1))
        .cloned()
        .collect::<Vec<_>>();
    let constraints = all_constraints
        .iter()
        .take(max_constraints.max(1))
        .cloned()
        .collect::<Vec<_>>();

    CompanionDispatchSlice {
        mode,
        inherited_fragment_labels: fragments
            .iter()
            .map(|fragment| fragment.label.clone())
            .collect(),
        inherited_constraint_keys: constraints
            .iter()
            .map(|constraint| constraint.key.clone())
            .collect(),
        omitted_fragment_count: all_fragments.len().saturating_sub(fragments.len()),
        omitted_constraint_count: all_constraints.len().saturating_sub(constraints.len()),
        fragments,
        constraints,
    }
}

fn build_companion_execution_slice(
    address_space: Option<&ExecutionAddressSpace>,
    mcp_servers: &[McpServer],
    mode: CompanionSliceMode,
) -> CompanionExecutionSlice {
    match mode {
        CompanionSliceMode::Full => CompanionExecutionSlice {
            address_space: address_space.cloned(),
            mcp_servers: mcp_servers.to_vec(),
        },
        CompanionSliceMode::Compact => CompanionExecutionSlice {
            address_space: Some(filter_address_space_capabilities(
                address_space,
                &[
                    ExecutionMountCapability::Read,
                    ExecutionMountCapability::List,
                    ExecutionMountCapability::Search,
                    ExecutionMountCapability::Exec,
                ],
            )),
            mcp_servers: Vec::new(),
        },
        CompanionSliceMode::WorkflowOnly | CompanionSliceMode::ConstraintsOnly => {
            CompanionExecutionSlice {
                // 显式传空 address_space，避免执行层回退到 full workspace builtin tools。
                address_space: Some(ExecutionAddressSpace::default()),
                mcp_servers: Vec::new(),
            }
        }
    }
}

fn filter_address_space_capabilities(
    address_space: Option<&ExecutionAddressSpace>,
    allowed: &[ExecutionMountCapability],
) -> ExecutionAddressSpace {
    let Some(address_space) = address_space else {
        return ExecutionAddressSpace::default();
    };

    let mounts = address_space
        .mounts
        .iter()
        .filter_map(|mount| {
            let capabilities = mount
                .capabilities
                .iter()
                .filter(|capability| allowed.contains(capability))
                .cloned()
                .collect::<Vec<_>>();
            if capabilities.is_empty() {
                return None;
            }

            let mut next_mount = mount.clone();
            next_mount.capabilities = capabilities;
            next_mount.default_write = next_mount
                .capabilities
                .contains(&ExecutionMountCapability::Write);
            Some(next_mount)
        })
        .collect::<Vec<_>>();

    let default_mount_id = address_space
        .default_mount_id
        .as_ref()
        .and_then(|default_id| {
            mounts
                .iter()
                .any(|mount| mount.id == *default_id)
                .then(|| default_id.clone())
        });

    ExecutionAddressSpace {
        mounts,
        default_mount_id,
        source_project_id: address_space.source_project_id.clone(),
        source_story_id: address_space.source_story_id.clone(),
    }
}

fn build_companion_owner_summary(
    snapshot: &agentdash_executor::SessionHookSnapshot,
) -> Option<String> {
    if snapshot.owners.is_empty() {
        return None;
    }
    Some(format!(
        "## 当前归属\n{}",
        snapshot
            .owners
            .iter()
            .map(|owner| format!(
                "- {}: {}",
                owner.owner_type,
                owner.label.as_deref().unwrap_or(owner.owner_id.as_str())
            ))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn build_companion_event_notification(
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: String,
    data: serde_json::Value,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new(event_type);
    event.severity = Some("info".to_string());
    event.message = Some(message);
    event.data = Some(data);

    let source = AgentDashSourceV1::new("agentdash-companion", "runtime_tool");
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));

    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new()
                .meta(merge_agentdash_meta(None, &agentdash).unwrap_or_default()),
        ),
    )
}

fn build_hook_action_resolved_notification(
    session_id: &str,
    turn_id: &str,
    action: &HookPendingAction,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new("hook_action_resolved");
    event.severity = Some("info".to_string());
    event.message = Some(format!("Hook action `{}` 已显式结案", action.title));
    event.data = Some(serde_json::json!({
        "action_id": action.id,
        "action_type": action.action_type,
        "status": hook_action_status_key(action.status),
        "resolution_kind": action.resolution_kind.map(hook_action_resolution_key),
        "resolution_note": action.resolution_note,
        "resolution_turn_id": action.resolution_turn_id,
        "resolved_at_ms": action.resolved_at_ms,
        "summary": action.summary,
        "title": action.title,
    }));

    let source = AgentDashSourceV1::new("agentdash-hook-runtime", "runtime_tool");
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));

    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new()
                .meta(merge_agentdash_meta(None, &agentdash).unwrap_or_default()),
        ),
    )
}

fn map_hook_action_resolution_kind(
    mode: HookActionResolutionMode,
) -> HookPendingActionResolutionKind {
    match mode {
        HookActionResolutionMode::Adopted => HookPendingActionResolutionKind::Adopted,
        HookActionResolutionMode::Rejected => HookPendingActionResolutionKind::Rejected,
        HookActionResolutionMode::Completed => HookPendingActionResolutionKind::Completed,
        HookActionResolutionMode::Superseded => HookPendingActionResolutionKind::Superseded,
        HookActionResolutionMode::UserDismissed => HookPendingActionResolutionKind::UserDismissed,
    }
}

fn hook_action_status_key(status: agentdash_executor::HookPendingActionStatus) -> &'static str {
    match status {
        agentdash_executor::HookPendingActionStatus::Pending => "pending",
        agentdash_executor::HookPendingActionStatus::Injected => "injected",
        agentdash_executor::HookPendingActionStatus::Resolved => "resolved",
        agentdash_executor::HookPendingActionStatus::Dismissed => "dismissed",
    }
}

fn hook_action_resolution_key(kind: HookPendingActionResolutionKind) -> &'static str {
    match kind {
        HookPendingActionResolutionKind::Adopted => "adopted",
        HookPendingActionResolutionKind::Rejected => "rejected",
        HookPendingActionResolutionKind::Completed => "completed",
        HookPendingActionResolutionKind::Superseded => "superseded",
        HookPendingActionResolutionKind::UserDismissed => "user_dismissed",
    }
}

fn normalize_companion_result_status(status: Option<&str>) -> Result<&str, AgentToolError> {
    match status.unwrap_or("completed").trim() {
        "" => Ok("completed"),
        "completed" => Ok("completed"),
        "blocked" => Ok("blocked"),
        "needs_follow_up" => Ok("needs_follow_up"),
        other => Err(AgentToolError::InvalidArguments(format!(
            "status 仅支持 completed / blocked / needs_follow_up，收到 `{other}`"
        ))),
    }
}

fn companion_slice_mode_key(mode: CompanionSliceMode) -> &'static str {
    match mode {
        CompanionSliceMode::Compact => "compact",
        CompanionSliceMode::Full => "full",
        CompanionSliceMode::WorkflowOnly => "workflow_only",
        CompanionSliceMode::ConstraintsOnly => "constraints_only",
    }
}

fn companion_adoption_mode_key(mode: CompanionAdoptionMode) -> &'static str {
    match mode {
        CompanionAdoptionMode::Suggestion => "suggestion",
        CompanionAdoptionMode::FollowUpRequired => "follow_up_required",
        CompanionAdoptionMode::BlockingReview => "blocking_review",
    }
}

fn companion_owner_candidates(
    snapshot: &agentdash_executor::SessionHookSnapshot,
) -> Result<Vec<(SessionOwnerType, Uuid, Option<String>)>, AgentToolError> {
    let mut owners = Vec::new();
    for owner in &snapshot.owners {
        if let Some(candidate) = parse_owner_candidate(
            owner.owner_type.as_str(),
            &owner.owner_id,
            owner.label.clone(),
        )? {
            owners.push(candidate);
        }
        if owner.owner_type == "task" {
            if let Some(story_id) = owner.story_id.as_deref() {
                if let Some(candidate) =
                    parse_owner_candidate("story", story_id, owner.label.clone())?
                {
                    owners.push(candidate);
                }
            }
        }
    }
    owners.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    Ok(owners)
}

fn parse_owner_candidate(
    owner_type: &str,
    owner_id: &str,
    label: Option<String>,
) -> Result<Option<(SessionOwnerType, Uuid, Option<String>)>, AgentToolError> {
    let owner_type = match owner_type {
        "project" => SessionOwnerType::Project,
        "story" => SessionOwnerType::Story,
        "task" => SessionOwnerType::Task,
        _ => return Ok(None),
    };
    let owner_id = Uuid::parse_str(owner_id).map_err(|error| {
        AgentToolError::ExecutionFailed(format!("owner_id 不是有效 UUID: {error}"))
    })?;
    Ok(Some((owner_type, owner_id, label)))
}

fn companion_project_id_for_owner(
    snapshot: &agentdash_executor::SessionHookSnapshot,
    owner_type: SessionOwnerType,
    owner_id: Uuid,
) -> Result<Uuid, AgentToolError> {
    let owner_id_raw = owner_id.to_string();
    let matching_owner = snapshot
        .owners
        .iter()
        .find(|owner| owner.owner_type == owner_type.to_string() && owner.owner_id == owner_id_raw)
        .ok_or_else(|| {
            AgentToolError::ExecutionFailed("当前 session owner 缺少 project 范围信息".to_string())
        })?;

    match owner_type {
        SessionOwnerType::Project => Ok(owner_id),
        SessionOwnerType::Story | SessionOwnerType::Task => matching_owner
            .project_id
            .as_deref()
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "当前 session owner 缺少 project_id，无法创建 companion session".to_string(),
                )
            })
            .and_then(|project_id| {
                Uuid::parse_str(project_id).map_err(|error| {
                    AgentToolError::ExecutionFailed(format!(
                        "owner.project_id 不是有效 UUID: {error}"
                    ))
                })
            }),
    }
}

#[derive(Clone)]
struct MountsListTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: ExecutionAddressSpace,
}

impl MountsListTool {
    fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self {
        Self {
            service,
            address_space,
        }
    }
}

#[async_trait]
impl AgentTool for MountsListTool {
    fn name(&self) -> &str {
        "mounts_list"
    }
    fn description(&self) -> &str {
        "列出当前会话可访问的 Address Space 挂载与能力"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {}, "required": [], "additionalProperties": false })
    }
    async fn execute(
        &self,
        _: &str,
        _: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let mounts = self.service.list_mounts(&self.address_space);
        let body = mounts
            .iter()
            .map(|mount| {
                let capabilities = mount
                    .capabilities
                    .iter()
                    .map(capability_name)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "- {}: {} (provider={}, root_ref={}, capabilities=[{}])",
                    mount.id, mount.display_name, mount.provider, mount.root_ref, capabilities
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ok_text(if body.is_empty() {
            "当前会话没有可用 mount".to_string()
        } else {
            body
        }))
    }
}

#[derive(Clone)]
struct FsReadTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: ExecutionAddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsReadTool {
    fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: ExecutionAddressSpace,
        overlay: Option<Arc<InlineContentOverlay>>,
    ) -> Self {
        Self {
            service,
            address_space,
            overlay,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FsReadParams {
    pub mount: Option<String>,
    pub path: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

#[async_trait]
impl AgentTool for FsReadTool {
    fn name(&self) -> &str {
        "fs_read"
    }
    fn description(&self) -> &str {
        "读取指定 mount 下的文本文件内容"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsReadParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsReadParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref())
            .map_err(AgentToolError::ExecutionFailed)?;
        let result = self
            .service
            .read_text(
                &self.address_space,
                &ResourceRef {
                    mount_id,
                    path: params.path,
                },
                self.overlay.as_ref().map(|arc| arc.as_ref()),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let lines = result.content.lines().collect::<Vec<_>>();
        let start = params.start_line.unwrap_or(1).max(1);
        let end = params.end_line.unwrap_or(lines.len()).max(start);
        let selected = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                let n = i + 1;
                (n >= start && n <= end).then(|| format!("{:>4} | {}", n, line))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ok_text(format!(
            "文件: {}\n{}",
            result.path,
            if selected.is_empty() {
                "   1 | ".to_string()
            } else {
                selected
            }
        )))
    }
}

#[derive(Clone)]
struct FsWriteTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: ExecutionAddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsWriteTool {
    fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: ExecutionAddressSpace,
        overlay: Option<Arc<InlineContentOverlay>>,
    ) -> Self {
        Self {
            service,
            address_space,
            overlay,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FsWriteParams {
    pub mount: Option<String>,
    pub path: String,
    pub content: String,
    pub append: Option<bool>,
}

#[async_trait]
impl AgentTool for FsWriteTool {
    fn name(&self) -> &str {
        "fs_write"
    }
    fn description(&self) -> &str {
        "向指定 mount 下的文件写入内容"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsWriteParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsWriteParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref())
            .map_err(AgentToolError::ExecutionFailed)?;
        let target = ResourceRef {
            mount_id,
            path: params.path,
        };
        let overlay_ref = self.overlay.as_ref().map(|arc| arc.as_ref());
        let final_content = if params.append.unwrap_or(false) {
            match self
                .service
                .read_text(&self.address_space, &target, overlay_ref)
                .await
            {
                Ok(existing) => format!("{}{}", existing.content, params.content),
                Err(_) => params.content,
            }
        } else {
            params.content
        };
        self.service
            .write_text(&self.address_space, &target, &final_content, overlay_ref)
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        Ok(ok_text(format!("已写入文件: {}", target.path)))
    }
}

#[derive(Clone)]
struct FsListTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: ExecutionAddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsListTool {
    fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: ExecutionAddressSpace,
        overlay: Option<Arc<InlineContentOverlay>>,
    ) -> Self {
        Self {
            service,
            address_space,
            overlay,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FsListParams {
    pub mount: Option<String>,
    pub path: Option<String>,
    pub recursive: Option<bool>,
    pub pattern: Option<String>,
}

#[async_trait]
impl AgentTool for FsListTool {
    fn name(&self) -> &str {
        "fs_list"
    }
    fn description(&self) -> &str {
        "列出指定 mount 下的目录内容"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsListParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsListParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref())
            .map_err(AgentToolError::ExecutionFailed)?;
        let result = self
            .service
            .list(
                &self.address_space,
                &mount_id,
                ListOptions {
                    path: params.path.unwrap_or_else(|| ".".to_string()),
                    pattern: params.pattern,
                    recursive: params.recursive.unwrap_or(false),
                },
                self.overlay.as_ref().map(|arc| arc.as_ref()),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let lines = result
            .entries
            .into_iter()
            .map(|e| {
                let kind = if e.is_dir { "dir" } else { "file" };
                format!("[{}] {}", kind, e.path.replace('\\', "/"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ok_text(if lines.is_empty() {
            "(空目录)".to_string()
        } else {
            lines
        }))
    }
}

#[derive(Clone)]
struct FsSearchTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: ExecutionAddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsSearchTool {
    fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: ExecutionAddressSpace,
        overlay: Option<Arc<InlineContentOverlay>>,
    ) -> Self {
        Self {
            service,
            address_space,
            overlay,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FsSearchParams {
    pub mount: Option<String>,
    pub query: String,
    pub path: Option<String>,
    #[serde(default)]
    pub regex: bool,
    pub include: Option<String>,
    pub max_results: Option<usize>,
    pub context_lines: Option<usize>,
}

#[async_trait]
impl AgentTool for FsSearchTool {
    fn name(&self) -> &str {
        "fs_search"
    }
    fn description(&self) -> &str {
        "在指定 mount 下进行文本搜索，支持正则和 glob 过滤"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsSearchParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsSearchParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref())
            .map_err(AgentToolError::ExecutionFailed)?;
        let (hits, truncated) = self
            .service
            .search_text_extended(
                &self.address_space,
                &mount_id,
                params.path.as_deref().unwrap_or("."),
                &params.query,
                params.regex,
                params.include.as_deref(),
                params.max_results.unwrap_or(50).max(1),
                params.context_lines.unwrap_or(0),
                self.overlay.as_ref().map(|arc| arc.as_ref()),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let mut output = if hits.is_empty() {
            "未找到匹配结果".to_string()
        } else {
            hits.join("\n")
        };
        if truncated {
            output.push_str("\n(结果已截断，请缩小搜索范围)");
        }
        Ok(ok_text(output))
    }
}

#[derive(Clone)]
struct ShellExecTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: ExecutionAddressSpace,
}
impl ShellExecTool {
    fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self {
        Self {
            service,
            address_space,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ShellExecParams {
    pub mount: Option<String>,
    pub cwd: Option<String>,
    pub command: String,
    pub timeout_secs: Option<u64>,
}

#[async_trait]
impl AgentTool for ShellExecTool {
    fn name(&self) -> &str {
        "shell_exec"
    }
    fn description(&self) -> &str {
        "在指定 mount 下执行 shell 命令"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<ShellExecParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ShellExecParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref())
            .map_err(AgentToolError::ExecutionFailed)?;
        let cwd = params.cwd.unwrap_or_else(|| ".".to_string());
        let result = self
            .service
            .exec(
                &self.address_space,
                &ExecRequest {
                    mount_id: mount_id.clone(),
                    cwd: cwd.clone(),
                    command: params.command.clone(),
                    timeout_ms: params.timeout_secs.map(|s| s.saturating_mul(1000)),
                },
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let merged = if result.stderr.trim().is_empty() {
            result.stdout
        } else if result.stdout.trim().is_empty() {
            format!("[stderr]\n{}", result.stderr)
        } else {
            format!("[stdout]\n{}\n\n[stderr]\n{}", result.stdout, result.stderr)
        };
        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "命令: {}\nmount: {}\ncwd: {}\n退出码: {}\n{}",
                params.command, mount_id, cwd, result.exit_code, merged
            ))],
            is_error: result.exit_code != 0,
            details: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tokio::sync::mpsc;

    use agentdash_domain::context_container::{
        ContextContainerCapability, ContextContainerDefinition, ContextContainerExposure,
        ContextContainerFile, ContextContainerProvider, MountDerivationPolicy,
    };
    use agentdash_domain::workspace::Workspace;

    use crate::relay::registry::ConnectedBackend;

    fn sample_workspace() -> Workspace {
        let mut workspace = Workspace::new(
            uuid::Uuid::new_v4(),
            "repo".to_string(),
            agentdash_domain::workspace::WorkspaceIdentityKind::LocalDir,
            serde_json::json!({ "root_hint": "/workspace/repo" }),
            agentdash_domain::workspace::WorkspaceResolutionPolicy::PreferOnline,
        );
        let mut binding = agentdash_domain::workspace::WorkspaceBinding::new(
            workspace.id,
            "backend-a".to_string(),
            "/workspace/repo".to_string(),
            serde_json::json!({}),
        );
        binding.status = agentdash_domain::workspace::WorkspaceBindingStatus::Ready;
        workspace.status = agentdash_domain::workspace::WorkspaceStatus::Ready;
        workspace.set_bindings(vec![binding]);
        workspace.refresh_default_binding();
        workspace
    }

    fn inline_container(
        id: &str,
        mount_id: &str,
        path: &str,
        content: &str,
    ) -> ContextContainerDefinition {
        ContextContainerDefinition {
            id: id.to_string(),
            mount_id: mount_id.to_string(),
            display_name: id.to_string(),
            provider: ContextContainerProvider::InlineFiles {
                files: vec![ContextContainerFile {
                    path: path.to_string(),
                    content: content.to_string(),
                }],
            },
            capabilities: vec![
                ContextContainerCapability::Read,
                ContextContainerCapability::List,
                ContextContainerCapability::Search,
            ],
            default_write: false,
            exposure: ContextContainerExposure::default(),
        }
    }

    #[test]
    fn normalize_mount_relative_path_blocks_escape() {
        let err = normalize_mount_relative_path("../secret", false).expect_err("should fail");
        assert!(err.contains("路径越界"));
    }

    #[test]
    fn session_for_workspace_creates_main_mount() {
        let registry = crate::relay::registry::BackendRegistry::new();
        let service = RelayAddressSpaceService::new(registry);
        let session = service
            .session_for_workspace(&sample_workspace())
            .expect("session should build");
        assert_eq!(session.default_mount_id.as_deref(), Some("main"));
        assert_eq!(session.mounts.len(), 1);
        assert!(session.mounts[0].supports(ExecutionMountCapability::Exec));
    }

    #[test]
    fn build_task_address_space_merges_project_story_and_workspace_policy() {
        let registry = crate::relay::registry::BackendRegistry::new();
        let service = RelayAddressSpaceService::new(registry);
        let mut project = agentdash_domain::project::Project::new("proj".into(), "desc".into());
        project.config.context_containers = vec![inline_container(
            "project-spec",
            "spec",
            "backend/spec.md",
            "# spec",
        )];
        project.config.mount_policy = MountDerivationPolicy {
            include_local_workspace: true,
            local_workspace_capabilities: vec![
                ContextContainerCapability::Read,
                ContextContainerCapability::List,
            ],
        };

        let mut story =
            agentdash_domain::story::Story::new(project.id, "story".into(), "desc".into());
        story.context.context_containers = vec![inline_container(
            "story-brief",
            "brief",
            "brief.md",
            "story brief",
        )];

        let address_space = service
            .build_task_address_space(
                &project,
                &story,
                Some(&sample_workspace()),
                Some("PI_AGENT"),
            )
            .expect("address space should build");

        assert_eq!(address_space.default_mount_id.as_deref(), Some("main"));
        assert_eq!(address_space.mounts.len(), 3);
        let main = address_space
            .mounts
            .iter()
            .find(|m| m.id == "main")
            .expect("main mount");
        assert!(!main.supports(ExecutionMountCapability::Exec));
        assert!(main.supports(ExecutionMountCapability::Read));
        assert!(address_space.mounts.iter().any(|m| m.id == "spec"));
        assert!(address_space.mounts.iter().any(|m| m.id == "brief"));
    }

    #[test]
    fn story_containers_can_disable_and_override_project_defaults() {
        let registry = crate::relay::registry::BackendRegistry::new();
        let service = RelayAddressSpaceService::new(registry);
        let mut project = agentdash_domain::project::Project::new("proj".into(), "desc".into());
        project.config.context_containers = vec![
            inline_container("project-spec", "shared", "spec.md", "project spec"),
            inline_container("project-km", "km", "index.md", "project km"),
        ];

        let mut story =
            agentdash_domain::story::Story::new(project.id, "story".into(), "desc".into());
        story.context.disabled_container_ids = vec!["project-km".into()];
        story.context.context_containers = vec![inline_container(
            "story-spec",
            "shared",
            "spec.md",
            "story override",
        )];

        let address_space = service
            .build_task_address_space(&project, &story, None, Some("PI_AGENT"))
            .expect("address space should build");

        assert_eq!(address_space.mounts.len(), 1);
        let mount = &address_space.mounts[0];
        assert_eq!(mount.id, "shared");
        let files = inline_files_from_mount(mount).expect("inline files");
        assert_eq!(
            files.get("spec.md").map(String::as_str),
            Some("story override")
        );
    }

    #[tokio::test]
    async fn inline_mount_supports_read_list_and_search() {
        let registry = crate::relay::registry::BackendRegistry::new();
        let service = RelayAddressSpaceService::new(registry);
        let address_space = ExecutionAddressSpace {
            mounts: vec![
                build_context_container_mount(&ContextContainerDefinition {
                    id: "story-brief".to_string(),
                    mount_id: "brief".to_string(),
                    display_name: "brief".to_string(),
                    provider: ContextContainerProvider::InlineFiles {
                        files: vec![
                            ContextContainerFile {
                                path: "brief.md".to_string(),
                                content: "hello inline mount".to_string(),
                            },
                            ContextContainerFile {
                                path: "notes/todo.md".to_string(),
                                content: "todo: verify inline search".to_string(),
                            },
                        ],
                    },
                    capabilities: vec![
                        ContextContainerCapability::Read,
                        ContextContainerCapability::List,
                        ContextContainerCapability::Search,
                    ],
                    default_write: false,
                    exposure: ContextContainerExposure::default(),
                })
                .expect("mount should build"),
            ],
            default_mount_id: Some("brief".to_string()),
            ..Default::default()
        };

        let read = service
            .read_text(
                &address_space,
                &ResourceRef {
                    mount_id: "brief".to_string(),
                    path: "brief.md".to_string(),
                },
                None,
            )
            .await
            .expect("inline read");
        assert_eq!(read.content, "hello inline mount");

        let listed = service
            .list(
                &address_space,
                "brief",
                ListOptions {
                    path: ".".to_string(),
                    pattern: None,
                    recursive: true,
                },
                None,
            )
            .await
            .expect("inline list");
        assert!(listed.entries.iter().any(|e| e.path == "brief.md"));
        assert!(listed.entries.iter().any(|e| e.path == "notes/todo.md"));

        let hits = service
            .search_text(&address_space, "brief", ".", "verify", 10, None)
            .await
            .expect("inline search");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].contains("notes/todo.md:1"));
    }

    #[tokio::test]
    async fn read_text_routes_via_tool_transport() {
        let registry = crate::relay::registry::BackendRegistry::new();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        registry
            .try_register(ConnectedBackend {
                backend_id: "backend-a".to_string(),
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                capabilities: agentdash_relay::CapabilitiesPayload {
                    executors: Vec::new(),
                    supports_cancel: true,
                    supports_workspace_files: true,
                    supports_discover_options: true,
                },
                accessible_roots: vec!["/workspace".to_string()],
                sender,
                connected_at: Utc::now(),
            })
            .await
            .expect("backend should register");

        let service = RelayAddressSpaceService::new(registry.clone());
        let session = service
            .session_for_workspace(&sample_workspace())
            .expect("session");

        let handle = tokio::spawn({
            let service = service.clone();
            let session = session.clone();
            async move {
                service
                    .read_text(
                        &session,
                        &ResourceRef {
                            mount_id: "main".to_string(),
                            path: "src/main.rs".to_string(),
                        },
                        None,
                    )
                    .await
            }
        });

        let message = receiver.recv().await.expect("command sent");
        let id = message.id().to_string();
        match message {
            RelayMessage::CommandToolFileRead { payload, .. } => {
                assert_eq!(payload.workspace_root, "/workspace/repo");
                assert_eq!(payload.path, "src/main.rs");
            }
            other => panic!("unexpected: {other:?}"),
        }

        let resolved = registry
            .resolve_response(&RelayMessage::ResponseToolFileRead {
                id,
                payload: Some(agentdash_relay::ToolFileReadResponse {
                    call_id: "call".to_string(),
                    content: "fn main() {}".to_string(),
                    encoding: "utf-8".to_string(),
                }),
                error: None,
            })
            .await;
        assert!(resolved);

        let result = handle.await.expect("task").expect("read");
        assert_eq!(result.content, "fn main() {}");
    }

    #[test]
    fn runtime_tool_schemas_are_openai_compatible() {
        let registry = crate::relay::registry::BackendRegistry::new();
        let service = Arc::new(RelayAddressSpaceService::new(registry));
        let address_space = ExecutionAddressSpace {
            mounts: vec![agentdash_executor::ExecutionMount {
                id: "brief".to_string(),
                provider: PROVIDER_INLINE_FS.to_string(),
                backend_id: String::new(),
                root_ref: "context://inline/brief".to_string(),
                capabilities: vec![
                    ExecutionMountCapability::Read,
                    ExecutionMountCapability::List,
                    ExecutionMountCapability::Search,
                ],
                default_write: false,
                display_name: "brief".to_string(),
                metadata: serde_json::json!({ "files": { "brief.md": "hello" } }),
            }],
            default_mount_id: Some("brief".to_string()),
            ..Default::default()
        };

        let schemas = vec![
            MountsListTool::new(service.clone(), address_space.clone()).parameters_schema(),
            FsReadTool::new(service.clone(), address_space.clone(), None).parameters_schema(),
            FsWriteTool::new(service.clone(), address_space.clone(), None).parameters_schema(),
            FsListTool::new(service.clone(), address_space.clone(), None).parameters_schema(),
            FsSearchTool::new(service.clone(), address_space.clone(), None).parameters_schema(),
            ShellExecTool::new(service, address_space).parameters_schema(),
        ];

        for schema in schemas {
            let properties = schema["properties"].as_object().expect("properties");
            let required = schema["required"]
                .as_array()
                .expect("required")
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
            for key in properties.keys() {
                assert!(
                    required.contains(key.as_str()),
                    "required should contain `{key}`"
                );
            }
        }
    }

    #[test]
    fn companion_owner_candidates_fallback_from_task_to_story() {
        let story_id = Uuid::new_v4();
        let snapshot = agentdash_executor::SessionHookSnapshot {
            session_id: "sess-test".to_string(),
            owners: vec![agentdash_executor::HookOwnerSummary {
                owner_type: "task".to_string(),
                owner_id: Uuid::new_v4().to_string(),
                label: Some("Task A".to_string()),
                project_id: None,
                story_id: Some(story_id.to_string()),
                task_id: None,
            }],
            ..agentdash_executor::SessionHookSnapshot::default()
        };

        let candidates = companion_owner_candidates(&snapshot).expect("candidates");

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].0, SessionOwnerType::Task);
        assert_eq!(candidates[1].0, SessionOwnerType::Story);
        assert_eq!(candidates[1].1, story_id);
    }

    #[test]
    fn compact_companion_slice_keeps_owner_summary_and_limits_payload() {
        let snapshot = agentdash_executor::SessionHookSnapshot {
            session_id: "sess-parent".to_string(),
            owners: vec![agentdash_executor::HookOwnerSummary {
                owner_type: "task".to_string(),
                owner_id: Uuid::new_v4().to_string(),
                label: Some("Task A".to_string()),
                project_id: None,
                story_id: None,
                task_id: None,
            }],
            ..agentdash_executor::SessionHookSnapshot::default()
        };
        let resolution = agentdash_executor::HookResolution {
            context_fragments: vec![
                agentdash_executor::HookContextFragment {
                    slot: "workflow".to_string(),
                    label: "active_workflow_phase".to_string(),
                    content: "phase info".to_string(),
                    source_summary: vec![],
                    source_refs: vec![],
                },
                agentdash_executor::HookContextFragment {
                    slot: "instruction_append".to_string(),
                    label: "workflow_phase_constraints".to_string(),
                    content: "follow rules".to_string(),
                    source_summary: vec![],
                    source_refs: vec![],
                },
                agentdash_executor::HookContextFragment {
                    slot: "workflow".to_string(),
                    label: "overflow".to_string(),
                    content: "should be omitted".to_string(),
                    source_summary: vec![],
                    source_refs: vec![],
                },
            ],
            constraints: vec![
                agentdash_executor::HookConstraint {
                    key: "constraint:1".to_string(),
                    description: "first".to_string(),
                    source_summary: vec![],
                    source_refs: vec![],
                },
                agentdash_executor::HookConstraint {
                    key: "constraint:2".to_string(),
                    description: "second".to_string(),
                    source_summary: vec![],
                    source_refs: vec![],
                },
            ],
            ..agentdash_executor::HookResolution::default()
        };

        let slice = build_companion_dispatch_slice(
            &snapshot,
            &resolution,
            CompanionSliceMode::Compact,
            2,
            1,
        );

        assert_eq!(slice.fragments.len(), 2);
        assert_eq!(slice.constraints.len(), 1);
        assert_eq!(slice.omitted_fragment_count, 1);
        assert_eq!(slice.omitted_constraint_count, 1);
        assert_eq!(slice.inherited_fragment_labels[0], "owner_summary");
    }

    #[test]
    fn compact_execution_slice_drops_write_and_mcp_servers() {
        let address_space = ExecutionAddressSpace {
            mounts: vec![agentdash_executor::ExecutionMount {
                id: "main".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-1".to_string(),
                root_ref: "/workspace".to_string(),
                capabilities: vec![
                    ExecutionMountCapability::Read,
                    ExecutionMountCapability::Write,
                    ExecutionMountCapability::List,
                    ExecutionMountCapability::Search,
                    ExecutionMountCapability::Exec,
                ],
                default_write: true,
                display_name: "main".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            ..Default::default()
        };

        let slice = build_companion_execution_slice(
            Some(&address_space),
            &[McpServer::Stdio(
                agent_client_protocol::McpServerStdio::new("test-mcp", "cmd"),
            )],
            CompanionSliceMode::Compact,
        );

        let sliced_space = slice
            .address_space
            .expect("compact should keep sliced address_space");
        assert_eq!(slice.mcp_servers.len(), 0);
        assert_eq!(sliced_space.mounts.len(), 1);
        assert!(
            !sliced_space.mounts[0]
                .capabilities
                .contains(&ExecutionMountCapability::Write)
        );
        assert!(
            sliced_space.mounts[0]
                .capabilities
                .contains(&ExecutionMountCapability::Exec)
        );
        assert!(!sliced_space.mounts[0].default_write);
    }

    #[test]
    fn workflow_only_execution_slice_uses_empty_address_space() {
        let address_space = ExecutionAddressSpace {
            mounts: vec![agentdash_executor::ExecutionMount {
                id: "main".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-1".to_string(),
                root_ref: "/workspace".to_string(),
                capabilities: vec![
                    ExecutionMountCapability::Read,
                    ExecutionMountCapability::Write,
                ],
                default_write: true,
                display_name: "main".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            ..Default::default()
        };

        let slice = build_companion_execution_slice(
            Some(&address_space),
            &[McpServer::Stdio(
                agent_client_protocol::McpServerStdio::new("test-mcp", "cmd"),
            )],
            CompanionSliceMode::WorkflowOnly,
        );

        let sliced_space = slice
            .address_space
            .expect("workflow_only should force empty address_space");
        assert!(sliced_space.mounts.is_empty());
        assert!(sliced_space.default_mount_id.is_none());
        assert!(slice.mcp_servers.is_empty());
    }

    #[test]
    fn companion_dispatch_prompt_includes_return_instruction() {
        let plan = CompanionDispatchPlan {
            dispatch_id: "dispatch-1".to_string(),
            companion_label: "companion".to_string(),
            parent_session_id: "sess-parent".to_string(),
            parent_turn_id: "turn-parent-1".to_string(),
            adoption_mode: CompanionAdoptionMode::BlockingReview,
            slice: CompanionDispatchSlice {
                mode: CompanionSliceMode::Compact,
                fragments: vec![agentdash_executor::HookContextFragment {
                    slot: "workflow".to_string(),
                    label: "active_workflow_phase".to_string(),
                    content: "phase info".to_string(),
                    source_summary: vec![],
                    source_refs: vec![],
                }],
                constraints: vec![agentdash_executor::HookConstraint {
                    key: "constraint:1".to_string(),
                    description: "first".to_string(),
                    source_summary: vec![],
                    source_refs: vec![],
                }],
                inherited_fragment_labels: vec!["active_workflow_phase".to_string()],
                inherited_constraint_keys: vec!["constraint:1".to_string()],
                omitted_fragment_count: 0,
                omitted_constraint_count: 0,
            },
        };

        let prompt = build_companion_dispatch_prompt(&plan, "请帮我 review 当前实现");

        assert!(prompt.contains("companion_complete"));
        assert!(prompt.contains("dispatch_id: dispatch-1"));
        assert!(prompt.contains("请帮我 review 当前实现"));
    }
}
