//! MountProvider SPI — 统一 mount I/O 契约。
//!
//! 定义 `MountProvider` trait 及其关联类型，
//! 供企业插件直接实现外部服务的文件系统级操作。

use async_trait::async_trait;

use crate::Mount;

// ============================================================================
// Error
// ============================================================================

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

// ============================================================================
// I/O types
// ============================================================================

#[derive(Debug, Clone)]
pub struct ReadResult {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    pub path: String,
    pub pattern: Option<String>,
    pub recursive: bool,
}

#[derive(Debug, Clone)]
pub struct ListResult {
    pub entries: Vec<RuntimeFileEntry>,
}

#[derive(Debug, Clone)]
pub struct RuntimeFileEntry {
    pub path: String,
    pub size: Option<u64>,
    pub modified_at: Option<i64>,
    pub is_dir: bool,
}

// ============================================================================
// Search
// ============================================================================

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

// ============================================================================
// Exec
// ============================================================================

#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub mount_id: String,
    pub cwd: String,
    pub command: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

// ============================================================================
// Context
// ============================================================================

/// Runtime context passed to every `MountProvider` operation.
///
/// Currently empty — providers that need infrastructure references
/// (e.g. `BackendRegistry`, overlay) hold them via constructor injection.
/// Kept as a named struct so future cross-cutting concerns (tracing,
/// cancellation) can be added without changing every call site.
#[derive(Debug, Default)]
pub struct MountOperationContext;

// ============================================================================
// MountProvider trait
// ============================================================================

/// Unified mount I/O provider trait.
///
/// Each provider handles a specific `provider` string (e.g. `"relay_fs"`,
/// `"inline_fs"`, `"km_bridge"`).  The mount dispatcher resolves the
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
