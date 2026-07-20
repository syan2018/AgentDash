pub mod context;
pub mod extension_package;
pub mod hooks;
pub mod platform;
pub mod workflow;

// ─── agent-types re-export（保持外部 API 不变）──────────────

pub use agentdash_agent::{
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
pub use agentdash_agent::{
    AgentTool, AgentToolError, AgentToolResult, ContentPart, DynAgentTool, ToolDefinition,
    ToolProtocolProjector, ToolUpdateCallback,
};

// ─── domain re-export ───────────────────────────────────────

pub use agentdash_domain::common::{
    AgentConfig, AgentPresetConfig, Mount, MountCapability, MountLink, ThinkingLevel, Vfs,
};

// ─── platform runtime surface ───────────────────────────────

pub use agentdash_domain::backend::{
    MissingRuntimeBackendAnchor, RuntimeBackendAnchor, RuntimeBackendAnchorError,
    RuntimeBackendAnchorSource,
};
pub use platform::capability_delta::{
    CapabilityStateDelta, DefaultMountDelta, McpServerReadinessSummary, NamedEntityDelta, SetDelta,
    VfsSurfaceDelta, compute_capability_state_delta,
};
pub use platform::runtime_surface::{
    CapabilityState, ChannelDimension, CompanionDimension, DiscoveredGuideline, DiscoveryContext,
    ExecutionBackendPlacement, ExecutionContext, ExecutionSessionFrame, ExecutionTurnFrame,
    ExecutionTurnMode, McpEnvVar, McpHttpHeader, McpTransportConfig, PlatformRuntimeError,
    PlatformToolExecutionContext, PlatformToolInvocationCoordinates, RestoredSessionState,
    RuntimeMcpServer, RuntimeMcpSourceReadiness, RuntimeVfsAccessPolicy, RuntimeVfsAccessRule,
    RuntimeVfsAccessSource, RuntimeVfsOperation, RuntimeVfsPathPattern, SkillClusterMeta,
    SkillDimension, ToolCapabilityFilter, ToolCluster, ToolDimension, VfsDimension,
    WorkspaceModuleDimension, WorkspaceModuleVisibilityMode, partition_runtime_mcp_servers,
    workspace_path_from_context,
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
pub use hooks::{
    ActiveWorkflowMeta, AgentFrameHookEvaluationQuery, AgentFrameHookSnapshot,
    AgentFrameHookSnapshotQuery, AgentFrameRuntimeSnapshot, ContextTokenStats, HookApprovalRequest,
    HookCompactionDecision, HookCompletionStatus, HookControlTarget, HookDiagnosticEntry,
    HookEffect, HookError, HookEvaluationTrigger, HookInjection, HookPendingAction,
    HookPendingActionResolutionKind, HookPendingActionStatus, HookResolution, HookRuntimeAccess,
    HookRuntimeEvaluationQuery, HookRuntimeRefreshQuery, HookStepAdvanceRequest, HookTraceEntry,
    HookTraceTrigger, HookTrigger, HookTurnStartNotice, RuntimeAdapterProvenance,
    RuntimeEventSource, SessionSnapshotMetadata, SharedHookRuntime, SubjectRunContext, action_type,
    context_usage_kind,
};

// ─── workflow scripts ──────────────────────────────────────

pub use workflow::script::WorkflowScriptEvaluator;

// ─── platform ───────────────────────────────────────────────

pub use platform::auth::{AuthGroup, AuthIdentity, AuthMode};
pub use platform::function_runner::{
    ApiRequestOutcome, BashExecOutcome, FunctionEffectObservation, FunctionEffectRawOutcome,
    FunctionEffectRequest, FunctionEffectSpec, FunctionRunner,
};
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
