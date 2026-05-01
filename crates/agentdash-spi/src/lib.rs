pub mod auth;
pub mod connector;
pub mod context_injection;
pub mod hook_trace_notification;
pub mod hooks;
pub mod mcp_relay;
pub mod mount;
pub mod routine;
pub mod schema;
pub mod session_capabilities;
pub mod session_context_bundle;
pub mod skill;
pub mod tool_capability;

pub use agentdash_agent_types::{
    AfterToolCallContext, AfterToolCallEffects, AfterToolCallInput, AfterToolCallResult,
    AfterTurnInput, AgentContext, AgentMessage, AgentRuntimeDelegate, AgentRuntimeError,
    BeforeProviderRequestInput, BeforeStopInput, BeforeToolCallContext, BeforeToolCallInput,
    BeforeToolCallResult, CompactionParams, CompactionResult, CompactionTriggerStats,
    DynAgentRuntimeDelegate, EvaluateCompactionInput, MessageRef, ProjectedEntry,
    ProjectedTranscript, ProjectionKind, StopDecision, StopReason, TokenUsage, ToolApprovalOutcome,
    ToolApprovalRequest, ToolCallDecision, ToolCallInfo, TransformContextInput,
    TransformContextOutput, TurnControlDecision, now_millis,
};
pub use agentdash_agent_types::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolDefinition,
    ToolUpdateCallback,
};
pub use agentdash_domain::common::{
    AgentConfig, Mount, MountCapability, MountLink, SystemPromptMode, ThinkingLevel, Vfs,
};
pub use connector::{
    AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
    DiscoveredGuideline, ExecutionContext, ExecutionSessionFrame, ExecutionStream,
    ExecutionTurnFrame, FlowCapabilities, PromptPayload, RestoredSessionState, ToolCluster,
    content_block_to_text, workspace_path_from_context,
};
pub use context_injection::{
    ContextFragment, FragmentScope, FragmentScopeSet, InjectionError, MergeStrategy,
    RUNTIME_AGENT_CONTEXT_SLOTS, ResolveSourcesOutput, ResolveSourcesRequest, SelectorHint,
    SourceResolver, VfsContext, VfsDescriptor, VfsDiscoveryProvider,
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
pub use mcp_relay::{McpRelayProvider, RelayMcpCallResult, RelayMcpToolInfo};
pub use mount::MountEditCapabilities;
pub use routine::{RoutineFireCallback, RoutineTriggerProvider};
pub use session_capabilities::{CompanionAgentEntry, SessionBaselineCapabilities, SkillEntry};
pub use session_context_bundle::SessionContextBundle;
pub use skill::SkillRef;
pub use tool_capability::{
    CapabilityVisibilityRule, PlatformMcpScope, ToolCapability, ToolDescriptor, ToolSource,
    capability_to_platform_mcp_scope, capability_to_tool_clusters, default_visibility_rules,
    format_tool_for_prompt, is_capability_visible, platform_tool_descriptors,
    platform_tools_for_capability,
};
