//! Trusted Integration Driver Host.
//!
//! The Host owns service instances, activations, offers, sticky bindings, driver leases, source
//! coordinates, and generation-fenced routing. Managed Runtime remains the authority for Thread,
//! Turn, Item, Interaction, context, hooks, and terminal state.

mod complete_agent;
mod complete_agent_callbacks;
mod complete_agent_repository;
mod conformance;
mod host;
mod memory;
mod model;
mod ports;
mod registry;

pub use agentdash_integration_api::{
    AgentRuntimeCredentialRef, AgentRuntimeCredentialSlot, AgentRuntimePlacement,
    AgentServiceBuildDigest, AgentServiceDefinitionId, AgentServiceOfferId, AgentServiceProvenance,
};
pub use complete_agent::*;
pub use complete_agent_callbacks::*;
pub use complete_agent_repository::*;
pub use conformance::*;
pub use host::*;
pub use memory::*;
pub use model::*;
pub use ports::*;
pub use registry::*;
