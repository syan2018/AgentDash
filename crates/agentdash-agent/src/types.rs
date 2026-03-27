use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

// ─── re-exports from connector-contract (tool SPI) ─────────
pub use agentdash_connector_contract::tool::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolUpdateCallback,
};
pub use agentdash_connector_contract::connector::ThinkingLevel;

/// 当前时间戳（毫秒），对齐 Pi 的 `Date.now()`
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ─── ToolExecutionMode ──────────────────────────────────────

/// 工具执行模式 — 对齐 Pi `ToolExecutionMode`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionMode {
    Sequential,
    #[default]
    Parallel,
}

// ─── Tool Call Hook Types ───────────────────────────────────

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

// ─── AgentEvent ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    AgentStart,
    AgentEnd {
        /// 对齐 Pi：这里表达的是本轮新增消息，不是全量历史。
        messages: Vec<AgentMessage>,
    },
    TurnStart,
    TurnEnd {
        message: AgentMessage,
        tool_results: Vec<AgentMessage>,
    },
    MessageStart {
        message: AgentMessage,
    },
    MessageUpdate {
        message: AgentMessage,
        event: AssistantStreamEvent,
    },
    MessageEnd {
        message: AgentMessage,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    ToolExecutionUpdate {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        partial_result: serde_json::Value,
    },
    ToolExecutionPendingApproval {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
    },
    ToolExecutionApprovalResolved {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        approved: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    },
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

// ─── AgentState ─────────────────────────────────────────────

#[derive(Clone)]
pub struct AgentState {
    pub system_prompt: String,
    pub thinking_level: ThinkingLevel,
    pub tools: Vec<DynAgentTool>,
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub stream_message: Option<AgentMessage>,
    pub pending_tool_calls: std::collections::HashSet<String>,
    pub error: Option<String>,
}

impl std::fmt::Debug for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentState")
            .field("thinking_level", &self.thinking_level)
            .field("messages_count", &self.messages.len())
            .field("tools_count", &self.tools.len())
            .field("is_streaming", &self.is_streaming)
            .field("pending_tool_calls", &self.pending_tool_calls)
            .field("error", &self.error)
            .finish()
    }
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            system_prompt: String::new(),
            thinking_level: ThinkingLevel::default(),
            tools: Vec::new(),
            messages: Vec::new(),
            is_streaming: false,
            stream_message: None,
            pending_tool_calls: std::collections::HashSet::new(),
            error: None,
        }
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── AssistantStreamEvent ───────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantStreamEvent {
    TextStart {
        content_index: usize,
    },
    TextDelta {
        content_index: usize,
        text: String,
    },
    TextEnd {
        content_index: usize,
        text: String,
    },
    ThinkingStart {
        content_index: usize,
        id: Option<String>,
    },
    ThinkingDelta {
        content_index: usize,
        id: Option<String>,
        text: String,
    },
    ThinkingEnd {
        content_index: usize,
        id: Option<String>,
        text: String,
        signature: Option<String>,
    },
    ToolCallStart {
        content_index: usize,
        tool_call_id: String,
        name: String,
    },
    ToolCallDelta {
        content_index: usize,
        tool_call_id: String,
        name: String,
        delta: String,
    },
    ToolCallEnd {
        content_index: usize,
        tool_call: ToolCallInfo,
    },
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
    #[error("Continue 错误: {0}")]
    ContinueError(String),
    #[error("{0}")]
    InvalidState(String),
    #[error("运行时委托错误: {0}")]
    RuntimeDelegate(String),
}
