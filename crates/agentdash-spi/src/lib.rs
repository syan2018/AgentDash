pub mod connector;
pub mod context;
pub mod hooks;
pub mod platform;
pub mod session_persistence;

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
    AgentConfig, AgentPresetConfig, Mount, MountCapability, MountLink, SystemPromptMode,
    ThinkingLevel, Vfs,
};

// ─── connector ──────────────────────────────────────────────

pub use connector::{
    AgentConnector, AgentInfo, CapabilityState, CompanionDimension, ConnectorCapabilities,
    ConnectorError, ConnectorType, DiscoveredGuideline, ExecutionBackendPlacement,
    ExecutionContext, ExecutionSessionFrame, ExecutionStream, ExecutionTurnFrame, McpEnvVar,
    McpHeader, McpTransportConfig, PromptPayload, RestoredSessionState, SessionMcpServer,
    SkillDimension, ToolCapabilityFilter, ToolCluster, ToolDimension, VfsDimension,
    content_block_to_text, partition_session_mcp_servers, workspace_path_from_context,
};

// ─── context injection ──────────────────────────────────────

pub use context::injection::{
    ASSIGNMENT_CONTEXT_SLOTS, ContextFragment, FragmentScope, FragmentScopeSet, InjectionError,
    MergeStrategy, RUNTIME_AGENT_CONTEXT_SLOTS, ResolveSourcesOutput, ResolveSourcesRequest,
    SelectorHint, SourceResolver, VfsContext, VfsDescriptor, VfsDiscoveryProvider,
};

// ─── context bundle & capabilities ──────────────────────────

pub use context::bundle::SessionContextBundle;
pub use context::capability::{
    CompanionAgentEntry, CompanionSliceMode, SessionBaselineCapabilities, SkillEntry,
};
pub use context::tool_schema_sanitizer::{sanitize_tool_schema, schema_value};

// ─── hooks ──────────────────────────────────────────────────

pub use hooks::trace::build_hook_trace_envelope;
pub use hooks::{
    ActiveWorkflowMeta, ContextFrame, ContextFrameSection, ContextTokenStats,
    ExecutionHookProvider, HookApprovalRequest, HookCompactionDecision, HookCompletionStatus,
    HookDiagnosticEntry, HookEffect, HookError, HookEvaluationQuery, HookEvaluationTrigger,
    HookInjection, HookOwnerSummary, HookPendingAction, HookPendingActionResolutionKind,
    HookPendingActionStatus, HookResolution, HookSessionRuntimeAccess, HookSessionRuntimeSnapshot,
    HookStepAdvanceRequest, HookTraceEntry, HookTraceTrigger, HookTrigger, HookTurnStartNotice,
    NoopExecutionHookProvider, RuntimeContextFragmentEntry, RuntimeEventSource,
    RuntimeHookInjectionEntry, RuntimeSkillEntry, RuntimeToolSchemaEntry, SessionHookRefreshQuery,
    SessionHookSnapshot, SessionHookSnapshotQuery, SessionSnapshotMetadata,
    SharedHookSessionRuntime, action_type,
};

// ─── platform ───────────────────────────────────────────────

pub use platform::auth::{AuthGroup, AuthIdentity, AuthMode};
pub use platform::mcp_relay::{
    McpRelayProvider, RelayMcpCallContext, RelayMcpCallResult, RelayMcpToolInfo,
};
pub use platform::mount::MountEditCapabilities;
pub use platform::routine::{RoutineFireCallback, RoutineTriggerProvider};
pub use platform::skill::SkillRef;
pub use platform::tool_capability::{
    CapabilityVisibilityRule, PlatformMcpScope, ToolCapability, ToolDescriptor, ToolSource,
    capability_to_platform_mcp_scope, capability_to_tool_clusters, default_visibility_rules,
    format_tool_for_prompt, is_capability_visible, platform_tool_descriptors,
    platform_tools_for_capability,
};

pub use session_persistence::{
    ApplyMountOperationsEffect, ApplyVfsOverlayEffect, CapabilityArtifactSource,
    CapabilityContributionRecord, CapabilityDeclarationRecord, CapabilityDimensionKey,
    CompactionProjectionCommitResult, CompanionSessionContext, EFFECT_TYPE_APPLY_MOUNT_OPERATIONS,
    EFFECT_TYPE_APPLY_VFS_OVERLAY, EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER,
    EFFECT_TYPE_SET_MCP_SERVER_SET, EFFECT_TYPE_SET_TOOL_ACCESS, ExecutionStatus,
    NewCompactionProjectionCommit, NewTerminalEffectRecord, PendingCapabilityStateTransition,
    PersistedSessionEvent, RuntimeCapabilityEffectRecord, RuntimeCapabilityTransition,
    RuntimeCommandRecord, RuntimeCommandStatus, SESSION_PROJECTION_KIND_AUDIT,
    SESSION_PROJECTION_KIND_HANDOFF, SESSION_PROJECTION_KIND_MODEL_CONTEXT,
    SESSION_PROJECTION_KIND_TIMELINE, SessionBootstrapState, SessionCompactionRecord,
    SessionCompactionStatus, SessionCompactionStore, SessionEventBacklog, SessionEventPage,
    SessionEventStore, SessionMeta, SessionMetaStore, SessionPersistence,
    SessionProjectionHeadRecord, SessionProjectionSegmentRecord, SessionProjectionStore,
    SessionRuntimeCommandStore, SessionTerminalEffectStore, SetCompanionAgentRosterEffect,
    SetMcpServerSetEffect, SetToolAccessEffect, TerminalEffectRecord, TerminalEffectStatus,
    TerminalEffectType, TitleSource,
};
