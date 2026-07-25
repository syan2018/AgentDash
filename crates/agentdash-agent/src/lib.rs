pub mod agent;
pub mod agent_loop;
pub mod bridge;
pub mod compaction;
pub mod conversation_naming;
pub mod dash;
pub mod event_stream;
pub mod model;
pub mod runtime;
pub mod token_estimation;
pub mod tool_result_ref;
pub mod tools;
pub mod types;

pub use agent::{Agent, AgentConfig, QueueMode, process_event};
pub use bridge::{
    BridgeError, BridgeRequest, BridgeResponse, LlmBridge, ProviderErrorClassification,
    ProviderErrorKind, ProviderRetryPolicy, StreamChunk, ToolCallDeltaContent, sleep_for_retry,
};
pub use conversation_naming::{
    ConversationName, ConversationNamer, ConversationNamingError, ConversationNamingInput,
};
pub use event_stream::{EventReceiver, EventSender, event_channel};
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
pub use tool_result_ref::{
    ReadableBodyKind, ReadableTerminalRef, ReadableToolResultRef, ToolResultAddressProvider,
    ToolResultCacheWrite, ToolResultCacheWriter, ToolResultRefContext, stable_tool_result_item_id,
};
pub use tools::{ToolInfo, ToolRegistry};
pub use types::{
    AgentError, AgentEvent, AgentRunError, AgentRunErrorKind, AgentState, AssistantStreamEvent,
    ProviderAttemptPhase, ProviderAttemptStatus, ToolExecutionMode,
};

/// Provider-neutral reasoning effort requested by the complete Dash Agent layer.
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
