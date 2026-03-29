mod event_bridge;
pub mod hook_delegate;
pub mod hook_events;
pub mod hook_runtime;
pub mod hub;
mod hub_support;
mod prompt_pipeline;
mod session_store;
pub mod types;

pub use hook_delegate::HookRuntimeDelegate;
pub use hook_events::build_hook_trace_notification;
pub use hook_runtime::HookSessionRuntime;
pub use hub::SessionHub;
pub use types::{
    CompanionSessionContext, PromptSessionRequest, ResolvedPromptPayload, SessionExecutionState,
    SessionMeta, UserPromptInput,
};
