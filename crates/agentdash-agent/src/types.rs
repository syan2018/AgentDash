use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── re-exports from agent-types ────────────────────────────

pub use agentdash_agent_types::{
    AfterToolCallContext, AfterToolCallEffects, AfterToolCallInput, AfterToolCallResult,
    AfterTurnInput, AgentContext, AgentMessage, AgentRuntimeDelegateSet, AgentRuntimeError,
    AgentTool, AgentToolError, AgentToolResult, BeforeProviderRequestInput, BeforeStopInput,
    BeforeToolCallContext, BeforeToolCallInput, BeforeToolCallResult, CompactionFailureInput,
    CompactionImplementation, CompactionMetadata, CompactionNoopInput, CompactionParams,
    CompactionPhase, CompactionReason, CompactionResult, CompactionStrategy, CompactionTrigger,
    CompactionTriggerStats, ContentPart, DynAgentTool, DynRuntimeContextTransformDelegate,
    DynRuntimeProviderObserverDelegate, DynRuntimeToolPolicyDelegate,
    DynRuntimeTurnBoundaryDelegate, MessageRef, ProjectedEntry, ProjectedTranscript,
    ProjectionKind, ProviderVisibleContextStats, RuntimeContextTransformDelegate,
    RuntimeProviderObserverDelegate, RuntimeToolPolicyDelegate, RuntimeTurnBoundaryDelegate,
    StopDecision, StopReason, ThinkingLevel, TokenUsage, ToolApprovalOutcome, ToolApprovalRequest,
    ToolCallDecision, ToolCallInfo, ToolDefinition, ToolUpdateCallback, TransformContextInput,
    TransformContextOutput, TurnControlDecision, estimate_message_tokens, estimate_request_tokens,
    now_millis,
};

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
    ContextCompactionStarted {
        item_id: String,
    },
    ContextCompactionNoop {
        item_id: String,
        reason: String,
        metadata: CompactionMetadata,
    },
    ContextCompacted {
        item_id: String,
        messages: Vec<AgentMessage>,
        message_refs: Vec<Option<MessageRef>>,
        compacted_until_ref: MessageRef,
        first_kept_ref: Option<MessageRef>,
        metadata: CompactionMetadata,
        newly_compacted_messages: u32,
    },
    ContextCompactionFailed {
        item_id: String,
        error: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<CompactionMetadata>,
    },
    ProviderAttemptStatus {
        status: ProviderAttemptStatus,
    },
    RunError {
        error: AgentRunError,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderAttemptStatus {
    pub phase: ProviderAttemptPhase,
    pub attempt: u32,
    pub max_attempts: u32,
    #[serde(default)]
    pub will_retry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunErrorKind {
    Provider,
    HookBlocked,
    Runtime,
    Tool,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Error)]
#[error("{message}")]
pub struct AgentRunError {
    pub kind: AgentRunErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub aborted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl AgentRunError {
    pub fn new(kind: AgentRunErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            code: None,
            retryable: false,
            aborted: false,
            http_status: None,
            provider: None,
            model: None,
            details: None,
        }
    }

    pub fn with_code(mut self, code: Option<String>) -> Self {
        self.code = code;
        self
    }

    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn with_aborted(mut self, aborted: bool) -> Self {
        self.aborted = aborted;
        self
    }

    pub fn with_http_status(mut self, http_status: Option<u16>) -> Self {
        self.http_status = http_status;
        self
    }

    pub fn with_provider(mut self, provider: Option<String>) -> Self {
        self.provider = provider;
        self
    }

    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAttemptPhase {
    Connecting,
    ConnectedWaitingFirstDelta,
    Streaming,
    RetryScheduled,
    Retrying,
    Failed,
    Succeeded,
}

// ─── AgentState ─────────────────────────────────────────────

#[derive(Clone)]
pub struct AgentState {
    pub system_prompt: String,
    pub thinking_level: ThinkingLevel,
    pub tools: Vec<DynAgentTool>,
    pub messages: Vec<AgentMessage>,
    pub message_refs: Vec<Option<MessageRef>>,
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
            .field("message_refs_count", &self.message_refs.len())
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
            message_refs: Vec::new(),
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
    #[error("{0}")]
    Run(Box<AgentRunError>),
    #[error("LLM 桥接层错误: {0}")]
    Bridge(#[from] crate::bridge::BridgeError),
    #[error("工具执行错误: {tool_name}: {source}")]
    ToolExecution {
        tool_name: String,
        source: AgentToolError,
    },
    #[error("Agent 已被取消")]
    Cancelled,
    #[error("Continue 错误: {0}")]
    ContinueError(String),
    #[error("{0}")]
    InvalidState(String),
    #[error("运行时委托错误: {0}")]
    RuntimeDelegate(String),
}

impl From<AgentRunError> for AgentError {
    fn from(error: AgentRunError) -> Self {
        Self::Run(Box::new(error))
    }
}
