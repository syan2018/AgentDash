// 全部类型已迁移到 agentdash-agent-types，这里仅 re-export 保持向后兼容。
pub use agentdash_agent_types::{
    AfterToolCallContext, AfterToolCallEffects, AfterToolCallInput, AfterToolCallResult,
    AfterTurnInput, AgentContext, AgentMessage, AgentRuntimeDelegate, AgentRuntimeError,
    BeforeProviderRequestInput, BeforeStopInput, BeforeToolCallContext, BeforeToolCallInput,
    BeforeToolCallResult, CompactionParams, CompactionResult, CompactionTriggerStats,
    DynAgentRuntimeDelegate, EvaluateCompactionInput, MessageRef, ProjectedEntry,
    ProjectedTranscript, ProjectionKind, StopDecision, StopReason, TokenUsage,
    ToolApprovalOutcome, ToolApprovalRequest, ToolCallDecision, ToolCallInfo,
    TransformContextInput, TransformContextOutput, TurnControlDecision, now_millis,
};
