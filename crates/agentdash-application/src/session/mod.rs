pub mod baseline_capabilities;
pub mod bootstrap;
pub mod companion_wait;
pub mod context;
mod continuation;
mod event_bridge;
pub mod hook_delegate;
pub mod hook_events;
mod hook_messages;
pub mod hook_runtime;
pub mod hub;
mod hub_support;
mod memory_persistence;
pub mod persistence;
pub mod plan;
pub mod post_turn_handler;
mod prompt_vfs;
mod prompt_pipeline;
pub mod stall_detector;
pub mod title_generator;
pub mod turn_processor;
pub mod types;

pub use hook_delegate::HookRuntimeDelegate;
pub use hook_events::build_hook_trace_notification;
pub use hook_runtime::HookSessionRuntime;
pub use hub::SessionHub;
pub use hub_support::TurnTerminalKind;
pub use memory_persistence::MemorySessionPersistence;
pub use persistence::{
    PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionPersistence,
};
pub use post_turn_handler::{DynSessionTerminalCallback, PostTurnHandler, SessionTerminalCallback};
pub use prompt_vfs::local_workspace_vfs;
pub use title_generator::SessionTitleGenerator;
pub use turn_processor::{SessionTurnProcessor, SessionTurnProcessorConfig, TurnEvent};
pub use types::{
    CompanionSessionContext, PromptSessionRequest, ResolvedPromptPayload, SessionBootstrapAction,
    SessionBootstrapState, SessionExecutionState, SessionMeta, SessionPromptLifecycle,
    SessionRepositoryRehydrateMode, TitleSource, UserPromptInput, resolve_session_prompt_lifecycle,
};
