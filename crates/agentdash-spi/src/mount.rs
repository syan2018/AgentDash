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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MountEditCapabilities {
    #[serde(default)]
    pub create: bool,
    #[serde(default)]
    pub delete: bool,
    #[serde(default)]
    pub rename: bool,
}

impl MountEditCapabilities {
    pub fn supports_move(self) -> bool {
        self.rename || (self.create && self.delete)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApplyPatchRequest {
    /// apply_patch 自由格式文本；其中路径必须相对 mount 根目录。
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApplyPatchResult {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
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

    // ---- 服务协商元信息 ----
    // 内置 provider (relay_fs, inline_fs, lifecycle_vfs) 不需要覆盖这些方法。
    // 插件 provider 覆盖后，前端通过 /api/mount-providers 自动发现可配置的服务。

    /// 用户可见的显示名称。默认返回 provider_id。
    fn display_name(&self) -> &str {
        self.provider_id()
    }

    /// root_ref 的格式提示，前端作为 placeholder 展示。
    fn root_ref_hint(&self) -> &str {
        ""
    }

    /// 该 provider 支持的 capability 列表（用于前端预填）。
    fn supported_capabilities(&self) -> Vec<&str> {
        vec!["read", "list"]
    }

    /// 是否允许用户在 Context Container 编辑器中直接配置。
    /// 内置 provider 返回 false，插件 provider 按需返回 true。
    fn is_user_configurable(&self) -> bool {
        false
    }

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

    fn edit_capabilities(&self, mount: &Mount) -> MountEditCapabilities {
        let _ = mount;
        MountEditCapabilities::default()
    }

    async fn delete_text(
        &self,
        mount: &Mount,
        path: &str,
        ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let _ = (mount, path, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support delete_text",
            self.provider_id()
        )))
    }

    async fn rename_text(
        &self,
        mount: &Mount,
        from_path: &str,
        to_path: &str,
        ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let _ = (mount, from_path, to_path, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support rename_text",
            self.provider_id()
        )))
    }

    async fn apply_patch(
        &self,
        mount: &Mount,
        request: &ApplyPatchRequest,
        ctx: &MountOperationContext,
    ) -> Result<ApplyPatchResult, MountError> {
        let _ = (mount, request, ctx);
        Err(MountError::NotSupported(format!(
            "provider `{}` does not support apply_patch",
            self.provider_id()
        )))
    }

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
