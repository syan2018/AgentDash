pub mod channel_binding;
pub mod connector;
pub mod context;
pub mod extension_package;
pub mod hooks;
pub mod platform;
pub mod session_persistence;
pub mod workflow;

// ─── agent-types re-export（保持外部 API 不变）──────────────

pub use agentdash_agent_types::{
    AfterToolCallContext, AfterToolCallEffects, AfterToolCallInput, AfterToolCallResult,
    AfterTurnInput, AgentContext, AgentContextEnvelope, AgentInputMessage, AgentMessage,
    AgentRuntimeDelegateSet, AgentRuntimeError, BeforeProviderRequestInput, BeforeStopInput,
    BeforeToolCallContext, BeforeToolCallInput, BeforeToolCallResult, CompactionFailureInput,
    CompactionImplementation, CompactionMetadata, CompactionNoopInput, CompactionParams,
    CompactionPhase, CompactionReason, CompactionResult, CompactionStrategy, CompactionTrigger,
    CompactionTriggerStats, DynRuntimeCompactionDelegate, DynRuntimeContextTransformDelegate,
    DynRuntimeProviderObserverDelegate, DynRuntimeToolPolicyDelegate,
    DynRuntimeTurnBoundaryDelegate, EvaluateCompactionInput, MessageRef, ProjectedEntry,
    ProjectedTranscript, ProjectionKind, ProjectionOrigin, ProjectionSourceRange,
    ProviderVisibleContextStats, RuntimeCompactionDelegate, RuntimeContextTransformDelegate,
    RuntimeProviderObserverDelegate, RuntimeToolPolicyDelegate, RuntimeTurnBoundaryDelegate,
    StopDecision, StopReason, TokenUsage, ToolApprovalOutcome, ToolApprovalRequest,
    ToolCallDecision, ToolCallInfo, TransformContextInput, TransformContextOutput,
    TurnControlDecision, now_millis,
};
pub use agentdash_agent_types::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolDefinition,
    ToolUpdateCallback,
};

// ─── domain re-export ───────────────────────────────────────

pub use agentdash_domain::common::{
    AgentConfig, AgentPresetConfig, Mount, MountCapability, MountLink, ThinkingLevel, Vfs,
};

// ─── connector ──────────────────────────────────────────────

pub use agentdash_domain::backend::{
    MissingRuntimeBackendAnchor, RuntimeBackendAnchor, RuntimeBackendAnchorError,
    RuntimeBackendAnchorSource,
};
pub use connector::{
    CapabilityState, CapabilityStateDelta, ChannelDimension, CompanionDimension, ConnectorError,
    DefaultMountDelta, DiscoveredGuideline, DiscoveryContext, ExecutionBackendPlacement,
    ExecutionContext, ExecutionSessionFrame, ExecutionTurnFrame, ExecutionTurnMode, McpEnvVar,
    McpHttpHeader, McpServerReadinessSummary, McpTransportConfig, NamedEntityDelta, PromptPayload,
    RestoredSessionState, RuntimeMcpServer, RuntimeMcpSourceReadiness, RuntimeVfsAccessPolicy,
    RuntimeVfsAccessRule, RuntimeVfsAccessSource, RuntimeVfsOperation, RuntimeVfsPathPattern,
    SetDelta, SkillClusterMeta, SkillDimension, ToolCapabilityFilter, ToolCluster, ToolDimension,
    VfsDimension, VfsSurfaceDelta, WorkspaceModuleDimension, WorkspaceModuleVisibilityMode,
    compute_capability_state_delta, partition_runtime_mcp_servers, workspace_path_from_context,
};

// ─── context injection ──────────────────────────────────────

pub use context::injection::{
    ASSIGNMENT_CONTEXT_SLOTS, ContextFragment, FragmentScope, FragmentScopeSet, InjectionError,
    MergeStrategy, ResolveSourcesOutput, ResolveSourcesRequest, SelectorHint, SourceResolver,
    VfsContext, VfsDescriptor, VfsDiscoveryProvider,
};

// ─── context bundle & capabilities ──────────────────────────

pub use context::bundle::SessionContextBundle;
pub use context::capability::{
    CompanionAgentEntry, CompanionSliceMode, SessionBaselineCapabilities, SkillCapabilityEntry,
    SkillEntry, SkillProviderCluster,
};
pub use context::tool_schema_sanitizer::{sanitize_tool_schema, schema_value};

// ─── extension package storage ──────────────────────────────

pub use extension_package::{
    ExtensionPackageArtifactStorage, ExtensionPackageArtifactStorageError,
};

// ─── hooks ──────────────────────────────────────────────────

pub use hooks::script::HookScriptEvaluator;
pub use hooks::trace::{
    HookTraceStorageDisposition, build_hook_trace_envelope, hook_trace_entry_storage_disposition,
    hook_trace_payload_storage_disposition,
};
pub use hooks::{
    ActiveWorkflowMeta, AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery,
    AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, AgentFrameRuntimeSnapshot,
    ContextAgentConsumption, ContextAgentConsumptionMode, ContextCachePolicy,
    ContextConnectorProfile, ContextDeliveryEntry, ContextDeliveryMetadata, ContextDeliveryPhase,
    ContextDeliveryPlan, ContextDeliveryTarget, ContextFrame, ContextFrameSection,
    ContextModelChannel, ContextTokenStats, ExecutionHookProvider, HookApprovalRequest,
    HookCompactionDecision, HookCompletionStatus, HookControlTarget, HookDiagnosticEntry,
    HookEffect, HookError, HookEvaluationQuery, HookEvaluationTrigger, HookInjection,
    HookPendingAction, HookPendingActionResolutionKind, HookPendingActionStatus, HookResolution,
    HookRuntimeAccess, HookRuntimeEvaluationQuery, HookRuntimeRefreshQuery, HookStepAdvanceRequest,
    HookTraceEntry, HookTraceTrigger, HookTrigger, HookTurnStartNotice, NoopExecutionHookProvider,
    RuntimeAdapterProvenance, RuntimeContextFragmentEntry, RuntimeEventSource,
    RuntimeHookInjectionEntry, RuntimeSkillEntry, RuntimeToolSchemaEntry, SessionSnapshotMetadata,
    SharedHookRuntime, SubjectRunContext, action_type, context_usage_kind,
};

