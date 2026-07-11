mod business_frame_surface_query;
mod conversation_snapshot;
mod execution_state;
pub mod frame;
pub(crate) mod lifecycle_read_model_facade;
pub mod message_delivery;
mod presentation_read_model;
mod project_agent_context;
pub mod runtime_capability;
pub mod runtime_capability_projection;
pub mod runtime_facade;
pub mod runtime_mailbox;
pub mod runtime_session_boundary;

pub use runtime_facade::{
    AgentRunCommandGuard, AgentRunRuntime, AgentRunRuntimeError, AgentRunRuntimeView,
    GuardedAgentRunCommand, ManagedAgentRunRuntime, ReadAgentRunEvents, ResolveAgentRunInteraction,
    SendAgentRunMessage, SteerAgentRunTurn,
};
pub use runtime_mailbox::{
    AgentRunProductDelivery, AgentRunProductDeliveryPort, DeliverAgentRunProductInput,
    EnqueueRuntimeMailboxMessage, RuntimeAgentRunMailbox, RuntimeMailboxError,
    RuntimeMailboxSubmitOutcome,
};

#[async_trait::async_trait]
pub trait ProjectAgentLifecycleLaunchPort: Send + Sync {
    async fn launch_project_agent(
        &self,
        intent: &agentdash_domain::workflow::AgentLaunchIntent,
    ) -> Result<
        agentdash_domain::workflow::AgentLaunchDispatchResult,
        crate::WorkflowApplicationError,
    >;
}
mod runtime_target;
pub mod terminal_registry;
pub mod workspace;

