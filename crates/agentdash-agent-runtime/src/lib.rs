//! Managed Runtime's authoritative state kernel.
//!
//! The crate owns canonical runtime transitions and persistence interfaces. Driver and database
//! implementations sit below these ports; application use cases consume the contract gateway.

mod context;
mod gateway;
mod memory;
mod model;
mod ports;

pub use context::*;
pub use gateway::*;
pub use memory::*;
pub use model::*;
pub use ports::*;
