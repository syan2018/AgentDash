use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::model::content::ContentPart;

// ─── ToolDefinition ─────────────────────────────────────────

/// 工具定义 — 仅 schema 级描述，不持有可执行实例。
/// 工具定义 — 纯 schema 级描述，不持有可执行实例。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema 描述参数结构
    pub parameters: serde_json::Value,
}

// ─── AgentToolResult ────────────────────────────────────────

/// 工具执行结果
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub content: Vec<ContentPart>,
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// 工具执行进度回调
pub type ToolUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>;

/// Owner-declared conversation presentation family. This metadata travels with a tool into the
/// Business Surface; the Runtime never infers presentation from the runtime tool name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum ToolProtocolProjector {
    Command,
    FileChange,
    FsRead,
    FsGrep,
    FsGlob,
    Mcp { server_key: String },
    Dynamic { namespace: Option<String> },
    Vfs { operation: String },
    RuntimeAction { action_key: String },
    WorkspaceModule { operation: String },
    Companion { operation: String },
    Task { operation: String },
    Wait,
    LifecycleComplete,
}

// ─── AgentToolError ─────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AgentToolError {
    #[error("工具执行失败: {0}")]
    ExecutionFailed(String),
    #[error("参数无效: {0}")]
    InvalidArguments(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// ─── AgentTool trait ────────────────────────────────────────

/// 统一工具执行接口。
///
/// 任何 agent runtime（Pi Agent、vibe-kanban、远程 ACP agent）共享的工具合同。
/// 具体 tool 实现在 application 层按业务归属分布。
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;

    /// Returning `None` is an explicit admission failure at Business Surface compilation.
    fn protocol_projector(&self) -> Option<ToolProtocolProjector> {
        None
    }

    fn label(&self) -> &str {
        self.name()
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError>;
}

pub type DynAgentTool = Arc<dyn AgentTool>;

// ─── AgentTool → ToolDefinition 便利转换 ────────────────────

impl ToolDefinition {
    /// 从 `AgentTool` 实例提取 schema 级定义
    pub fn from_tool(tool: &dyn AgentTool) -> Self {
        Self {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            parameters: tool.parameters_schema(),
        }
    }
}
