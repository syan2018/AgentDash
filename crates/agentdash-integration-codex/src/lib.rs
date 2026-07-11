//! Codex App Server first-party runtime adapter.
//!
//! Codex JSON-RPC DTOs terminate inside this crate. Public APIs expose only AgentDash-owned
//! runtime and Integration contracts.

mod artifact;
mod contribution;
mod driver;
mod hook_bridge;
mod mapping;
mod rpc;

pub use contribution::{
    CodexRuntimeIntegration, codex_runtime_contribution, codex_runtime_contribution_with_launcher,
    codex_runtime_trust_manifest,
};
pub use driver::{CodexAppServerLauncher, CodexRuntimeDriverFactory};
