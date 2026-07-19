use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::Mutex;

use agentdash_platform_spi::Vfs;
use agentdash_platform_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_platform_spi::platform::mount::MountError;
use agentdash_platform_spi::{
    AgentTool, AgentToolError, AgentToolResult, RuntimeVfsOperation, ToolUpdateCallback,
};
use async_trait::async_trait;
use base64::Engine;
use lru::LruCache;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::ResourceRef;
use crate::inline_persistence::InlineContentOverlay;
use crate::runtime_tool_execution::{
    VfsToolContent, VfsToolExecutionError, VfsToolExecutionResult,
};
use crate::service::{VfsService, ensure_runtime_vfs_access};
use crate::tools::common::{SharedRuntimeVfs, resolve_uri_path};
use crate::tools::{legacy_error, legacy_result};
use crate::types::{runtime_entry_is_binary, runtime_entry_mime_type};

// ---------------------------------------------------------------------------
// fs_read
// ---------------------------------------------------------------------------

/// 不带 limit 时的字节阈值。超过 ⇒ is_error 提示用 offset/limit 分段读。
const MAX_BYTES: usize = 256 * 1024;
/// 不带 limit 时的行数阈值。
const MAX_LINES: usize = 5000;
/// Per-tool-instance dedup 容量；FsReadTool 在 session/turn 维度构造，
/// 各 session 自然 LRU 隔离。
const DEDUP_CAPACITY: usize = 64;

type DedupKey = (
    String,        /* mount_id */
    String,        /* path */
    Option<usize>, /* offset, 1-based 入参（None = unset） */
    Option<usize>, /* limit */
);

#[derive(Clone)]
struct DedupCache {
    inner: Arc<Mutex<LruCache<DedupKey, String>>>,
}

impl DedupCache {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(DEDUP_CAPACITY).expect("DEDUP_CAPACITY > 0"),
            ))),
        }
    }

    fn lookup(&self, key: &DedupKey) -> Option<String> {
        self.inner.lock().ok()?.get(key).cloned()
    }

    fn put(&self, key: DedupKey, token: String) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.put(key, token);
        }
    }
}

#[derive(Clone)]
pub struct FsReadExecutor {
    service: Arc<VfsService>,
    vfs: SharedRuntimeVfs,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
    dedup: DedupCache,
}
impl FsReadExecutor {
    pub fn new(
        service: Arc<VfsService>,
        vfs: SharedRuntimeVfs,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        Self {
            service,
            vfs,
            overlay,
            identity,
            dedup: DedupCache::new(),
        }
    }

