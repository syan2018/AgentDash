use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use agentdash_agent::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolUpdateCallback,
};
use agentdash_agent::tools::schema_value;
use agentdash_domain::context_container::{
    ContextContainerCapability, ContextContainerDefinition, ContextContainerProvider,
    MountDerivationPolicy,
};
use agentdash_domain::{project::Project, story::Story, workspace::Workspace};
use agentdash_executor::{
    ConnectorError, ExecutionAddressSpace, ExecutionContext, ExecutionMount,
    ExecutionMountCapability, RuntimeToolProvider,
};
use agentdash_relay::{
    FileEntryRelay, RelayMessage, ToolFileListPayload, ToolFileReadPayload, ToolFileWritePayload,
    ToolShellExecPayload,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::relay::registry::BackendRegistry;

const MAX_SEARCH_FILE_BYTES: u64 = 256 * 1024;
const PROVIDER_RELAY_FS: &str = "relay_fs";
const PROVIDER_INLINE_FS: &str = "inline_fs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    pub path: String,
    pub pattern: Option<String>,
    pub recursive: bool,
}

#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub mount_id: String,
    pub cwd: String,
    pub command: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ReadResult {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ListResult {
    pub entries: Vec<FileEntryRelay>,
}

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

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
        workspace: &Workspace,
    ) -> Result<ExecutionAddressSpace, String> {
        Ok(ExecutionAddressSpace {
            mounts: vec![workspace_mount_from_policy(
                workspace,
                &MountDerivationPolicy::default(),
            )?],
            default_mount_id: Some("main".to_string()),
        })
    }

    pub fn build_task_address_space(
        &self,
        project: &Project,
        story: &Story,
        workspace: Option<&Workspace>,
        agent_type: Option<&str>,
    ) -> Result<ExecutionAddressSpace, String> {
        self.build_derived_address_space(
            project,
            Some(story),
            workspace,
            agent_type,
            SessionMountTarget::Task,
        )
    }

    pub fn build_story_address_space(
        &self,
        project: &Project,
        story: &Story,
        workspace: Option<&Workspace>,
        agent_type: Option<&str>,
    ) -> Result<ExecutionAddressSpace, String> {
        self.build_derived_address_space(
            project,
            Some(story),
            workspace,
            agent_type,
            SessionMountTarget::Story,
        )
    }

    fn build_derived_address_space(
        &self,
        project: &Project,
        story: Option<&Story>,
        workspace: Option<&Workspace>,
        agent_type: Option<&str>,
        target: SessionMountTarget,
    ) -> Result<ExecutionAddressSpace, String> {
        let mut mounts = Vec::new();
        let mount_policy = story
            .and_then(|item| item.context.mount_policy_override.clone())
            .unwrap_or_else(|| project.config.mount_policy.clone());

        if mount_policy.include_local_workspace
            && let Some(workspace) = workspace
        {
            mounts.push(workspace_mount_from_policy(workspace, &mount_policy)?);
        }

        for container in effective_context_containers(project, story) {
            if !container_visible_for_target(&container, target, agent_type) {
                continue;
            }
            mounts.push(build_context_container_mount(&container)?);
        }

        let default_mount_id = if mounts.iter().any(|mount| mount.id == "main") {
            Some("main".to_string())
        } else {
            mounts.first().map(|mount| mount.id.clone())
        };

        Ok(ExecutionAddressSpace {
            mounts,
            default_mount_id,
        })
    }

    pub fn list_mounts(&self, address_space: &ExecutionAddressSpace) -> Vec<ExecutionMount> {
        address_space.mounts.clone()
    }

    pub async fn read_text(
        &self,
        address_space: &ExecutionAddressSpace,
        target: &ResourceRef,
    ) -> Result<ReadResult, String> {
        let mount = resolve_mount(
            address_space,
            &target.mount_id,
            ExecutionMountCapability::Read,
        )?;
        let path = normalize_mount_relative_path(&target.path, false)?;
        if mount.provider == PROVIDER_INLINE_FS {
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
    ) -> Result<(), String> {
        let mount = resolve_mount(
            address_space,
            &target.mount_id,
            ExecutionMountCapability::Write,
        )?;
        let path = normalize_mount_relative_path(&target.path, false)?;
        if mount.provider == PROVIDER_INLINE_FS {
            return Err(format!(
                "mount `{}` 是只读内联容器，当前不支持写入",
                mount.id
            ));
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
    ) -> Result<ListResult, String> {
        let mount = resolve_mount(address_space, mount_id, ExecutionMountCapability::List)?;
        let path = normalize_mount_relative_path(&options.path, true)?;
        if mount.provider == PROVIDER_INLINE_FS {
            return Ok(ListResult {
                entries: list_inline_entries(
                    &inline_files_from_mount(mount)?,
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
    ) -> Result<Vec<String>, String> {
        let mount = resolve_mount(address_space, mount_id, ExecutionMountCapability::Search)?;
        let base_path = normalize_mount_relative_path(path, true)?;
        let listed = self
            .list(
                address_space,
                &mount.id,
                ListOptions {
                    path: base_path,
                    pattern: None,
                    recursive: true,
                },
            )
            .await?;

        let mut hits = Vec::new();
        for entry in listed.entries {
            if entry.is_dir || entry.size.unwrap_or(0) > MAX_SEARCH_FILE_BYTES {
                continue;
            }

            let read = match self
                .read_text(
                    address_space,
                    &ResourceRef {
                        mount_id: mount.id.clone(),
                        path: entry.path.clone(),
                    },
                )
                .await
            {
                Ok(result) => result,
                Err(_) => continue,
            };

            for (index, line) in read.content.lines().enumerate() {
                if line.contains(query) {
                    hits.push(format!("{}:{}: {}", entry.path, index + 1, line.trim()));
                    if hits.len() >= max_results {
                        return Ok(hits);
                    }
                }
            }
        }

        Ok(hits)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionMountTarget {
    Story,
    Task,
}

fn workspace_mount_from_policy(
    workspace: &Workspace,
    policy: &MountDerivationPolicy,
) -> Result<ExecutionMount, String> {
    let backend_id = workspace.backend_id.trim();
    if backend_id.is_empty() {
        return Err("Workspace.backend_id 不能为空".to_string());
    }
    if workspace.container_ref.trim().is_empty() {
        return Err("Workspace.container_ref 不能为空".to_string());
    }

    let capabilities = if policy.local_workspace_capabilities.is_empty() {
        vec![
            ExecutionMountCapability::Read,
            ExecutionMountCapability::Write,
            ExecutionMountCapability::List,
            ExecutionMountCapability::Search,
            ExecutionMountCapability::Exec,
        ]
    } else {
        map_container_capabilities(&policy.local_workspace_capabilities)
    };

    Ok(ExecutionMount {
        id: "main".to_string(),
        provider: PROVIDER_RELAY_FS.to_string(),
        backend_id: backend_id.to_string(),
        root_ref: workspace.container_ref.clone(),
        capabilities,
        default_write: true,
        display_name: if workspace.name.trim().is_empty() {
            "主工作空间".to_string()
        } else {
            workspace.name.clone()
        },
        metadata: serde_json::Value::Null,
    })
}

fn effective_context_containers(
    project: &Project,
    story: Option<&Story>,
) -> Vec<ContextContainerDefinition> {
    let mut containers = project.config.context_containers.clone();
    if let Some(story) = story {
        let disabled = story
            .context
            .disabled_container_ids
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .map(|item| item.to_string())
            .collect::<BTreeSet<_>>();
        if !disabled.is_empty() {
            containers.retain(|container| !disabled.contains(container.id.trim()));
        }

        for container in &story.context.context_containers {
            containers.retain(|item| {
                item.id.trim() != container.id.trim()
                    && item.mount_id.trim() != container.mount_id.trim()
            });
            containers.push(container.clone());
        }
    }
    containers
}

fn container_visible_for_target(
    container: &ContextContainerDefinition,
    target: SessionMountTarget,
    agent_type: Option<&str>,
) -> bool {
    let exposure = &container.exposure;
    let target_enabled = match target {
        SessionMountTarget::Story => exposure.include_in_story_sessions,
        SessionMountTarget::Task => exposure.include_in_task_sessions,
    };
    if !target_enabled {
        return false;
    }

    if exposure.allowed_agent_types.is_empty() {
        return true;
    }

    let Some(agent_type) = agent_type.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };

    exposure
        .allowed_agent_types
        .iter()
        .any(|item| item.trim().eq_ignore_ascii_case(agent_type))
}

fn build_context_container_mount(
    container: &ContextContainerDefinition,
) -> Result<ExecutionMount, String> {
    let id = non_empty_trimmed(&container.mount_id, "mount_id")?.to_string();
    let display_name = if container.display_name.trim().is_empty() {
        container.id.trim().to_string()
    } else {
        container.display_name.trim().to_string()
    };
    let capabilities = if container.capabilities.is_empty() {
        vec![
            ExecutionMountCapability::Read,
            ExecutionMountCapability::List,
            ExecutionMountCapability::Search,
        ]
    } else {
        map_container_capabilities(&container.capabilities)
    };

    let (provider, root_ref, metadata) = match &container.provider {
        ContextContainerProvider::InlineFiles { files } => (
            PROVIDER_INLINE_FS.to_string(),
            format!("context://inline/{}", container.id.trim()),
            serde_json::json!({ "files": normalize_inline_files(files)? }),
        ),
        ContextContainerProvider::ExternalService {
            service_id,
            root_ref,
        } => (
            "external_service".to_string(),
            root_ref.trim().to_string(),
            serde_json::json!({
                "service_id": service_id.trim(),
                "root_ref": root_ref.trim(),
            }),
        ),
    };

    Ok(ExecutionMount {
        id,
        provider,
        backend_id: String::new(),
        root_ref,
        capabilities,
        default_write: container.default_write,
        display_name,
        metadata,
    })
}

fn map_container_capabilities(
    capabilities: &[ContextContainerCapability],
) -> Vec<ExecutionMountCapability> {
    let mut mapped = Vec::new();
    for capability in capabilities {
        let next = match capability {
            ContextContainerCapability::Read => ExecutionMountCapability::Read,
            ContextContainerCapability::Write => ExecutionMountCapability::Write,
            ContextContainerCapability::List => ExecutionMountCapability::List,
            ContextContainerCapability::Search => ExecutionMountCapability::Search,
            ContextContainerCapability::Exec => ExecutionMountCapability::Exec,
        };
        if !mapped.contains(&next) {
            mapped.push(next);
        }
    }
    mapped
}

fn non_empty_trimmed<'a>(value: &'a str, field_name: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field_name} 不能为空"))
    } else {
        Ok(trimmed)
    }
}

fn normalize_inline_files(
    files: &[agentdash_domain::context_container::ContextContainerFile],
) -> Result<BTreeMap<String, String>, String> {
    let mut normalized = BTreeMap::new();
    for file in files {
        let path = normalize_mount_relative_path(&file.path, false)?;
        normalized.insert(path, file.content.clone());
    }
    Ok(normalized)
}

fn inline_files_from_mount(mount: &ExecutionMount) -> Result<BTreeMap<String, String>, String> {
    let raw_files = mount
        .metadata
        .get("files")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    serde_json::from_value::<BTreeMap<String, String>>(raw_files)
        .map_err(|error| format!("mount `{}` 的 inline metadata 无效: {error}", mount.id))
}

fn list_inline_entries(
    files: &BTreeMap<String, String>,
    base_path: &str,
    pattern: Option<&str>,
    recursive: bool,
) -> Vec<FileEntryRelay> {
    let normalized_base = base_path.trim_matches('/');
    let mut dirs = BTreeSet::new();
    let mut file_entries = BTreeMap::new();

    for (path, content) in files {
        let matches_base = if normalized_base.is_empty() {
            true
        } else {
            path == normalized_base
                || path
                    .strip_prefix(normalized_base)
                    .is_some_and(|rest| rest.starts_with('/'))
        };
        if !matches_base {
            continue;
        }

        let relative = if normalized_base.is_empty() {
            path.as_str()
        } else if path == normalized_base {
            ""
        } else {
            path.strip_prefix(normalized_base)
                .and_then(|rest| rest.strip_prefix('/'))
                .unwrap_or("")
        };

        if relative.is_empty() {
            file_entries.insert(path.clone(), content.len() as u64);
            continue;
        }

        let parts = relative.split('/').collect::<Vec<_>>();
        if recursive {
            let full_parts = path.split('/').collect::<Vec<_>>();
            for depth in 1..full_parts.len() {
                dirs.insert(full_parts[..depth].join("/"));
            }
            file_entries.insert(path.clone(), content.len() as u64);
        } else if parts.len() == 1 {
            file_entries.insert(path.clone(), content.len() as u64);
        } else {
            let dir_path = if normalized_base.is_empty() {
                parts[0].to_string()
            } else {
                format!("{}/{}", normalized_base, parts[0])
            };
            dirs.insert(dir_path);
        }
    }

    let normalized_pattern = pattern.map(str::trim).filter(|value| !value.is_empty());
    let mut entries = Vec::new();
    for dir in dirs {
        if path_matches_pattern(&dir, normalized_pattern) {
            entries.push(FileEntryRelay {
                path: dir,
                size: None,
                modified_at: None,
                is_dir: true,
            });
        }
    }
    for (path, size) in file_entries {
        if path_matches_pattern(&path, normalized_pattern) {
            entries.push(FileEntryRelay {
                path,
                size: Some(size),
                modified_at: None,
                is_dir: false,
            });
        }
    }
    entries
}

fn path_matches_pattern(path: &str, pattern: Option<&str>) -> bool {
    pattern.is_none_or(|needle| path.contains(needle))
}

pub fn resolve_mount<'a>(
    address_space: &'a ExecutionAddressSpace,
    mount_id: &str,
    capability: ExecutionMountCapability,
) -> Result<&'a ExecutionMount, String> {
    let mount = address_space
        .mounts
        .iter()
        .find(|mount| mount.id == mount_id)
        .ok_or_else(|| format!("mount 不存在: {mount_id}"))?;
    if !mount.supports(capability) {
        return Err(format!("mount `{}` 不支持该能力", mount.id));
    }
    Ok(mount)
}

pub fn resolve_mount_id(
    address_space: &ExecutionAddressSpace,
    mount: Option<&str>,
) -> Result<String, String> {
    if let Some(mount_id) = mount.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(mount_id.to_string());
    }
    address_space
        .default_mount_id
        .clone()
        .or_else(|| address_space.mounts.first().map(|mount| mount.id.clone()))
        .ok_or_else(|| "当前会话没有可用 mount".to_string())
}

