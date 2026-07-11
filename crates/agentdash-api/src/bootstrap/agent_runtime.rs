//! API composition facade for the infrastructure-owned Agent Runtime assembly.
//!
//! Runtime state transitions, provisioning, durable workers, Provider resolution and Host
//! activation live below the transport layer. The API crate only imports the completed
//! composition surface and starts it from `AppState`.

pub use agentdash_infrastructure::agent_runtime_composition::*;
