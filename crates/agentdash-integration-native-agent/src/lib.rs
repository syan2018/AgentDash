//! First-party adapter from AgentDash's provider-neutral Agent Core to the managed Runtime SPI.
//!
//! This crate owns lifecycle/context/surface translation. `agentdash-agent` remains a single-turn
//! provider/tool loop and never imports Runtime, Integration, repository, or vendor vocabulary.

mod context;
mod driver;
mod hook;
mod mapping;
mod presentation;
mod tool;

pub use driver::{
    NATIVE_STREAM_USAGE_RESERVE_TOKENS, NativeAgentDriver, NativeAgentDriverFactory,
    NativeAgentRuntimeIntegration, NativeAgentServiceConfig, NativeBridgeResolveError,
    NativeBridgeResolver, NativeCredentialScope, NativePresentationMetadata, ResolvedNativeBridge,
    native_agent_contribution, native_runtime_profile, native_runtime_trust_manifest,
};
