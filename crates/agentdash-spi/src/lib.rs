pub mod auth;
pub mod connector;
pub mod context_injection;
pub mod hook_trace_notification;
pub mod hooks;
pub mod lifecycle;
pub mod mcp_relay;
pub mod mount;
pub mod routine;
pub mod schema;
pub mod session_capabilities;
pub mod skill;
pub mod tool;

pub use agentdash_domain::common::{
    Vfs, AgentConfig, Mount, MountCapability, SystemPromptMode, ThinkingLevel,
};
pub use connector::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    ExecutionContext, ExecutionStream, FlowCapabilities, PromptPayload, RestoredSessionState,
    ToolCluster, content_block_to_text, workspace_path_from_context,
};
pub use context_injection::{
    VfsContext, VfsDescriptor, VfsDiscoveryProvider, ContextFragment,
    InjectionError, MergeStrategy, ResolveSourcesOutput, ResolveSourcesRequest, SelectorHint,
    SourceResolver,
};
pub use hook_trace_notification::build_hook_trace_notification;
pub use hooks::{
    ActiveWorkflowMeta, ContextTokenStats, ExecutionHookProvider, HookApprovalRequest,
    HookCompactionDecision, HookCompletionStatus, HookDiagnosticEntry, HookEffect, HookError,
    HookEvaluationQuery, HookInjection, HookOwnerSummary, HookPendingAction,
    HookPendingActionResolutionKind, HookPendingActionStatus, HookResolution,
    HookSessionRuntimeAccess, HookSessionRuntimeSnapshot, HookStepAdvanceRequest, HookTraceEntry,
    HookTrigger, NoopExecutionHookProvider, SessionHookRefreshQuery, SessionHookSnapshot,
    SessionHookSnapshotQuery, SessionSnapshotMetadata, SharedHookSessionRuntime, action_type,
};
pub use lifecycle::{
    AfterToolCallContext, AfterToolCallEffects, AfterToolCallInput, AfterToolCallResult,
    AfterTurnInput, AgentContext, AgentMessage, AgentRuntimeDelegate, AgentRuntimeError,
    BeforeProviderRequestInput, BeforeStopInput, BeforeToolCallContext, BeforeToolCallInput,
    BeforeToolCallResult, CompactionParams, CompactionResult, CompactionTriggerStats,
    DynAgentRuntimeDelegate, EvaluateCompactionInput, MessageRef, ProjectedEntry,
    ProjectedTranscript, ProjectionKind, StopDecision, StopReason, TokenUsage, ToolApprovalOutcome,
    ToolApprovalRequest, ToolCallDecision, ToolCallInfo, TransformContextInput,
    TransformContextOutput, TurnControlDecision, now_millis,
};
pub use mcp_relay::{McpRelayProvider, RelayMcpCallResult, RelayMcpToolInfo};
pub use mount::MountEditCapabilities;
pub use routine::{RoutineFireCallback, RoutineTriggerProvider};
pub use session_capabilities::{CompanionAgentEntry, SessionBaselineCapabilities, SkillEntry};
pub use skill::SkillRef;
pub use tool::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolDefinition,
    ToolUpdateCallback,
};
