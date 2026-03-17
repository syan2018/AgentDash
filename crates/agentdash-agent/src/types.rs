use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

/// 当前时间戳（毫秒），对齐 Pi 的 `Date.now()`
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ─── ToolExecutionMode ──────────────────────────────────────

/// 工具执行模式 — 对齐 Pi `ToolExecutionMode`
///
/// - `Sequential`：逐个执行 tool call（prepare → execute → finalize）
/// - `Parallel`：顺序 prepare，允许的工具并发执行，结果按原始顺序聚合
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionMode {
    Sequential,
    #[default]
    Parallel,
}

// ─── Tool Call Hook Types ───────────────────────────────────

/// `before_tool_call` 返回值 — 对齐 Pi `BeforeToolCallResult`
///
/// 返回 `block: true` 阻止工具执行，loop 会发出一条错误 tool result。
#[derive(Debug, Clone, Default)]
pub struct BeforeToolCallResult {
    pub block: bool,
    pub reason: Option<String>,
}

/// `after_tool_call` 返回值 — 对齐 Pi `AfterToolCallResult`
///
/// 字段级覆盖语义：提供的字段替换原值，省略的字段保持不变。
#[derive(Debug, Clone, Default)]
pub struct AfterToolCallResult {
    pub content: Option<Vec<ContentPart>>,
    pub details: Option<serde_json::Value>,
    pub is_error: Option<bool>,
}

/// `before_tool_call` 上下文 — 对齐 Pi `BeforeToolCallContext`
pub struct BeforeToolCallContext<'a> {
    pub assistant_message: &'a AgentMessage,
    pub tool_call: &'a ToolCallInfo,
    pub args: &'a serde_json::Value,
    pub context: &'a AgentContext,
}

/// `after_tool_call` 上下文 — 对齐 Pi `AfterToolCallContext`
pub struct AfterToolCallContext<'a> {
    pub assistant_message: &'a AgentMessage,
    pub tool_call: &'a ToolCallInfo,
    pub args: &'a serde_json::Value,
    pub result: &'a AgentToolResult,
    pub is_error: bool,
    pub context: &'a AgentContext,
}

// ─── ContentPart ────────────────────────────────────────────

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

// ─── StopReason ─────────────────────────────────────────────

/// Assistant 消息终止原因 — 对齐 Pi `StopReason`
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

/// Token 用量信息 — 对齐 Pi `AssistantMessage.usage`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
}

// ─── AgentMessage ───────────────────────────────────────────

