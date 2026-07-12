pub mod model;
pub mod runtime;
pub mod token_estimation;

/// Provider-neutral reasoning effort requested from Agent Core.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
pub use runtime::decisions::{
    AfterToolCallEffects, AfterToolCallInput, AfterTurnInput, BeforeProviderRequestInput,
    BeforeStopInput, BeforeToolCallInput, CompactionFailureInput, CompactionImplementation,
    CompactionMetadata, CompactionNoopInput, CompactionParams, CompactionPhase, CompactionReason,
    CompactionResult, CompactionStrategy, CompactionTrigger, CompactionTriggerStats,
    EvaluateCompactionInput, ProviderVisibleContextStats, StopDecision, ToolCallDecision,
    TransformContextInput, TransformContextOutput, TurnControlDecision,
};
pub use runtime::delegate::{
    AgentRuntimeDelegateSet, AgentRuntimeError, DynRuntimeCompactionDelegate,
    DynRuntimeContextTransformDelegate, DynRuntimeProviderObserverDelegate,
    DynRuntimeToolPolicyDelegate, DynRuntimeTurnBoundaryDelegate, RuntimeCompactionDelegate,
    RuntimeContextTransformDelegate, RuntimeProviderObserverDelegate, RuntimeToolPolicyDelegate,
    RuntimeTurnBoundaryDelegate,
};
pub use runtime::hooks_io::{
    AfterToolCallContext, AfterToolCallResult, BeforeToolCallContext, BeforeToolCallResult,
    ToolApprovalOutcome, ToolApprovalRequest,
};
pub use runtime::tool::{
    AgentTool, AgentToolError, AgentToolResult, DynAgentTool, ToolDefinition,
    ToolProtocolProjector, ToolUpdateCallback,
};
pub use token_estimation::{
    chars_to_tokens, estimate_content_tokens, estimate_message_tokens, estimate_request_tokens,
    estimate_tool_tokens, text_tokens,
};
