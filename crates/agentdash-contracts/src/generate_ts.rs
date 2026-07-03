use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_contracts::agent_run_mailbox::{
    AgentRunAcceptedRefs, AgentRunCommandReceipt, AgentRunComposerSubmitRequest,
    AgentRunForkLineageView, AgentRunForkOutcomeView, AgentRunForkRequest, AgentRunForkResponse,
    AgentRunForkSubmitRequest, AgentRunMailboxMessageContentView, AgentRunMailboxMoveRequest,
    AgentRunMailboxView, AgentRunMessageAcceptedRefs, AgentRunMessageCommandOutcome,
    AgentRunMessageCommandResponse, AgentRunToolCallApprovalResponse,
    AgentRunToolCallRejectionResponse, BackendSelectionModeDto, BackendSelectionRequestDto,
    ConsumptionBarrier, MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin,
    MailboxMessageStatus, MailboxMessageView, MailboxSourceIdentity, MailboxStateView,
    RuntimeSessionCommandStateDto, SteeringStopEffect,
};
use agentdash_contracts::auth::{
    AuthGroup, AuthMode, AuthStartRequest, AuthStartResponse, CurrentUser, DirectoryGroup,
    DirectoryGroupResolveResponse, DirectoryGroupSearchResponse, DirectoryResolveRequest,
    DirectoryTreeNode, DirectoryTreeResponse, DirectoryUser, DirectoryUserResolveResponse,
    DirectoryUserSearchResponse, LoginCredentials, LoginFieldDescriptor, LoginMetadata, LoginMode,
    LoginResponse,
};
use agentdash_contracts::backend::{
    BackendActiveSessionResponse, BackendCapabilitiesResponse, BackendExecutionLeaseState,
    BackendExecutionSelectionMode, BackendExecutorCapabilityResponse,
    BackendMcpServerCapabilityResponse, BackendResponse, BackendRuntimeExecutorResponse,
    BackendRuntimeHealthResponse, BackendRuntimeSummaryResponse, BackendShareScopeKind,
    BackendType, BackendVisibility, BackendWithStatusResponse, BackendWorkspaceInventoryResponse,
    BackendWorkspaceInventorySource, BackendWorkspaceInventoryStatus, CapabilityHealthAction,
    CapabilityHealthDomain, CapabilityHealthItem, CapabilityHealthStatus,
    CreateProjectBackendAccessRequest, ProjectBackendAccessMode, ProjectBackendAccessResponse,
    ProjectBackendAccessStatus, RegisterBackendWorkspaceInventoryRequest,
    RunnerRegistrationClaimRequest, RunnerRegistrationClaimResponse,
    RunnerRegistrationTokenCreateRequest, RunnerRegistrationTokenCreateResponse,
    RunnerRegistrationTokenMetadataResponse, RunnerRegistrationTokenRevokeResponse,
    RunnerRegistrationTokenRotateResponse, RunnerRegistrationTokenStatus, RuntimeHealthStatus,
    UpdateProjectBackendAccessRequest,
};
use agentdash_contracts::canvas::{
    CanvasAccessDto, CanvasAgentInputSubmitRequest, CanvasAgentRunRuntimeSnapshotDto,
    CanvasFileDto, CanvasImportMapDto, CanvasInteractionEventDto, CanvasInteractionSnapshot,
    CanvasInteractionSnapshotUpsertRequest, CanvasListScopeDto, CanvasResponse,
    CanvasRuntimeBindingDto, CanvasRuntimeBindingUpsertRequest, CanvasRuntimeBridgeSnapshotDto,
    CanvasRuntimeDiagnosticDto, CanvasRuntimeDocumentStateDto, CanvasRuntimeFileDto,
    CanvasRuntimeInvokeRequest, CanvasRuntimeObservation, CanvasRuntimeObservationStatusDto,
    CanvasRuntimeObservationUpsertRequest, CanvasRuntimeSnapshotDto, CanvasRuntimeViewportDto,
    CanvasSandboxConfigDto, CanvasScopeDto, CopyCanvasToPersonalRequest, CreateCanvasRequest,
    DeleteCanvasResponse, ListCanvasesQuery, PublishCanvasToProjectRequest,
    RuntimeActionDescriptorDto, RuntimeActionKindDto, RuntimeContextDto,
    RuntimeInvocationOutputDto, RuntimeInvocationResultDto, RuntimePolicyDto, RuntimeSurfaceDto,
    RuntimeTraceDto, UnpublishCanvasResponse, UpdateCanvasRequest,
};
use agentdash_contracts::common_response::{
    DeletedFlagResponse, DeletedIdResponse, PendingExecutionResponse, RevokedIdResponse,
    UpdatedIdResponse,
};
use agentdash_contracts::companion::{CompanionGateRespondRequest, CompanionGateRespondResponse};
use agentdash_contracts::context::{
    ContextContainerDefinition, ContextContainerFile, ContextContainerProvider, ContextDelivery,
    ContextSlot, ContextSourceKind, ContextSourceRef, SessionComposition,
    SessionRequiredContextBlock, VfsCapabilityDto,
};
use agentdash_contracts::contract_generation::{
    GeneratedTsFile, NDJSON_STREAM_VALIDATORS_FILENAME, render_common_json_value,
    render_domain_file, render_ndjson_stream_validators,
};
use agentdash_contracts::extension_management::{
    ProjectExtensionCapabilitySummaryResponse, ProjectExtensionInstalledSourceResponse,
    ProjectExtensionManagementItemResponse, ProjectExtensionManagementListResponse,
    ProjectExtensionPackageArtifactRefResponse, ProjectExtensionPackageModeResponse,
};
use agentdash_contracts::extension_package::{
    ExtensionPackageArtifactResponse, ExtensionPackageInstallationResponse,
    ImportExtensionPackageResponse, InstallExtensionPackageArtifactRequest,
};
use agentdash_contracts::extension_runtime::{
    ExtensionBundleKindResponse, ExtensionBundleProjectionResponse,
    ExtensionCommandHandlerResponse, ExtensionCommandProjectionResponse,
    ExtensionDependencyDeclarationResponse, ExtensionDependencyProjectionResponse,
    ExtensionFlagProjectionResponse, ExtensionFlagTypeResponse,
    ExtensionInstallationProjectionResponse, ExtensionInstalledAssetSourceResponse,
    ExtensionMessageRendererDeclarationResponse, ExtensionMessageRendererProjectionResponse,
    ExtensionPackageArtifactRefResponse, ExtensionPermissionAccessResponse,
    ExtensionPermissionDeclarationResponse, ExtensionPermissionProjectionResponse,
    ExtensionProcessPermissionAccessResponse, ExtensionProtocolChannelMethodProjectionResponse,
    ExtensionProtocolChannelProjectionResponse, ExtensionRuntimeActionKindResponse,
    ExtensionRuntimeActionProjectionResponse, ExtensionRuntimeInvocationOutputResponse,
    ExtensionRuntimeInvokeActionRequest, ExtensionRuntimeInvokeActionResponse,
    ExtensionRuntimeInvokeChannelRequest, ExtensionRuntimeInvokeChannelResponse,
    ExtensionRuntimeProjectionResponse, ExtensionRuntimeTraceResponse,
    ExtensionWorkspaceTabLoadabilityModeResponse, ExtensionWorkspaceTabLoadabilityResponse,
    ExtensionWorkspaceTabProjectionResponse, ExtensionWorkspaceTabRendererResponse,
    UninstallExtensionInstallationResponse,
};
use agentdash_contracts::external_marketplace::{
    ExternalMarketplaceAssetDetailDto, ExternalMarketplaceAssetListingDto,
    ExternalMarketplaceAssetPageDto, ExternalMarketplaceInstallRequirementDto,
    ExternalMarketplaceRefreshStatus, ImportExternalMarketplaceAssetRequest,
    ImportExternalMarketplaceAssetResponse, ListExternalMarketplaceAssetsQuery,
    MarketplaceInstallRequirementKindDto, MarketplaceSourceDto, MarketplaceSourceProviderKindDto,
    MarketplaceSourceTrustLevelDto, RefreshExternalMarketplaceAssetRequest,
    RefreshExternalMarketplaceAssetResponse,
};
use agentdash_contracts::llm_provider::{
    CodexOAuthCredentialTargetDto, CodexOAuthFlowStatusDto, CodexOAuthStatusResponse,
    CompleteCodexOAuthRequest, CreateLlmProviderRequest, DeleteLlmProviderResponse,
    DeleteLlmProviderUserCredentialResponse, EffectiveLlmModelProfileDto, EffectiveLlmProviderDto,
    FailCodexOAuthRequest, LlmCredentialModeDto, LlmCredentialSourceDto,
    LlmCredentialVerificationStatusDto, LlmProviderAdminDto, LlmProviderProtocol,
    PrepareCodexOAuthRequest, ProbeLlmProviderModelDto, ProbeLlmProviderModelsRequest,
    ReorderLlmProvidersRequest, ReorderLlmProvidersResponse, StartCodexOAuthResponse,
    UpdateLlmProviderRequest, UpsertLlmProviderUserCredentialRequest,
};
use agentdash_contracts::mcp_preset::{
    CloneMcpPresetRequest, CreateMcpPresetRequest, DeleteMcpPresetResponse, ListMcpPresetQuery,
    McpPresetResponse, McpProbeTargetDto, ProbeMcpPresetRequest, ProbeMcpPresetResponse,
    UpdateMcpPresetRequest,
};
use agentdash_contracts::permission::{
    ListPermissionGrantsQuery, PermissionGrantResponse, PermissionGrantScopeDto,
    PermissionGrantStatusDto, PermissionGrantStatusGroupDto, PermissionGrantVfsAccessRuleDto,
    PermissionGrantVfsOperationDto, PermissionGrantVfsPathScopeDto, PolicyDecisionDto,
    PolicyOutcomeDto, ScopeEscalationIntentDto,
};
use agentdash_contracts::project::{
    AgentPreset, DeletedProjectSubjectGrantResponse, ProjectAccessSummaryResponse, ProjectConfig,
    ProjectDetailResponse, ProjectEventStreamEnvelope, ProjectResponse, ProjectRole,
    ProjectStateChange, ProjectStateChangeKind, ProjectSubjectGrantResponse, ProjectSubjectType,
    ProjectVisibility, RevokeProjectGrantResponse, SchedulingConfig,
};
use agentdash_contracts::project_agent::{
    CreateProjectAgentRequest, CreateProjectAgentRunRequest, ProjectAgent, ProjectAgentExecutor,
    ProjectAgentRunStartResult, ProjectAgentSummary, UpdateProjectAgentRequest,
};
use agentdash_contracts::routine::{
    CreateRoutineRequest, EnableRoutineRequest, FireWebhookRequest, ListExecutionsQuery,
    RegenerateTokenResponse, RoutineAgentRuntimeRefsDto, RoutineCreationResponse,
    RoutineDispatchStrategyDto, RoutineExecutionResponse, RoutineExecutionStatusDto,
    RoutineOrchestrationBindingRefsDto, RoutineResponse, RoutineTriggerConfigRequest,
    RoutineTriggerConfigResponse, UpdateRoutineRequest,
};
use agentdash_contracts::session::{
    ApproveToolCallResponse, CreateSessionForkRequest, DeleteSessionResponse,
    RejectToolCallResponse, RollbackSessionProjectionRequest, SessionCommandStateResponse,
    SessionEventResponse, SessionEventsPageResponse, SessionForkChildSessionResponse,
    SessionForkResponse, SessionLineageRecordResponse, SessionLineageRelationKindDto,
    SessionLineageStatusDto, SessionLineageViewResponse, SessionMessageRefDto,
    SessionNdjsonEnvelope, SessionProjectionMessageRefResponse, SessionProjectionRollbackResponse,
    SessionProjectionSegmentProvenanceResponse, SessionProjectionSegmentViewResponse,
    SessionProjectionSourceRangeResponse, SessionProjectionViewResponse,
};
use agentdash_contracts::settings::{
    SettingResponse, SettingUpdate, SettingsScopeKind, SettingsScopeQuery, UpdateSettingsRequest,
    UpdateSettingsResponse,
};
use agentdash_contracts::shared_library::{
    InstallLibraryAssetOptions, InstallLibraryAssetRequest, InstallLibraryAssetResponse,
    InstalledAssetSourceDto, LibraryAssetDto, LibraryExtensionPackageArtifactDto,
    ListLibraryAssetsQuery, McpServerTemplatePayloadDto, McpTransportTemplateDto,
    ProjectAssetSourceStatusDto, PublishLibraryAssetRequest, SeedBuiltinLibraryAssetsRequest,
};
use agentdash_contracts::skill_asset::{
    CreateSkillAssetRequest, ImportRemoteSkillAssetRequest, ListSkillAssetQuery,
    RemoteSkillAssetSourceDto, RemoteSkillAssetSourceType, SkillAssetDto,
    SkillAssetFileContentKind, SkillAssetFileDto, SkillAssetFileKind, SkillAssetSource,
    UpdateSkillAssetRequest,
};
use agentdash_contracts::story::{
    StoryContext, StoryPriority, StoryResponse, StoryStatus, StoryTaskProjectionItem,
    StoryTaskProjectionResponse, StoryTaskProjectionSource, StoryTaskProjectionSourceKind,
    StoryType,
};
use agentdash_contracts::task::{
    CreateRunTaskRequest, RunTaskCommandResponse, RunTaskPlanResponse, TaskPlanStatus,
    TaskPriority, TaskResponse, TaskStatus, UpdateRunTaskRequest, UpdateRunTaskStatusRequest,
};
use agentdash_contracts::vfs::{
    ConfigurableProviderInfo, CreateProjectVfsMountRequest, DeleteProjectVfsMountResponse,
    ListEntriesResponse, ListVfssResponse, ProjectVfsMountResponse, ResolveSurfaceRequest,
    ResolvedVfsSurface, SurfaceApplyPatchRequest, SurfaceApplyPatchResponse,
    SurfaceCreateFileRequest, SurfaceCreateFileResponse, SurfaceDeleteFileRequest,
    SurfaceDeleteFileResponse, SurfaceEntriesResponse, SurfaceReadBinaryFileRequest,
    SurfaceReadFileRequest, SurfaceReadFileResponse, SurfaceRenameFileRequest,
    SurfaceRenameFileResponse, SurfaceStatFileRequest, SurfaceStatFileResponse,
    SurfaceUploadBinaryFileResponse, SurfaceWriteFileRequest, SurfaceWriteFileResponse,
    UpdateProjectVfsMountRequest,
};
use agentdash_contracts::workflow::{
    ActiveRuntimeNodeRefDto, ActivityDefinition, ActivityTransition,
    AgentConversationContentPartView, AgentConversationFeedMessage, AgentConversationFeedSnapshot,
    AgentConversationIdentity, AgentConversationLifecycleContext, AgentConversationMessageRefView,
    AgentConversationMessageRole, AgentConversationSnapshot, AgentConversationSourceRangeView,
    AgentConversationToolCallView, AgentConversationToolResultView, AgentFrameRefDto,
    AgentFrameRuntimeView, AgentProcedureContract, AgentProcedureResponse,
    AgentRunCommandOnlyRequest, AgentRunCommandPreconditionView, AgentRunRefDto,
    AgentRunResourceSurfaceCoordinateView, AgentRunResourceSurfaceSourceAnchorView, AgentRunView,
    AgentRunWorkspaceControlPlaneStatus, AgentRunWorkspaceControlPlaneView,
    AgentRunWorkspaceListEntry, AgentRunWorkspaceListView, AgentRunWorkspaceShell,
    AgentRunWorkspaceView, CapabilityCatalogEntryDto, CapabilityCatalogResponse,
    CapabilityScopeDto, ContinueLifecycleRunResponse, ConversationCommandKind,
    ConversationCommandPlacement, ConversationCommandSetView, ConversationCommandStaleGuardView,
    ConversationCommandView, ConversationDiagnosticView, ConversationEffectiveExecutorConfigView,
    ConversationExecutionStatus, ConversationExecutionView, ConversationKeyboardMapView,
    ConversationMailboxSnapshotView, ConversationModelConfigSource, ConversationModelConfigStatus,
    ConversationModelConfigView, ConversationWaitingItemView, DefinitionSource,
    DeleteAgentProcedureResponse, DeleteAgentRunResponse, DeleteHookPresetResponse,
    DeleteWorkflowGraphResponse, EffectiveSessionContract, HookPresetResponse, HookPresetsResponse,
    LaunchedAgentNodeDto, LifecycleExecutionEntry, LifecycleRunRefDto, LifecycleRunStatus,
    LifecycleRunTopology, LifecycleRunView, LifecycleSubjectAssociationDto, OpenedHumanGateDto,
    OrchestrationExecutorDrainResultDto, OrchestrationInstanceView, PlatformMcpScopeDto,
    PreflightWorkflowScriptRequest, PreflightWorkflowScriptResponse, ProjectActiveAgentsView,
    RegisterHookPresetResponse, RuntimeNodeView, RuntimeSessionExecutionAnchorDto,
    RuntimeSessionRefDto, RuntimeSessionTraceMeta, RuntimeSessionTraceView,
    SessionRuntimeControlPlaneStatus, SessionRuntimeControlPlaneView, SessionRuntimeControlView,
    SessionShellDto, SubjectExecutionView, SubjectRefDto, SubjectRuntimeAttemptView,
    SubmitOrchestrationHumanDecisionRequest, SubmitOrchestrationHumanDecisionResponse,
    ToolClusterDto, ToolDescriptorDto, ToolSourceDto, ValidateHookScriptResponse, ValidationIssue,
    WorkflowGraphResponse, WorkflowHookTrigger, WorkflowScriptApiEndpointDto,
    WorkflowScriptBashCommandDto, WorkflowScriptCapabilitySummaryDto,
    WorkflowScriptHumanGateCapabilityDto, WorkflowScriptPlanPreviewDto,
    WorkflowScriptPlanPreviewNodeDto, WorkflowScriptPreflightDiagnosticDto, WorkflowTargetKind,
};
use agentdash_contracts::workspace::{
    BindDiscoveredWorkspaceBindingRequest, BindDiscoveredWorkspaceBindingsRequest,
    BindDiscoveredWorkspaceBindingsResponse, DiscoverLocalWorkspaceBindingsRequest,
    DiscoverLocalWorkspaceBindingsResponse, DiscoveredWorkspaceBindingCandidate,
    WorkspaceBindingResponse, WorkspaceBindingStatus, WorkspaceBindingSyncResult,
    WorkspaceIdentityDiscoverySkipped, WorkspaceIdentityKind, WorkspaceInventoryCandidate,
    WorkspaceResolutionPolicy, WorkspaceResponse, WorkspaceStatus,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch, WorkspaceModuleOperationReadiness,
    WorkspaceModuleOperationReadinessKind, WorkspaceModulePresentRequest,
    WorkspaceModulePresentation, WorkspaceModuleStatus, WorkspaceModuleStatusKind,
    WorkspaceModuleSummary, WorkspaceModuleUiEntry,
};
use ts_rs::TS;

