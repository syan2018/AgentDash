mod event_bridge;
pub mod hook_delegate;
pub mod hook_events;
mod hook_messages;
pub mod hook_runtime;
pub mod hub;
mod hub_support;
mod memory_persistence;
pub mod persistence;
pub mod post_turn_handler;
mod prompt_pipeline;
pub mod stall_detector;
pub mod turn_processor;
pub mod types;

pub use hook_delegate::HookRuntimeDelegate;
pub use hook_events::build_hook_trace_notification;
pub use hook_runtime::HookSessionRuntime;
pub use hub::SessionHub;
pub use hub_support::TurnTerminalKind;
pub use memory_persistence::MemorySessionPersistence;
pub use turn_processor::{SessionTurnProcessor, SessionTurnProcessorConfig, TurnEvent};
pub use persistence::{
    PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionPersistence,
};
pub use post_turn_handler::PostTurnHandler;
pub use types::{
    CompanionSessionContext, PromptSessionRequest, ResolvedPromptPayload, SessionBootstrapAction,
    SessionBootstrapState, SessionExecutionState, SessionMeta, SessionPromptLifecycle,
    SessionRepositoryRehydrateMode, UserPromptInput, resolve_session_prompt_lifecycle,
};
