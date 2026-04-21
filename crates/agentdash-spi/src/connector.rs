use std::{collections::BTreeSet, collections::HashMap, path::PathBuf, pin::Pin, sync::Arc};

use agent_client_protocol::{
    ContentBlock, EmbeddedResourceResource, McpServer, SessionNotification,
};
use agentdash_agent_types::AgentMessage;
use agentdash_domain::common::{Vfs, AgentConfig};
use async_trait::async_trait;
use futures::Stream;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hooks::HookSessionRuntimeAccess;
use agentdash_agent_types::DynAgentRuntimeDelegate;
use crate::session_capabilities::SessionBaselineCapabilities;

/// 连接器类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorType {
    /// 本地子进程执行器（Claude Code, Codex, AMP 等）
    LocalExecutor,
    /// 远程 ACP 后端
    RemoteAcpBackend,
}

/// 连接器能力声明
#[derive(Debug, Clone, Default, Serialize)]
pub struct ConnectorCapabilities {
    pub supports_cancel: bool,
    pub supports_discovery: bool,
    pub supports_variants: bool,
    pub supports_model_override: bool,
    pub supports_permission_policy: bool,
}

/// 连接器对外暴露的执行器选项（用于前端选择器渲染）
#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
}

#[derive(Clone)]
pub struct ExecutionContext {
    pub turn_id: String,
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: AgentConfig,
    pub mcp_servers: Vec<McpServer>,
    /// 配置中显式标记为 relay 的 MCP server name 集合。
    /// connector 根据此集合决定走 relay 路径还是直连路径。
    #[allow(dead_code)]
    pub relay_mcp_server_names: std::collections::HashSet<String>,
    pub vfs: Option<Vfs>,
    pub hook_session: Option<Arc<dyn HookSessionRuntimeAccess>>,
    #[allow(clippy::type_complexity)]
    pub flow_capabilities: FlowCapabilities,
    pub system_context: Option<String>,
    /// 预构建的 Agent Runtime Delegate（由 Application 层基于 hook_session 创建）。
    /// Connector 可直接使用，无需自行构造具体 delegate 类型。
    pub runtime_delegate: Option<DynAgentRuntimeDelegate>,
    /// 发起本次执行的用户身份（由 HTTP 层注入）。
    pub identity: Option<crate::auth::AuthIdentity>,
    /// 当 session 生命周期层判定为”冷启动仓储恢复”且执行器支持原生恢复时，
    /// 会把重建出的消息历史放在这里，供 connector 恢复连续会话。
    pub restored_session_state: Option<RestoredSessionState>,
    /// 会话级 baseline capabilities（companion agents + skills），
    /// 由 prompt pipeline 统一组装。
    pub session_capabilities: Option<SessionBaselineCapabilities>,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("turn_id", &self.turn_id)
            .field("executor_config", &self.executor_config)
            .field("hook_session", &self.hook_session)
            .field(
                "runtime_delegate",
                &self.runtime_delegate.as_ref().map(|_| ".."),
            )
            .field(
                "restored_session_state",
                &self
                    .restored_session_state
                    .as_ref()
                    .map(|state| state.messages.len()),
            )
            .finish_non_exhaustive()
    }
}

/// 从 `ExecutionContext.vfs` 的 default mount 解析工作区路径（`root_ref` 按本地路径处理）。
pub fn workspace_path_from_context(context: &ExecutionContext) -> Result<PathBuf, ConnectorError> {
    let space = context.vfs.as_ref().ok_or_else(|| {
        ConnectorError::InvalidConfig("ExecutionContext 缺少 vfs".to_string())
    })?;
    let mount = space.default_mount().ok_or_else(|| {
        ConnectorError::InvalidConfig("vfs 缺少 default_mount".to_string())
    })?;
    let path = PathBuf::from(mount.root_ref.trim());
    if path.as_os_str().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "default mount 的 root_ref 为空".to_string(),
        ));
    }
    Ok(path)
}

#[derive(Debug, Clone, Default)]
pub struct RestoredSessionState {
    pub messages: Vec<AgentMessage>,
}

