//! Pure projection of business surface facts into durable context presentation plans.
//!
//! This deep module deliberately has no repository, connector, clock, or adapter dependency.
//! Callers provide canonical operation identity and time explicitly, making projection replayable.

mod artifact;
mod delta;
mod facts;
mod projector;

pub use artifact::*;
pub use delta::*;
pub use facts::*;
pub use projector::*;
