//! Complete Agent coordination Host.
//!
//! The Host durably owns service instances, offers, placements, bindings, source coordinates,
//! generations, effects, and leases. Agent history belongs to each Complete Agent and normalized
//! platform projection belongs to Managed Runtime.

mod complete_agent;
mod complete_agent_callbacks;
mod complete_agent_repository;
mod runtime_hook_handler;
mod runtime_tool_handler;

pub use complete_agent::*;
pub use complete_agent_callbacks::*;
pub use complete_agent_repository::*;
pub use runtime_hook_handler::*;
pub use runtime_tool_handler::*;
