//! Runtime Wire transport and proxy for remote Complete Agent services.
//!
//! Relay provides placement only. Agent identity, lifecycle, history, callbacks, and changes stay
//! in the Complete Agent service contract.

use agentdash_agent_runtime_wire::RuntimeWireEnvelope;
use async_trait::async_trait;
use thiserror::Error;

mod complete_agent;
mod registration;

pub use complete_agent::*;
pub use registration::{
    RemoteCompleteAgentIntegration, RemoteCompleteAgentRegistration,
    remote_complete_agent_contribution,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoteRuntimeTransportError {
    #[error("remote runtime placement is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("remote runtime returned a protocol violation: {reason}")]
    Protocol { reason: String, critical: bool },
}

#[async_trait]
pub trait RuntimeWirePlacement: Send + Sync {
    async fn send(&self, frame: RuntimeWireEnvelope) -> Result<(), RemoteRuntimeTransportError>;

    async fn receive(&self) -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError>;

    async fn acknowledge_disconnect(&self) {}

    /// Explicitly retires a placement that was opened but must not become or remain selectable.
    async fn close(&self, _reason: &str) {}
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeWirePlacementEvent {
    Frame(Box<RuntimeWireEnvelope>),
    Disconnected { reason: String },
    Reconnected,
}
