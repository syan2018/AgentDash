use crate::content::ContentPart;
use crate::context::AgentContext;
use crate::message::{AgentMessage, ToolCallInfo};
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