/// Agent 消息 — 面向会话的消息层（区别于 LLM 原始消息）
///
/// 设计参照 Pi 的 `AgentMessage`：在 LLM Message 之上增加自定义类型扩展能力。
/// `convert_to_llm` 负责在调用模型前将 AgentMessage 映射为 rig::Message。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum AgentMessage {
    User {
        content: Vec<ContentPart>,
        /// 对齐 Pi — 消息创建时间戳（ms）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    Assistant {
        content: Vec<ContentPart>,
        #[serde(default)]
        tool_calls: Vec<ToolCallInfo>,
        /// 对齐 Pi `AssistantMessage.stopReason`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stop_reason: Option<StopReason>,
        /// 对齐 Pi `AssistantMessage.errorMessage`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
        /// 对齐 Pi `AssistantMessage.usage`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        /// 对齐 Pi — 消息创建时间戳（ms）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    ToolResult {
        tool_call_id: String,
        #[serde(default)]
        call_id: Option<String>,
        /// 对齐 Pi `ToolResultMessage.toolName`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        content: Vec<ContentPart>,
        /// 对齐 Pi `ToolResultMessage.details`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
        #[serde(default)]
        is_error: bool,
        /// 对齐 Pi — 消息创建时间戳（ms）
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

    /// 构建一条包含错误信息的 Assistant 消息 — 对齐 Pi `agent.ts:573-591`
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
            usage: None,
            timestamp: Some(now_millis()),
        }
    }

    /// 检查 assistant 消息是否表示错误或中止
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

    /// 完整构造 ToolResult — 对齐 Pi `emitToolCallOutcome` (agent-loop.ts:603-611)
    pub fn tool_result_full(
        tool_call_id: impl Into<String>,
        call_id: Option<String>,
        tool_name: Option<String>,
        text: impl Into<String>,
        details: Option<serde_json::Value>,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            tool_call_id: tool_call_id.into(),
            call_id,
            tool_name,
            content: vec![ContentPart::text(text)],
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

/// Agent 生命周期事件 — 严格对齐 Pi 的 `AgentEvent`
///
/// 事件类型与 Pi `types.ts:295-310` 一一对应。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    // ── Agent 生命周期 ──
    AgentStart,
    AgentEnd {
        messages: Vec<AgentMessage>,
    },

    // ── Turn 生命周期 ──
    TurnStart,
    /// 对齐 Pi: `turn_end { message, toolResults }`
    TurnEnd {
        message: AgentMessage,
        tool_results: Vec<AgentMessage>,
    },

    // ── Message 生命周期 ──
    /// 对齐 Pi: `message_start { message }` — 用于 user/assistant/toolResult 消息
    MessageStart {
        message: AgentMessage,
    },
    /// 流式文本增量 — 对应 Pi `message_update` 中的 `text_delta` 子事件
    ///
    /// 注：Pi 的 `message_update` 还包含 `thinking_*` / `toolcall_*` 等子事件，
    /// 当前简化为纯文本 delta；后续扩展思考模型时需补齐。
    MessageDelta {
        text: String,
    },
    MessageEnd {
        message: AgentMessage,
    },

    // ── Tool Execution 生命周期 ──
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    /// 工具执行进度更新 — 对齐 Pi `tool_execution_update`
    ToolExecutionUpdate {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        partial_result: serde_json::Value,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    },
}

// ─── AgentTool ──────────────────────────────────────────────

/// 工具执行结果 — 对齐 Pi `AgentToolResult<T>`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub content: Vec<ContentPart>,
    pub is_error: bool,
    /// 对齐 Pi `AgentToolResult.details` — 工具结果详情（用于 UI 展示）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// 工具执行进度回调 — 对齐 Pi `AgentToolUpdateCallback`
pub type ToolUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>;

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

/// Agent 工具 trait — 严格对齐 Pi 的 `AgentTool`
///
/// 实现此 trait 的类型可被注册到 Agent，在模型请求工具调用时自动执行。
///
/// 与 Pi 的对应关系：
/// - `name` / `description` / `parameters_schema` → Pi `Tool` base
/// - `label` → Pi `AgentTool.label`
/// - `execute(tool_call_id, args, cancel, on_update)` → Pi `execute(toolCallId, params, signal?, onUpdate?)`
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;

    /// 人类可读标签 — 对齐 Pi `AgentTool.label`
    ///
    /// 默认返回 `name()`。工具可覆盖此方法提供更友好的 UI 展示名称。
    fn label(&self) -> &str {
        self.name()
    }

    /// 执行工具 — 对齐 Pi `AgentTool.execute(toolCallId, params, signal?, onUpdate?)`
    ///
    /// - `cancel` 用于监听取消信号（对齐 Pi 的 `signal?: AbortSignal`）
    /// - `on_update` 用于流式进度报告（对齐 Pi 的 `onUpdate?: AgentToolUpdateCallback`）
    async fn execute(
        &self,
        tool_call_id: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        on_update: Option<ToolUpdateCallback>,
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

// ─── ThinkingLevel ──────────────────────────────────────────

/// 思考/推理级别 — 对齐 Pi `ThinkingLevel`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
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
    /// 对齐 Pi: `agentLoopContinue` 的安全检查
    #[error("Continue 错误: {0}")]
    ContinueError(String),
}
