pub mod assembler;
mod assignment_context_frame;
mod auto_resume_context_frame;
pub mod baseline_capabilities;
pub mod bootstrap;
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
pub mod eventing;
pub mod hook_delegate;
pub mod hook_events;
mod hook_messages;
pub mod hook_runtime;
pub mod hub;
mod hub_support;
mod identity_context_frame;
pub mod launch;
mod launch_planner;
mod memory_persistence;
pub mod ownership;
pub(crate) mod path_policy;
mod pending_action_context_frame;
pub mod persistence;
pub mod plan;
pub mod post_turn_handler;
mod prompt_pipeline;
mod prompt_vfs;
pub mod runtime_commands;
pub mod runtime_control;
mod runtime_registry;
pub mod stall_detector;
pub mod terminal_cache;
pub mod terminal_effects;
pub mod title_generator;
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
pub use capability_state::{
    CapabilityStateDelta, NamedEntityDelta, RuntimeContextTransition, SetDelta, VfsSurfaceDelta,
    compose_vfs_with_overlay_and_directives, compute_capability_state_delta, merge_vfs_overlay,
};
pub use construction_provider::{
    CompanionLaunchSource, CompanionLaunchWorkflowSource, SessionConstructionProvider,
    SharedSessionConstructionProvider, TaskLaunchPhase, TaskLaunchSource,
};
pub use context::ExecutorResolution;
pub use control::SessionControlService;
pub use core::SessionCoreService;
pub use eventing::SessionEventingService;
pub use hook_delegate::HookRuntimeDelegate;
pub use hook_events::build_hook_trace_envelope;
pub use hook_runtime::HookSessionRuntime;
pub use hub::SessionHub;
pub use hub_support::TurnTerminalKind;
pub use launch::{LaunchCommand, LaunchCommandOutcome, LaunchSource, LaunchStrictness};
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
pub use runtime_commands::{PendingRuntimeCommandRecord, RuntimeCommandStatus};
pub use runtime_control::SessionRuntimeService;
pub use terminal_effects::{
    NewTerminalEffectRecord, TerminalEffectRecord, TerminalEffectStatus, TerminalEffectType,
};
pub use title_generator::SessionTitleGenerator;
pub use turn_processor::{SessionTurnProcessor, SessionTurnProcessorConfig, TurnEvent};
pub use types::{
    CapabilityState, CompanionSessionContext, ExecutionStatus, HookSnapshotReloadTrigger,
    PendingCapabilityStateTransition, ResolvedPromptPayload, SessionBootstrapState,
    SessionExecutionState, SessionMeta, SessionPromptLifecycle, SessionRepositoryRehydrateMode,
    TitleSource, UserPromptInput, resolve_session_prompt_lifecycle,
};