fn main() {
    let check = env::args().any(|arg| arg == "--check");
    let generated_dir: PathBuf =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/app-web/src/generated");

    write_common_json_value(&generated_dir.join("common-contracts.ts"), check);

    // Upstream registry: type_name -> import source (e.g. "./backbone-protocol").
    // Each domain strips types already claimed upstream and emits `import type` instead.
    let mut upstream: BTreeMap<String, String> = BTreeMap::new();

    // --- auth-contracts.ts ---
    emit_domain(
        &generated_dir,
        "auth-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<AuthMode>(dir);
            export_all::<AuthGroup>(dir);
            export_all::<CurrentUser>(dir);
            export_all::<LoginCredentials>(dir);
            export_all::<LoginMode>(dir);
            export_all::<LoginFieldDescriptor>(dir);
            export_all::<LoginMetadata>(dir);
            export_all::<AuthStartRequest>(dir);
            export_all::<AuthStartResponse>(dir);
            export_all::<LoginResponse>(dir);
            export_all::<DirectoryUser>(dir);
            export_all::<DirectoryGroup>(dir);
            export_all::<DirectoryResolveRequest>(dir);
            export_all::<DirectoryTreeNode>(dir);
            export_all::<DirectoryUserSearchResponse>(dir);
            export_all::<DirectoryGroupSearchResponse>(dir);
            export_all::<DirectoryTreeResponse>(dir);
            export_all::<DirectoryUserResolveResponse>(dir);
            export_all::<DirectoryGroupResolveResponse>(dir);
        },
    );

    // --- backbone-protocol.ts (canonical source for codex/agent-protocol types) ---
    emit_domain(
        &generated_dir,
        "backbone-protocol.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<BackboneEnvelope>(dir);
        },
    );

    // --- agent-run-mailbox-contracts.ts ---
    emit_domain(
        &generated_dir,
        "agent-run-mailbox-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<AgentRunComposerSubmitRequest>(dir);
            export_all::<AgentRunForkLineageView>(dir);
            export_all::<AgentRunForkOutcomeView>(dir);
            export_all::<AgentRunForkRequest>(dir);
            export_all::<AgentRunForkResponse>(dir);
            export_all::<AgentRunForkSubmitRequest>(dir);
            export_all::<BackendSelectionModeDto>(dir);
            export_all::<BackendSelectionRequestDto>(dir);
            export_all::<AgentRunCommandReceipt>(dir);
            export_all::<AgentRunAcceptedRefs>(dir);
            export_all::<AgentRunMessageCommandResponse>(dir);
            export_all::<AgentRunToolCallApprovalResponse>(dir);
            export_all::<AgentRunToolCallRejectionResponse>(dir);
            export_all::<MailboxMessageStatus>(dir);
            export_all::<MailboxMessageOrigin>(dir);
            export_all::<MailboxSourceIdentity>(dir);
            export_all::<SteeringStopEffect>(dir);
            export_all::<MailboxDelivery>(dir);
            export_all::<ConsumptionBarrier>(dir);
            export_all::<MailboxDrainMode>(dir);
            export_all::<AgentRunMessageAcceptedRefs>(dir);
            export_all::<MailboxMessageView>(dir);
            export_all::<AgentRunMailboxMoveRequest>(dir);
            export_all::<AgentRunMailboxMessageContentView>(dir);
            export_all::<MailboxStateView>(dir);
            export_all::<AgentRunMessageCommandOutcome>(dir);
            export_all::<RuntimeSessionCommandStateDto>(dir);
            export_all::<AgentRunMailboxView>(dir);
        },
    );

    // --- project-agent-contracts.ts (canonical source for agent-construct Ref DTOs) ---
    emit_domain(
        &generated_dir,
        "project-agent-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<ProjectAgent>(dir);
            export_all::<ProjectAgentExecutor>(dir);
            export_all::<ProjectAgentSummary>(dir);
            export_all::<CreateProjectAgentRunRequest>(dir);
            export_all::<ProjectAgentRunStartResult>(dir);
            export_all::<CreateProjectAgentRequest>(dir);
            export_all::<UpdateProjectAgentRequest>(dir);
        },
    );

    // --- routine-contracts.ts ---
    emit_domain(
        &generated_dir,
        "routine-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<RoutineTriggerConfigRequest>(dir);
            export_all::<RoutineTriggerConfigResponse>(dir);
            export_all::<RoutineDispatchStrategyDto>(dir);
            export_all::<RoutineExecutionStatusDto>(dir);
            export_all::<RoutineOrchestrationBindingRefsDto>(dir);
            export_all::<RoutineAgentRuntimeRefsDto>(dir);
            export_all::<RoutineResponse>(dir);
            export_all::<RoutineCreationResponse>(dir);
            export_all::<RoutineExecutionResponse>(dir);
            export_all::<CreateRoutineRequest>(dir);
            export_all::<UpdateRoutineRequest>(dir);
            export_all::<EnableRoutineRequest>(dir);
            export_all::<RegenerateTokenResponse>(dir);
            export_all::<FireWebhookRequest>(dir);
            export_all::<ListExecutionsQuery>(dir);
        },
    );

    // --- common-response-contracts.ts ---
    emit_domain(
        &generated_dir,
        "common-response-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<DeletedIdResponse>(dir);
            export_all::<DeletedFlagResponse>(dir);
            export_all::<UpdatedIdResponse>(dir);
            export_all::<RevokedIdResponse>(dir);
            export_all::<PendingExecutionResponse>(dir);
        },
    );

    // --- context-contracts.ts ---
    emit_domain(
        &generated_dir,
        "context-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<VfsCapabilityDto>(dir);
            export_all::<ContextContainerFile>(dir);
            export_all::<ContextContainerProvider>(dir);
            export_all::<ContextContainerDefinition>(dir);
            export_all::<ContextSourceKind>(dir);
            export_all::<ContextSlot>(dir);
            export_all::<ContextDelivery>(dir);
            export_all::<ContextSourceRef>(dir);
            export_all::<SessionRequiredContextBlock>(dir);
            export_all::<SessionComposition>(dir);
        },
    );

    // --- backend-contracts.ts ---
    emit_domain(
        &generated_dir,
        "backend-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<BackendType>(dir);
            export_all::<BackendVisibility>(dir);
            export_all::<BackendShareScopeKind>(dir);
            export_all::<RuntimeHealthStatus>(dir);
            export_all::<BackendRuntimeHealthResponse>(dir);
            export_all::<BackendExecutorCapabilityResponse>(dir);
            export_all::<BackendMcpServerCapabilityResponse>(dir);
            export_all::<BackendCapabilitiesResponse>(dir);
            export_all::<CapabilityHealthStatus>(dir);
            export_all::<CapabilityHealthDomain>(dir);
            export_all::<CapabilityHealthAction>(dir);
            export_all::<CapabilityHealthItem>(dir);
            export_all::<BackendExecutionSelectionMode>(dir);
            export_all::<BackendExecutionLeaseState>(dir);
            export_all::<BackendRuntimeExecutorResponse>(dir);
            export_all::<BackendActiveSessionResponse>(dir);
            export_all::<BackendRuntimeSummaryResponse>(dir);
            export_all::<BackendResponse>(dir);
            export_all::<BackendWithStatusResponse>(dir);
            export_all::<ProjectBackendAccessStatus>(dir);
            export_all::<ProjectBackendAccessMode>(dir);
            export_all::<CreateProjectBackendAccessRequest>(dir);
            export_all::<UpdateProjectBackendAccessRequest>(dir);
            export_all::<ProjectBackendAccessResponse>(dir);
            export_all::<BackendWorkspaceInventoryStatus>(dir);
            export_all::<BackendWorkspaceInventorySource>(dir);
            export_all::<BackendWorkspaceInventoryResponse>(dir);
            export_all::<RegisterBackendWorkspaceInventoryRequest>(dir);
            export_all::<RunnerRegistrationTokenStatus>(dir);
            export_all::<RunnerRegistrationTokenCreateRequest>(dir);
            export_all::<RunnerRegistrationTokenMetadataResponse>(dir);
            export_all::<RunnerRegistrationTokenCreateResponse>(dir);
            export_all::<RunnerRegistrationTokenRotateResponse>(dir);
            export_all::<RunnerRegistrationTokenRevokeResponse>(dir);
            export_all::<RunnerRegistrationClaimRequest>(dir);
            export_all::<RunnerRegistrationClaimResponse>(dir);
        },
    );

    // --- workspace-contracts.ts ---
    emit_domain(
        &generated_dir,
        "workspace-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<WorkspaceIdentityKind>(dir);
            export_all::<WorkspaceBindingStatus>(dir);
            export_all::<WorkspaceResolutionPolicy>(dir);
            export_all::<WorkspaceStatus>(dir);
            export_all::<WorkspaceBindingResponse>(dir);
            export_all::<WorkspaceResponse>(dir);
            export_all::<WorkspaceInventoryCandidate>(dir);
            export_all::<WorkspaceBindingSyncResult>(dir);
            export_all::<DiscoverLocalWorkspaceBindingsRequest>(dir);
            export_all::<DiscoveredWorkspaceBindingCandidate>(dir);
            export_all::<WorkspaceIdentityDiscoverySkipped>(dir);
            export_all::<DiscoverLocalWorkspaceBindingsResponse>(dir);
            export_all::<BindDiscoveredWorkspaceBindingRequest>(dir);
            export_all::<BindDiscoveredWorkspaceBindingsRequest>(dir);
            export_all::<BindDiscoveredWorkspaceBindingsResponse>(dir);
        },
    );

    // --- task-contracts.ts ---
    emit_domain(
        &generated_dir,
        "task-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<TaskPlanStatus>(dir);
            export_all::<TaskStatus>(dir);
            export_all::<TaskPriority>(dir);
            export_all::<TaskResponse>(dir);
            export_all::<RunTaskPlanResponse>(dir);
            export_all::<CreateRunTaskRequest>(dir);
            export_all::<UpdateRunTaskRequest>(dir);
            export_all::<UpdateRunTaskStatusRequest>(dir);
            export_all::<RunTaskCommandResponse>(dir);
        },
    );

    // --- story-contracts.ts ---
    emit_domain(
        &generated_dir,
        "story-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<StoryContext>(dir);
            export_all::<StoryStatus>(dir);
            export_all::<StoryPriority>(dir);
            export_all::<StoryType>(dir);
            export_all::<StoryResponse>(dir);
            export_all::<StoryTaskProjectionSourceKind>(dir);
            export_all::<StoryTaskProjectionSource>(dir);
            export_all::<StoryTaskProjectionItem>(dir);
            export_all::<StoryTaskProjectionResponse>(dir);
        },
    );

    // --- project-contracts.ts ---
    emit_domain(
        &generated_dir,
        "project-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<SchedulingConfig>(dir);
            export_all::<AgentPreset>(dir);
            export_all::<ProjectConfig>(dir);
            export_all::<ProjectVisibility>(dir);
            export_all::<ProjectRole>(dir);
            export_all::<ProjectSubjectType>(dir);
            export_all::<ProjectAccessSummaryResponse>(dir);
            export_all::<ProjectResponse>(dir);
            export_all::<ProjectSubjectGrantResponse>(dir);
            export_all::<DeletedProjectSubjectGrantResponse>(dir);
            export_all::<RevokeProjectGrantResponse>(dir);
            export_all::<ProjectDetailResponse>(dir);
            export_all::<ProjectStateChangeKind>(dir);
            export_all::<ProjectStateChange>(dir);
            export_all::<ProjectEventStreamEnvelope>(dir);
        },
    );

    // --- mcp-preset-contracts.ts ---
    emit_domain(
        &generated_dir,
        "mcp-preset-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<McpPresetResponse>(dir);
            export_all::<CreateMcpPresetRequest>(dir);
            export_all::<UpdateMcpPresetRequest>(dir);
            export_all::<CloneMcpPresetRequest>(dir);
            export_all::<ListMcpPresetQuery>(dir);
            export_all::<McpProbeTargetDto>(dir);
            export_all::<ProbeMcpPresetRequest>(dir);
            export_all::<ProbeMcpPresetResponse>(dir);
            export_all::<DeleteMcpPresetResponse>(dir);
        },
    );

    // --- companion-contracts.ts ---
    emit_domain(
        &generated_dir,
        "companion-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<CompanionGateRespondRequest>(dir);
            export_all::<CompanionGateRespondResponse>(dir);
        },
    );

    // --- session-contracts.ts ---
    emit_domain(
        &generated_dir,
        "session-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<SessionEventResponse>(dir);
            export_all::<SessionEventsPageResponse>(dir);
            export_all::<SessionNdjsonEnvelope>(dir);
            export_all::<SessionCommandStateResponse>(dir);
            export_all::<DeleteSessionResponse>(dir);
            export_all::<ApproveToolCallResponse>(dir);
            export_all::<RejectToolCallResponse>(dir);
            export_all::<SessionProjectionSourceRangeResponse>(dir);
            export_all::<SessionProjectionMessageRefResponse>(dir);
            export_all::<SessionProjectionSegmentProvenanceResponse>(dir);
            export_all::<SessionProjectionSegmentViewResponse>(dir);
            export_all::<SessionProjectionViewResponse>(dir);
            export_all::<SessionLineageRelationKindDto>(dir);
            export_all::<SessionLineageStatusDto>(dir);
            export_all::<SessionMessageRefDto>(dir);
            export_all::<CreateSessionForkRequest>(dir);
            export_all::<RollbackSessionProjectionRequest>(dir);
            export_all::<SessionLineageRecordResponse>(dir);
            export_all::<SessionForkChildSessionResponse>(dir);
            export_all::<SessionForkResponse>(dir);
            export_all::<SessionLineageViewResponse>(dir);
            export_all::<SessionProjectionRollbackResponse>(dir);
        },
    );

    // --- llm-provider-contracts.ts ---
    emit_domain(
        &generated_dir,
        "llm-provider-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<LlmProviderProtocol>(dir);
            export_all::<LlmCredentialModeDto>(dir);
            export_all::<LlmCredentialSourceDto>(dir);
            export_all::<LlmCredentialVerificationStatusDto>(dir);
            export_all::<LlmProviderAdminDto>(dir);
            export_all::<EffectiveLlmModelProfileDto>(dir);
            export_all::<EffectiveLlmProviderDto>(dir);
            export_all::<CreateLlmProviderRequest>(dir);
            export_all::<UpdateLlmProviderRequest>(dir);
            export_all::<ReorderLlmProvidersRequest>(dir);
            export_all::<ReorderLlmProvidersResponse>(dir);
            export_all::<DeleteLlmProviderResponse>(dir);
            export_all::<ProbeLlmProviderModelsRequest>(dir);
            export_all::<ProbeLlmProviderModelDto>(dir);
            export_all::<UpsertLlmProviderUserCredentialRequest>(dir);
            export_all::<DeleteLlmProviderUserCredentialResponse>(dir);
            export_all::<CodexOAuthCredentialTargetDto>(dir);
            export_all::<PrepareCodexOAuthRequest>(dir);
            export_all::<CompleteCodexOAuthRequest>(dir);
            export_all::<FailCodexOAuthRequest>(dir);
            export_all::<CodexOAuthFlowStatusDto>(dir);
            export_all::<StartCodexOAuthResponse>(dir);
            export_all::<CodexOAuthStatusResponse>(dir);
        },
    );

    // --- permission-contracts.ts ---
    emit_domain(
        &generated_dir,
        "permission-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<PermissionGrantScopeDto>(dir);
            export_all::<PermissionGrantStatusDto>(dir);
            export_all::<PermissionGrantStatusGroupDto>(dir);
            export_all::<PolicyOutcomeDto>(dir);
            export_all::<PolicyDecisionDto>(dir);
            export_all::<ScopeEscalationIntentDto>(dir);
            export_all::<PermissionGrantVfsOperationDto>(dir);
            export_all::<PermissionGrantVfsPathScopeDto>(dir);
            export_all::<PermissionGrantVfsAccessRuleDto>(dir);
            export_all::<ListPermissionGrantsQuery>(dir);
            export_all::<PermissionGrantResponse>(dir);
        },
    );

    // --- shared-library-contracts.ts ---
    emit_domain(
        &generated_dir,
        "shared-library-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<InstalledAssetSourceDto>(dir);
            export_all::<LibraryExtensionPackageArtifactDto>(dir);
            export_all::<LibraryAssetDto>(dir);
            export_all::<ListLibraryAssetsQuery>(dir);
            export_all::<SeedBuiltinLibraryAssetsRequest>(dir);
            export_all::<InstallLibraryAssetOptions>(dir);
            export_all::<InstallLibraryAssetRequest>(dir);
            export_all::<InstallLibraryAssetResponse>(dir);
            export_all::<McpTransportTemplateDto>(dir);
            export_all::<McpServerTemplatePayloadDto>(dir);
            export_all::<PublishLibraryAssetRequest>(dir);
            export_all::<ProjectAssetSourceStatusDto>(dir);
        },
    );

    // --- skill-asset-contracts.ts ---
    emit_domain(
        &generated_dir,
        "skill-asset-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<SkillAssetSource>(dir);
            export_all::<RemoteSkillAssetSourceType>(dir);
            export_all::<SkillAssetFileContentKind>(dir);
            export_all::<SkillAssetFileKind>(dir);
            export_all::<RemoteSkillAssetSourceDto>(dir);
            export_all::<SkillAssetFileDto>(dir);
            export_all::<SkillAssetDto>(dir);
            export_all::<CreateSkillAssetRequest>(dir);
            export_all::<UpdateSkillAssetRequest>(dir);
            export_all::<ImportRemoteSkillAssetRequest>(dir);
            export_all::<ListSkillAssetQuery>(dir);
        },
    );

    // --- vfs-contracts.ts ---
    emit_domain(
        &generated_dir,
        "vfs-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<ListVfssResponse>(dir);
            export_all::<ListEntriesResponse>(dir);
            export_all::<ConfigurableProviderInfo>(dir);
            export_all::<ResolvedVfsSurface>(dir);
            export_all::<ResolveSurfaceRequest>(dir);
            export_all::<SurfaceEntriesResponse>(dir);
            export_all::<SurfaceReadFileRequest>(dir);
            export_all::<SurfaceReadFileResponse>(dir);
            export_all::<SurfaceReadBinaryFileRequest>(dir);
            export_all::<SurfaceWriteFileRequest>(dir);
            export_all::<SurfaceWriteFileResponse>(dir);
            export_all::<SurfaceCreateFileRequest>(dir);
            export_all::<SurfaceCreateFileResponse>(dir);
            export_all::<SurfaceDeleteFileRequest>(dir);
            export_all::<SurfaceDeleteFileResponse>(dir);
            export_all::<SurfaceRenameFileRequest>(dir);
            export_all::<SurfaceRenameFileResponse>(dir);
            export_all::<SurfaceStatFileRequest>(dir);
            export_all::<SurfaceStatFileResponse>(dir);
            export_all::<SurfaceApplyPatchRequest>(dir);
            export_all::<SurfaceApplyPatchResponse>(dir);
            export_all::<SurfaceUploadBinaryFileResponse>(dir);
            export_all::<CreateProjectVfsMountRequest>(dir);
            export_all::<UpdateProjectVfsMountRequest>(dir);
            export_all::<ProjectVfsMountResponse>(dir);
            export_all::<DeleteProjectVfsMountResponse>(dir);
        },
    );

    // --- workflow-contracts.ts ---
    let workflow_footer = workflow_contracts_footer();
    emit_domain_with_footer(
        &generated_dir,
        "workflow-contracts.ts",
        &mut upstream,
        check,
        Some(&workflow_footer),
        |dir| {
            export_all::<AgentProcedureContract>(dir);
            export_all::<AgentProcedureResponse>(dir);
            export_all::<WorkflowGraphResponse>(dir);
            export_all::<ActivityDefinition>(dir);
            export_all::<ActivityTransition>(dir);
            export_all::<LifecycleExecutionEntry>(dir);
            export_all::<LifecycleRunStatus>(dir);
            export_all::<LifecycleRunTopology>(dir);
            export_all::<EffectiveSessionContract>(dir);
            export_all::<ValidationIssue>(dir);
            export_all::<SubjectRefDto>(dir);
            export_all::<LifecycleRunRefDto>(dir);
            export_all::<AgentRunRefDto>(dir);
            export_all::<AgentFrameRefDto>(dir);
            export_all::<RuntimeSessionRefDto>(dir);
            export_all::<SessionShellDto>(dir);
            export_all::<RuntimeSessionExecutionAnchorDto>(dir);
            export_all::<AgentRunCommandPreconditionView>(dir);
            export_all::<AgentRunCommandOnlyRequest>(dir);
            export_all::<LifecycleSubjectAssociationDto>(dir);
            export_all::<RuntimeNodeView>(dir);
            export_all::<ActiveRuntimeNodeRefDto>(dir);
            export_all::<OrchestrationInstanceView>(dir);
            export_all::<LifecycleRunView>(dir);
            export_all::<SubmitOrchestrationHumanDecisionRequest>(dir);
            export_all::<SubmitOrchestrationHumanDecisionResponse>(dir);
            export_all::<ContinueLifecycleRunResponse>(dir);
            export_all::<OrchestrationExecutorDrainResultDto>(dir);
            export_all::<LaunchedAgentNodeDto>(dir);
            export_all::<OpenedHumanGateDto>(dir);
            export_all::<AgentRunView>(dir);
            export_all::<AgentFrameRuntimeView>(dir);
            export_all::<RuntimeSessionTraceMeta>(dir);
            export_all::<AgentRunWorkspaceShell>(dir);
            export_all::<AgentRunWorkspaceControlPlaneStatus>(dir);
            export_all::<AgentRunWorkspaceControlPlaneView>(dir);
            export_all::<AgentRunResourceSurfaceSourceAnchorView>(dir);
            export_all::<AgentRunResourceSurfaceCoordinateView>(dir);
            export_all::<ConversationExecutionStatus>(dir);
            export_all::<ConversationModelConfigStatus>(dir);
            export_all::<ConversationModelConfigSource>(dir);
            export_all::<ConversationEffectiveExecutorConfigView>(dir);
            export_all::<ConversationModelConfigView>(dir);
            export_all::<ConversationCommandKind>(dir);
            export_all::<ConversationCommandPlacement>(dir);
            export_all::<ConversationCommandStaleGuardView>(dir);
            export_all::<ConversationCommandView>(dir);
            export_all::<ConversationKeyboardMapView>(dir);
            export_all::<ConversationCommandSetView>(dir);
            export_all::<ConversationExecutionView>(dir);
            export_all::<ConversationWaitingItemView>(dir);
            export_all::<ConversationMailboxSnapshotView>(dir);
            export_all::<ConversationDiagnosticView>(dir);
            export_all::<AgentConversationIdentity>(dir);
            export_all::<AgentConversationLifecycleContext>(dir);
            export_all::<AgentConversationSnapshot>(dir);
            export_all::<AgentConversationMessageRefView>(dir);
            export_all::<AgentConversationSourceRangeView>(dir);
            export_all::<AgentConversationMessageRole>(dir);
            export_all::<AgentConversationContentPartView>(dir);
            export_all::<AgentConversationToolCallView>(dir);
            export_all::<AgentConversationToolResultView>(dir);
            export_all::<AgentConversationFeedMessage>(dir);
            export_all::<AgentConversationFeedSnapshot>(dir);
            export_all::<AgentRunWorkspaceView>(dir);
            export_all::<SubjectRuntimeAttemptView>(dir);
            export_all::<SubjectExecutionView>(dir);
            export_all::<ProjectActiveAgentsView>(dir);
            export_all::<RuntimeSessionTraceView>(dir);
            export_all::<SessionRuntimeControlPlaneStatus>(dir);
            export_all::<SessionRuntimeControlPlaneView>(dir);
            export_all::<SessionRuntimeControlView>(dir);
            export_all::<AgentRunWorkspaceListEntry>(dir);
            export_all::<AgentRunWorkspaceListView>(dir);
            export_all::<DefinitionSource>(dir);
            export_all::<WorkflowTargetKind>(dir);
            export_all::<CapabilityScopeDto>(dir);
            export_all::<ToolClusterDto>(dir);
            export_all::<PlatformMcpScopeDto>(dir);
            export_all::<ToolSourceDto>(dir);
            export_all::<ToolDescriptorDto>(dir);
            export_all::<CapabilityCatalogEntryDto>(dir);
            export_all::<CapabilityCatalogResponse>(dir);
            export_all::<DeleteWorkflowGraphResponse>(dir);
            export_all::<DeleteAgentProcedureResponse>(dir);
            export_all::<DeleteAgentRunResponse>(dir);
            export_all::<PreflightWorkflowScriptRequest>(dir);
            export_all::<WorkflowScriptPreflightDiagnosticDto>(dir);
            export_all::<WorkflowScriptPlanPreviewNodeDto>(dir);
            export_all::<WorkflowScriptPlanPreviewDto>(dir);
            export_all::<WorkflowScriptApiEndpointDto>(dir);
            export_all::<WorkflowScriptBashCommandDto>(dir);
            export_all::<WorkflowScriptHumanGateCapabilityDto>(dir);
            export_all::<WorkflowScriptCapabilitySummaryDto>(dir);
            export_all::<PreflightWorkflowScriptResponse>(dir);
            export_all::<HookPresetResponse>(dir);
            export_all::<HookPresetsResponse>(dir);
            export_all::<ValidateHookScriptResponse>(dir);
            export_all::<RegisterHookPresetResponse>(dir);
            export_all::<DeleteHookPresetResponse>(dir);
        },
    );

    // --- canvas-contracts.ts ---
    emit_domain(
        &generated_dir,
        "canvas-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<CanvasFileDto>(dir);
            export_all::<CanvasImportMapDto>(dir);
            export_all::<CanvasSandboxConfigDto>(dir);
            export_all::<CanvasScopeDto>(dir);
            export_all::<CanvasListScopeDto>(dir);
            export_all::<CanvasAccessDto>(dir);
            export_all::<ListCanvasesQuery>(dir);
            export_all::<CanvasResponse>(dir);
            export_all::<CreateCanvasRequest>(dir);
            export_all::<UpdateCanvasRequest>(dir);
            export_all::<DeleteCanvasResponse>(dir);
            export_all::<PublishCanvasToProjectRequest>(dir);
            export_all::<CopyCanvasToPersonalRequest>(dir);
            export_all::<UnpublishCanvasResponse>(dir);
            export_all::<CanvasRuntimeFileDto>(dir);
            export_all::<CanvasRuntimeBindingDto>(dir);
            export_all::<CanvasRuntimeBindingUpsertRequest>(dir);
            export_all::<RuntimeActionKindDto>(dir);
            export_all::<RuntimePolicyDto>(dir);
            export_all::<RuntimeActionDescriptorDto>(dir);
            export_all::<RuntimeContextDto>(dir);
            export_all::<RuntimeSurfaceDto>(dir);
            export_all::<CanvasRuntimeBridgeSnapshotDto>(dir);
            export_all::<CanvasRuntimeSnapshotDto>(dir);
            export_all::<CanvasAgentRunRuntimeSnapshotDto>(dir);
            export_all::<CanvasRuntimeInvokeRequest>(dir);
            export_all::<CanvasRuntimeObservationStatusDto>(dir);
            export_all::<CanvasRuntimeViewportDto>(dir);
            export_all::<CanvasRuntimeDocumentStateDto>(dir);
            export_all::<CanvasRuntimeDiagnosticDto>(dir);
            export_all::<CanvasRuntimeObservationUpsertRequest>(dir);
            export_all::<CanvasRuntimeObservation>(dir);
            export_all::<CanvasInteractionEventDto>(dir);
            export_all::<CanvasInteractionSnapshotUpsertRequest>(dir);
            export_all::<CanvasInteractionSnapshot>(dir);
            export_all::<CanvasAgentInputSubmitRequest>(dir);
            export_all::<RuntimeTraceDto>(dir);
            export_all::<RuntimeInvocationOutputDto>(dir);
            export_all::<RuntimeInvocationResultDto>(dir);
        },
    );

    // --- extension-runtime-contracts.ts ---
    emit_domain(
        &generated_dir,
        "extension-runtime-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<ExtensionRuntimeActionKindResponse>(dir);
            export_all::<ExtensionFlagTypeResponse>(dir);
            export_all::<ExtensionPermissionAccessResponse>(dir);
            export_all::<ExtensionProcessPermissionAccessResponse>(dir);
            export_all::<ExtensionBundleKindResponse>(dir);
            export_all::<ExtensionCommandHandlerResponse>(dir);
            export_all::<ExtensionMessageRendererDeclarationResponse>(dir);
            export_all::<ExtensionWorkspaceTabRendererResponse>(dir);
            export_all::<ExtensionPermissionDeclarationResponse>(dir);
            export_all::<ExtensionInstalledAssetSourceResponse>(dir);
            export_all::<ExtensionPackageArtifactRefResponse>(dir);
            export_all::<ExtensionInstallationProjectionResponse>(dir);
            export_all::<ExtensionCommandProjectionResponse>(dir);
            export_all::<ExtensionFlagProjectionResponse>(dir);
            export_all::<ExtensionMessageRendererProjectionResponse>(dir);
            export_all::<ExtensionRuntimeActionProjectionResponse>(dir);
            export_all::<ExtensionProtocolChannelMethodProjectionResponse>(dir);
            export_all::<ExtensionProtocolChannelProjectionResponse>(dir);
            export_all::<ExtensionDependencyDeclarationResponse>(dir);
            export_all::<ExtensionDependencyProjectionResponse>(dir);
            export_all::<ExtensionWorkspaceTabLoadabilityModeResponse>(dir);
            export_all::<ExtensionWorkspaceTabLoadabilityResponse>(dir);
            export_all::<ExtensionWorkspaceTabProjectionResponse>(dir);
            export_all::<ExtensionPermissionProjectionResponse>(dir);
            export_all::<ExtensionBundleProjectionResponse>(dir);
            export_all::<ExtensionRuntimeProjectionResponse>(dir);
            export_all::<ExtensionRuntimeInvokeActionRequest>(dir);
            export_all::<ExtensionRuntimeInvokeChannelRequest>(dir);
            export_all::<ExtensionRuntimeTraceResponse>(dir);
            export_all::<ExtensionRuntimeInvocationOutputResponse>(dir);
            export_all::<ExtensionRuntimeInvokeActionResponse>(dir);
            export_all::<ExtensionRuntimeInvokeChannelResponse>(dir);
            export_all::<UninstallExtensionInstallationResponse>(dir);
        },
    );

    // --- workspace-module-contracts.ts ---
    emit_domain(
        &generated_dir,
        "workspace-module-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<WorkspaceModuleKind>(dir);
            export_all::<WorkspaceModuleStatusKind>(dir);
            export_all::<WorkspaceModuleStatus>(dir);
            export_all::<WorkspaceModuleSummary>(dir);
            export_all::<WorkspaceModuleUiEntry>(dir);
            export_all::<WorkspaceModuleCanvasHostAction>(dir);
            export_all::<WorkspaceModuleOperationDispatch>(dir);
            export_all::<WorkspaceModuleOperationReadinessKind>(dir);
            export_all::<WorkspaceModuleOperationReadiness>(dir);
            export_all::<WorkspaceModuleOperation>(dir);
            export_all::<WorkspaceModuleDescriptor>(dir);
            export_all::<WorkspaceModulePresentRequest>(dir);
            export_all::<WorkspaceModulePresentation>(dir);
        },
    );

    // --- extension-management-contracts.ts ---
    emit_domain(
        &generated_dir,
        "extension-management-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<ProjectExtensionPackageModeResponse>(dir);
            export_all::<ProjectExtensionInstalledSourceResponse>(dir);
            export_all::<ProjectExtensionPackageArtifactRefResponse>(dir);
            export_all::<ProjectExtensionCapabilitySummaryResponse>(dir);
            export_all::<ProjectExtensionManagementItemResponse>(dir);
            export_all::<ProjectExtensionManagementListResponse>(dir);
        },
    );

    // --- extension-package-contracts.ts ---
    emit_domain(
        &generated_dir,
        "extension-package-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<ExtensionPackageArtifactResponse>(dir);
            export_all::<InstallExtensionPackageArtifactRequest>(dir);
            export_all::<ExtensionPackageInstallationResponse>(dir);
            export_all::<ImportExtensionPackageResponse>(dir);
        },
    );

    // --- external-marketplace-contracts.ts ---
    emit_domain(
        &generated_dir,
        "external-marketplace-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<MarketplaceSourceProviderKindDto>(dir);
            export_all::<MarketplaceSourceTrustLevelDto>(dir);
            export_all::<MarketplaceSourceDto>(dir);
            export_all::<ListExternalMarketplaceAssetsQuery>(dir);
            export_all::<MarketplaceInstallRequirementKindDto>(dir);
            export_all::<ExternalMarketplaceInstallRequirementDto>(dir);
            export_all::<ExternalMarketplaceAssetListingDto>(dir);
            export_all::<ExternalMarketplaceAssetPageDto>(dir);
            export_all::<ExternalMarketplaceAssetDetailDto>(dir);
            export_all::<ImportExternalMarketplaceAssetRequest>(dir);
            export_all::<ImportExternalMarketplaceAssetResponse>(dir);
            export_all::<RefreshExternalMarketplaceAssetRequest>(dir);
            export_all::<ExternalMarketplaceRefreshStatus>(dir);
            export_all::<RefreshExternalMarketplaceAssetResponse>(dir);
        },
    );

    // --- settings-contracts.ts ---
    emit_domain(
        &generated_dir,
        "settings-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<SettingsScopeKind>(dir);
            export_all::<SettingsScopeQuery>(dir);
            export_all::<SettingResponse>(dir);
            export_all::<SettingUpdate>(dir);
            export_all::<UpdateSettingsRequest>(dir);
            export_all::<UpdateSettingsResponse>(dir);
        },
    );

    write_ndjson_stream_validators(
        &generated_dir.join(NDJSON_STREAM_VALIDATORS_FILENAME),
        check,
    );
}

