use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

use agentdash_agent_protocol::BackboneEnvelope;
use agentdash_contracts::canvas::{
    CanvasImportMapDto, CanvasRuntimeBindingDto, CanvasRuntimeBridgeSnapshotDto,
    CanvasRuntimeFileDto, CanvasRuntimeSnapshotDto, RuntimeActionDescriptorDto,
    RuntimeActionKindDto, RuntimeContextDto, RuntimeInvocationOutputDto,
    RuntimeInvocationResultDto, RuntimePolicyDto, RuntimeSurfaceDto, RuntimeTraceDto,
};
use agentdash_contracts::companion::{CompanionGateRespondRequest, CompanionGateRespondResponse};
use agentdash_contracts::core::{
    AgentPreset, Artifact, ArtifactType, BackendCapabilitiesResponse,
    BackendExecutorCapabilityResponse, BackendMcpServerCapabilityResponse, BackendResponse,
    BackendRuntimeHealthResponse, BackendShareScopeKind, BackendType, BackendVisibility,
    BackendWithStatusResponse, ContextContainerDefinition, ContextContainerFile,
    ContextContainerProvider, ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
    DeletedFlagResponse, DeletedIdResponse, DeletedProjectSubjectGrantResponse,
    PendingExecutionResponse, ProjectAccessSummaryResponse, ProjectConfig, ProjectDetailResponse,
    ProjectResponse, ProjectRole, ProjectSubjectGrantResponse, ProjectSubjectType,
    ProjectVisibility, RevokeProjectGrantResponse, RevokedIdResponse, RuntimeHealthStatus,
    SchedulingConfig, SessionComposition, SessionRequiredContextBlock, StoryContext, StoryPriority,
    StoryResponse, StoryStatus, StoryType, TaskDispatchPreference, TaskResponse, TaskStatus,
    UpdatedIdResponse, VfsCapabilityDto, WorkspaceBindingResponse, WorkspaceBindingStatus,
    WorkspaceIdentityKind, WorkspaceResolutionPolicy, WorkspaceResponse, WorkspaceStatus,
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
    CodexOAuthFlowStatusDto, CodexOAuthStatusResponse, CreateLlmProviderRequest,
    DeleteLlmProviderResponse, DeleteLlmProviderUserCredentialResponse,
    EffectiveLlmModelProfileDto, EffectiveLlmProviderDto, LlmCredentialModeDto,
    LlmCredentialSourceDto, LlmCredentialVerificationStatusDto, LlmProviderAdminDto,
    LlmProviderProtocol, ProbeLlmProviderModelDto, ProbeLlmProviderModelsRequest,
    ReorderLlmProvidersRequest, ReorderLlmProvidersResponse, StartCodexOAuthResponse,
    UpdateLlmProviderRequest, UpsertLlmProviderUserCredentialRequest,
};
use agentdash_contracts::mcp_preset::{
    CloneMcpPresetRequest, CreateMcpPresetRequest, DeleteMcpPresetResponse, ListMcpPresetQuery,
    McpPresetResponse, ProbeMcpPresetRequest, ProbeMcpPresetResponse, UpdateMcpPresetRequest,
};
use agentdash_contracts::permission::{
    ListPermissionGrantsQuery, PermissionGrantResponse, PermissionGrantScopeDto,
    PermissionGrantStatusDto,
};
use agentdash_contracts::project_agent::{
    CreateProjectAgentRequest, CreateProjectAgentRunRequest, ProjectAgent, ProjectAgentExecutor,
    ProjectAgentLaunchResult, ProjectAgentRunStartResult, ProjectAgentSummary,
    UpdateProjectAgentRequest,
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
    ActiveRuntimeNodeRefDto, ActivityDefinition, ActivityTransition, AgentConversationIdentity,
    AgentConversationLifecycleContext, AgentConversationSnapshot, AgentFrameRefDto,
    AgentFrameRuntimeView, AgentProcedureContract, AgentProcedureResponse, AgentRunAcceptedRefs,
    AgentRunCommandReceipt, AgentRunMessageRequest, AgentRunMessageResponse, AgentRunRefDto,
    AgentRunSteeringRequest, AgentRunSteeringResponse, AgentRunView,
    AgentRunWorkspaceActionAvailabilityView, AgentRunWorkspaceActionSetView,
    AgentRunWorkspaceControlPlaneStatus, AgentRunWorkspaceControlPlaneView,
    AgentRunWorkspaceListEntry, AgentRunWorkspaceListView, AgentRunWorkspaceShell,
    AgentRunWorkspaceView, ConversationCommandKind, ConversationCommandPlacement,
    ConversationCommandSetView, ConversationCommandStaleGuardView, ConversationCommandView,
    ConversationDiagnosticView, ConversationEffectiveExecutorConfigView,
    ConversationExecutionStatus, ConversationExecutionView, ConversationKeyboardMapView,
    ConversationModelConfigSource, ConversationModelConfigStatus, ConversationModelConfigView,
    ConversationPendingSnapshotView, DefinitionSource, DeleteAgentProcedureResponse,
    DeleteHookPresetResponse, DeleteWorkflowGraphResponse, EffectiveSessionContract,
    EnqueuePendingMessageRequest, EnqueuePendingMessageResponse, HookPresetResponse,
    HookPresetsResponse, LifecycleExecutionEntry, LifecycleRunRefDto, LifecycleRunStatus,
    LifecycleRunTopology, LifecycleRunView, LifecycleSubjectAssociationDto,
    OrchestrationInstanceView, PendingMessageView, PendingQueuePauseReasonDto,
    PendingQueueStateView, PreflightWorkflowScriptRequest, PreflightWorkflowScriptResponse,
    ProjectActiveAgentsView, RegisterHookPresetResponse, ResumePendingQueueResponse,
    RuntimeNodeView, RuntimeSessionCommandStateDto, RuntimeSessionExecutionAnchorDto,
    RuntimeSessionRefDto, RuntimeSessionTraceMeta, RuntimeSessionTraceView,
    SessionRuntimeActionAvailabilityView, SessionRuntimeActionSetView,
    SessionRuntimeControlPlaneStatus, SessionRuntimeControlPlaneView, SessionRuntimeControlView,
    SessionShellDto, SubjectExecutionView, SubjectRefDto, SubmitOrchestrationHumanDecisionRequest,
    SubmitOrchestrationHumanDecisionResponse, ValidateHookScriptResponse, ValidationIssue,
    WorkflowGraphResponse, WorkflowScriptApiEndpointDto, WorkflowScriptBashCommandDto,
    WorkflowScriptCapabilitySummaryDto, WorkflowScriptHumanGateCapabilityDto,
    WorkflowScriptPlanPreviewDto, WorkflowScriptPlanPreviewNodeDto,
    WorkflowScriptPreflightDiagnosticDto, WorkflowTargetKind,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch, WorkspaceModulePresentRequest,
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
            export_all::<ProjectAgentLaunchResult>(dir);
            export_all::<CreateProjectAgentRunRequest>(dir);
            export_all::<ProjectAgentRunStartResult>(dir);
            export_all::<CreateProjectAgentRequest>(dir);
            export_all::<UpdateProjectAgentRequest>(dir);
        },
    );

    // --- core-contracts.ts ---
    emit_domain(
        &generated_dir,
        "core-contracts.ts",
        &mut upstream,
        check,
        |dir| {
            export_all::<VfsCapabilityDto>(dir);
            export_all::<ContextContainerFile>(dir);
            export_all::<ContextContainerProvider>(dir);
            export_all::<ContextContainerDefinition>(dir);
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
            export_all::<WorkspaceIdentityKind>(dir);
            export_all::<WorkspaceBindingStatus>(dir);
            export_all::<WorkspaceResolutionPolicy>(dir);
            export_all::<WorkspaceStatus>(dir);
            export_all::<WorkspaceBindingResponse>(dir);
            export_all::<WorkspaceResponse>(dir);
            export_all::<ContextSourceKind>(dir);
            export_all::<ContextSlot>(dir);
            export_all::<ContextDelivery>(dir);
            export_all::<ContextSourceRef>(dir);
            export_all::<DeletedIdResponse>(dir);
            export_all::<DeletedFlagResponse>(dir);
            export_all::<UpdatedIdResponse>(dir);
            export_all::<RevokedIdResponse>(dir);
            export_all::<PendingExecutionResponse>(dir);
            export_all::<BackendType>(dir);
            export_all::<BackendVisibility>(dir);
            export_all::<BackendShareScopeKind>(dir);
            export_all::<RuntimeHealthStatus>(dir);
            export_all::<BackendRuntimeHealthResponse>(dir);
            export_all::<BackendExecutorCapabilityResponse>(dir);
            export_all::<BackendMcpServerCapabilityResponse>(dir);
            export_all::<BackendCapabilitiesResponse>(dir);
            export_all::<BackendResponse>(dir);
            export_all::<BackendWithStatusResponse>(dir);
            export_all::<SessionRequiredContextBlock>(dir);
            export_all::<SessionComposition>(dir);
            export_all::<StoryContext>(dir);
            export_all::<StoryStatus>(dir);
            export_all::<StoryPriority>(dir);
            export_all::<StoryType>(dir);
            export_all::<StoryResponse>(dir);
            export_all::<TaskStatus>(dir);
            export_all::<ArtifactType>(dir);
            export_all::<Artifact>(dir);
            export_all::<TaskDispatchPreference>(dir);
            export_all::<TaskResponse>(dir);
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
    emit_domain(
        &generated_dir,
        "workflow-contracts.ts",
        &mut upstream,
        check,
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
            export_all::<AgentRunMessageRequest>(dir);
            export_all::<AgentRunMessageResponse>(dir);
            export_all::<AgentRunSteeringRequest>(dir);
            export_all::<RuntimeSessionCommandStateDto>(dir);
            export_all::<AgentRunSteeringResponse>(dir);
            export_all::<AgentRunCommandReceipt>(dir);
            export_all::<AgentRunAcceptedRefs>(dir);
            export_all::<LifecycleSubjectAssociationDto>(dir);
            export_all::<RuntimeNodeView>(dir);
            export_all::<ActiveRuntimeNodeRefDto>(dir);
            export_all::<OrchestrationInstanceView>(dir);
            export_all::<LifecycleRunView>(dir);
            export_all::<SubmitOrchestrationHumanDecisionRequest>(dir);
            export_all::<SubmitOrchestrationHumanDecisionResponse>(dir);
            export_all::<AgentRunView>(dir);
            export_all::<AgentFrameRuntimeView>(dir);
            export_all::<RuntimeSessionTraceMeta>(dir);
            export_all::<AgentRunWorkspaceShell>(dir);
            export_all::<AgentRunWorkspaceControlPlaneStatus>(dir);
            export_all::<AgentRunWorkspaceControlPlaneView>(dir);
            export_all::<AgentRunWorkspaceActionAvailabilityView>(dir);
            export_all::<AgentRunWorkspaceActionSetView>(dir);
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
            export_all::<ConversationPendingSnapshotView>(dir);
            export_all::<ConversationDiagnosticView>(dir);
            export_all::<AgentConversationIdentity>(dir);
            export_all::<AgentConversationLifecycleContext>(dir);
            export_all::<AgentConversationSnapshot>(dir);
            export_all::<AgentRunWorkspaceView>(dir);
            export_all::<SubjectExecutionView>(dir);
            export_all::<ProjectActiveAgentsView>(dir);
            export_all::<RuntimeSessionTraceView>(dir);
            export_all::<SessionRuntimeControlPlaneStatus>(dir);
            export_all::<SessionRuntimeControlPlaneView>(dir);
            export_all::<SessionRuntimeActionAvailabilityView>(dir);
            export_all::<SessionRuntimeActionSetView>(dir);
            export_all::<SessionRuntimeControlView>(dir);
            export_all::<PendingMessageView>(dir);
            export_all::<PendingQueuePauseReasonDto>(dir);
            export_all::<PendingQueueStateView>(dir);
            export_all::<EnqueuePendingMessageRequest>(dir);
            export_all::<EnqueuePendingMessageResponse>(dir);
            export_all::<ResumePendingQueueResponse>(dir);
            export_all::<AgentRunWorkspaceListEntry>(dir);
            export_all::<AgentRunWorkspaceListView>(dir);
            export_all::<DefinitionSource>(dir);
            export_all::<WorkflowTargetKind>(dir);
            export_all::<DeleteWorkflowGraphResponse>(dir);
            export_all::<DeleteAgentProcedureResponse>(dir);
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
            export_all::<CanvasImportMapDto>(dir);
            export_all::<CanvasRuntimeFileDto>(dir);
            export_all::<CanvasRuntimeBindingDto>(dir);
            export_all::<RuntimeActionKindDto>(dir);
            export_all::<RuntimePolicyDto>(dir);
            export_all::<RuntimeActionDescriptorDto>(dir);
            export_all::<RuntimeContextDto>(dir);
            export_all::<RuntimeSurfaceDto>(dir);
            export_all::<CanvasRuntimeBridgeSnapshotDto>(dir);
            export_all::<CanvasRuntimeSnapshotDto>(dir);
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
}

/// Emit a single domain file, register its exported types into the upstream registry.
fn emit_domain(
    dir: &Path,
    filename: &str,
    upstream: &mut BTreeMap<String, String>,
    check: bool,
    export: impl FnOnce(&Path),
) {
    let types = write_domain_dedup(&dir.join(filename), upstream, check, export);
    let source = format!("./{}", filename.strip_suffix(".ts").unwrap());
    for name in types {
        upstream.insert(name, source.clone());
    }
}

/// Write a domain file, stripping types already claimed by upstream domains
/// and replacing them with `import type` lines. Returns the set of type names
/// that this domain actually exported (i.e. NOT stripped).
fn write_domain_dedup(
    out: &Path,
    upstream: &BTreeMap<String, String>,
    check: bool,
    export: impl FnOnce(&Path),
) -> BTreeSet<String> {
    fs::create_dir_all(out.parent().expect("generated dir")).expect("create generated dir");

    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    export(tmp_dir.path());

    let mut declarations = BTreeMap::new();
    collect_ts_files(tmp_dir.path(), &mut declarations);
    declarations.remove("JsonValue");

    // Strip types already defined upstream (remove from declarations).
    let mut stripped: Vec<(String, String)> = Vec::new();
    for (type_name, source) in upstream.iter() {
        if declarations.remove(type_name).is_some() {
            stripped.push((type_name.clone(), source.clone()));
        }
    }

    // Only import types that the *remaining* declarations actually reference.
    // This avoids importing transitive sub-types that were generated by ts_rs
    // but aren't directly used (e.g. TextElement inside UserInput).
    let remaining_text: String = declarations
        .values()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let mut dedup_imports: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (type_name, source) in &stripped {
        if text_references_type(&remaining_text, type_name) {
            dedup_imports
                .entry(source.clone())
                .or_default()
                .push(type_name.clone());
        }
    }
    // Also catch types NOT in declarations but referenced (cross-crate phantom deps)
    for (type_name, source) in upstream.iter() {
        if stripped.iter().any(|(n, _)| n == type_name) {
            continue;
        }
        if text_references_type(&remaining_text, type_name) {
            dedup_imports
                .entry(source.clone())
                .or_default()
                .push(type_name.clone());
        }
    }

    let mut lines = Vec::new();
    lines.push(
        "// This file is generated by `cargo run -p agentdash-contracts --bin generate_contracts_ts`."
            .to_string(),
    );
    lines.push("// Do not edit manually.".to_string());
    lines.push(String::new());

    let mut has_imports = false;
    if text_references_type(&remaining_text, "JsonValue") {
        lines.push("import type { JsonValue } from \"./common-contracts\";".to_string());
        has_imports = true;
    }
    for (source, names) in &dedup_imports {
        let joined = names.join(", ");
        lines.push(format!("import type {{ {joined} }} from \"{source}\";"));
        has_imports = true;
    }
    if has_imports {
        lines.push(String::new());
    }

    let written: BTreeSet<String> = declarations.keys().cloned().collect();

    for decl in declarations.values() {
        lines.push(decl.clone());
        lines.push(String::new());
    }

    let output = lines.join("\n");

    if check {
        match fs::read_to_string(out) {
            Ok(existing) if existing == output => {
                eprintln!("{} is up to date", out.display());
                return written;
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

    fs::write(out, output).expect("write generated TS");
    eprintln!("Wrote {} ({} types)", out.display(), written.len());

    written
}

fn write_common_json_value(out: &std::path::Path, check: bool) {
    fs::create_dir_all(out.parent().expect("generated dir")).expect("create generated dir");

    let output = [
        "// This file is generated by `cargo run -p agentdash-contracts --bin generate_contracts_ts`.",
        "// Do not edit manually.",
        "",
        "export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]?: JsonValue } | null;",
        "",
    ]
    .join("\n");

    if check {
        match fs::read_to_string(out) {
            Ok(existing) if existing == output => {
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

    fs::write(out, output).expect("write generated common TS");

    eprintln!("Wrote {}", out.display());
}

fn export_all<T: TS + 'static>(dir: &std::path::Path) {
    T::export_all_to(dir).expect("export TS type");
}

/// Word-boundary check: does `text` contain `name` as a standalone identifier?
fn text_references_type(text: &str, name: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| word == name)
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
