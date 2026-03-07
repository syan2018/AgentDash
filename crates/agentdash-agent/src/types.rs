use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 内容片段 — AgentMessage 中的原子内容单元
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    Image { mime_type: String, data: String },
}

impl ContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn extract_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// 工具调用信息（从 LLM 响应中提取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    #[serde(default)]
    pub call_id: Option<String>,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Agent 消息 — 面向会话的消息层（区别于 LLM 原始消息）
///
/// 设计参照 Pi 的 `AgentMessage`：在 LLM Message 之上增加自定义类型扩展能力。
/// `convert_to_llm` 负责在调用模型前将 AgentMessage 映射为 rig::Message。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum AgentMessage {
    User {
        content: Vec<ContentPart>,
    },
    Assistant {
        content: Vec<ContentPart>,
        #[serde(default)]
        tool_calls: Vec<ToolCallInfo>,
    },
    ToolResult {
        tool_call_id: String,
        #[serde(default)]
        call_id: Option<String>,
        content: Vec<ContentPart>,
        #[serde(default)]
        is_error: bool,
    },
}

impl AgentMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            content: vec![ContentPart::text(text)],
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::Assistant {
            content: vec![ContentPart::text(text)],
            tool_calls: vec![],
        }
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        text: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            tool_call_id: tool_call_id.into(),
            call_id: None,
            content: vec![ContentPart::text(text)],
            is_error,
        }
    }

    pub fn tool_result_with_call_id(
        tool_call_id: impl Into<String>,
        call_id: Option<String>,
        text: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            tool_call_id: tool_call_id.into(),
            call_id,
            content: vec![ContentPart::text(text)],
            is_error,
        }
    }

    pub fn first_text(&self) -> Option<&str> {
        match self {
            Self::User { content }
            | Self::Assistant { content, .. }
            | Self::ToolResult { content, .. } => {
                content.iter().find_map(ContentPart::extract_text)
            }
        }
    }
}

// ─── AgentEvent ─────────────────────────────────────────────

/// Agent 生命周期事件 — 参照 Pi 的事件模型
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    AgentStart,
    AgentEnd {
        messages: Vec<AgentMessage>,
    },
    TurnStart,
    TurnEnd,
    MessageStart,
    MessageDelta {
        text: String,
    },
    MessageEnd {
        message: AgentMessage,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    },
}

// ─── AgentTool ──────────────────────────────────────────────

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub content: Vec<ContentPart>,
    pub is_error: bool,
}

/// Agent 工具执行错误
#[derive(Debug, Error)]
pub enum AgentToolError {
    #[error("工具执行失败: {0}")]
    ExecutionFailed(String),
    #[error("参数无效: {0}")]
    InvalidArguments(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Agent 工具 trait — 参照 Pi 的 `AgentTool`
///
/// 实现此 trait 的类型可被注册到 Agent，在模型请求工具调用时自动执行。
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;

    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
    ) -> Result<AgentToolResult, AgentToolError>;
}

/// 类型擦除的工具引用
pub type DynAgentTool = Arc<dyn AgentTool>;

// ─── AgentContext ───────────────────────────────────────────

/// Agent 上下文 — 一次 Agent Loop 运行所需的完整上下文
pub struct AgentContext {
    pub system_prompt: String,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<DynAgentTool>,
}

// ─── AgentError ─────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM 桥接层错误: {0}")]
    Bridge(#[from] crate::bridge::BridgeError),
    #[error("工具执行错误: {tool_name}: {source}")]
    ToolExecution {
        tool_name: String,
        source: AgentToolError,
    },
    #[error("Agent 已被取消")]
    Cancelled,
    #[error("超过最大循环轮数: {0}")]
    MaxTurnsExceeded(usize),
}
