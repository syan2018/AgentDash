use std::{collections::BTreeSet, collections::HashMap, path::PathBuf, pin::Pin, sync::Arc};

use agent_client_protocol::{ContentBlock, EmbeddedResourceResource, McpServer};
use agentdash_agent_types::AgentMessage;
use agentdash_domain::common::{AgentConfig, Vfs};
use async_trait::async_trait;
use futures::Stream;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hooks::HookSessionRuntimeAccess;
use crate::session_context_bundle::SessionContextBundle;
use agentdash_agent_types::DynAgentRuntimeDelegate;

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

/// Session 级执行上下文（Who + Where）。
///
/// 在当前 turn 内语义上不可变：它是 session 启动时拍下的"身份 + 执行环境"快照。
/// 下一 turn 若有改动则重新组装，不在 turn 内 mutate 这里的字段。
#[derive(Clone)]
pub struct ExecutionSessionFrame {
    pub turn_id: String,
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: AgentConfig,
    /// 本轮完整 MCP 声明。
    ///
    /// 云端内嵌 connector 不自行处理这里的 MCP，而是消费 Application 已经预构建好的
    /// `turn.assembled_tools`。Relay/remote transport connector 可将该结构原样下发给
    /// 远端 agent，由远端 agent 自行建联。
    pub mcp_servers: Vec<McpServer>,
    pub vfs: Option<Vfs>,
    /// 发起本次执行的用户身份（由 HTTP 层注入）。
    pub identity: Option<crate::auth::AuthIdentity>,
}

/// Turn 级执行上下文（How + 运行时控制面）。
///
/// per-turn 动态：工具集、hook runtime、runtime delegate、系统 prompt 产出等；
/// 会随 session hot-update 或 hook 触发而重建。
#[derive(Clone, Default)]
pub struct ExecutionTurnFrame {
    pub hook_session: Option<Arc<dyn HookSessionRuntimeAccess>>,
    pub flow_capabilities: FlowCapabilities,
    pub runtime_delegate: Option<DynAgentRuntimeDelegate>,
    /// 当 session 生命周期层判定为"冷启动仓储恢复"且执行器支持原生恢复时，
    /// 会把重建出的消息历史放在这里，供 connector 恢复连续会话。
    pub restored_session_state: Option<RestoredSessionState>,
    /// 业务上下文 Bundle — connector 侧应优先消费此主数据面。
    ///
    /// 当 connector 能结构化消费 Bundle 时（如 PiAgent），
    /// 应通过 `bundle_id` 跨 turn 差异触发 system prompt 热更新，
    /// 以取代对下方 `assembled_system_prompt` 的依赖。
    pub context_bundle: Option<SessionContextBundle>,
    /// Application 层预组装的完整 system prompt（backward-compat fallback）。
    ///
    /// 自 PR 3 起：主数据面为 `context_bundle`；
    /// 本字段仅作为 Relay / vibe_kanban / 过渡期 PiAgent 的兜底。
    /// PR 8 计划将此字段彻底下线。
    #[deprecated(
        note = "主数据面已迁至 `context_bundle`；connector 应优先消费 Bundle，\
                仅当 Bundle 缺失或协议尚未支持时才退化到该预渲染文本。"
    )]
    pub assembled_system_prompt: Option<String>,
    /// Application 层预构建的工具列表（runtime + direct MCP + relay MCP）。
    ///
    /// 内嵌 connector 只持有并调用这里的 `DynAgentTool`，不重新持有
    /// `McpServer` 声明，也不自行区分 direct / relay MCP。
    pub assembled_tools: Vec<agentdash_agent_types::DynAgentTool>,
}

/// 连接器拿到的一次 `prompt(...)` 调用上下文。
///
/// 拆分为 `session`（Who/Where，不可变）与 `turn`（How，可变）两层，让 connector
/// 能清晰区分"身份 + 执行环境"与"本轮工具 + 运行时控制面"。
#[derive(Clone)]
pub struct ExecutionContext {
    pub session: ExecutionSessionFrame,
    pub turn: ExecutionTurnFrame,
}

/// VFS 中发现的项目级指导文件。
#[derive(Debug, Clone)]
pub struct DiscoveredGuideline {
    /// 文件名（如 `AGENTS.md`）。
    pub file_name: String,
    /// 所属 mount 标识。
    pub mount_id: String,
    /// 相对于 mount 根的路径（如 `AGENTS.md` 或 `packages/foo/AGENTS.md`）。
    pub path: String,
    /// 文件全文内容。
    pub content: String,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("turn_id", &self.session.turn_id)
            .field("executor_config", &self.session.executor_config)
            .field("hook_session", &self.turn.hook_session)
            .field(
                "runtime_delegate",
                &self.turn.runtime_delegate.as_ref().map(|_| ".."),
            )
            .field(
                "restored_session_state",
                &self
                    .turn
                    .restored_session_state
                    .as_ref()
                    .map(|state| state.messages.len()),
            )
            .finish_non_exhaustive()
    }
}

/// 从 `ExecutionContext.session.vfs` 的 default mount 解析工作区路径（`root_ref` 按本地路径处理）。
pub fn workspace_path_from_context(context: &ExecutionContext) -> Result<PathBuf, ConnectorError> {
    let space =
        context.session.vfs.as_ref().ok_or_else(|| {
            ConnectorError::InvalidConfig("ExecutionContext 缺少 vfs".to_string())
        })?;
    let mount = space
        .default_mount()
        .ok_or_else(|| ConnectorError::InvalidConfig("vfs 缺少 default_mount".to_string()))?;
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
    /// Workflow 节点终结：complete_lifecycle_node
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

pub type ExecutionStream = Pin<
    Box<
        dyn Stream<Item = Result<agentdash_protocol::BackboneEnvelope, ConnectorError>>
            + Send
            + 'static,
    >,
>;

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

    /// Phase Node 切换时热更新 session 的工具集。
    /// `tools` 为 application 层预构建好的完整工具列表（runtime + MCP），
    /// connector 直接 replace-set 到运行中 agent。
    /// 默认 no-op — 仅 PiAgentConnector 等 in-process connector 需要实现。
    async fn update_session_tools(
        &self,
        _session_id: &str,
        _tools: Vec<agentdash_agent_types::DynAgentTool>,
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

    /// 向活跃 session 热更新业务上下文 Bundle（out-of-band，不等下轮 prompt）。
    ///
    /// 场景：application 层在非 prompt 边界检测到 context 变化（MCP 热更、
    /// hook snapshot 刷新等）时调用。内嵌 connector 可据此重新渲染 system prompt
    /// 并 `set_system_prompt`；Relay / 远程 connector 保持 no-op（由下一次 prompt
    /// 的 Bundle 透传完成刷新）。
    ///
    /// 默认 no-op —— 仅对结构化消费 Bundle 的 connector（如 PiAgent）实现。
    async fn update_session_context_bundle(
        &self,
        _session_id: &str,
        _bundle: SessionContextBundle,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }
}
