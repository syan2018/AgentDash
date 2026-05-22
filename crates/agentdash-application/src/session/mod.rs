pub mod assembler;
mod assignment_context_frame;
mod auto_resume_context_frame;
pub mod baseline_capabilities;
pub mod bootstrap;
pub mod capability_projection;
pub mod capability_service;
pub mod capability_state;
mod compaction_context_frame;
pub mod companion_wait;
pub mod construction;
pub mod construction_planner;
pub mod construction_provider;
pub mod context;
mod context_frame;
pub mod continuation;
pub mod control;
pub mod core;
pub(crate) mod dimension;
pub mod effects_service;
pub mod eventing;
pub mod hook_delegate;
pub mod hook_events;
mod hook_messages;
pub mod hook_runtime;
pub mod hooks_service;
pub(crate) mod hub;
mod hub_support;
mod identity_context_frame;
pub mod launch;
mod launch_planner;
pub mod launch_service;
mod memory_persistence;
pub mod ownership;
pub(crate) mod path_policy;
mod pending_action_context_frame;
pub mod persistence;
pub mod plan;
pub mod post_turn_handler;
mod prompt_pipeline;
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
pub mod turn_processor;
mod turn_supervisor;
pub mod types;

pub use assembler::{
    AgentLevelMcp, CompanionParentSpec, CompanionParentWorkflowSpec, CompanionSpec,
    CompanionWorkflowSpec, LifecycleNodeSpec, OwnerBootstrapSpec, OwnerPromptLifecycle, OwnerScope,
    SessionRequestAssembler, StoryStepPhase, StoryStepSpec, compose_companion_prompt,
    compose_companion_with_workflow_prompt, compose_lifecycle_node_prompt,
    compose_lifecycle_node_prompt_with_audit, extract_agent_mcp_entries, load_available_presets,
};
pub use capability_projection::{
    SessionCapabilityProjection, SessionCapabilityProjectionInput,
    derive_session_capability_projection, derive_session_guidelines, derive_session_skill_baseline,
    merge_live_vfs_skill_entries, normalize_capability_state_dimensions,
};
pub use capability_service::SessionCapabilityService;
pub use capability_state::{
    CapabilityDimensionModule, CapabilityDimensionRegistry, CapabilityStateDelta,
    CompanionCapabilityDimensionModule, McpCapabilityDimensionModule, NamedEntityDelta,
    RuntimeCapabilityProjectionContext, RuntimeCapabilityReplay, RuntimeCapabilityReplayContext,
    RuntimeContextTransition, SetDelta, ToolCapabilityDimensionModule,
    VfsCapabilityDimensionModule, VfsSurfaceDelta, apply_runtime_capability_transition,
    compose_vfs_with_overlay_and_directives, compute_capability_state_delta, merge_vfs_overlay,
    replay_runtime_capability_transition, replay_runtime_capability_transitions,
};
pub use construction_provider::{
    CompanionLaunchSource, CompanionLaunchWorkflowSource, SessionConstructionProvider,
    SessionConstructionProviderInput, SharedSessionConstructionProvider, TaskLaunchPhase,
    TaskLaunchSource,
};
pub use context::ExecutorResolution;
pub use control::SessionControlService;
pub use core::SessionCoreService;
pub use effects_service::SessionEffectsService;
pub use eventing::SessionEventingService;
pub use hook_delegate::HookRuntimeDelegate;
pub use hook_events::build_hook_trace_envelope;
pub use hook_runtime::HookSessionRuntime;
pub use hooks_service::SessionHookService;
pub use hub_support::TurnTerminalKind;
pub use launch::{LaunchCommand, LaunchCommandOutcome, LaunchSource, LaunchStrictness};
pub use launch_service::SessionLaunchService;
pub use memory_persistence::MemorySessionPersistence;
pub use persistence::{
    PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionPersistence,
};
pub use post_turn_handler::{
    DynPostTurnHandler, DynSessionTerminalCallback, DynTerminalHookEffectHandlerRegistry,
    PostTurnHandler, SessionTerminalCallback, TerminalHookEffectBinding,
    TerminalHookEffectHandlerRegistry,
};
pub use prompt_vfs::local_workspace_vfs;
pub use runtime_builder::SessionRuntimeBuilder;
pub use runtime_commands::{RuntimeCommandRecord, RuntimeCommandStatus};
pub use runtime_control::SessionRuntimeService;
pub use runtime_services::SessionRuntimeServices;
pub use terminal_effects::{
    NewTerminalEffectRecord, TerminalEffectRecord, TerminalEffectStatus, TerminalEffectType,
};
pub use title_generator::SessionTitleGenerator;
pub use title_service::SessionTitleService;
pub use turn_processor::{SessionTurnProcessor, SessionTurnProcessorConfig, TurnEvent};
pub use types::{
    ApplyMountOperationsEffect, ApplyVfsOverlayEffect, CapabilityArtifactSource,
    CapabilityContributionRecord, CapabilityDeclarationRecord, CapabilityDimensionKey,
    CapabilityState, CompanionSessionContext, DECLARATION_TYPE_CAPABILITY_DIRECTIVE,
    DECLARATION_TYPE_MOUNT_OPERATION, EFFECT_TYPE_APPLY_MOUNT_OPERATIONS,
    EFFECT_TYPE_APPLY_VFS_OVERLAY, EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER,
    EFFECT_TYPE_SET_MCP_SERVER_SET, EFFECT_TYPE_SET_TOOL_ACCESS, ExecutionStatus,
    HookSnapshotReloadTrigger, PendingCapabilityStateTransition, ResolvedPromptPayload,
    RuntimeCapabilityEffectRecord, RuntimeCapabilityTransition, SessionBootstrapState,
    SessionExecutionState, SessionMeta, SessionPromptLifecycle, SessionRepositoryRehydrateMode,
    SetCompanionAgentRosterEffect, SetMcpServerSetEffect, SetToolAccessEffect, TitleSource,
    UserPromptInput, resolve_session_prompt_lifecycle,
};
