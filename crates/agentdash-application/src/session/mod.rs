pub mod assembler;
pub mod augmenter;
pub mod baseline_capabilities;
pub mod bootstrap;
pub mod capability_state;
pub mod companion_wait;
pub mod context;
pub mod continuation;
pub mod hook_delegate;
pub mod hook_events;
mod hook_messages;
pub mod hook_runtime;
pub mod hub;
mod hub_support;
pub mod launch_intent;
mod memory_persistence;
pub(crate) mod path_policy;
pub mod persistence;
pub mod plan;
pub mod post_turn_handler;
mod prompt_pipeline;
mod prompt_vfs;
pub mod stall_detector;
pub mod system_prompt_assembler;
pub mod terminal_cache;
pub mod title_generator;
mod tool_schema_notice;
pub mod turn_processor;
pub mod types;

pub use assembler::{
    AgentLevelMcp, CompanionSpec, CompanionWorkflowOutput, CompanionWorkflowSpec,
    LifecycleNodeSpec, OwnerBootstrapSpec, OwnerPromptLifecycle, OwnerScope, PreparedSessionInputs,
    SessionAssemblyBuilder, SessionRequestAssembler, StoryStepPhase, StoryStepSpec,
    compose_companion, compose_companion_with_workflow, compose_lifecycle_node,
    compose_lifecycle_node_with_audit, extract_agent_mcp_entries, finalize_request,
    load_available_presets,
};
pub use augmenter::{PromptRequestAugmenter, SharedPromptRequestAugmenter};
pub use capability_state::{
    CapabilityStateDelta, NamedEntityDelta, RuntimeContextTransition, SetDelta, VfsSurfaceDelta,
    compose_vfs_with_overlay_and_directives, compute_capability_state_delta, merge_vfs_overlay,
};
pub use context::ExecutorResolution;
pub use hook_delegate::HookRuntimeDelegate;
pub use hook_events::build_hook_trace_envelope;
pub use hook_runtime::HookSessionRuntime;
pub use hub::SessionHub;
pub use hub_support::TurnTerminalKind;
pub use launch_intent::{
    SessionLaunchIntent, SessionLaunchPreparation, SessionLaunchSource, SessionLaunchStrictness,
};
pub use memory_persistence::MemorySessionPersistence;
pub use persistence::{
    PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionPersistence,
};
pub use post_turn_handler::{DynSessionTerminalCallback, PostTurnHandler, SessionTerminalCallback};
pub use prompt_vfs::local_workspace_vfs;
pub use title_generator::SessionTitleGenerator;
pub use turn_processor::{SessionTurnProcessor, SessionTurnProcessorConfig, TurnEvent};
pub use types::{
    CapabilityState, CompanionSessionContext, ExecutionStatus, HookSnapshotReloadTrigger,
    PendingCapabilityStateTransition, PromptSessionRequest, ResolvedPromptPayload,
    SessionBootstrapState, SessionExecutionState, SessionMeta, SessionPromptLifecycle,
    SessionRepositoryRehydrateMode, TitleSource, UserPromptInput, resolve_session_prompt_lifecycle,
};
