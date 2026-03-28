use std::{collections::HashMap, path::PathBuf, pin::Pin, sync::Arc};

use agent_client_protocol::{
    ContentBlock, EmbeddedResourceResource, McpServer, SessionNotification,
};
use agentdash_domain::common::AgentConfig;
use async_trait::async_trait;
use futures::Stream;
use futures::stream::BoxStream;
use serde::Serialize;
use thiserror::Error;

use crate::hooks::HookSessionRuntime;

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

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// One prompt invocation == one turn. Used to correlate injected user message
    /// with connector-emitted updates via `_meta.agentdash.trace.turnId`.
    pub turn_id: String,
    /// 当前执行绑定的工作空间根目录。
    /// 对第三方 Agent，这是真正的执行仓根；对云端原生 Agent，则是 mount `main` 的物理根引用。
    pub workspace_root: PathBuf,
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: AgentConfig,
    /// ACP 协议 per-session MCP Server 列表，由 Connector 负责传递给 Agent
    pub mcp_servers: Vec<McpServer>,
    /// 会话级 Address Space 视图。云端原生 Agent 可基于它生成 provider-backed runtime tools。
    pub address_space: Option<ExecutionAddressSpace>,
    /// 会话级 Hook Runtime 快照。
    /// 当前阶段仅作为执行层承载位，后续由 SessionHub / Hook provider 正式填充。
    pub hook_session: Option<Arc<HookSessionRuntime>>,
    /// Session 级别声明的流程工具能力集。
    /// 工具构建时按此裁剪：仅注入声明可用的流程工具。
    #[allow(clippy::type_complexity)]
    pub flow_capabilities: FlowCapabilities,
    /// 会话级 owner 上下文 markdown（project/story 摘要）。
    /// 由 session plan 层填充，注入到 system prompt 头部，不写入用户消息流。
    pub system_context: Option<String>,
}

/// 流程工具能力声明。
/// 按 session 类型在 session plan 阶段填充，runtime tool provider 据此裁剪注入。
#[derive(Debug, Clone, Default)]
pub struct FlowCapabilities {
    /// 是否允许汇报 workflow artifact
    pub workflow_artifact: bool,
    /// 是否允许发起 companion dispatch（仅 Story session）
    pub companion_dispatch: bool,
    /// 是否允许完成 companion session
    pub companion_complete: bool,
    /// 是否允许解析 hook pending action
    pub resolve_hook_action: bool,
}

pub type ExecutionMountCapability = agentdash_domain::common::MountCapability;
pub type ExecutionMount = agentdash_domain::common::Mount;
pub type ExecutionAddressSpace = agentdash_domain::common::AddressSpace;

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

    fn list_executors(&self) -> Vec<AgentInfo>;

    async fn discover_options_stream(
        &self,
        executor: &str,
        variant: Option<&str>,
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError>;

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
}