/// Emit a single domain file, register its exported types into the upstream registry.
fn emit_domain(
    dir: &Path,
    filename: &str,
    upstream: &mut BTreeMap<String, String>,
    check: bool,
    export: impl FnOnce(&Path),
) {
    emit_domain_with_footer(dir, filename, upstream, check, None, export);
}

fn emit_domain_with_footer(
    dir: &Path,
    filename: &str,
    upstream: &mut BTreeMap<String, String>,
    check: bool,
    footer: Option<&str>,
    export: impl FnOnce(&Path),
) {
    let types = write_domain_dedup(&dir.join(filename), upstream, check, footer, export);
    let source = format!("./{}", filename.strip_suffix(".ts").unwrap());
    for name in types {
        upstream.insert(name, source.clone());
    }
}

fn workflow_contracts_footer() -> String {
    let trigger_values = WorkflowHookTrigger::ALL
        .iter()
        .map(|trigger| format!("  \"{}\",", trigger.wire_value()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"export const WORKFLOW_HOOK_TRIGGERS = [
{trigger_values}
] as const satisfies ReadonlyArray<WorkflowHookTrigger>;
"#
    )
}

/// Write a domain file, stripping types already claimed by upstream domains
/// and replacing them with `import type` lines. Returns the set of type names
/// that this domain actually exported (i.e. NOT stripped).
fn write_domain_dedup(
    out: &Path,
    upstream: &BTreeMap<String, String>,
    check: bool,
    footer: Option<&str>,
    export: impl FnOnce(&Path),
) -> BTreeSet<String> {
    fs::create_dir_all(out.parent().expect("generated dir")).expect("create generated dir");

    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    export(tmp_dir.path());

    let mut declarations = BTreeMap::new();
    collect_ts_files(tmp_dir.path(), &mut declarations);
    let mut rendered = render_domain_file(filename_from_path(out), declarations, upstream);
    if let Some(footer) = footer {
        rendered.contents.push('\n');
        rendered.contents.push_str(footer.trim_end());
        rendered.contents.push('\n');
    }
    let written = rendered.exported_types.clone();
    check_or_write_rendered(out, &rendered, check);
    if !check {
        eprintln!("Wrote {} ({} types)", out.display(), written.len());
    }

    written
}

