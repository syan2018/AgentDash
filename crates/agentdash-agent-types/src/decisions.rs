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

/// HookRuntimeDelegate.transform_context 的产出。
///
/// 三字段对应 Hook 的三类语义，物理分离、互不相关
/// （`.trellis/tasks/04-30-session-pipeline-architecture-refactor/target-architecture.md`
/// §C11）：
///
/// 1. **Bundle 改写** — 此处**不**承载，通过 Bundle 的 `turn_delta` 字段与
///    `ContextAuditBus` 以独立数据面传递（保留 SPI crate 边界，不让
///    `agentdash-agent-types` 反向依赖 `agentdash-spi`）。
/// 2. **Per-turn steering** — `steering_messages`，只承每轮 agent loop 之间的
///    user message 级动态内容；静态上下文（companion_agents / workflow 等）
///    已经进入 Bundle，**不应**再出现在此字段。
/// 3. **控制决策** — `blocked`，当 Hook 决定阻止本轮 agent 前进时带上原因。
#[derive(Debug, Clone)]
pub struct TransformContextOutput {
    /// 本轮要追加到 agent context 的 user message 列表。
    ///
    /// 禁令：不得包含已进入 Bundle 的 slot 内容（companion_agents / workflow /
    /// 等静态上下文）。这条命名本身就是对此语义的约束。
    pub steering_messages: Vec<AgentMessage>,
    /// 当 hook 规则阻止当前用户输入时设置。
    /// agent loop 检测到此字段后应终止当前轮次并向用户报告原因。
    pub blocked: Option<String>,
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

// ─── BeforeProviderRequest ─────────────────────────────────

#[derive(Debug, Clone)]
pub struct BeforeProviderRequestInput {
    pub system_prompt_len: usize,
    pub message_count: usize,
    pub tool_count: usize,
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
