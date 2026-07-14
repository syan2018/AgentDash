//! Managed Runtime's authoritative state kernel.
//!
//! The crate owns canonical runtime transitions and persistence interfaces. Driver and database
//! implementations sit below these ports; application use cases consume the contract gateway.

mod context;
pub mod context_projection;
mod gateway;
mod hook;
mod memory;
mod model;
mod ports;
mod surface;
mod tool_broker;

pub use agentdash_agent_runtime_contract::{
    RuntimePresentationAppendError, RuntimePresentationAppendReceipt,
    RuntimePresentationAppendRequest, RuntimeTransientPresentationAppendRequest,
    ToolProtocolProjection,
};
pub use context::*;
pub use gateway::*;
pub use hook::*;
pub use memory::*;
pub use model::*;
pub use ports::*;
pub use surface::*;
pub use tool_broker::*;
