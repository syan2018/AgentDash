//! Trusted Integration Driver Host.
//!
//! The Host owns service instances, activations, offers, sticky bindings, driver leases, source
//! coordinates, and generation-fenced routing. Managed Runtime remains the authority for Thread,
//! Turn, Item, Interaction, context, hooks, and terminal state.

mod host;
mod memory;
mod model;
mod ports;
mod registry;

pub use agentdash_integration_api::{
    AgentRuntimeCredentialRef, AgentRuntimeCredentialSlot, AgentRuntimePlacement,
    AgentServiceBuildDigest, AgentServiceDefinitionId, AgentServiceOfferId, AgentServiceProvenance,
};
pub use host::*;
pub use memory::*;
pub use model::*;
pub use ports::*;
pub use registry::*;
