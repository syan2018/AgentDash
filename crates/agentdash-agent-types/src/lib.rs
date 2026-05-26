pub mod model;
pub mod protocol;
pub mod runtime;

// ─── 集中 re-export（保持外部 API 不变）───────────────────

pub use model::content::ContentPart;
pub use model::context::AgentContext;
pub use model::message::{
    AgentMessage, MessageRef, StopReason, TokenUsage, ToolCallInfo, now_millis,
};
pub use model::projection::{
    AgentContextEnvelope, AgentInputMessage, ProjectedEntry, ProjectedTranscript, ProjectionKind,
    ProjectionOrigin, ProjectionSourceRange,
};
pub use protocol::{
    AgentDashNativeThreadItem, AgentDashThreadItem, CodexThreadItem, CommandExecutionStatus,
    DynamicToolCallOutputContentItem, DynamicToolCallStatus, McpToolCallStatus, PatchApplyStatus,
};

pub use runtime::decisions::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, BeforeProviderRequestInput,
    BeforeStopInput, BeforeToolCallInput, CompactionFailureInput, CompactionParams,
    CompactionResult, CompactionTriggerStats, EvaluateCompactionInput, ProviderVisibleContextStats,
    StopDecision, ToolCallDecision, TransformContextInput, TransformContextOutput,
    TurnControlDecision,
};
pub use runtime::delegate::{AgentRuntimeDelegate, AgentRuntimeError, DynAgentRuntimeDelegate};
pub use runtime::hooks_io::{
    AfterToolCallContext, AfterToolCallResult, BeforeToolCallContext, BeforeToolCallResult,
    ToolApprovalOutcome, ToolApprovalRequest,
};
pub use runtime::tool::{
    AgentTool, AgentToolError, AgentToolResult, DynAgentTool, ToolDefinition, ToolUpdateCallback,
};
