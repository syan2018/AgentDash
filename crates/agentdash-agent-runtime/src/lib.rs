//! Managed Agent Runtime projection and surface domain.
//!
//! Runtime owns the normalized platform snapshot/change projection and the desired-surface
//! admission result. Complete Agent history remains source-owned and Host coordination remains in
//! `agentdash-agent-runtime-host`.

mod complete_agent_state;
mod complete_agent_surface;
mod gateway;
mod lifecycle;
mod managed_runtime;

pub use complete_agent_state::*;
pub use complete_agent_surface::*;
pub use gateway::*;
pub use lifecycle::*;
pub use managed_runtime::*;
