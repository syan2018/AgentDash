pub mod connector;
pub mod context;
pub mod hooks;
pub mod platform;

// ─── agent-types re-export（保持外部 API 不变）──────────────

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

// ─── domain re-export ───────────────────────────────────────

pub use agentdash_domain::common::{
    AgentConfig, Mount, MountCapability, MountLink, SystemPromptMode, ThinkingLevel, Vfs,
};

// ─── connector ──────────────────────────────────────────────

pub use connector::{
    AgentConnector, AgentInfo, CapabilityState, ConnectorCapabilities, ConnectorError,
    ConnectorType, DiscoveredGuideline, ExecutionContext, ExecutionSessionFrame, ExecutionStream,
    ExecutionTurnFrame, McpEnvVar, McpHeader, McpTransportConfig, PromptPayload,
    RestoredSessionState, SessionMcpServer, ToolCapabilityFilter, ToolCluster,
    content_block_to_text, partition_session_mcp_servers, workspace_path_from_context,
};

// ─── context injection ──────────────────────────────────────

pub use context::injection::{
    ContextFragment, FragmentScope, FragmentScopeSet, InjectionError, MergeStrategy,
    RUNTIME_AGENT_CONTEXT_SLOTS, ResolveSourcesOutput, ResolveSourcesRequest, SelectorHint,
    SourceResolver, VfsContext, VfsDescriptor, VfsDiscoveryProvider,
};

// ─── context bundle & capabilities ──────────────────────────

pub use context::bundle::SessionContextBundle;
pub use context::capability::{CompanionAgentEntry, SessionBaselineCapabilities, SkillEntry};
pub use context::tool_schema_sanitizer::{sanitize_tool_schema, schema_value};

// ─── hooks ──────────────────────────────────────────────────

pub use hooks::trace::build_hook_trace_envelope;
pub use hooks::{
    ActiveWorkflowMeta, ContextTokenStats, ExecutionHookProvider, HookApprovalRequest,
    HookCompactionDecision, HookCompletionStatus, HookDiagnosticEntry, HookEffect, HookError,
    HookEvaluationQuery, HookEvaluationTrigger, HookInjection, HookOwnerSummary, HookPendingAction,
    HookPendingActionResolutionKind, HookPendingActionStatus, HookResolution,
    HookSessionRuntimeAccess, HookSessionRuntimeSnapshot, HookStepAdvanceRequest, HookTraceEntry,
    HookTraceTrigger, HookTrigger, HookTurnStartNotice, NoopExecutionHookProvider,
    RuntimeEventSource, SessionHookRefreshQuery, SessionHookSnapshot, SessionHookSnapshotQuery,
    SessionSnapshotMetadata, SharedHookSessionRuntime, action_type,
};

// ─── platform ───────────────────────────────────────────────

pub use platform::auth::{AuthGroup, AuthIdentity, AuthMode};
pub use platform::mcp_relay::{McpRelayProvider, RelayMcpCallResult, RelayMcpToolInfo};
pub use platform::mount::MountEditCapabilities;
pub use platform::routine::{RoutineFireCallback, RoutineTriggerProvider};
pub use platform::skill::SkillRef;
pub use platform::tool_capability::{
    CapabilityVisibilityRule, PlatformMcpScope, ToolCapability, ToolDescriptor, ToolSource,
    capability_to_platform_mcp_scope, capability_to_tool_clusters, default_visibility_rules,
    format_tool_for_prompt, is_capability_visible, platform_tool_descriptors,
    platform_tools_for_capability,
};