pub use agentdash_application_ports::agent_run_surface::{
    AgentRunEffectiveCapabilityView, AgentRunRuntimeSurface, AgentRunRuntimeSurfaceClosure,
    AgentRunRuntimeSurfaceProvenance, AgentRunRuntimeSurfaceQueryError,
    AgentRunRuntimeSurfaceQueryPort, AgentRunRuntimeSurfaceWithBackend, RuntimeSurfaceQueryPurpose,
};
pub use business_frame_surface_query::{
    BusinessFrameSurfaceQuery, BusinessFrameSurfaceQueryDeps, BusinessResourceSurfaceQuery,
    BusinessResourceSurfaceQueryDeps,
};
pub use conversation_snapshot::{
    AgentConversationFrameRefModel, AgentConversationIdentityModel,
    AgentConversationLifecycleContextModel, AgentConversationSnapshotInput,
    AgentConversationSnapshotModel, AgentConversationSnapshotResolver,
    AgentRunCommandPreconditionModel, AgentRunOwnershipModel, ConversationCommandAvailability,
    ConversationCommandAvailabilityInput, ConversationCommandAvailabilityResolver,
    ConversationCommandKindModel, ConversationCommandModel, ConversationCommandPlacementModel,
    ConversationCommandSetModel, ConversationCommandStaleGuardModel, ConversationDiagnosticModel,
    ConversationEffectiveExecutorConfigModel, ConversationExecutionModel,
    ConversationExecutionStatusModel, ConversationKeyboardMapModel,
    ConversationMailboxSnapshotModel, ConversationModelConfigInput, ConversationModelConfigModel,
    ConversationModelConfigResolution, ConversationModelConfigResolver,
    ConversationModelConfigSourceModel, ConversationModelConfigStatusModel,
    ConversationWaitingItemModel, ValidationSeverityModel, conversation_command_id_for,
    conversation_execution_state_code, conversation_snapshot_id, merge_executor_config_fields,
};
pub use execution_state::AgentRunExecutionState;
pub use frame::{
    AGENT_FRAME_WRITE_BOUNDARIES, AgentFrameBuilder, AgentFrameHookRuntime, AgentFrameSurfaceExt,
    AgentFrameWriteBoundary, AgentFrameWritePrimitive, AgentFrameWriteRole,
    AgentRunFrameConstructionAdapter, AgentRunFrameSurfaceCommand,
    AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError, AgentRunFrameSurfaceService,
    AgentRunHookTargetRuntimeAdapter, AgentRunRuntimeSurfaceUpdateAdapter,
    AgentRunSurfaceProjectionContext, AgentRunSurfaceProjectionContextResolver,
    AgentRunSurfaceProjectionContextSource, FrameConstructionCommand,
    FrameContextBundleSummary, FrameLaunchEnvelope, FrameLaunchIntent, FrameRuntimeSurface,
    FrameSurfaceDraft, RejectingFrameConstructionAdapter, RuntimeSurfaceKind,
    RuntimeSurfaceUpdateRequest, TerminalHookEffectBinding, agent_frame_write_boundaries,
    hook_target_runtime_port,
};
pub use lifecycle_read_model_facade::{
    ActiveRuntimeNodeRefView as PresentationActiveRuntimeNodeRefView,
    AgentRunRefView as PresentationAgentRunRefView, AgentRunView as PresentationAgentRunView,
    ExecutorRunRefView as PresentationExecutorRunRefView,
    LifecycleExecutionEntryView as PresentationLifecycleExecutionEntryView,
    LifecycleExecutionEventKindView as PresentationLifecycleExecutionEventKindView,
    LifecycleRunRefView as PresentationLifecycleRunRefView,
    LifecycleRunStatusView as PresentationLifecycleRunStatusView,
    LifecycleRunTopologyView as PresentationLifecycleRunTopologyView,
    LifecycleRunView as PresentationLifecycleRunView,
    LifecycleSubjectAssociationView as PresentationLifecycleSubjectAssociationView,
    OrchestrationInstanceView as PresentationOrchestrationInstanceView,
    RuntimeNodeView as PresentationRuntimeNodeView,
    RuntimeSessionRefView as PresentationRuntimeSessionRefView,
    SubjectRefView as PresentationSubjectRefView,
};
pub use message_delivery::{
    AgentRunMessageDelivery, AgentRunMessageDeliveryPort, SessionTurnMessageDeliveryPort,
};
pub use presentation_read_model::{
    AgentFrameRefReadModel, AgentFrameRuntimeReadModel, AgentRunPresentationReadModelError,
    AgentRunPresentationReadModelQuery, AgentRunPresentationReadModelQueryDeps,
    AgentRunPresentationReadModelQueryRepos, RuntimeSessionRefReadModel,
    RuntimeSessionTraceReadModel,
};
pub use project_agent_context::{
    PROJECT_AGENT_BINDING_LABEL_PREFIX, ResolvedProjectAgentContext, build_project_agent_context,
    resolve_project_workspace,
};
pub use runtime_capability::{
    CapabilityDimensionModule, CapabilityDimensionRegistry, CapabilityStateDelta,
    CompanionCapabilityDimensionModule, FrameCapabilitySurfaces, McpCapabilityDimensionModule,
    NamedEntityDelta, RuntimeCapabilityProjectionContext, RuntimeCapabilityReplay,
    RuntimeCapabilityReplayContext, RuntimeContextTransition, SetDelta,
    ToolCapabilityDimensionModule, VfsCapabilityDimensionModule, VfsSurfaceDelta,
    apply_runtime_capability_transition, capability_state_to_frame_surfaces,
    compose_vfs_with_overlay_and_directives, compute_capability_state_delta, merge_vfs_overlay,
    project_capability_state_from_frame, replay_runtime_capability_transition,
    replay_runtime_capability_transitions,
};
pub use runtime_capability_projection::{
    LaunchContextDiscoveryInput, LaunchContextDiscoveryOutput, RuntimeCapabilityProjection,
    RuntimeCapabilityProjectionInput, derive_launch_context_discovery,
    derive_runtime_capability_projection, derive_runtime_guidelines, derive_runtime_skill_baseline,
    merge_live_vfs_skill_entries, normalize_capability_state_dimensions,
};
pub use runtime_session_boundary::{
    PromptLaunchPath, RuntimeCommandRecord, RuntimeSessionControlPort, RuntimeSessionCorePort,
    RuntimeSessionEventSubscription, RuntimeSessionEventingPort, RuntimeSessionLaunchPort,
    RuntimeTraceLaunchState, SessionControlService, SessionCoreService, SessionEventPage,
    SessionEventingService, SessionExecutionState, SessionLaunchService, SessionMeta,
    SessionRepositoryRehydrateMode, SessionTurnSteerCommand, resolve_prompt_launch_path,
};
pub use runtime_target::{AgentFrameHookRuntimeTarget, AgentFrameRuntimeTarget};
pub use terminal_registry::{
    AgentRunKey, AgentRunTerminalRegistry, TerminalOutputSnapshot, TerminalState,
};
