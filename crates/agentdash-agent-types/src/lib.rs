pub mod content;
pub mod context;
pub mod decisions;
pub mod delegate;
pub mod hooks_io;
pub mod message;
pub mod tool;

// ─── 集中 re-export ─────────────────────────────────────────

pub use content::ContentPart;
pub use context::AgentContext;
pub use decisions::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, BeforeStopInput,
    BeforeToolCallInput, StopDecision, ToolCallDecision, TransformContextInput,
    TransformContextOutput, TurnControlDecision,
};
pub use delegate::{AgentRuntimeDelegate, AgentRuntimeError, DynAgentRuntimeDelegate};
pub use hooks_io::{
    AfterToolCallContext, AfterToolCallResult, BeforeToolCallContext, BeforeToolCallResult,
    ToolApprovalOutcome, ToolApprovalRequest,
};
pub use message::{AgentMessage, StopReason, TokenUsage, ToolCallInfo, now_millis};
pub use tool::{
    AgentTool, AgentToolError, AgentToolResult, DynAgentTool, ToolDefinition, ToolUpdateCallback,
};
