use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreRole {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreMessage {
    pub role: CoreRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<CoreToolCall>,
    #[serde(default)]
    pub is_error: bool,
}

impl CoreMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: CoreRole::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
            is_error: false,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: CoreRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
            is_error: false,
        }
    }

    pub fn assistant_with_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<CoreToolCall>,
    ) -> Self {
        Self {
            role: CoreRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls,
            is_error: false,
        }
    }

    pub fn tool(call_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self {
            role: CoreRole::Tool,
            content: content.into(),
            tool_call_id: Some(call_id.into()),
            tool_calls: Vec::new(),
            is_error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreInput {
    pub message: CoreMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreContext {
    pub system_prompt: String,
    pub history: Vec<CoreMessage>,
    pub tools: Vec<CoreTool>,
    pub max_provider_rounds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub system_prompt: String,
    pub messages: Vec<CoreMessage>,
    pub tools: Vec<CoreTool>,
    pub round: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreToolResult {
    pub call_id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum CoreBeforeToolDecision {
    Invoke { call: CoreToolCall },
    Deny { result: CoreToolResult },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl TokenUsage {
    pub(crate) fn accumulate(&mut self, other: Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    ToolCalls,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderEvent {
    TextDelta {
        delta: String,
    },
    ReasoningDelta {
        delta: String,
    },
    ToolCall {
        call: CoreToolCall,
    },
    Completed {
        finish_reason: FinishReason,
        usage: TokenUsage,
    },
}

pub type ProviderEventStream =
    Pin<Box<dyn Stream<Item = Result<ProviderEvent, CoreError>> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreEvent {
    ProviderRoundStarted {
        round: u32,
    },
    TextDelta {
        round: u32,
        delta: String,
    },
    ReasoningDelta {
        round: u32,
        delta: String,
    },
    ToolCallRequested {
        round: u32,
        call: CoreToolCall,
    },
    ToolCallCompleted {
        round: u32,
        call: CoreToolCall,
        result: CoreToolResult,
    },
    ProviderRoundCompleted {
        round: u32,
        finish_reason: FinishReason,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreOutput {
    pub assistant_message: CoreMessage,
    pub transcript_delta: Vec<CoreMessage>,
    pub usage: TokenUsage,
    pub provider_rounds: u32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("AgentCore 已取消")]
    Cancelled,
    #[error("provider stream 未产生 completed 终态")]
    ProviderStreamDisconnected,
    #[error("provider 返回了与 finish reason 不一致的 tool call 状态")]
    InvalidProviderTerminal,
    #[error("达到 provider round 上限 {max_rounds}")]
    ProviderRoundLimit { max_rounds: u32 },
    #[error("provider 失败 ({code}): {message}")]
    Provider {
        code: String,
        message: String,
        retryable: bool,
    },
    #[error("tool callback 失败: {message}")]
    Tool { message: String, retryable: bool },
    #[error("core callback 失败: {message}")]
    Callback { message: String },
    #[error("provider 请求 typed interaction {interaction_id}: {prompt}")]
    InteractionRequired {
        interaction_id: String,
        prompt: String,
    },
    #[error("provider context overflow requires Agent-owned compaction")]
    ContextOverflow,
}

#[async_trait]
pub trait CoreProvider: Send + Sync {
    async fn stream(&self, request: ProviderRequest) -> Result<ProviderEventStream, CoreError>;
}

#[async_trait]
pub trait CoreToolCallbacks: Send + Sync {
    async fn before_tool(&self, call: CoreToolCall) -> Result<CoreBeforeToolDecision, CoreError> {
        Ok(CoreBeforeToolDecision::Invoke { call })
    }

    async fn invoke(&self, call: CoreToolCall) -> Result<CoreToolResult, CoreError>;

    async fn after_tool(
        &self,
        _call: &CoreToolCall,
        result: CoreToolResult,
    ) -> Result<CoreToolResult, CoreError> {
        Ok(result)
    }
}

#[async_trait]
pub trait CoreCallbacks: Send + Sync {
    async fn emit(&self, event: CoreEvent) -> Result<(), CoreError>;
}