pub fn capability_name(capability: &ExecutionMountCapability) -> &'static str {
    match capability {
        ExecutionMountCapability::Read => "read",
        ExecutionMountCapability::Write => "write",
        ExecutionMountCapability::List => "list",
        ExecutionMountCapability::Search => "search",
        ExecutionMountCapability::Exec => "exec",
    }
}

pub fn normalize_mount_relative_path(input: &str, allow_empty: bool) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "." {
        return if allow_empty {
            Ok(String::new())
        } else {
            Err("路径不能为空".to_string())
        };
    }

    if is_absolute_like(trimmed) {
        return Err("路径必须是相对于 mount 根目录的相对路径".to_string());
    }

    let mut parts = Vec::new();
    for part in trimmed.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.pop().is_none() {
                return Err("路径越界：不允许访问 mount 之外的路径".to_string());
            }
            continue;
        }
        parts.push(part.to_string());
    }

    if parts.is_empty() {
        if allow_empty {
            Ok(String::new())
        } else {
            Err("路径不能为空".to_string())
        }
    } else {
        Ok(parts.join("/"))
    }
}

fn is_absolute_like(raw: &str) -> bool {
    raw.starts_with('/')
        || raw.starts_with('\\')
        || raw.starts_with("//")
        || raw.starts_with("\\\\")
        || raw
            .as_bytes()
            .get(1)
            .zip(raw.as_bytes().get(2))
            .is_some_and(|(second, third)| *second == b':' && (*third == b'\\' || *third == b'/'))
}

