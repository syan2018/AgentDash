//! LLM provider catalog, effective credential resolution, and provider protocol bridges.
//!
//! This crate owns the seam between provider/account configuration and the provider-neutral
//! Agent Core `LlmBridge`. It deliberately has no dependency on AgentRun, Runtime, Integration,
//! connector, or API composition concepts.

mod anthropic_bridge;
mod bridge_support;
mod openai_codex_responses_bridge;
mod openai_completions_bridge;
mod openai_content;
mod openai_responses_bridge;
mod openai_responses_common;
mod provider_registry;
mod sse;

pub use provider_registry::{
    BridgeFactory, BuiltProviderEntry, CONTEXT_WINDOW_STANDARD, EffectiveLlmProfileCatalog,
    EffectiveLlmProviderProfile, ModelCatalogSnapshot, ModelDiscoveryStatus, ModelMeta,
    ModelProfileSource, ProbeModelResult, ProviderBridgeResolveError, ProviderCallProfile,
    ProviderCredentialScope, ProviderEntry, ProviderModelResolveError, ProviderUnavailableReason,
    UnavailableProviderEntry, build_effective_profile_catalog_from_db,
    build_effective_provider_profile, build_provider_entries_from_db,
    describe_provider_unavailable_reason, preflight_effective_model_selection,
    probe_models_for_protocol, resolve_effective_bridge_for_scope,
    resolve_effective_bridge_from_db,
};

pub type ProviderCatalogEntry = ProviderEntry;

pub(crate) use anthropic_bridge::AnthropicBridge;
pub(crate) use bridge_support::*;
pub(crate) use openai_codex_responses_bridge::OpenAiCodexResponsesBridge;
pub(crate) use openai_completions_bridge::OpenAiCompletionsBridge;
pub(crate) use openai_responses_bridge::OpenAiResponsesBridge;
