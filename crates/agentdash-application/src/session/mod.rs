pub mod assembler;
pub(crate) mod assembly_builder;
mod assignment_context_frame;
mod auto_resume_context_frame;
pub mod baseline_capabilities;
pub mod bootstrap;
mod branching;
pub mod capability_projection;
pub mod capability_service;
pub mod capability_state;
mod compaction_checkpoint;
mod compaction_context_frame;
#[cfg(test)]
pub(crate) mod construction;
pub mod construction_planner;
pub mod construction_provider;
pub mod context;
mod context_frame;
mod context_projector;
mod context_usage_marking;
// context_query_use_case 已删除：所有 API 消费者已迁移至 frame-based read model
pub mod continuation;
pub mod control;
pub mod core;
pub(crate) mod dimension;
pub mod effects_service;
pub mod eventing;
mod guidelines_context_frame;
pub mod hook_delegate;
pub mod hook_events;
pub(crate) mod hook_injection_sink;
mod hook_messages;
pub mod hooks_service;
pub(crate) mod hub;
mod hub_support;
mod identity_context_frame;
pub mod launch;
pub(crate) mod mailbox_delegate;
#[cfg(test)]
#[path = "../../test-support/session_memory_persistence.rs"]
mod memory_persistence;
pub(crate) mod path_policy;
mod pending_action_context_frame;
pub mod persistence;
pub mod plan;
pub mod post_turn_handler;
mod prompt_vfs;
pub mod runtime_builder;
pub mod runtime_commands;
pub mod runtime_control;
mod runtime_registry;
pub mod runtime_services;
pub mod stall_detector;
pub mod terminal_cache;
pub mod terminal_effects;
pub mod title_generator;
pub mod title_service;
pub(crate) mod tool_assembly;
pub mod turn_processor;
mod turn_supervisor;
pub mod types;

pub use crate::agent_run::frame::hook_runtime::AgentFrameHookRuntime;
pub use crate::lifecycle::WorkflowApplicationError;
pub use assembler::{
    CompanionParentSpec, CompanionParentWorkflowSpec, CompanionSpec, CompanionWorkflowSpec,
    LifecycleNodeSpec, SessionRequestAssembler, compose_lifecycle_node_to_frame_with_audit,
};
pub use assembly_builder::AssemblyLaunchExtras;
pub use branching::{
    SessionBranchingService, SessionForkRequest, SessionForkResult, SessionLineageView,
    SessionProjectionRollbackRequest, SessionProjectionRollbackResult,
};
pub use capability_projection::{
    SessionCapabilityProjection, SessionCapabilityProjectionInput,
    derive_session_capability_projection, derive_session_guidelines, derive_session_skill_baseline,
    merge_live_vfs_skill_entries, normalize_capability_state_dimensions,
};
pub use capability_service::SessionCapabilityService;
pub use capability_state::{
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
pub use construction_provider::{
    CompanionLaunchSource, CompanionLaunchWorkflowSource, RoutineLaunchSource,
    SessionConstructionProvider, SessionConstructionProviderInput,
    SharedSessionConstructionProvider,
};
pub use context::ExecutorResolution;
pub use context_projector::ContextProjector;
pub use control::{SessionControlService, SessionTurnSteerCommand};
pub use core::SessionCoreService;
pub use effects_service::SessionEffectsService;
pub use eventing::SessionEventingService;
pub use hook_delegate::HookRuntimeDelegate;
pub use hook_events::build_hook_trace_envelope;
pub use hooks_service::SessionHookService;
pub use hub_support::TurnTerminalKind;
pub use launch::{LaunchCommand, LaunchCommandOutcome, LaunchSource, SessionLaunchService};
#[cfg(test)]
pub use memory_persistence::MemorySessionPersistence;
pub use persistence::{
    PersistedSessionEvent, SessionCompactionStore, SessionEventBacklog, SessionEventPage,
    SessionEventStore, SessionLineageRecord, SessionLineageRelationKind, SessionLineageStatus,
    SessionLineageStore, SessionMetaStore, SessionPersistence, SessionProjectionStore,
    SessionRuntimeCommandStore, SessionTerminalEffectStore,
};
pub use post_turn_handler::{
    DynPostTurnHandler, DynSessionTerminalCallback, DynTerminalHookEffectHandlerRegistry,
    EmptyTerminalHookEffectHandlerRegistry, PostTurnHandler, SessionTerminalCallback,
    TerminalHookEffectBinding, TerminalHookEffectHandlerRegistry,
};
pub use prompt_vfs::local_workspace_vfs;
pub use runtime_builder::SessionRuntimeBuilder;
pub use runtime_commands::{
    AgentFrameTransitionRecord, RuntimeCommandRecord, RuntimeCommandStatus, RuntimeDeliveryCommand,
    RuntimeDeliveryCommandKind,
};
pub use runtime_control::SessionRuntimeService;
pub use runtime_services::SessionRuntimeServices;
pub use terminal_effects::{
    NewTerminalEffectRecord, TerminalEffectRecord, TerminalEffectStatus, TerminalEffectType,
};
pub use title_service::SessionTitleService;
pub use turn_processor::{SessionTurnProcessor, SessionTurnProcessorConfig, TurnEvent};
pub use types::{
    AgentFrameHookRuntimeTarget, AgentFrameRuntimeTarget, ApplyMountOperationsEffect,
    ApplyVfsOverlayEffect, CapabilityArtifactSource, CapabilityContributionRecord,
    CapabilityDeclarationRecord, CapabilityDimensionKey, CapabilityState,
    DECLARATION_TYPE_CAPABILITY_DIRECTIVE, DECLARATION_TYPE_MOUNT_OPERATION,
    EFFECT_TYPE_APPLY_MOUNT_OPERATIONS, EFFECT_TYPE_APPLY_VFS_OVERLAY,
    EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER, EFFECT_TYPE_SET_MCP_SERVER_SET,
    EFFECT_TYPE_SET_TOOL_ACCESS, ExecutionStatus, HookSnapshotReloadTrigger,
    PendingCapabilityStateTransition, PromptLaunchPath, ResolvedPromptPayload,
    RuntimeCapabilityEffectRecord, RuntimeCapabilityTransition, RuntimeTraceLaunchState,
    SessionExecutionState, SessionMeta, SessionRepositoryRehydrateMode,
    SetCompanionAgentRosterEffect, SetMcpServerSetEffect, SetToolAccessEffect, TitleSource,
    UserPromptInput, resolve_prompt_launch_path,
};