    pub async fn execute(
        &self,
        args: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<VfsToolExecutionResult, VfsToolExecutionError> {
        let params: FsReadParams = serde_json::from_value(args).map_err(|error| {
            VfsToolExecutionError::InvalidArguments(format!("invalid arguments: {error}"))
        })?;
        let state = self.vfs.snapshot_state().await;
        let vfs = state.vfs;
        let access_policy = state.access_policy;
        let target =
            resolve_uri_path(&vfs, &params.path).map_err(VfsToolExecutionError::ExecutionFailed)?;
        ensure_runtime_vfs_access(
            &access_policy,
            &target.mount_id,
            &target.path,
            RuntimeVfsOperation::Read,
        )
        .map_err(|error| VfsToolExecutionError::ExecutionFailed(error.to_string()))?;

        if let Ok(entry) = tokio::select! {
            _ = cancel.cancelled() => return Err(VfsToolExecutionError::Cancelled),
            result = self.service.stat_with_policy(
                &vfs,
                Some(&access_policy),
                &target,
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
            ) => result,
        } && runtime_entry_is_binary(&entry)
        {
            return self
                .read_binary_entry(&vfs, &access_policy, &target, entry, cancel)
                .await;
        }

        let spi_offset = params.offset.map(|n| n.saturating_sub(1)).unwrap_or(0);
        let result = tokio::select! {
            _ = cancel.cancelled() => return Err(VfsToolExecutionError::Cancelled),
            result = self.service.read_text_range_with_policy(
                &vfs,
                Some(&access_policy),
                &target,
                spi_offset,
                params.limit,
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
            ) => result,
        };
        let result = match result {
            Ok(result) => result,
            Err(MountError::NotFound(missing)) => {
                let suggestions = tokio::select! {
                    _ = cancel.cancelled() => return Err(VfsToolExecutionError::Cancelled),
                    result = self.service.suggest_paths_with_policy(
                        &vfs,
                        Some(&access_policy),
                        &target,
                        3,
                        self.identity.as_ref(),
                    ) => result.unwrap_or_default(),
                };
                let hint = if suggestions.is_empty() {
                    String::new()
                } else {
                    format!(" Did you mean: {}?", suggestions.join(", "))
                };
                return Err(VfsToolExecutionError::ExecutionFailed(format!(
                    "File not found: {missing}.{hint}"
                )));
            }
            Err(error) => {
                return Err(VfsToolExecutionError::ExecutionFailed(error.to_string()));
            }
        };

        if params.limit.is_none() {
            if result.content.len() > MAX_BYTES {
                return Ok(too_large_bytes_result(&result.path, result.content.len()));
            }
            let line_count = result.content.lines().count();
            if line_count > MAX_LINES {
                return Ok(too_many_lines_result(&result.path, line_count));
            }
        }

        let dedup_key: DedupKey = (
            target.mount_id.clone(),
            target.path.clone(),
            params.offset,
            params.limit,
        );
        if let Some(token) = result.version_token.as_deref()
            && let Some(cached) = self.dedup.lookup(&dedup_key)
            && cached == token
        {
            return Ok(unchanged_stub_result(
                &result.path,
                params.offset,
                params.limit,
            ));
        }
        if let Some(token) = result.version_token.as_deref() {
            self.dedup.put(dedup_key, token.to_string());
        }

        let lines: Vec<&str> = result.content.lines().collect();
        let formatted = if lines.is_empty() {
            "   1 | ".to_string()
        } else {
            lines
                .iter()
                .enumerate()
                .map(|(index, line)| format!("{:>4} | {}", spi_offset + index + 1, line))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(VfsToolExecutionResult::text(format!(
            "file: {}\n{}",
            result.path, formatted
        )))
    }
}

#[derive(Clone)]
pub struct FsReadTool {
    executor: FsReadExecutor,
}

impl FsReadTool {
    pub fn new(
        service: Arc<VfsService>,
        vfs: SharedRuntimeVfs,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        Self {
            executor: FsReadExecutor::new(service, vfs, overlay, identity),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FsReadParams {
    /// Unified path in `mount_id://relative/path` format (e.g., `main://src/lib.rs`). The mount prefix may be omitted when the session has exactly one mount.
    pub path: String,
    /// 1-based starting line number to read from. If omitted, reading starts at line 1.
    pub offset: Option<usize>,
    /// Maximum number of lines to return. If omitted, reads to EOF (subject to MAX_BYTES / MAX_LINES guards when offset/limit are both unset).
    pub limit: Option<usize>,
}

#[async_trait]
impl AgentTool for FsReadTool {
    fn name(&self) -> &str {
        "fs_read"
    }
    fn description(&self) -> &str {
        "Reads a file from a mount.\n\
         \n\
         Usage:\n\
         - The path parameter must use `mount_id://relative/path` format (e.g., `main://src/lib.rs`).\n\
         - When the session has only one mount, the prefix may be omitted.\n\
         - By default, this reads the whole file from the beginning, subject to size guards.\n\
         - You can optionally specify a 1-based line offset and line limit, but omit them for normal short-file reads.\n\
         - Files larger than 256KB or 5000 lines return an error without offset/limit; read those in chunks.\n\
         - Output uses cat -n format: each line prefixed with `   N | `.\n\
         - Image files (PNG/JPEG/etc) are returned as an image block plus metadata.\n\
         - This tool reads files only, not directories — use fs_glob for directory listings."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsReadParams>()
    }
    fn protocol_projector(&self) -> Option<agentdash_agent_types::ToolProtocolProjector> {
        Some(agentdash_agent_types::ToolProtocolProjector::FsRead)
    }
    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_fs_read_lifecycle".to_string())
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        cancel: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        self.executor
            .execute(args, cancel)
            .await
            .map(legacy_result)
            .map_err(legacy_error)
    }
}

fn too_large_bytes_result(path: &str, size: usize) -> VfsToolExecutionResult {
    VfsToolExecutionResult {
        content: vec![VfsToolContent::text(format!(
            "File too large: {} bytes (max {} bytes without limit). Use offset/limit to read in chunks.",
            size, MAX_BYTES
        ))],
        is_error: true,
        details: Some(serde_json::json!({
            "type": "file_too_large",
            "path": path,
            "size_bytes": size,
            "max_bytes": MAX_BYTES,
        })),
    }
}

fn too_many_lines_result(path: &str, line_count: usize) -> VfsToolExecutionResult {
    VfsToolExecutionResult {
        content: vec![VfsToolContent::text(format!(
            "File too long: {} lines (max {} lines without limit). Use offset/limit to read in chunks.",
            line_count, MAX_LINES
        ))],
        is_error: true,
        details: Some(serde_json::json!({
            "type": "file_too_many_lines",
            "path": path,
            "line_count": line_count,
            "max_lines": MAX_LINES,
        })),
    }
}

fn unchanged_stub_result(
    path: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> VfsToolExecutionResult {
    let range_label = match (offset, limit) {
        (Some(o), Some(l)) => format!("L{}-L{}", o, o + l - 1),
        (Some(o), None) => format!("L{}-EOF", o),
        (None, Some(l)) => format!("L1-L{}", l),
        (None, None) => "full file".to_string(),
    };
    VfsToolExecutionResult {
        content: vec![VfsToolContent::text(format!(
            "file: {}\n[unchanged since previous read of {}]",
            path, range_label
        ))],
        is_error: false,
        details: Some(serde_json::json!({
            "type": "file_unchanged",
            "path": path,
            "offset": offset,
            "limit": limit,
        })),
    }
}

impl FsReadExecutor {
    async fn read_binary_entry(
        &self,
        vfs: &Vfs,
        access_policy: &agentdash_platform_spi::RuntimeVfsAccessPolicy,
        target: &ResourceRef,
        entry: agentdash_platform_spi::platform::mount::RuntimeFileEntry,
        cancel: CancellationToken,
    ) -> Result<VfsToolExecutionResult, VfsToolExecutionError> {
        if entry.is_dir {
            return Err(VfsToolExecutionError::ExecutionFailed(format!(
                "目标是目录，不是文件: {}://{}",
                target.mount_id, target.path
            )));
        }
        let mime_type = runtime_entry_mime_type(&entry)
            .ok_or_else(|| {
                VfsToolExecutionError::ExecutionFailed(format!(
                    "二进制文件缺少 MIME metadata: {}://{}",
                    target.mount_id, target.path
                ))
            })?
            .to_string();
        if !mime_type.starts_with("image/") {
            return Ok(VfsToolExecutionResult {
                content: vec![VfsToolContent::text(format!(
                    "file: {}\nunsupported binary content: mime_type={}",
                    entry.path, mime_type
                ))],
                is_error: true,
                details: Some(serde_json::json!({
                    "type": "unsupported_binary",
                    "path": entry.path,
                    "mime_type": mime_type,
                })),
            });
        }

        let result = tokio::select! {
            _ = cancel.cancelled() => return Err(VfsToolExecutionError::Cancelled),
            result = self.service.read_binary_with_policy(
                vfs,
                Some(access_policy),
                target,
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
            ) => result.map_err(|error| VfsToolExecutionError::ExecutionFailed(error.to_string()))?,
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(&result.data);
        Ok(VfsToolExecutionResult {
            content: vec![
                VfsToolContent::text(format!(
                    "file: {}\nmime_type: {}\nsize_bytes: {}",
                    result.path,
                    result.mime_type,
                    result.data.len()
                )),
                VfsToolContent::Image {
                    mime_type: result.mime_type,
                    data: encoded,
                },
            ],
            is_error: false,
            details: Some(serde_json::json!({
                "type": "image_file",
                "path": result.path,
                "mime_type": mime_type,
                "size_bytes": result.data.len(),
            })),
        })
    }
}

#[cfg(test)]
mod fs_read_tests {
    use super::*;
    use crate::types::{RUNTIME_FILE_CONTENT_KIND_ATTR, RUNTIME_FILE_MIME_TYPE_ATTR};
    use crate::{BinaryReadResult, ListResult, MountProviderRegistry, ReadResult};
    use agentdash_platform_spi::platform::mount::{
        ApplyPatchRequest, ApplyPatchResult, ExecRequest, ExecResult, ListOptions,
        MountEditCapabilities, MountError, MountOperationContext, MountProvider, RuntimeFileEntry,
        SearchQuery, SearchResult,
    };
    use agentdash_platform_spi::{ContentPart, Mount, MountCapability};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    struct FileFixture {
        text: Option<String>,
        binary: Option<(Vec<u8>, String)>,
        /// `None` 表示该文件不提供 version_token（dedup 不命中）。
        token: StdMutex<Option<String>>,
    }

    struct MemoryReadProvider {
        files: StdMutex<HashMap<String, FileFixture>>,
    }

    impl MemoryReadProvider {
        fn with_default_files() -> Self {
            let mut map = HashMap::new();
            map.insert(
                "note.md".to_string(),
                FileFixture {
                    text: Some("alpha\nbeta\ngamma".to_string()),
                    binary: None,
                    token: StdMutex::new(Some("t1".to_string())),
                },
            );
            map.insert(
                "image.png".to_string(),
                FileFixture {
                    text: None,
                    binary: Some((vec![0, 1, 2, 3], "image/png".to_string())),
                    token: StdMutex::new(None),
                },
            );
            map.insert(
                "archive.zip".to_string(),
                FileFixture {
                    text: None,
                    binary: Some((vec![1, 2, 3], "application/zip".to_string())),
                    token: StdMutex::new(None),
                },
            );
            Self {
                files: StdMutex::new(map),
            }
        }

        fn add_text(&self, path: &str, content: String, token: Option<String>) {
            self.files.lock().unwrap().insert(
                path.to_string(),
                FileFixture {
                    text: Some(content),
                    binary: None,
                    token: StdMutex::new(token),
                },
            );
        }

        fn rotate_token(&self, path: &str, new_token: Option<String>) {
            if let Some(file) = self.files.lock().unwrap().get(path) {
                *file.token.lock().unwrap() = new_token;
            }
        }
    }

    fn attrs(content_kind: &str, mime_type: &str) -> serde_json::Map<String, serde_json::Value> {
        let mut attrs = serde_json::Map::new();
        attrs.insert(
            RUNTIME_FILE_CONTENT_KIND_ATTR.to_string(),
            serde_json::Value::String(content_kind.to_string()),
        );
        attrs.insert(
            RUNTIME_FILE_MIME_TYPE_ATTR.to_string(),
            serde_json::Value::String(mime_type.to_string()),
        );
        attrs
    }

    #[async_trait::async_trait]
    impl MountProvider for MemoryReadProvider {
        fn provider_id(&self) -> &str {
            "memory_read"
        }

        async fn read_text(
            &self,
            _mount: &Mount,
            path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<ReadResult, MountError> {
            let files = self.files.lock().unwrap();
            let file = files
                .get(path)
                .ok_or_else(|| MountError::NotFound(path.to_string()))?;
            let text = file.text.as_ref().ok_or_else(|| {
                MountError::NotSupported(format!("binary file cannot be read as text: {path}"))
            })?;
            let mut result = ReadResult::new(path, text.clone());
            if let Some(token) = file.token.lock().unwrap().clone() {
                result = result.with_version_token(token);
            }
            Ok(result)
        }

        async fn read_binary(
            &self,
            _mount: &Mount,
            path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<BinaryReadResult, MountError> {
            let files = self.files.lock().unwrap();
            let file = files
                .get(path)
                .ok_or_else(|| MountError::NotFound(path.to_string()))?;
            let (bytes, mime) = file.binary.as_ref().ok_or_else(|| {
                MountError::NotSupported(format!("text file cannot be read as binary: {path}"))
            })?;
            Ok(BinaryReadResult::new(path, bytes.clone(), mime)
                .with_attributes(attrs("binary", mime)))
        }

        async fn write_text(
            &self,
            _mount: &Mount,
            _path: &str,
            _content: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            Err(MountError::NotSupported("read only".to_string()))
        }

        fn edit_capabilities(&self, _mount: &Mount) -> MountEditCapabilities {
            MountEditCapabilities::default()
        }

        async fn apply_patch(
            &self,
            _mount: &Mount,
            _request: &ApplyPatchRequest,
            _ctx: &MountOperationContext,
        ) -> Result<ApplyPatchResult, MountError> {
            Err(MountError::NotSupported("read only".to_string()))
        }

        async fn list(
            &self,
            _mount: &Mount,
            _options: &ListOptions,
            _ctx: &MountOperationContext,
        ) -> Result<ListResult, MountError> {
            let files = self.files.lock().unwrap();
            let entries = files
                .iter()
                .map(|(path, file)| {
                    let kind = if file.binary.is_some() {
                        "binary"
                    } else {
                        "text"
                    };
                    let mime = file
                        .binary
                        .as_ref()
                        .map(|(_, m)| m.as_str())
                        .unwrap_or("text/markdown");
                    RuntimeFileEntry::file(path.clone()).with_attributes(attrs(kind, mime))
                })
                .collect();
            Ok(ListResult { entries })
        }

        async fn search_text(
            &self,
            _mount: &Mount,
            _query: &SearchQuery,
            _ctx: &MountOperationContext,
        ) -> Result<SearchResult, MountError> {
            Ok(SearchResult::default())
        }

        async fn exec(
            &self,
            _mount: &Mount,
            _request: &ExecRequest,
            _ctx: &MountOperationContext,
        ) -> Result<ExecResult, MountError> {
            Err(MountError::NotSupported("read only".to_string()))
        }

        async fn stat(
            &self,
            _mount: &Mount,
            path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<RuntimeFileEntry, MountError> {
            let files = self.files.lock().unwrap();
            let file = files
                .get(path)
                .ok_or_else(|| MountError::NotFound(path.to_string()))?;
            let kind = if file.binary.is_some() {
                "binary"
            } else {
                "text"
            };
            let mime = file
                .binary
                .as_ref()
                .map(|(_, m)| m.as_str())
                .unwrap_or("text/markdown");
            Ok(RuntimeFileEntry::file(path).with_attributes(attrs(kind, mime)))
        }
    }

    fn tool_with_provider(provider: Arc<MemoryReadProvider>) -> FsReadTool {
        let mut registry = MountProviderRegistry::new();
        registry.register(provider);
        let service = Arc::new(VfsService::new(Arc::new(registry)));
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "mem".to_string(),
                provider: "memory_read".to_string(),
                backend_id: String::new(),
                root_ref: "memory://root".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "Memory".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("mem".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        FsReadTool::new(service, SharedRuntimeVfs::new(vfs), None, None)
    }

    fn tool() -> FsReadTool {
        tool_with_provider(Arc::new(MemoryReadProvider::with_default_files()))
    }

    #[test]
    fn fs_read_schema_only_requires_path() {
        let schema = tool().parameters_schema();
        let required = schema["required"]
            .as_array()
            .expect("required should be array")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>();

        assert_eq!(required, vec!["path"]);
        assert!(schema["properties"].get("path").is_some());
        assert!(schema["properties"].get("offset").is_some());
        assert!(schema["properties"].get("limit").is_some());
        assert!(
            !required.contains(&"offset"),
            "offset 应保持可省略，避免短文件读取也被迫传参"
        );
        assert!(
            !required.contains(&"limit"),
            "limit 应保持可省略，避免短文件读取也被迫传参"
        );
    }

    // T2 — 1-based offset 转换 + cat -n 格式
    #[tokio::test]
    async fn fs_read_offset_limit_returns_numbered_window() {
        let result = tool()
            .execute(
                "call-1",
                json!({ "path": "note.md", "offset": 2, "limit": 2 }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");

        assert!(!result.is_error);
        let text = result.content[0].extract_text().expect("text");
        assert!(text.contains("file: note.md"));
        assert!(text.contains("   2 | beta"));
        assert!(text.contains("   3 | gamma"));
        assert!(!text.contains("   1 | alpha"));
    }

    #[tokio::test]
    async fn fs_read_image_returns_image_block() {
        let result = tool()
            .execute(
                "call-1",
                json!({ "path": "image.png" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");

        assert!(!result.is_error);
        assert_eq!(result.content.len(), 2);
        let text = result.content[0].extract_text().expect("metadata text");
        assert!(text.contains("file: image.png"));
        assert!(text.contains("mime_type: image/png"));
        match &result.content[1] {
            ContentPart::Image { mime_type, data } => {
                assert_eq!(mime_type, "image/png");
                assert_eq!(data, "AAECAw==");
            }
            other => panic!("expected image block, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fs_read_non_image_binary_returns_unsupported_error_result() {
        let result = tool()
            .execute(
                "call-1",
                json!({ "path": "archive.zip" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");

        assert!(result.is_error);
        assert_eq!(result.content.len(), 1);
        let text = result.content[0].extract_text().expect("text");
        assert!(text.contains("unsupported binary content"));
        assert!(text.contains("application/zip"));
    }

    // T3 — 字节超限：300KB + 不传 limit ⇒ is_error。
    #[tokio::test]
    async fn fs_read_too_large_bytes_is_error() {
        let provider = Arc::new(MemoryReadProvider::with_default_files());
        provider.add_text("big.txt", "a".repeat(300 * 1024), Some("t1".to_string()));
        let tool = tool_with_provider(provider);

        let result = tool
            .execute(
                "call-1",
                json!({ "path": "big.txt" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        assert!(result.is_error);
        let text = result.content[0].extract_text().expect("text");
        assert!(text.contains("File too large"));
    }

    // T4 — 行数超限：6000 行 + 不传 limit ⇒ is_error。
    #[tokio::test]
    async fn fs_read_too_many_lines_is_error() {
        let provider = Arc::new(MemoryReadProvider::with_default_files());
        let big = (0..6000)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        provider.add_text("long.txt", big, Some("t1".to_string()));
        let tool = tool_with_provider(provider);

        let result = tool
            .execute(
                "call-1",
                json!({ "path": "long.txt" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        assert!(result.is_error);
        let text = result.content[0].extract_text().expect("text");
        assert!(text.contains("File too long"));
    }

    // T5 — 上限 bypass：传 limit ⇒ 即使内容超阈值也放行。
    #[tokio::test]
    async fn fs_read_limit_bypasses_byte_guard() {
        let provider = Arc::new(MemoryReadProvider::with_default_files());
        provider.add_text("big.txt", "a".repeat(300 * 1024), Some("t1".to_string()));
        let tool = tool_with_provider(provider);

        let result = tool
            .execute(
                "call-1",
                json!({ "path": "big.txt", "limit": 100 }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");
        assert!(!result.is_error, "limit 已传，应该放行");
    }

    // T6 — dedup 命中：连续两次相同 (path, offset, limit) ⇒ 第二次短桩。
    #[tokio::test]
    async fn fs_read_dedup_returns_unchanged_stub_on_repeat() {
        let tool = tool();
        // 第一次：完整结果
        let first = tool
            .execute(
                "call-1",
                json!({ "path": "note.md", "offset": 1, "limit": 2 }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("first read");
        assert!(!first.is_error);
        let first_text = first.content[0].extract_text().expect("text");
        assert!(first_text.contains("alpha"));

        // 第二次：相同参数 + token 未变 ⇒ 短桩
        let second = tool
            .execute(
                "call-2",
                json!({ "path": "note.md", "offset": 1, "limit": 2 }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("second read");
        assert!(!second.is_error);
        let second_text = second.content[0].extract_text().expect("text");
        assert!(second_text.contains("[unchanged since previous read"));
        assert!(!second_text.contains("alpha"));
    }

    // T7 — dedup 失效：rotate token ⇒ 第二次走完整路径。
    #[tokio::test]
    async fn fs_read_dedup_invalidates_when_token_changes() {
        let provider = Arc::new(MemoryReadProvider::with_default_files());
        let tool = tool_with_provider(provider.clone());

        let _first = tool
            .execute(
                "call-1",
                json!({ "path": "note.md", "offset": 1, "limit": 2 }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("first read");

        provider.rotate_token("note.md", Some("t2".to_string()));

        let second = tool
            .execute(
                "call-2",
                json!({ "path": "note.md", "offset": 1, "limit": 2 }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("second read");
        let text = second.content[0].extract_text().expect("text");
        assert!(text.contains("alpha"), "token 变化后应走完整路径，含正文");
        assert!(!text.contains("[unchanged"));
    }

    // T8 — ENOENT 友好提示：拼错文件名 ⇒ 错误含候选。
    #[tokio::test]
    async fn fs_read_not_found_includes_suggestions() {
        let result = tool()
            .execute(
                "call-1",
                json!({ "path": "not.md" }),
                CancellationToken::new(),
                None,
            )
            .await;
        match result {
            Err(AgentToolError::ExecutionFailed(msg)) => {
                assert!(msg.contains("File not found"));
                // 候选列表里至少包含 note.md（最短编辑距离）
                assert!(
                    msg.contains("note.md"),
                    "msg should include candidate, got: {msg}"
                );
            }
            other => panic!("expected ExecutionFailed, got {other:?}"),
        }
    }
}
