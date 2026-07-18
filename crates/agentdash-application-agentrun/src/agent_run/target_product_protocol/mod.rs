//! S4 product/protocol target lane.
//!
//! This module deliberately has no production composition root.  It owns the
//! AgentRun product state and the target ports that a later activation change
//! will bind to the Complete Agent service.

mod activation;
mod companion;
mod feed;
mod fork_saga;

pub use activation::*;
pub use companion::*;
pub use feed::*;
pub use fork_saga::*;
