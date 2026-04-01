mod event_bridge;
pub mod hook_delegate;
pub mod hook_events;
mod hook_messages;
pub mod hook_runtime;
pub mod hub;
mod hub_support;
mod memory_persistence;
pub mod persistence;
mod prompt_pipeline;
pub mod types;

pub use hook_delegate::HookRuntimeDelegate;
pub use hook_events::build_hook_trace_notification;
pub use hook_runtime::HookSessionRuntime;
pub use hub::SessionHub;
pub use memory_persistence::MemorySessionPersistence;
pub use persistence::{
    PersistedSessionEvent, SessionEventBacklog, SessionEventPage, SessionPersistence,
};
pub use types::{
    CompanionSessionContext, PromptSessionRequest, ResolvedPromptPayload, SessionExecutionState,
    SessionMeta, UserPromptInput,
};
