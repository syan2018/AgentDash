//! First-party adapter from Dash Agent and its provider-neutral Agent Core to Agent Runtime.
//!
//! This crate owns lifecycle/context/surface translation and the Backbone-to-Core
//! anti-corruption projection. Dash Agent depends one-way on the pure Core; neither layer imports
//! Runtime, Integration, repository, or vendor vocabulary.

mod context;
mod core_callbacks;
mod core_projection;
mod driver;
mod hook;
mod mapping;
mod presentation;
mod service;
mod tool;

pub use core_callbacks::DashAgentCoreToolCallbacks;
pub use core_projection::{
    NativeCoreProjectionError, NativeCoreProjectionEvent, project_native_core_input,
    project_native_core_transcript,
};
pub use driver::{
    NATIVE_STREAM_USAGE_RESERVE_TOKENS, NativeAgentDriver, NativeAgentDriverFactory,
    NativeAgentRuntimeIntegration, NativeAgentServiceConfig, NativeBridgeResolveError,
    NativeBridgeResolver, NativeCredentialScope, NativePresentationMetadata, ResolvedNativeBridge,
    native_agent_contribution, native_runtime_profile, native_runtime_trust_manifest,
};
pub use service::{
    DashAgentCompleteService, NativeCompleteAgentRegistration, native_complete_agent_registration,
};
