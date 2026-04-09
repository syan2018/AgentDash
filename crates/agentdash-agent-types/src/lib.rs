pub mod content;
pub mod context;
pub mod decisions;
pub mod delegate;
pub mod hooks_io;
pub mod message;
pub mod projection;
pub mod tool;

// ─── 集中 re-export ─────────────────────────────────────────

pub use content::ContentPart;
pub use context::AgentContext;
pub use decisions::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, BeforeStopInput,
    BeforeToolCallInput, CompactionParams, CompactionResult, CompactionTriggerStats,
    EvaluateCompactionInput, StopDecision, ToolCallDecision, TransformContextInput,
    TransformContextOutput, TurnControlDecision,
};
pub use delegate::{AgentRuntimeDelegate, AgentRuntimeError, DynAgentRuntimeDelegate};
pub use hooks_io::{
    AfterToolCallContext, AfterToolCallResult, BeforeToolCallContext, BeforeToolCallResult,
    ToolApprovalOutcome, ToolApprovalRequest,
};
pub use message::{AgentMessage, MessageRef, StopReason, TokenUsage, ToolCallInfo, now_millis};
pub use projection::{ProjectedEntry, ProjectedTranscript, ProjectionKind};
pub use tool::{
    AgentTool, AgentToolError, AgentToolResult, DynAgentTool, ToolDefinition, ToolUpdateCallback,
};
