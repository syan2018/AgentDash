/// Address Space 访问层 — Relay 传输实现与 Runtime 工具
///
/// 值类型、路径工具和 Mount 推导逻辑已迁移到 `agentdash_application::address_space`。

use std::sync::Arc;

use agentdash_agent::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolUpdateCallback,
};
use agentdash_agent::tools::schema_value;
use agentdash_executor::{
    ConnectorError, ExecutionAddressSpace, ExecutionContext, ExecutionMountCapability,
    RuntimeToolProvider,
};
use agentdash_relay::{
    RelayMessage, ToolFileListPayload, ToolFileReadPayload, ToolFileWritePayload,
    ToolShellExecPayload,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

pub use agentdash_application::address_space::*;

use crate::relay::registry::BackendRegistry;

const MAX_SEARCH_FILE_BYTES: u64 = 256 * 1024;

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
        build_derived_address_space(project, Some(story), workspace, agent_type, SessionMountTarget::Task)
    }

    pub fn build_story_address_space(
        &self,
        project: &agentdash_domain::project::Project,
        story: &agentdash_domain::story::Story,
        workspace: Option<&agentdash_domain::workspace::Workspace>,
        agent_type: Option<&str>,
    ) -> Result<ExecutionAddressSpace, String> {
        build_derived_address_space(project, Some(story), workspace, agent_type, SessionMountTarget::Story)
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
        let response = self.backend_registry
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
        let response = self.backend_registry
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
        let response = self.backend_registry
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
        let response = self.backend_registry
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

// ─── Runtime Tool Provider ──────────────────────────────────

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

// ─── Tool Implementations ───────────────────────────────────

#[derive(Clone)]
struct MountsListTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: ExecutionAddressSpace,
}

impl MountsListTool {
    fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self {
        Self { service, address_space }
    }
}

#[async_trait]
impl AgentTool for MountsListTool {
    fn name(&self) -> &str { "mounts_list" }
    fn description(&self) -> &str { "列出当前会话可访问的 Address Space 挂载与能力" }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {}, "required": [], "additionalProperties": false })
    }
    async fn execute(&self, _: &str, _: serde_json::Value, _: CancellationToken, _: Option<ToolUpdateCallback>) -> Result<AgentToolResult, AgentToolError> {
        let mounts = self.service.list_mounts(&self.address_space);
        let body = mounts.iter().map(|mount| {
            let capabilities = mount.capabilities.iter().map(capability_name).collect::<Vec<_>>().join(", ");
            format!("- {}: {} (provider={}, root_ref={}, capabilities=[{}])", mount.id, mount.display_name, mount.provider, mount.root_ref, capabilities)
        }).collect::<Vec<_>>().join("\n");
        Ok(ok_text(if body.is_empty() { "当前会话没有可用 mount".to_string() } else { body }))
    }
}

#[derive(Clone)]
struct FsReadTool { service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace }
impl FsReadTool { fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self { Self { service, address_space } } }

#[derive(Debug, Deserialize, JsonSchema)]
struct FsReadParams { pub mount: Option<String>, pub path: String, pub start_line: Option<usize>, pub end_line: Option<usize> }

#[async_trait]
impl AgentTool for FsReadTool {
    fn name(&self) -> &str { "fs_read" }
    fn description(&self) -> &str { "读取指定 mount 下的文本文件内容" }
    fn parameters_schema(&self) -> serde_json::Value { schema_value::<FsReadParams>() }
    async fn execute(&self, _: &str, args: serde_json::Value, _: CancellationToken, _: Option<ToolUpdateCallback>) -> Result<AgentToolResult, AgentToolError> {
        let params: FsReadParams = serde_json::from_value(args).map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref()).map_err(AgentToolError::ExecutionFailed)?;
        let result = self.service.read_text(&self.address_space, &ResourceRef { mount_id, path: params.path }).await.map_err(AgentToolError::ExecutionFailed)?;
        let lines = result.content.lines().collect::<Vec<_>>();
        let start = params.start_line.unwrap_or(1).max(1);
        let end = params.end_line.unwrap_or(lines.len()).max(start);
        let selected = lines.iter().enumerate().filter_map(|(i, line)| {
            let n = i + 1;
            (n >= start && n <= end).then(|| format!("{:>4} | {}", n, line))
        }).collect::<Vec<_>>().join("\n");
        Ok(ok_text(format!("文件: {}\n{}", result.path, if selected.is_empty() { "   1 | ".to_string() } else { selected })))
    }
}

