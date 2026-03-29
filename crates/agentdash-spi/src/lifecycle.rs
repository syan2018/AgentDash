//! Agent 生命周期 SPI — 定义 Agent Runtime 与 Hook/编排层的交互接口。
//!
//! 这些类型从 `agentdash-agent` 抽取到此处，作为所有 Agent 运行时的通用合约。
//! `agentdash-agent` 通过 re-export 保持向后兼容。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::tool::{AgentToolResult, ContentPart, DynAgentTool};

/// 当前时间戳（毫秒）
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ─── ToolCallInfo ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    #[serde(default)]
    pub call_id: Option<String>,
    pub name: String,
    pub arguments: serde_json::Value,
}

// ─── StopReason ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

// ─── TokenUsage ─────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
}

// ─── AgentMessage ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum AgentMessage {
    User {
        content: Vec<ContentPart>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    Assistant {
        content: Vec<ContentPart>,
        #[serde(default)]
        tool_calls: Vec<ToolCallInfo>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stop_reason: Option<StopReason>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    ToolResult {
        tool_call_id: String,
        #[serde(default)]
        call_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        content: Vec<ContentPart>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
        #[serde(default)]
        is_error: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
}

impl AgentMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            content: vec![ContentPart::text(text)],
            timestamp: Some(now_millis()),
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::Assistant {
            content: vec![ContentPart::text(text)],
            tool_calls: vec![],
            stop_reason: None,
            error_message: None,
            usage: None,
            timestamp: Some(now_millis()),
        }
    }

    pub fn error_assistant(error_message: impl Into<String>, aborted: bool) -> Self {
        let msg = error_message.into();
        Self::Assistant {
            content: vec![ContentPart::text("")],
            tool_calls: vec![],
            stop_reason: Some(if aborted {
                StopReason::Aborted
            } else {
                StopReason::Error
            }),
            error_message: Some(msg),
            usage: Some(TokenUsage::default()),
            timestamp: Some(now_millis()),
        }
    }

    pub fn is_error_or_aborted(&self) -> bool {
        matches!(
            self,
            AgentMessage::Assistant {
                stop_reason: Some(StopReason::Error | StopReason::Aborted),
                ..
            }
        )
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        text: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            tool_call_id: tool_call_id.into(),
            call_id: None,
            tool_name: None,
            content: vec![ContentPart::text(text)],
            details: None,
            is_error,
            timestamp: Some(now_millis()),
        }
    }

    pub fn tool_result_full(
        tool_call_id: impl Into<String>,
        call_id: Option<String>,
        tool_name: Option<String>,
        content: Vec<ContentPart>,
        details: Option<serde_json::Value>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            tool_call_id: tool_call_id.into(),
            call_id,
            tool_name,
            content,
            details,
            is_error,
            timestamp: Some(now_millis()),
        }
    }

    pub fn first_text(&self) -> Option<&str> {
        match self {
            Self::User { content, .. }
            | Self::Assistant { content, .. }
            | Self::ToolResult { content, .. } => {
                content.iter().find_map(ContentPart::extract_text)
            }
        }
    }
}

// ─── AgentContext ───────────────────────────────────────────

#[derive(Clone)]
pub struct AgentContext {
    pub system_prompt: String,
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<DynAgentTool>,
}

impl std::fmt::Debug for AgentContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentContext")
            .field("system_prompt", &self.system_prompt)
            .field("messages_count", &self.messages.len())
            .field("tools_count", &self.tools.len())
            .finish()
    }
}

// ─── Tool Approval Types ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolApprovalRequest {
    pub tool_call: ToolCallInfo,
    pub args: serde_json::Value,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ToolApprovalOutcome {
    Approved,
    Rejected {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

// ─── Hook 输入/输出 DTO ────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct BeforeToolCallResult {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AfterToolCallResult {
    pub content: Option<Vec<ContentPart>>,
    pub details: Option<serde_json::Value>,
    pub is_error: Option<bool>,
}

pub struct BeforeToolCallContext<'a> {
    pub assistant_message: &'a AgentMessage,
    pub tool_call: &'a ToolCallInfo,
    pub args: &'a serde_json::Value,
    pub context: &'a AgentContext,
}