pub fn join_root_ref(root_ref: &str, relative_path: &str) -> String {
    if relative_path.is_empty() {
        return root_ref.to_string();
    }

    let use_backslash = root_ref.contains('\\');
    let root = root_ref.trim_end_matches(['/', '\\']);
    let rel = if use_backslash {
        relative_path.replace('/', "\\")
    } else {
        relative_path.replace('\\', "/")
    };

    if use_backslash {
        format!("{root}\\{rel}")
    } else {
        format!("{root}/{rel}")
    }
}

#[derive(Clone)]
pub struct RelayRuntimeToolProvider {
    service: Arc<RelayAddressSpaceService>,
}

impl RelayRuntimeToolProvider {
    pub fn new(service: Arc<RelayAddressSpaceService>) -> Self {
        Self { service }
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

        Ok(vec![
            Arc::new(MountsListTool::new(
                self.service.clone(),
                address_space.clone(),
            )) as DynAgentTool,
            Arc::new(FsReadTool::new(self.service.clone(), address_space.clone())) as DynAgentTool,
            Arc::new(FsWriteTool::new(
                self.service.clone(),
                address_space.clone(),
            )) as DynAgentTool,
            Arc::new(FsListTool::new(self.service.clone(), address_space.clone())) as DynAgentTool,
            Arc::new(FsSearchTool::new(
                self.service.clone(),
                address_space.clone(),
            )) as DynAgentTool,
            Arc::new(ShellExecTool::new(self.service.clone(), address_space)) as DynAgentTool,
        ])
    }
}