#[derive(Clone)]
struct FsWriteTool { service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace }
impl FsWriteTool { fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self { Self { service, address_space } } }

#[derive(Debug, Deserialize, JsonSchema)]
struct FsWriteParams { pub mount: Option<String>, pub path: String, pub content: String, pub append: Option<bool> }

#[async_trait]
impl AgentTool for FsWriteTool {
    fn name(&self) -> &str { "fs_write" }
    fn description(&self) -> &str { "向指定 mount 下的文件写入内容" }
    fn parameters_schema(&self) -> serde_json::Value { schema_value::<FsWriteParams>() }
    async fn execute(&self, _: &str, args: serde_json::Value, _: CancellationToken, _: Option<ToolUpdateCallback>) -> Result<AgentToolResult, AgentToolError> {
        let params: FsWriteParams = serde_json::from_value(args).map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref()).map_err(AgentToolError::ExecutionFailed)?;
        let target = ResourceRef { mount_id, path: params.path };
        let final_content = if params.append.unwrap_or(false) {
            match self.service.read_text(&self.address_space, &target).await {
                Ok(existing) => format!("{}{}", existing.content, params.content),
                Err(_) => params.content,
            }
        } else { params.content };
        self.service.write_text(&self.address_space, &target, &final_content).await.map_err(AgentToolError::ExecutionFailed)?;
        Ok(ok_text(format!("已写入文件: {}", target.path)))
    }
}

#[derive(Clone)]
struct FsListTool { service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace }
impl FsListTool { fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self { Self { service, address_space } } }

#[derive(Debug, Deserialize, JsonSchema)]
struct FsListParams { pub mount: Option<String>, pub path: Option<String>, pub recursive: Option<bool>, pub pattern: Option<String> }

#[async_trait]
impl AgentTool for FsListTool {
    fn name(&self) -> &str { "fs_list" }
    fn description(&self) -> &str { "列出指定 mount 下的目录内容" }
    fn parameters_schema(&self) -> serde_json::Value { schema_value::<FsListParams>() }
    async fn execute(&self, _: &str, args: serde_json::Value, _: CancellationToken, _: Option<ToolUpdateCallback>) -> Result<AgentToolResult, AgentToolError> {
        let params: FsListParams = serde_json::from_value(args).map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref()).map_err(AgentToolError::ExecutionFailed)?;
        let result = self.service.list(&self.address_space, &mount_id, ListOptions { path: params.path.unwrap_or_else(|| ".".to_string()), pattern: params.pattern, recursive: params.recursive.unwrap_or(false) }).await.map_err(AgentToolError::ExecutionFailed)?;
        let lines = result.entries.into_iter().map(|e| { let kind = if e.is_dir { "dir" } else { "file" }; format!("[{}] {}", kind, e.path.replace('\\', "/")) }).collect::<Vec<_>>().join("\n");
        Ok(ok_text(if lines.is_empty() { "(空目录)".to_string() } else { lines }))
    }
}

#[derive(Clone)]
struct FsSearchTool { service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace }
impl FsSearchTool { fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self { Self { service, address_space } } }

#[derive(Debug, Deserialize, JsonSchema)]
struct FsSearchParams { pub mount: Option<String>, pub query: String, pub path: Option<String>, pub max_results: Option<usize> }

