//! In-memory Agent coordination and presentation mapping.
//!
//! Complete Agents own execution history and effect outcomes. This crate maps authoritative Agent
//! snapshots into request-scoped Product presentation, compiles desired surfaces, and coordinates
//! process-local platform tools without defining a persistence boundary.

mod agent_snapshot_projection;
mod complete_agent_surface;
mod lifecycle;
mod platform_tool_broker;

pub use agent_snapshot_projection::*;
pub use agentdash_agent_protocol::ToolProtocolProjector;
pub use complete_agent_surface::*;
pub use lifecycle::*;
pub use platform_tool_broker::*;
