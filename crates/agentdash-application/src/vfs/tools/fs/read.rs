use std::sync::Arc;

use agentdash_spi::Vfs;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use base64::Engine;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::vfs::ResourceRef;
use crate::vfs::inline_persistence::InlineContentOverlay;
use crate::vfs::relay_service::RelayVfsService;
use crate::vfs::tools::common::{SharedRuntimeVfs, ok_text, resolve_uri_path};

// ---------------------------------------------------------------------------
// fs_read
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FsReadTool {
    service: Arc<RelayVfsService>,
    vfs: SharedRuntimeVfs,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
}
impl FsReadTool {
    pub fn new(
        service: Arc<RelayVfsService>,
        vfs: SharedRuntimeVfs,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        Self {
            service,
            vfs,
            overlay,
            identity,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FsReadParams {
    /// Unified path in `mount_id://relative/path` format (e.g., `main://src/lib.rs`). The mount prefix may be omitted when the session has exactly one mount.
    pub path: String,
    /// 1-based start line number. If omitted, reading starts from line 1.
    pub start_line: Option<usize>,
    /// 1-based end line number (inclusive). If omitted, reads to the end of the file.
    pub end_line: Option<usize>,
}

#[async_trait]
impl AgentTool for FsReadTool {
    fn name(&self) -> &str {
        "fs_read"
    }
    fn description(&self) -> &str {
        "Read the contents of a file from a mount.\n\
         \n\
         Usage:\n\
         - The path parameter uses `mount_id://relative/path` format (e.g., `main://src/lib.rs`).\n\
         - When the session has only one mount, the prefix may be omitted.\n\
         - Text files are returned with numbered lines in `cat -n` style (line_number | content).\n\
         - Use start_line / end_line for partial text reads.\n\
         - Image files stored in typed VFS providers are returned as an image block plus metadata.\n\
         - This tool can only read files, not directories. Use fs_glob for directory contents."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsReadParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsReadParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;
        let vfs = self.vfs.snapshot().await;
        let target =
            resolve_uri_path(&vfs, &params.path).map_err(AgentToolError::ExecutionFailed)?;
        if let Ok(entry) = self
            .service
            .stat(
                &vfs,
                &target,
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
            )
            .await
            && entry_content_kind(&entry).as_deref() == Some("binary")
        {
            return self.read_binary_entry(&vfs, &target, entry).await;
        }
        let result = self
            .service
            .read_text(
                &vfs,
                &target,
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let lines = result.content.lines().collect::<Vec<_>>();
        let start = params.start_line.unwrap_or(1).max(1);
        let end = params.end_line.unwrap_or(lines.len()).max(start);
        let selected = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                let n = i + 1;
                (n >= start && n <= end).then(|| format!("{:>4} | {}", n, line))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ok_text(format!(
            "file: {}\n{}",
            result.path,
            if selected.is_empty() {
                "   1 | ".to_string()
            } else {
                selected
            }
        )))
    }
}

impl FsReadTool {
    async fn read_binary_entry(
        &self,
        vfs: &Vfs,
        target: &ResourceRef,
        entry: agentdash_spi::platform::mount::RuntimeFileEntry,
    ) -> Result<AgentToolResult, AgentToolError> {
        if entry.is_dir {
            return Err(AgentToolError::ExecutionFailed(format!(
                "目标是目录，不是文件: {}://{}",
                target.mount_id, target.path
            )));
        }
        let mime_type = entry_mime_type(&entry).ok_or_else(|| {
            AgentToolError::ExecutionFailed(format!(
                "二进制文件缺少 MIME metadata: {}://{}",
                target.mount_id, target.path
            ))
        })?;
        if !mime_type.starts_with("image/") {
            return Ok(AgentToolResult {
                content: vec![ContentPart::text(format!(
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

        let result = self
            .service
            .read_binary(
                vfs,
                target,
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&result.data);
        Ok(AgentToolResult {
            content: vec![
                ContentPart::text(format!(
                    "file: {}\nmime_type: {}\nsize_bytes: {}",
                    result.path,
                    result.mime_type,
                    result.data.len()
                )),
                ContentPart::Image {
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

fn entry_content_kind(entry: &agentdash_spi::platform::mount::RuntimeFileEntry) -> Option<String> {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get("content_kind"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn entry_mime_type(entry: &agentdash_spi::platform::mount::RuntimeFileEntry) -> Option<String> {
    entry
        .attributes
        .as_ref()
        .and_then(|attrs| attrs.get("mime_type"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

#[cfg(test)]
mod fs_read_tests {
    use super::*;
    use crate::vfs::{BinaryReadResult, ListResult, MountProviderRegistry, ReadResult};
    use agentdash_spi::platform::mount::{
        ApplyPatchRequest, ApplyPatchResult, ExecRequest, ExecResult, ListOptions,
        MountEditCapabilities, MountError, MountOperationContext, MountProvider, RuntimeFileEntry,
        SearchQuery, SearchResult,
    };
    use agentdash_spi::{Mount, MountCapability};
    use serde_json::json;

    struct MemoryReadProvider;

    fn attrs(content_kind: &str, mime_type: &str) -> serde_json::Map<String, serde_json::Value> {
        let mut attrs = serde_json::Map::new();
        attrs.insert(
            "content_kind".to_string(),
            serde_json::Value::String(content_kind.to_string()),
        );
        attrs.insert(
            "mime_type".to_string(),
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
            match path {
                "note.md" => Ok(ReadResult::new(path, "alpha\nbeta\ngamma")),
                "image.png" | "archive.zip" => Err(MountError::NotSupported(format!(
                    "binary file cannot be read as text: {path}"
                ))),
                _ => Err(MountError::NotFound(path.to_string())),
            }
        }

        async fn read_binary(
            &self,
            _mount: &Mount,
            path: &str,
            _ctx: &MountOperationContext,
        ) -> Result<BinaryReadResult, MountError> {
            match path {
                "image.png" => Ok(BinaryReadResult::new(path, vec![0, 1, 2, 3], "image/png")
                    .with_attributes(attrs("binary", "image/png"))),
                "archive.zip" => Ok(
                    BinaryReadResult::new(path, vec![1, 2, 3], "application/zip")
                        .with_attributes(attrs("binary", "application/zip")),
                ),
                "note.md" => Err(MountError::NotSupported(format!(
                    "text file cannot be read as binary: {path}"
                ))),
                _ => Err(MountError::NotFound(path.to_string())),
            }
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
            Ok(ListResult {
                entries: vec![
                    RuntimeFileEntry::file("note.md")
                        .with_size(16)
                        .with_attributes(attrs("text", "text/markdown")),
                    RuntimeFileEntry::file("image.png")
                        .with_size(4)
                        .with_attributes(attrs("binary", "image/png")),
                    RuntimeFileEntry::file("archive.zip")
                        .with_size(3)
                        .with_attributes(attrs("binary", "application/zip")),
                ],
            })
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
            match path {
                "note.md" => Ok(RuntimeFileEntry::file(path)
                    .with_size(16)
                    .with_attributes(attrs("text", "text/markdown"))),
                "image.png" => Ok(RuntimeFileEntry::file(path)
                    .with_size(4)
                    .with_attributes(attrs("binary", "image/png"))),
                "archive.zip" => Ok(RuntimeFileEntry::file(path)
                    .with_size(3)
                    .with_attributes(attrs("binary", "application/zip"))),
                _ => Err(MountError::NotFound(path.to_string())),
            }
        }
    }

    fn tool() -> FsReadTool {
        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(MemoryReadProvider));
        let service = Arc::new(RelayVfsService::new(Arc::new(registry)));
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

    #[tokio::test]
    async fn fs_read_text_keeps_numbered_lines() {
        let result = tool()
            .execute(
                "call-1",
                json!({ "path": "note.md", "start_line": 2, "end_line": 3 }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("execute");

        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
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
}