#[async_trait]
impl AgentTool for FsSearchTool {
    fn name(&self) -> &str { "fs_search" }
    fn description(&self) -> &str { "在指定 mount 下进行文本搜索" }
    fn parameters_schema(&self) -> serde_json::Value { schema_value::<FsSearchParams>() }
    async fn execute(&self, _: &str, args: serde_json::Value, _: CancellationToken, _: Option<ToolUpdateCallback>) -> Result<AgentToolResult, AgentToolError> {
        let params: FsSearchParams = serde_json::from_value(args).map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref()).map_err(AgentToolError::ExecutionFailed)?;
        let hits = self.service.search_text(&self.address_space, &mount_id, params.path.as_deref().unwrap_or("."), &params.query, params.max_results.unwrap_or(50).max(1)).await.map_err(AgentToolError::ExecutionFailed)?;
        Ok(ok_text(if hits.is_empty() { "未找到匹配结果".to_string() } else { hits.join("\n") }))
    }
}

#[derive(Clone)]
struct ShellExecTool { service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace }
impl ShellExecTool { fn new(service: Arc<RelayAddressSpaceService>, address_space: ExecutionAddressSpace) -> Self { Self { service, address_space } } }

#[derive(Debug, Deserialize, JsonSchema)]
struct ShellExecParams { pub mount: Option<String>, pub cwd: Option<String>, pub command: String, pub timeout_secs: Option<u64> }

#[async_trait]
impl AgentTool for ShellExecTool {
    fn name(&self) -> &str { "shell_exec" }
    fn description(&self) -> &str { "在指定 mount 下执行 shell 命令" }
    fn parameters_schema(&self) -> serde_json::Value { schema_value::<ShellExecParams>() }
    async fn execute(&self, _: &str, args: serde_json::Value, _: CancellationToken, _: Option<ToolUpdateCallback>) -> Result<AgentToolResult, AgentToolError> {
        let params: ShellExecParams = serde_json::from_value(args).map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let mount_id = resolve_mount_id(&self.address_space, params.mount.as_deref()).map_err(AgentToolError::ExecutionFailed)?;
        let cwd = params.cwd.unwrap_or_else(|| ".".to_string());
        let result = self.service.exec(&self.address_space, &ExecRequest { mount_id: mount_id.clone(), cwd: cwd.clone(), command: params.command.clone(), timeout_ms: params.timeout_secs.map(|s| s.saturating_mul(1000)) }).await.map_err(AgentToolError::ExecutionFailed)?;
        let merged = if result.stderr.trim().is_empty() { result.stdout } else if result.stdout.trim().is_empty() { format!("[stderr]\n{}", result.stderr) } else { format!("[stdout]\n{}\n\n[stderr]\n{}", result.stdout, result.stderr) };
        Ok(AgentToolResult { content: vec![ContentPart::text(format!("命令: {}\nmount: {}\ncwd: {}\n退出码: {}\n{}", params.command, mount_id, cwd, result.exit_code, merged))], is_error: result.exit_code != 0, details: None })
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
        let mut project = agentdash_domain::project::Project::new("proj".into(), "desc".into(), "backend-a".into());
        project.config.context_containers = vec![inline_container("project-spec", "spec", "backend/spec.md", "# spec")];
        project.config.mount_policy = MountDerivationPolicy {
            include_local_workspace: true,
            local_workspace_capabilities: vec![ContextContainerCapability::Read, ContextContainerCapability::List],
        };

        let mut story = agentdash_domain::story::Story::new(project.id, "backend-a".into(), "story".into(), "desc".into());
        story.context.context_containers = vec![inline_container("story-brief", "brief", "brief.md", "story brief")];

        let address_space = service
            .build_task_address_space(&project, &story, Some(&sample_workspace()), Some("PI_AGENT"))
            .expect("address space should build");

        assert_eq!(address_space.default_mount_id.as_deref(), Some("main"));
        assert_eq!(address_space.mounts.len(), 3);
        let main = address_space.mounts.iter().find(|m| m.id == "main").expect("main mount");
        assert!(!main.supports(ExecutionMountCapability::Exec));
        assert!(main.supports(ExecutionMountCapability::Read));
        assert!(address_space.mounts.iter().any(|m| m.id == "spec"));
        assert!(address_space.mounts.iter().any(|m| m.id == "brief"));
    }

