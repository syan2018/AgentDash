//! RuntimeSession delivery, trace, eventing, and live-runtime coordination.

pub mod backend_execution_placement;
pub(crate) mod capability;
pub mod context;
pub(crate) mod hooks;
pub(crate) mod runtime;
pub mod session;
