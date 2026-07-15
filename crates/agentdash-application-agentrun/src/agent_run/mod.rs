mod business_frame_surface_query;
mod context_projection;
mod context_sources;
mod control_effects;
mod conversation_snapshot;
mod delete_command;
mod delivery_runtime_selection;
mod execution_state;
mod fork_command;
pub mod frame;
mod journal;
pub(crate) mod lifecycle_read_model_facade;
mod presentation_read_model;
mod product_command;
mod project_agent_context;
mod runtime_application_presentation;
pub mod runtime_capability;
pub mod runtime_capability_projection;
pub mod runtime_facade;
pub mod runtime_mailbox;
pub mod runtime_session_boundary;
mod runtime_surface_update;
pub mod workspace;

pub use context_projection::AgentRunContextCompactionArchive;
pub use context_sources::{
    AgentBusinessSurfaceContextDeps, AgentBusinessSurfaceSource, AgentContextProjectionIdentity,
    AgentContextSurfaceSourceError, AgentContextSurfaceSourceFacts, BaseIdentitySource,
    LoadedAgentBusinessSurfaceFacts, project_tool_protocol, resolve_tool_capability,
};
pub use control_effects::{AgentRunControlEffectDeps, AgentRunControlEffectService};
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
pub use delete_command::{
    AgentRunDeleteCommand, AgentRunDeleteCommandService, AgentRunDeleteOutcome,
};
pub use delivery_runtime_selection::{
    DeliveryRuntimeSelection, DeliveryRuntimeSelectionError, DeliveryRuntimeSelectionPolicy,
    DeliveryRuntimeSelectionRepositories, DeliveryRuntimeSelectionService,
};
pub use runtime_application_presentation::AgentRunRuntimeApplicationPresentationProjector;
pub use runtime_facade::{
    AgentRunCommandGuard, AgentRunPresentationDraft, AgentRunPresentationInput, AgentRunRuntime,
    AgentRunRuntimeError, AgentRunRuntimeRecoverySummary, AgentRunRuntimeView,
    AppendAgentRunPresentation, ForkAgentRunRuntime, GuardedAgentRunCommand,
    LaunchPresentationSource, ManagedAgentRunRuntime, ReadAgentRunEvents,
    ResolveAgentRunInteraction, SendAgentRunMessage, SteerAgentRunTurn,
};
pub use runtime_mailbox::{
    AgentRunProductDelivery, AgentRunProductDeliveryPort, DeliverAgentRunProductInput,
    EnqueueRuntimeMailboxMessage, RuntimeAgentRunMailbox, RuntimeMailboxError,
    RuntimeMailboxSubmitOutcome, RuntimeMailboxTerminalConvergence,
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

pub use agentdash_application_ports::agent_run_fork::{AgentRunForkGraph, AgentRunForkGraphStore};
pub use agentdash_application_ports::agent_run_surface::{
    AgentRunEffectiveCapabilityView, AgentRunRuntimeSurface, AgentRunRuntimeSurfaceClosure,
    AgentRunRuntimeSurfaceProvenance, AgentRunRuntimeSurfaceQueryError,
    AgentRunRuntimeSurfaceQueryPort, AgentRunRuntimeSurfaceWithBackend, RuntimeSurfaceQueryPurpose,
};
pub use business_frame_surface_query::{
    BusinessFrameSurfaceQuery, BusinessFrameSurfaceQueryDeps, BusinessResourceSurfaceQuery,
    BusinessResourceSurfaceQueryDeps,
};
pub use execution_state::AgentRunExecutionState;
pub use fork_command::{AgentRunForkCommandService, AgentRunForkRuntimePort};
pub use frame::{
    AGENT_FRAME_WRITE_BOUNDARIES, AgentContextSourceFragment, AgentContextSourceSnapshot,
    AgentFrameBuilder, AgentFrameHookRuntime, AgentFrameSurfaceExt, AgentFrameWriteBoundary,
    AgentFrameWritePrimitive, AgentFrameWriteRole, AgentRunFrameConstructionAdapter,
    AgentRunFrameSurfaceCommand, AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError,
    AgentRunFrameSurfaceService, AgentRunHookTargetRuntimeAdapter,
    AgentRunRuntimeSurfaceUpdateAdapter, AgentRunSurfaceProjectionContext,
    AgentRunSurfaceProjectionContextResolver, AgentRunSurfaceProjectionContextSource,
    CanvasVisibilityReason, FrameConstructionCommand, FrameContextBundleSummary,
    FrameLaunchEnvelope, FrameLaunchIntent, FrameRuntimeSurface, FrameSurfaceDraft,
    RejectingFrameConstructionAdapter, RuntimeSurfaceKind, RuntimeSurfaceUpdateRequest,
    TerminalHookEffectBinding, agent_frame_write_boundaries, hook_target_runtime_port,
};
pub use journal::{
    AgentRunJournalBindingResolver, AgentRunJournalEvent, AgentRunJournalLiveEvent,
    AgentRunJournalPage, AgentRunJournalQuery, AgentRunJournalSegmentRole, AgentRunJournalService,
    AgentRunJournalSource, AgentRunJournalSourceSubscription, AgentRunJournalStreamState,
    AgentRunJournalStreamSubscription, agent_run_journal_session_id,
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
pub use presentation_read_model::{
    AgentFrameRefReadModel, AgentFrameRuntimeReadModel, AgentRunPresentationReadModelError,
    AgentRunPresentationReadModelQuery, AgentRunPresentationReadModelQueryDeps,
    AgentRunPresentationReadModelQueryRepos, RuntimeSessionRefReadModel,
    RuntimeSessionTraceReadModel,
};
pub use product_command::{AgentRunProductCommandClaim, AgentRunProductCommandService};
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
pub use runtime_surface_update::{
    AgentRunRuntimeSurfaceUpdateDeps, AgentRunRuntimeSurfaceUpdateService,
};
pub use runtime_target::{AgentFrameHookRuntimeTarget, AgentFrameRuntimeTarget};
pub use terminal_registry::{
    AgentRunKey, AgentRunTerminalRegistry, TerminalOutputSnapshot, TerminalState,
};
