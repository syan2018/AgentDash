use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::RuntimeMount;

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
        mount: &RuntimeMount,
        path: &str,
        ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError>;

    async fn write_text(
        &self,
        mount: &RuntimeMount,
        path: &str,
        content: &str,
        ctx: &MountOperationContext,
    ) -> Result<(), MountError>;

    async fn list(
        &self,
        mount: &RuntimeMount,
        options: &ListOptions,
        ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError>;

    async fn search_text(
        &self,
        mount: &RuntimeMount,
        query: &SearchQuery,
        ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError>;

    async fn exec(
        &self,
        mount: &RuntimeMount,
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
    async fn is_available(&self, _mount: &RuntimeMount) -> bool {
        true
    }
}

/// Runtime context passed to every `MountProvider` operation.
///
/// Carries infrastructure references that individual providers may need
/// (e.g. `BackendRegistry` for relay, overlay for inline_fs) without
/// forcing the trait itself to own them.
pub struct MountOperationContext {
    pub extra: HashMap<String, Box<dyn std::any::Any + Send + Sync>>,
}

impl MountOperationContext {
    pub fn new() -> Self {
        Self {
            extra: HashMap::new(),
        }
    }

    pub fn get<T: 'static + Send + Sync>(&self, key: &str) -> Option<&T> {
        self.extra.get(key)?.downcast_ref::<T>()
    }

    pub fn insert<T: 'static + Send + Sync>(&mut self, key: impl Into<String>, value: T) {
        self.extra.insert(key.into(), Box::new(value));
    }
}

impl Default for MountOperationContext {
    fn default() -> Self {
        Self::new()
    }
}

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
