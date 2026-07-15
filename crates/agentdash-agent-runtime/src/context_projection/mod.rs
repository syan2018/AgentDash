//! Pure projection of business surface facts into durable context presentation plans.
//!
//! This deep module deliberately has no repository, connector, clock, or adapter dependency.
//! Callers provide canonical operation identity and time explicitly, making projection replayable.

mod artifact;
mod bootstrap;
mod compaction;
mod delta;
mod dimension;
mod facts;
mod live;
mod projector;
mod surface_state;
mod turn_runtime;

pub use artifact::*;
pub use bootstrap::*;
pub use compaction::*;
pub use delta::*;
pub use facts::*;
pub use live::*;
pub use projector::*;
pub use surface_state::*;
pub use turn_runtime::*;
