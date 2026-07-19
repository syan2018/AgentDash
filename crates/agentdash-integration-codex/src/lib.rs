//! Codex App Server first-party Complete Agent adapter.
//!
//! Codex JSON-RPC DTOs terminate inside this crate. Public APIs expose only AgentDash-owned
//! Complete Agent contracts.

mod complete_agent;
mod process_transport;
mod registration;
#[allow(dead_code, clippy::all)]
mod vendor_generated;

pub use complete_agent::{
    CODEX_APP_SERVER_PROTOCOL_REVISION, CODEX_CHILD_HISTORY_DIGEST_VERSION,
    CODEX_INITIAL_CONTEXT_RENDERER_VERSION, CodexAppServerObservation,
    CodexAppServerObservationPage, CodexAppServerTransport, CodexCompleteAgentConfig,
    CodexCompleteAgentTransportError,
};
pub use registration::{
    CODEX_COMPLETE_AGENT_CONFORMANCE_SUITE, CODEX_COMPLETE_AGENT_DEFINITION_ID,
    CODEX_COMPLETE_AGENT_INSTANCE_ID, CodexCompleteAgentIntegration,
    CodexCompleteAgentRegistration, codex_complete_agent_contribution,
    codex_complete_agent_descriptor,
};
