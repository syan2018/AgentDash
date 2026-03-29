use std::sync::Arc;

use agentdash_spi::AddressSpace;
use agentdash_spi::schema::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::address_space::inline_persistence::InlineContentOverlay;
use crate::address_space::relay_service::RelayAddressSpaceService;
use crate::address_space::{
    ExecRequest, ListOptions, ResourceRef, capability_name, parse_mount_uri,
};

pub fn resolve_uri_path(address_space: &AddressSpace, path: &str) -> Result<ResourceRef, String> {
    parse_mount_uri(path, address_space)
}

pub fn ok_text(text: String) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(text)],
        is_error: false,
        details: None,
    }
}

#[derive(Clone)]
pub struct MountsListTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: AddressSpace,
}

impl MountsListTool {
    pub fn new(service: Arc<RelayAddressSpaceService>, address_space: AddressSpace) -> Self {
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
                    "- {}:// — {} (capabilities=[{}])",
                    mount.id, mount.display_name, capabilities
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ok_text(if body.is_empty() {
            "当前会话没有可用 mount".to_string()
        } else {
            format!(
                "路径格式: mount_id://relative/path（省略前缀使用默认 mount）\n\n{}",
                body
            )
        }))
    }
}

#[derive(Clone)]
pub struct FsReadTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: AddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsReadTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: AddressSpace,
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
pub struct FsReadParams {
    /// 统一路径，支持 `mount_id://relative/path` 格式（如 `lifecycle://active/steps/start`）；省略 mount 前缀时使用默认 mount
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
        let target = resolve_uri_path(&self.address_space, &params.path)
            .map_err(AgentToolError::ExecutionFailed)?;
        let result = self
            .service
            .read_text(
                &self.address_space,
                &target,
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
pub struct FsWriteTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: AddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsWriteTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: AddressSpace,
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
pub struct FsWriteParams {
    /// 统一路径，支持 `mount_id://relative/path` 格式；省略 mount 前缀时使用默认 mount
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
        let target = resolve_uri_path(&self.address_space, &params.path)
            .map_err(AgentToolError::ExecutionFailed)?;
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
pub struct FsListTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: AddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsListTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: AddressSpace,
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
pub struct FsListParams {
    /// 统一路径，支持 `mount_id://relative/path` 格式；省略 mount 前缀时使用默认 mount
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
        let target = resolve_uri_path(&self.address_space, params.path.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let result = self
            .service
            .list(
                &self.address_space,
                &target.mount_id,
                ListOptions {
                    path: if target.path.is_empty() {
                        ".".to_string()
                    } else {
                        target.path
                    },
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
pub struct FsSearchTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: AddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsSearchTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: AddressSpace,
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
pub struct FsSearchParams {
    pub query: String,
    /// 搜索根路径，支持 `mount_id://relative/path` 格式；省略 mount 前缀时使用默认 mount
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
        let target = resolve_uri_path(&self.address_space, params.path.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let search_path = if target.path.is_empty() {
            ".".to_string()
        } else {
            target.path
        };
        let (hits, truncated) = self
            .service
            .search_text_extended(
                &self.address_space,
                &crate::address_space::TextSearchParams {
                    mount_id: &target.mount_id,
                    path: &search_path,
                    query: &params.query,
                    is_regex: params.regex,
                    include_glob: params.include.as_deref(),
                    max_results: params.max_results.unwrap_or(50).max(1),
                    context_lines: params.context_lines.unwrap_or(0),
                    overlay: self.overlay.as_ref().map(|arc| arc.as_ref()),
                },
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
pub struct ShellExecTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: AddressSpace,
}
impl ShellExecTool {
    pub fn new(service: Arc<RelayAddressSpaceService>, address_space: AddressSpace) -> Self {
        Self {
            service,
            address_space,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellExecParams {
    /// 工作目录，支持 `mount_id://relative/path` 格式；省略时使用默认 mount 根目录
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
        let target = resolve_uri_path(&self.address_space, params.cwd.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let cwd = if target.path.is_empty() {
            ".".to_string()
        } else {
            target.path
        };
        let result = self
            .service
            .exec(
                &self.address_space,
                &ExecRequest {
                    mount_id: target.mount_id.clone(),
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
                "命令: {}\ncwd: {}://{}\n退出码: {}\n{}",
                params.command, target.mount_id, cwd, result.exit_code, merged
            ))],
            is_error: result.exit_code != 0,
            details: None,
        })
    }
}
