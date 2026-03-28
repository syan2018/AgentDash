use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::Mount;

#[derive(Debug)]
pub enum MountError {
    NotSupported(String),
    NotFound(String),
    ProviderNotRegistered(String),
    OperationFailed(String),
}

impl std::fmt::Display for MountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountError::NotSupported(msg) => write!(f, "not supported: {msg}"),
            MountError::NotFound(msg) => write!(f, "not found: {msg}"),
            MountError::ProviderNotRegistered(msg) => write!(f, "provider not registered: {msg}"),
            MountError::OperationFailed(msg) => write!(f, "operation failed: {msg}"),
        }
    }
}

impl std::error::Error for MountError {}

pub struct SearchQuery {
    pub pattern: String,
    pub path: Option<String>,
    pub case_sensitive: bool,
    pub max_results: Option<usize>,
}

pub struct SearchMatch {
    pub path: String,
    pub line: Option<u32>,
    pub content: String,
}

pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
}

/// Unified mount I/O provider trait.
///
/// Each provider handles a specific `provider` string (e.g. `"relay_fs"`,
/// `"inline_fs"`, `"lifecycle_vfs"`).  The `MountDispatcher` resolves the
/// mount, looks up the matching provider, and delegates.
#[async_trait]
pub trait MountProvider: Send + Sync {
    fn provider_id(&self) -> &str;

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError>;

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        ctx: &MountOperationContext,
    ) -> Result<(), MountError>;

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError>;

    async fn search_text(
        &self,
        mount: &Mount,
        query: &SearchQuery,
        ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError>;

    async fn exec(
        &self,
        mount: &Mount,
        request: &ExecRequest,
        ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        let _ = (mount, request, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support exec",
            self.provider_id()
        )))
    }

    /// Whether this mount can be used right now (e.g. relay backend connected).
    async fn is_available(&self, _mount: &Mount) -> bool {
        true
    }
}

/// Runtime context passed to every `MountProvider` operation.
///
/// Currently empty — providers that need infrastructure references
/// (e.g. `BackendRegistry`, overlay) hold them via constructor injection.
/// Kept as a named struct so future cross-cutting concerns (tracing,
/// cancellation) can be added without changing every call site.
#[derive(Debug, Default)]
pub struct MountOperationContext;

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
        self.registry
            .register(Arc::new(super::provider_lifecycle::LifecycleMountProvider::new(
                lifecycle_run_repo,
            )));
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