/// 工具簇标识 — 每个簇控制一组相关工具的注入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCluster {
    /// Read-only access: mounts_list, fs_read, fs_glob, fs_grep
    Read,
    /// File editing: fs_apply_patch
    Write,
    /// 命令执行：shell_exec
    Execute,
    /// Workflow 产出汇报：report_workflow_artifact
    Workflow,
    /// 协作与交互：companion_request, companion_respond
    Collaboration,
    /// Canvas 资产：canvases_list, canvas_start, bind_canvas_data, present_canvas
    Canvas,
}

/// 流程工具能力声明。
/// 按 session 类型在 session plan 阶段填充，runtime tool provider 据此裁剪注入。
/// 可进一步与 agent base_config 中的 tool_clusters 做交集裁剪。
///
/// 支持两层裁剪：cluster 级开关 + 工具级排除。
#[derive(Debug, Clone, Default)]
pub struct FlowCapabilities {
    pub enabled_clusters: BTreeSet<ToolCluster>,
    /// 工具级排除集：即使所属 cluster 已启用，名在此集合中的工具仍不注入。
    pub excluded_tools: BTreeSet<String>,
}

impl FlowCapabilities {
    /// 全量簇 — 适用于 Story session 等需要全部工具的场景。
    pub fn all() -> Self {
        Self {
            enabled_clusters: BTreeSet::from([
                ToolCluster::Read,
                ToolCluster::Write,
                ToolCluster::Execute,
                ToolCluster::Workflow,
                ToolCluster::Collaboration,
                ToolCluster::Canvas,
            ]),
            excluded_tools: BTreeSet::new(),
        }
    }

    /// 检查指定簇是否启用。
    pub fn has(&self, cluster: ToolCluster) -> bool {
        self.enabled_clusters.contains(&cluster)
    }

    /// 检查指定工具是否可用（cluster 启用 且 未被排除）。
    pub fn is_tool_enabled(&self, tool_name: &str, cluster: ToolCluster) -> bool {
        self.has(cluster) && !self.excluded_tools.contains(tool_name)
    }

    /// 从簇数组构造。
    pub fn from_clusters(clusters: impl IntoIterator<Item = ToolCluster>) -> Self {
        Self {
            enabled_clusters: clusters.into_iter().collect(),
            excluded_tools: BTreeSet::new(),
        }
    }

