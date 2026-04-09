use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── re-exports from agent-types ────────────────────────────

pub use agentdash_agent_types::{
    AfterToolCallContext, AfterToolCallEffects, AfterToolCallInput, AfterToolCallResult,
    AfterTurnInput, AgentContext, AgentMessage, AgentRuntimeDelegate, AgentRuntimeError, AgentTool,
    AgentToolError, AgentToolResult, BeforeStopInput, BeforeToolCallContext, BeforeToolCallInput,
    BeforeToolCallResult, CompactionParams, CompactionResult, CompactionTriggerStats, ContentPart,
    DynAgentRuntimeDelegate, DynAgentTool, EvaluateCompactionInput, StopDecision,
    StopReason, TokenUsage, ToolApprovalOutcome, ToolApprovalRequest, ToolCallDecision,
    ToolCallInfo, ToolDefinition, ToolUpdateCallback, TransformContextInput,
    TransformContextOutput, TurnControlDecision, now_millis,
};
pub use agentdash_domain::common::ThinkingLevel;

// ─── Agent-specific types (不属于通用 SPI) ──────────────────

/// 工具执行模式 — 对齐 Pi `ToolExecutionMode`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionMode {
    Sequential,
    #[default]
    Parallel,
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
    ContextCompacted {
        messages: Vec<AgentMessage>,
        newly_compacted_messages: u32,
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
        draft: String,
        is_parseable: bool,
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