    #[test]
    fn story_containers_can_disable_and_override_project_defaults() {
        let registry = crate::relay::registry::BackendRegistry::new();
        let service = RelayAddressSpaceService::new(registry);
        let mut project = agentdash_domain::project::Project::new("proj".into(), "desc".into(), "backend-a".into());
        project.config.context_containers = vec![
            inline_container("project-spec", "shared", "spec.md", "project spec"),
            inline_container("project-km", "km", "index.md", "project km"),
        ];

        let mut story = agentdash_domain::story::Story::new(project.id, "backend-a".into(), "story".into(), "desc".into());
        story.context.disabled_container_ids = vec!["project-km".into()];
        story.context.context_containers = vec![inline_container("story-spec", "shared", "spec.md", "story override")];

        let address_space = service
            .build_task_address_space(&project, &story, None, Some("PI_AGENT"))
            .expect("address space should build");

        assert_eq!(address_space.mounts.len(), 1);
        let mount = &address_space.mounts[0];
        assert_eq!(mount.id, "shared");
        let files = inline_files_from_mount(mount).expect("inline files");
        assert_eq!(files.get("spec.md").map(String::as_str), Some("story override"));
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
                            ContextContainerFile { path: "brief.md".to_string(), content: "hello inline mount".to_string() },
                            ContextContainerFile { path: "notes/todo.md".to_string(), content: "todo: verify inline search".to_string() },
                        ],
                    },
                    capabilities: vec![ContextContainerCapability::Read, ContextContainerCapability::List, ContextContainerCapability::Search],
                    default_write: false,
                    exposure: ContextContainerExposure::default(),
                }).expect("mount should build"),
            ],
            default_mount_id: Some("brief".to_string()),
        };

        let read = service.read_text(&address_space, &ResourceRef { mount_id: "brief".to_string(), path: "brief.md".to_string() }).await.expect("inline read");
        assert_eq!(read.content, "hello inline mount");

        let listed = service.list(&address_space, "brief", ListOptions { path: ".".to_string(), pattern: None, recursive: true }).await.expect("inline list");
        assert!(listed.entries.iter().any(|e| e.path == "brief.md"));
        assert!(listed.entries.iter().any(|e| e.path == "notes/todo.md"));

        let hits = service.search_text(&address_space, "brief", ".", "verify", 10).await.expect("inline search");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].contains("notes/todo.md:1"));
    }

    #[tokio::test]
    async fn read_text_routes_via_tool_transport() {
        let registry = crate::relay::registry::BackendRegistry::new();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        registry.try_register(ConnectedBackend {
            backend_id: "backend-a".to_string(),
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            capabilities: agentdash_relay::CapabilitiesPayload { executors: Vec::new(), supports_cancel: true, supports_workspace_files: true, supports_discover_options: true },
            accessible_roots: vec!["/workspace".to_string()],
            sender,
            connected_at: Utc::now(),
        }).await.expect("backend should register");

        let service = RelayAddressSpaceService::new(registry.clone());
        let session = service.session_for_workspace(&sample_workspace()).expect("session");

        let handle = tokio::spawn({
            let service = service.clone();
            let session = session.clone();
            async move { service.read_text(&session, &ResourceRef { mount_id: "main".to_string(), path: "src/main.rs".to_string() }).await }
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

        let resolved = registry.resolve_response(&RelayMessage::ResponseToolFileRead {
            id,
            payload: Some(agentdash_relay::ToolFileReadResponse { call_id: "call".to_string(), content: "fn main() {}".to_string(), encoding: "utf-8".to_string() }),
            error: None,
        }).await;
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
                capabilities: vec![ExecutionMountCapability::Read, ExecutionMountCapability::List, ExecutionMountCapability::Search],
                default_write: false,
                display_name: "brief".to_string(),
                metadata: serde_json::json!({ "files": { "brief.md": "hello" } }),
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
            let properties = schema["properties"].as_object().expect("properties");
            let required = schema["required"].as_array().expect("required").iter().filter_map(serde_json::Value::as_str).collect::<std::collections::BTreeSet<_>>();
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
            for key in properties.keys() {
                assert!(required.contains(key.as_str()), "required should contain `{key}`");
            }
        }
    }
}