    /// 与另一个 FlowCapabilities 做交集（用于 agent 级配置裁剪）。
    pub fn intersect(&self, other: &FlowCapabilities) -> Self {
        Self {
            enabled_clusters: self
                .enabled_clusters
                .intersection(&other.enabled_clusters)
                .copied()
                .collect(),
            excluded_tools: self
                .excluded_tools
                .union(&other.excluded_tools)
                .cloned()
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PromptPayload {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

pub fn content_block_to_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => {
            if text.text.trim().is_empty() {
                None
            } else {
                Some(text.text.clone())
            }
        }
        ContentBlock::Resource(resource) => match &resource.resource {
            EmbeddedResourceResource::TextResourceContents(text_res) => Some(format!(
                "\n<file path=\"{}\">\n{}\n</file>",
                text_res.uri, text_res.text
            )),
            EmbeddedResourceResource::BlobResourceContents(blob_res) => Some(format!(
                "[引用二进制资源: {}; mimeType={}]",
                blob_res.uri,
                blob_res.mime_type.as_deref().unwrap_or("unknown")
            )),
            _ => Some("[引用资源: 未知类型]".to_string()),
        },
        ContentBlock::ResourceLink(link) => {
            Some(format!("[引用文件: {} ({})]", link.name, link.uri))
        }
        ContentBlock::Image(image) => Some(format!(
            "[引用图片: mimeType={}, base64Bytes={}]",
            image.mime_type,
            image.data.len()
        )),
        ContentBlock::Audio(audio) => Some(format!(
            "[引用音频: mimeType={}, base64Bytes={}]",
            audio.mime_type,
            audio.data.len()
        )),
        _ => Some("[引用内容块: 未知类型]".to_string()),
    }
}

impl PromptPayload {
    pub fn to_fallback_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Blocks(blocks) => blocks
                .iter()
                .filter_map(content_block_to_text)
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_block(value: serde_json::Value) -> ContentBlock {
        serde_json::from_value::<ContentBlock>(value).expect("valid ACP content block")
    }

    #[test]
    fn prompt_payload_blocks_to_fallback_text() {
        let blocks = vec![
            parse_block(json!({ "type": "text", "text": "请分析这个实现" })),
            parse_block(json!({
                "type": "resource",
                "resource": {
                    "uri": "file:///workspace/src/main.rs",
                    "mimeType": "text/rust",
                    "text": "fn main() {}"
                }
            })),
            parse_block(json!({
                "type": "resource_link",
                "name": "src/lib.rs",
                "uri": "file:///workspace/src/lib.rs"
            })),
            parse_block(json!({
                "type": "image",
                "mimeType": "image/png",
                "data": "AAAA"
            })),
            parse_block(json!({
                "type": "audio",
                "mimeType": "audio/wav",
                "data": "BBBB"
            })),
        ];

        let text = PromptPayload::Blocks(blocks).to_fallback_text();
        assert!(text.contains("请分析这个实现"));
        assert!(text.contains("<file path=\"file:///workspace/src/main.rs\">"));
        assert!(text.contains("[引用文件: src/lib.rs (file:///workspace/src/lib.rs)]"));
        assert!(text.contains("[引用图片: mimeType=image/png"));
        assert!(text.contains("[引用音频: mimeType=audio/wav"));
    }
}

pub type ExecutionStream =
    Pin<Box<dyn Stream<Item = Result<SessionNotification, ConnectorError>> + Send + 'static>>;

/// 运行时工具构建 SPI。
/// 由 application 层持有，executor 层提供具体实现。
#[async_trait]
pub trait RuntimeToolProvider: Send + Sync {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<agentdash_agent_types::DynAgentTool>, ConnectorError>;
}

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("执行器配置无效: {0}")]
    InvalidConfig(String),
    #[error("执行器启动失败: {0}")]
    SpawnFailed(String),
    #[error("执行器运行错误: {0}")]
    Runtime(String),
    #[error("连接失败: {0}")]
    ConnectionFailed(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[async_trait]
pub trait AgentConnector: Send + Sync {
    fn connector_id(&self) -> &'static str;

    fn connector_type(&self) -> ConnectorType;

    fn capabilities(&self) -> ConnectorCapabilities;

    /// 指示给定 executor 是否支持基于 session 仓储的原生消息恢复。
    ///
    /// 当返回 `true` 时，session 生命周期层会在冷启动 continuation 场景下
    /// 传入 `ExecutionContext.restored_session_state`，而不是退化为 continuation 文本。
    fn supports_repository_restore(&self, _executor: &str) -> bool {
        false
    }

    fn list_executors(&self) -> Vec<AgentInfo>;

    async fn discover_options_stream(
        &self,
        executor: &str,
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError>;

    /// 返回当前进程内该 session 是否仍有可直接续跑的执行器 runtime。
    ///
    /// 这与 session 事件广播或订阅状态不同；仅用于判断是否可以跳过仓储恢复。
    async fn has_live_session(&self, _session_id: &str) -> bool {
        false
    }

    async fn prompt(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        prompt: &PromptPayload,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError>;

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError>;

    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<(), ConnectorError>;

    async fn reject_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConnectorError>;

    /// Phase Node 切换时热更新 session 的 MCP server 列表。
    /// 语义为"目标集合替换"（replace-set），而非增量追加。
    /// 默认 no-op — 仅 PiAgentConnector 等 in-process connector 需要实现。
    async fn update_session_mcp_servers(
        &self,
        _session_id: &str,
        _mcp_servers: Vec<McpServer>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }

    /// 向活跃 session 注入一条用户消息（用于能力变更等 out-of-band 通知）。
    ///
    /// 与 `prompt` 不同，这里只是把消息塞进 steering 队列，
    /// 下一次 LLM 调用前会被自动合并到对话末尾，保持 KV cache 前缀稳定。
    /// 默认 no-op — 仅 in-process connector（如 PiAgent）需要实现。
    async fn push_session_notification(
        &self,
        _session_id: &str,
        _message: String,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}