fn write_common_json_value(out: &std::path::Path, check: bool) {
    fs::create_dir_all(out.parent().expect("generated dir")).expect("create generated dir");
    let rendered = render_common_json_value();
    check_or_write_rendered(out, &rendered, check);

    if !check {
        eprintln!("Wrote {}", out.display());
    }
}

fn write_ndjson_stream_validators(out: &std::path::Path, check: bool) {
    fs::create_dir_all(out.parent().expect("generated dir")).expect("create generated dir");
    let rendered = render_ndjson_stream_validators();
    check_or_write_rendered(out, &rendered, check);

    if !check {
        eprintln!("Wrote {}", out.display());
    }
}

fn check_or_write_rendered(out: &std::path::Path, rendered: &GeneratedTsFile, check: bool) {
    if check {
        match fs::read_to_string(out) {
            Ok(existing) if existing == rendered.contents => {
                eprintln!("{} is up to date", out.display());
                return;
            }
            Ok(_) => {
                eprintln!(
                    "{} is out of date; run `cargo run -p agentdash-contracts --bin generate_contracts_ts`",
                    out.display()
                );
                std::process::exit(1);
            }
            Err(error) => {
                eprintln!("failed to read {}: {error}", out.display());
                std::process::exit(1);
            }
        }
    }

    fs::write(out, &rendered.contents).expect("write generated TS");
}

fn export_all<T: TS + 'static>(dir: &std::path::Path) {
    T::export_all_to(dir).expect("export TS type");
}

fn filename_from_path(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .expect("generated file name")
}

fn collect_ts_files(dir: &std::path::Path, out: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("read entry");
        let path = entry.path();
        if path.is_dir() {
            collect_ts_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "ts") {
            let content = fs::read_to_string(&path).expect("read ts file");
            let stem = path
                .file_stem()
                .expect("file stem")
                .to_string_lossy()
                .to_string();

            let mut decl_lines = Vec::new();
            for line in content.lines() {
                if line.starts_with("// ") || line.starts_with("import ") {
                    continue;
                }
                if line.is_empty() && decl_lines.is_empty() {
                    continue;
                }
                decl_lines.push(line.trim_end().to_string());
            }

            while decl_lines.last().is_some_and(|l| l.is_empty()) {
                decl_lines.pop();
            }

            if !decl_lines.is_empty() {
                out.insert(stem, decl_lines.join("\n"));
            }
        }
    }
}
