use std::collections::HashMap;
use std::sync::Arc;

pub use agentdash_spi::mount::{
    MountError, MountOperationContext, MountProvider, SearchMatch, SearchQuery, SearchResult,
};

/// Registry holding all available `MountProvider` implementations.
pub struct MountProviderRegistry {
    providers: HashMap<String, Arc<dyn MountProvider>>,
}

impl MountProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn MountProvider>) {
        self.providers
            .insert(provider.provider_id().to_string(), provider);
    }

    pub fn get(&self, provider_id: &str) -> Option<Arc<dyn MountProvider>> {
        self.providers.get(provider_id).cloned()
    }

    pub fn provider_ids(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

impl Default for MountProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for assembling a `MountProviderRegistry` with built-in providers
/// in the application layer, allowing the API layer to only append
/// infrastructure-specific providers (e.g. `RelayFsMountProvider`).
pub struct MountProviderRegistryBuilder {
    registry: MountProviderRegistry,
}

impl Default for MountProviderRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MountProviderRegistryBuilder {
    pub fn new() -> Self {
        Self {
            registry: MountProviderRegistry::new(),
        }
    }

    /// Register the application-layer built-in providers (inline_fs, lifecycle_vfs).
    pub fn with_builtins(
        mut self,
        lifecycle_run_repo: Arc<dyn agentdash_domain::workflow::LifecycleRunRepository>,
    ) -> Self {
        self.registry
            .register(Arc::new(super::provider_inline::InlineFsMountProvider));
        self.registry.register(Arc::new(
            super::provider_lifecycle::LifecycleMountProvider::new(lifecycle_run_repo),
        ));
        self
    }

    /// Append an additional provider (typically API-layer specific).
    pub fn register(mut self, provider: Arc<dyn MountProvider>) -> Self {
        self.registry.register(provider);
        self
    }

    pub fn build(self) -> MountProviderRegistry {
        self.registry
    }
}