// ─── workflow scripts ──────────────────────────────────────

pub use workflow::script::WorkflowScriptEvaluator;

// ─── platform ───────────────────────────────────────────────

pub use platform::auth::{AuthGroup, AuthIdentity, AuthMode};
pub use platform::function_runner::{ApiRequestOutcome, BashExecOutcome, FunctionRunner};
pub use platform::marketplace_source::{
    MarketplaceAssetDetail, MarketplaceAssetListing, MarketplaceAssetPage, MarketplaceAssetQuery,
    MarketplaceFetchedAsset, MarketplaceFetchedAssetPayload, MarketplaceInstallRequirement,
    MarketplaceInstallRequirementKind, MarketplaceSourceDescriptor, MarketplaceSourceError,
    MarketplaceSourceProvider, MarketplaceSourceProviderKind, MarketplaceSourceTrustLevel,
};
pub use platform::mcp_injection::{McpInjectionConfig, ToolScope};
pub use platform::mcp_probe::{McpProbeTransport, McpProbedTool};
pub use platform::mcp_relay::{
    McpRelayProvider, RelayMcpCallContext, RelayMcpCallResult, RelayMcpListOutcome,
    RelayMcpSourceOutcome, RelayMcpToolInfo,
};
pub use platform::memory_discovery::{
    DiscoveredMemorySource, MemoryDiscoveryCluster, MemoryDiscoveryContext,
    MemoryDiscoveryDiagnostic, MemoryDiscoveryError, MemoryDiscoveryMount, MemoryDiscoveryOutput,
    MemoryDiscoveryOwnerKind, MemoryDiscoveryProvider, MemoryDiscoveryUserContext,
    MemoryDiscoveryVfsFile, MemoryDiscoveryVfsRule, MemoryIndexStatus, MemorySourceFormat,
    MemorySourceScope, MemorySourceTrustLevel, is_controlled_vfs_memory_uri,
};
pub use platform::mount::MountEditCapabilities;
pub use platform::routine::{RoutineFireCallback, RoutineTriggerProvider};
pub use platform::skill::SkillRef;
pub use platform::skill_discovery::{
    DiscoveredSkill, SkillCapabilityId, SkillContextExposure, SkillDiscoveryCluster,
    SkillDiscoveryContext, SkillDiscoveryDiagnostic, SkillDiscoveryError, SkillDiscoveryOutput,
    SkillDiscoveryOwnerKind, SkillDiscoveryProvider, SkillDiscoveryUserContext,
    SkillDiscoveryVfsFile, SkillDiscoveryVfsRule, skill_capability_key,
};
pub use platform::skill_source::{
    RemoteSkillFetch, RemoteSkillFile, RemoteSkillFileBody, RemoteSkillKind, RemoteSkillSource,
    RemoteSkillSourceError,
};
pub use platform::tool_capability::{
    CapabilityScope, CapabilityScopeCtx, CapabilityVisibilityRule, PlatformMcpScope,
    ToolCapability, ToolDescriptor, ToolSource, capability_to_platform_mcp_scope,
    capability_to_tool_clusters, default_visibility_rules, format_tool_for_prompt,
    is_capability_visible, platform_tool_descriptors, platform_tools_for_capability,
};

pub use session_persistence::{
    AccumulationPolicy, AgentFrameTransitionRecord, ApplyMountOperationsEffect,
    ApplyVfsOverlayEffect, CAPABILITY_DIMENSION_CHANNEL, CAPABILITY_DIMENSION_COMPANION,
    CAPABILITY_DIMENSION_MCP, CAPABILITY_DIMENSION_SKILL, CAPABILITY_DIMENSION_TOOL,
    CAPABILITY_DIMENSION_VFS, CAPABILITY_DIMENSION_WORKSPACE_MODULE, CapabilityArtifactSource,
    CapabilityContributionRecord, CapabilityDeclarationRecord, CapabilityDimensionKey,
    CompactionProjectionCommitResult, EFFECT_TYPE_APPLY_MOUNT_OPERATIONS,
    EFFECT_TYPE_APPLY_VFS_OVERLAY, EFFECT_TYPE_SET_CHANNEL_PROJECTION,
    EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER, EFFECT_TYPE_SET_MCP_SERVER_SET,
    EFFECT_TYPE_SET_TOOL_ACCESS, ExecutionStatus, NewCompactionProjectionCommit,
    PendingCapabilityStateTransition, PersistedSessionEvent, RuntimeCapabilityEffectRecord,
    RuntimeCapabilityTransition, RuntimeCommandRecord, RuntimeCommandStatus,
    RuntimeDeliveryCommand, RuntimeDeliveryCommandKind, SESSION_PROJECTION_KIND_AUDIT,
    SESSION_PROJECTION_KIND_HANDOFF, SESSION_PROJECTION_KIND_MODEL_CONTEXT,
    SESSION_PROJECTION_KIND_TIMELINE, SessionCompactionRecord, SessionCompactionStatus,
    SessionEventBacklog, SessionEventPage, SessionMeta, SessionProjectionHeadRecord,
    SessionProjectionSegmentRecord, SetChannelProjectionEffect, SetCompanionAgentRosterEffect,
    SetMcpServerSetEffect, SetToolAccessEffect,
};
