//! First-party adapter from AgentDash's provider-neutral Agent Core to the managed Runtime SPI.
//!
//! This crate owns lifecycle/context/surface translation. `agentdash-agent` remains a single-turn
//! provider/tool loop and never imports Runtime, Integration, repository, or vendor vocabulary.

mod context;
mod driver;
mod hook;
mod mapping;
mod tool;

pub use driver::{
    NativeAgentDriver, NativeAgentDriverFactory, NativeAgentRuntimeIntegration,
    NativeAgentServiceConfig, NativeBridgeResolveError, NativeBridgeResolver,
    NativeCredentialScope, native_agent_contribution, native_runtime_profile,
    native_runtime_trust_manifest,
};