fn ok_text(text: String) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(text)],
        is_error: false,
        details: None,
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
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        _args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
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
}

impl FsReadTool {
    fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self {
        Self {
            service,
            address_space,
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
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsReadParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
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
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let lines = result.content.lines().collect::<Vec<_>>();
        let start = params.start_line.unwrap_or(1).max(1);
        let end = params.end_line.unwrap_or(lines.len()).max(start);
        let selected = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                let line_no = index + 1;
                (line_no >= start && line_no <= end).then(|| format!("{:>4} | {}", line_no, line))
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
}

impl FsWriteTool {
    fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self {
        Self {
            service,
            address_space,
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
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsWriteParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref())
            .map_err(AgentToolError::ExecutionFailed)?;
        let target = ResourceRef {
            mount_id,
            path: params.path,
        };

        let final_content = if params.append.unwrap_or(false) {
            match self.service.read_text(&self.address_space, &target).await {
                Ok(existing) => format!("{}{}", existing.content, params.content),
                Err(_) => params.content,
            }
        } else {
            params.content
        };

        self.service
            .write_text(&self.address_space, &target, &final_content)
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        Ok(ok_text(format!("已写入文件: {}", target.path)))
    }
}

#[derive(Clone)]
struct FsListTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: ExecutionAddressSpace,
}

impl FsListTool {
    fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self {
        Self {
            service,
            address_space,
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
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsListParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
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
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let lines = result
            .entries
            .into_iter()
            .map(|entry| {
                let kind = if entry.is_dir { "dir" } else { "file" };
                format!("[{}] {}", kind, entry.path.replace('\\', "/"))
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
}

impl FsSearchTool {
    fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self {
        Self {
            service,
            address_space,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FsSearchParams {
    pub mount: Option<String>,
    pub query: String,
    pub path: Option<String>,
    pub max_results: Option<usize>,
}

#[async_trait]
impl AgentTool for FsSearchTool {
    fn name(&self) -> &str {
        "fs_search"
    }

    fn description(&self) -> &str {
        "在指定 mount 下进行文本搜索"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsSearchParams>()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsSearchParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref())
            .map_err(AgentToolError::ExecutionFailed)?;
        let hits = self
            .service
            .search_text(
                &self.address_space,
                &mount_id,
                params.path.as_deref().unwrap_or("."),
                &params.query,
                params.max_results.unwrap_or(50).max(1),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        Ok(ok_text(if hits.is_empty() {
            "未找到匹配结果".to_string()
        } else {
            hits.join("\n")
        }))
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
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ShellExecParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
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
                    timeout_ms: params
                        .timeout_secs
                        .map(|seconds| seconds.saturating_mul(1000)),
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

    use crate::relay::registry::ConnectedBackend;

    fn sample_workspace() -> Workspace {
        Workspace {
            id: uuid::Uuid::new_v4(),
            project_id: uuid::Uuid::new_v4(),
            backend_id: "backend-a".to_string(),
            name: "repo".to_string(),
            container_ref: "/workspace/repo".to_string(),
            workspace_type: agentdash_domain::workspace::WorkspaceType::Static,
            status: agentdash_domain::workspace::WorkspaceStatus::Ready,
            git_config: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
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
        let registry = BackendRegistry::new();
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
        let registry = BackendRegistry::new();
        let service = RelayAddressSpaceService::new(registry);
        let mut project = agentdash_domain::project::Project::new(
            "proj".into(),
            "desc".into(),
            "backend-a".into(),
        );
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

        let mut story = agentdash_domain::story::Story::new(
            project.id,
            "backend-a".into(),
            "story".into(),
            "desc".into(),
        );
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
            .find(|mount| mount.id == "main")
            .expect("main mount");
        assert!(!main.supports(ExecutionMountCapability::Exec));
        assert!(main.supports(ExecutionMountCapability::Read));
        assert!(address_space.mounts.iter().any(|mount| mount.id == "spec"));
        assert!(address_space.mounts.iter().any(|mount| mount.id == "brief"));
    }

    #[test]
    fn story_containers_can_disable_and_override_project_defaults() {
        let registry = BackendRegistry::new();
        let service = RelayAddressSpaceService::new(registry);
        let mut project = agentdash_domain::project::Project::new(
            "proj".into(),
            "desc".into(),
            "backend-a".into(),
        );
        project.config.context_containers = vec![
            inline_container("project-spec", "shared", "spec.md", "project spec"),
            inline_container("project-km", "km", "index.md", "project km"),
        ];

        let mut story = agentdash_domain::story::Story::new(
            project.id,
            "backend-a".into(),
            "story".into(),
            "desc".into(),
        );
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
        let registry = BackendRegistry::new();
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
        };

        let read = service
            .read_text(
                &address_space,
                &ResourceRef {
                    mount_id: "brief".to_string(),
                    path: "brief.md".to_string(),
                },
            )
            .await
            .expect("inline read should succeed");
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
            )
            .await
            .expect("inline list should succeed");
        assert!(listed.entries.iter().any(|entry| entry.path == "brief.md"));
        assert!(
            listed
                .entries
                .iter()
                .any(|entry| entry.path == "notes/todo.md")
        );

        let hits = service
            .search_text(&address_space, "brief", ".", "verify", 10)
            .await
            .expect("inline search should succeed");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].contains("notes/todo.md:1"));
    }

    #[tokio::test]
    async fn read_text_routes_via_tool_transport() {
        let registry = BackendRegistry::new();
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
            .expect("session should build");

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
                    )
                    .await
            }
        });

        let message = receiver.recv().await.expect("command should be sent");
        let id = message.id().to_string();
        match message {
            RelayMessage::CommandToolFileRead { payload, .. } => {
                assert_eq!(payload.workspace_root, "/workspace/repo");
                assert_eq!(payload.path, "src/main.rs");
            }
            other => panic!("unexpected message: {other:?}"),
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

        let result = handle
            .await
            .expect("task should complete")
            .expect("read should succeed");
        assert_eq!(result.content, "fn main() {}");
    }

    #[test]
    fn runtime_tool_schemas_are_openai_compatible() {
        let registry = BackendRegistry::new();
        let service = Arc::new(RelayAddressSpaceService::new(registry));
        let address_space = ExecutionAddressSpace {
            mounts: vec![ExecutionMount {
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
                metadata: serde_json::json!({
                    "files": {
                        "brief.md": "hello"
                    }
                }),
            }],
            default_mount_id: Some("brief".to_string()),
        };

        let schemas = vec![
            MountsListTool::new(service.clone(), address_space.clone()).parameters_schema(),
            FsReadTool::new(service.clone(), address_space.clone()).parameters_schema(),
            FsWriteTool::new(service.clone(), address_space.clone()).parameters_schema(),
            FsListTool::new(service.clone(), address_space.clone()).parameters_schema(),
            FsSearchTool::new(service.clone(), address_space.clone()).parameters_schema(),
            ShellExecTool::new(service, address_space).parameters_schema(),
        ];

        for schema in schemas {
            let properties = schema["properties"]
                .as_object()
                .expect("properties should be object");
            let required = schema["required"]
                .as_array()
                .expect("required should be array")
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<std::collections::BTreeSet<_>>();

            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
            for key in properties.keys() {
                assert!(
                    required.contains(key.as_str()),
                    "required should contain property `{key}`"
                );
            }
        }
    }
}
