use crate::content::ContentPart;
use crate::context::AgentContext;
use crate::message::AgentMessage;
use crate::message::ToolCallInfo;
use crate::tool::AgentToolResult;

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
pub struct EvaluateCompactionInput {
    pub context: AgentContext,
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
        allow_empty: bool,
    },
}

// ─── Compaction 参数 ──────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CompactionParams {
    /// 保留最近 N 条消息不压缩
    pub keep_last_n: u32,
    /// 预留给当前轮输出与工具调用的 token 空间
    pub reserve_tokens: u64,
    /// 外部提供的自定义摘要（跳过 LLM 调用）
    pub custom_summary: Option<String>,
    /// 覆盖默认摘要 prompt
    pub custom_prompt: Option<String>,
    /// 触发压缩的 token 统计
    pub trigger_stats: CompactionTriggerStats,
}

#[derive(Debug, Clone)]
pub struct CompactionTriggerStats {
    pub input_tokens: u64,
    pub context_window: u64,
    pub reserve_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// 压缩后应写回 runtime / restore 投影的完整消息序列
    pub messages: Vec<AgentMessage>,
    /// 本次生成的新摘要消息
    pub summary_message: AgentMessage,
    /// 压缩前的 token 触发统计
    pub trigger_stats: CompactionTriggerStats,
    /// 本次新增压缩的原始消息数量
    pub newly_compacted_messages: u32,
    /// 本次是否直接使用了外部自定义摘要
    pub used_custom_summary: bool,
}
