//! Application-facing Managed Agent Runtime contract.
//!
//! This crate owns only platform command, snapshot, change, availability, and projection
//! vocabulary. Complete Agent commands and source coordinates belong to
//! `agentdash-agent-service-api`; Host coordination and transport details never cross this
//! boundary.

pub mod gateway;
pub mod ids;
pub mod managed_projection;

pub use gateway::*;
pub use ids::*;
pub use managed_projection::*;
