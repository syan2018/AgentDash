//! Complete Agent coordination Host.
//!
//! The process-local catalog owns live attachments and callable handles. Runtime targets,
//! bindings, generations, surfaces, and callback routes are ephemeral Host routing facts. Agent
//! history and effect receipts belong to each concrete Complete Agent.

mod complete_agent;
mod complete_agent_callbacks;
mod live_catalog;
mod runtime_hook_handler;
mod runtime_tool_handler;
mod verification;

pub use complete_agent::*;
pub use complete_agent_callbacks::*;
pub use live_catalog::*;
pub use runtime_hook_handler::*;
pub use runtime_tool_handler::*;
pub use verification::*;