pub struct AfterToolCallContext<'a> {
    pub assistant_message: &'a AgentMessage,
    pub tool_call: &'a ToolCallInfo,
    pub args: &'a serde_json::Value,
    pub result: &'a AgentToolResult,
    pub is_error: bool,
    pub context: &'a AgentContext,
}

// ─── RuntimeDelegate 输入/输出 ─────────────────────────────

#[derive(Debug, Clone)]
pub struct TransformContextInput {
    pub context: AgentContext,
}

#[derive(Debug, Clone)]
pub struct TransformContextOutput {
    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone)]
pub struct BeforeToolCallInput {
    pub assistant_message: AgentMessage,
    pub tool_call: ToolCallInfo,
    pub args: serde_json::Value,
    pub context: AgentContext,
}

#[derive(Debug, Clone)]
pub enum ToolCallDecision {
    Allow,
    Deny {
        reason: String,
    },
    Ask {
        reason: String,
        args: Option<serde_json::Value>,
        details: Option<serde_json::Value>,
    },
    Rewrite {
        args: serde_json::Value,
        note: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct AfterToolCallInput {
    pub assistant_message: AgentMessage,
    pub tool_call: ToolCallInfo,
    pub args: serde_json::Value,
    pub result: AgentToolResult,
    pub is_error: bool,
    pub context: AgentContext,
}

#[derive(Debug, Clone, Default)]
pub struct AfterToolCallEffects {
    pub content: Option<Vec<ContentPart>>,
    pub details: Option<serde_json::Value>,
    pub is_error: Option<bool>,
    pub refresh_snapshot: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AfterTurnInput {
    pub context: AgentContext,
    pub message: AgentMessage,
    pub tool_results: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Default)]
pub struct TurnControlDecision {
    pub steering: Vec<AgentMessage>,
    pub follow_up: Vec<AgentMessage>,
    pub refresh_snapshot: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BeforeStopInput {
    pub context: AgentContext,
}

#[derive(Debug, Clone)]
pub enum StopDecision {
    Stop,
    Continue {
        steering: Vec<AgentMessage>,
        follow_up: Vec<AgentMessage>,
        reason: Option<String>,
    },
}

// ─── AgentRuntimeDelegate ──────────────────────────────────

#[derive(Debug, Error)]
pub enum AgentRuntimeError {
    #[error("{0}")]
    Runtime(String),
}

#[async_trait]
pub trait AgentRuntimeDelegate: Send + Sync {
    async fn transform_context(
        &self,
        input: TransformContextInput,
        cancel: CancellationToken,
    ) -> Result<TransformContextOutput, AgentRuntimeError>;

    async fn before_tool_call(
        &self,
        input: BeforeToolCallInput,
        cancel: CancellationToken,
    ) -> Result<ToolCallDecision, AgentRuntimeError>;

    async fn after_tool_call(
        &self,
        input: AfterToolCallInput,
        cancel: CancellationToken,
    ) -> Result<AfterToolCallEffects, AgentRuntimeError>;

    async fn after_turn(
        &self,
        input: AfterTurnInput,
        cancel: CancellationToken,
    ) -> Result<TurnControlDecision, AgentRuntimeError>;

    async fn before_stop(
        &self,
        input: BeforeStopInput,
        cancel: CancellationToken,
    ) -> Result<StopDecision, AgentRuntimeError>;
}

pub type DynAgentRuntimeDelegate = std::sync::Arc<dyn AgentRuntimeDelegate>;

// ─── AgentMessage 便利转换 ──────────────────────────────────

impl From<String> for AgentMessage {
    fn from(text: String) -> Self {
        AgentMessage::user(text)
    }
}

impl From<&str> for AgentMessage {
    fn from(text: &str) -> Self {
        AgentMessage::user(text)
    }
}
