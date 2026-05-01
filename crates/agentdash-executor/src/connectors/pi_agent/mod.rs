mod bridges;
mod connector;
pub(crate) mod factory;
mod slash_commands;
mod stream_mapper;
pub mod system_prompt;

#[deprecated(note = "use agentdash_executor::mcp::direct instead")]
pub use crate::mcp::direct as pi_agent_mcp;
#[deprecated(note = "use agentdash_executor::mcp::relay instead")]
pub use crate::mcp::relay as relay_mcp;

pub use bridges::provider_registry as pi_agent_provider_registry;
pub use connector::*;
pub use factory::build_pi_agent_connector;
