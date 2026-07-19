//! First-party adapter from Dash Agent to the Complete Agent service boundary.
//!
//! This crate owns lifecycle/context/surface translation and materializes typed host callbacks.
//! Dash Agent depends one-way on the pure Core; neither layer imports Runtime, Integration,
//! repository, legacy driver, journal, or vendor protocol vocabulary.

mod bridge_execution;
mod canonical_projection;
mod core_callbacks;
mod service;

pub use bridge_execution::{
    BridgeDashCompactor, BridgeDashProvider, NoopDashExecutionCallbacks,
    bridge_dash_execution_dependencies,
};
pub use core_callbacks::DashAgentCoreToolCallbacks;
pub use service::{
    DashAgentCompleteService, DashCompleteAgentStore, DashCompleteAtomicCommit,
    DashCompleteEffectRecord, DashCompleteRecordedReceipt, DashCompleteSourceMetadata,
    DashCompleteSourceMutation, NativeCompleteAgentIntegration, native_complete_agent_registration,
};
