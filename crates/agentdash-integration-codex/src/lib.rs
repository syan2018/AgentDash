//! Codex App Server first-party Complete Agent adapter.
//!
//! Codex JSON-RPC DTOs terminate inside this crate. Public APIs expose only AgentDash-owned
//! Complete Agent contracts.

mod complete_agent;
mod process_transport;
mod registration;

pub use complete_agent::{
    CODEX_APP_SERVER_PROTOCOL_REVISION, CODEX_INITIAL_CONTEXT_RENDERER_VERSION,
    CodexAppServerObservation, CodexAppServerObservationPage, CodexAppServerTransport,
    CodexCompleteAgentConfig, CodexCompleteAgentTransportError,
};
pub use registration::CodexCompleteAgentRegistration;
