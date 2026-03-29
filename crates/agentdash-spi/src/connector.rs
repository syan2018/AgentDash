use std::{collections::HashMap, path::PathBuf, pin::Pin, sync::Arc};

use agent_client_protocol::{
    ContentBlock, EmbeddedResourceResource, McpServer, SessionNotification,
};
use agentdash_domain::common::{AddressSpace, AgentConfig};
use async_trait::async_trait;
use futures::Stream;
use futures::stream::BoxStream;
use serde::Serialize;
use thiserror::Error;

use crate::hooks::HookSessionRuntimeAccess;
use crate::lifecycle::DynAgentRuntimeDelegate;

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
    pub workspace_root: PathBuf,
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: AgentConfig,
    pub mcp_servers: Vec<McpServer>,
    pub address_space: Option<AddressSpace>,
    pub hook_session: Option<Arc<dyn HookSessionRuntimeAccess>>,
    #[allow(clippy::type_complexity)]
    pub flow_capabilities: FlowCapabilities,
    pub system_context: Option<String>,
    /// 预构建的 Agent Runtime Delegate（由 Application 层基于 hook_session 创建）。
    /// Connector 可直接使用，无需自行构造具体 delegate 类型。
    pub runtime_delegate: Option<DynAgentRuntimeDelegate>,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("turn_id", &self.turn_id)
            .field("workspace_root", &self.workspace_root)
            .field("executor_config", &self.executor_config)
            .field("hook_session", &self.hook_session)
            .field("runtime_delegate", &self.runtime_delegate.as_ref().map(|_| ".."))
            .finish_non_exhaustive()
    }
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
    ) -> Result<Vec<crate::tool::DynAgentTool>, ConnectorError>;
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
