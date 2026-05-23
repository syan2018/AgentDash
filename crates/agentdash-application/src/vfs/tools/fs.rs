use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_spi::Vfs;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use base64::Engine;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::vfs::inline_persistence::InlineContentOverlay;
use crate::vfs::mutation_queue::MutationQueue;
use crate::vfs::relay_service::RelayVfsService;
use crate::vfs::rewrite::find_mount_uri_candidates;
use crate::vfs::{
    ExecRequest, ListOptions, MaterializationRewrite, PatchEntry, ResourceRef,
    RewriteShellCommandOutput, VfsMaterializationService, normalize_mount_relative_path,
    parse_patch_text, resolve_mount, resolve_mount_id,
};

pub use super::common::{SharedRuntimeVfs, ok_text, resolve_uri_path};
pub use super::mounts::MountsListTool;

// ---------------------------------------------------------------------------
// fs_apply_patch — Codex-style description
// ---------------------------------------------------------------------------

const FS_APPLY_PATCH_DESCRIPTION: &str = "\
Apply edits to one or more files using the Codex apply_patch format.\n\
This is NOT a unified diff. Use this tool for all file modifications: \
creating new files, editing existing files, deleting files, and renaming.\n\
\n\
Usage:\n\
- Paths inside the patch can use `mount_id://relative/path` to target a specific mount; \
paths without a prefix fall back to the `mount` parameter or the session default mount.\n\
- ALWAYS read the target file with fs_read before editing, so context lines are accurate.\n\
- To create a new file, use `*** Add File: path` with every content line prefixed by `+`.\n\
- NEVER use unified diff syntax (`---`/`+++`); use only the grammar below.\n\
\n\
Grammar:\n\
  Patch       := \"*** Begin Patch\" NL { FileOp } \"*** End Patch\" NL?\n\
  FileOp      := AddFile | DeleteFile | UpdateFile\n\
  AddFile     := \"*** Add File: \" path NL { \"+\" line NL }\n\
  DeleteFile  := \"*** Delete File: \" path NL\n\
  UpdateFile  := \"*** Update File: \" path NL [ MoveTo ] { Hunk }\n\
  MoveTo      := \"*** Move to: \" newPath NL\n\
  Hunk        := \"@@\" [ header ] NL { HunkLine } [ \"*** End of File\" NL ]\n\
  HunkLine    := (\" \" | \"-\" | \"+\") text NL\n\
\n\
Example:\n\
```\n\
*** Begin Patch\n\
*** Add File: src/util.rs\n\
+pub fn helper() -> &'static str {\n\
+    \"hello\"\n\
+}\n\
*** Update File: src/main.rs\n\
@@ fn main()\n\
 fn main() {\n\
-    println!(\"old\");\n\
+    println!(\"{}\", util::helper());\n\
 }\n\
*** Delete File: obsolete.rs\n\
*** End Patch\n\
```\n\
\n\
Important:\n\
- The patch MUST begin with `*** Begin Patch` and end with `*** End Patch`.\n\
- Context lines (space prefix) must exactly match the current file content.\n\
- Add File content lines must ALL begin with `+`.\n\
- Show ~3 lines of context above and below each change for reliable anchoring.";

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
            Ok(SearchResult { matches: vec![] })
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

// ---------------------------------------------------------------------------
// fs_apply_patch
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FsApplyPatchTool {
    service: Arc<RelayVfsService>,
    vfs: SharedRuntimeVfs,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    mutation_queue: MutationQueue,
}
impl FsApplyPatchTool {
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
            mutation_queue: MutationQueue::default(),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FsApplyPatchParams {
    /// Default mount ID. Paths in the patch that lack a mount prefix will use this mount. If omitted, the session's default mount is used.
    pub mount: Option<String>,
    /// The patch text in Codex apply_patch format. See the tool description for the full grammar and examples. Paths inside the patch may use `mount_id://relative/path` to target a specific mount.
    pub patch: String,
}

#[async_trait]
impl AgentTool for FsApplyPatchTool {
    fn name(&self) -> &str {
        "fs_apply_patch"
    }
    fn description(&self) -> &str {
        FS_APPLY_PATCH_DESCRIPTION
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsApplyPatchParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsApplyPatchParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;
        let vfs = self.vfs.snapshot().await;
        let mutation_keys =
            fs_apply_patch_mutation_keys(&vfs, params.mount.as_deref(), &params.patch)
                .unwrap_or_default();
        let result = self
            .mutation_queue
            .with_locks(
                mutation_keys,
                self.service.apply_patch_multi(
                    &vfs,
                    params.mount.as_deref(),
                    &params.patch,
                    self.overlay.as_ref().map(|arc| arc.as_ref()),
                    self.identity.as_ref(),
                ),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;

        let mut lines = Vec::new();
        if !result.added.is_empty() {
            lines.push(format!("added: {}", result.added.join(", ")));
        }
        if !result.modified.is_empty() {
            lines.push(format!("modified: {}", result.modified.join(", ")));
        }
        if !result.deleted.is_empty() {
            lines.push(format!("deleted: {}", result.deleted.join(", ")));
        }
        for err in &result.errors {
            lines.push(format!(
                "error: {}://{} — {}",
                err.mount_id, err.path, err.message
            ));
        }
        if lines.is_empty() {
            lines.push("patch produced no changes.".to_string());
        }
        let is_error = result.added.is_empty()
            && result.modified.is_empty()
            && result.deleted.is_empty()
            && !result.errors.is_empty();
        Ok(AgentToolResult {
            content: vec![ContentPart::text(lines.join("\n"))],
            is_error,
            details: None,
        })
    }
}

fn fs_apply_patch_mutation_keys(
    vfs: &Vfs,
    default_mount_id: Option<&str>,
    patch: &str,
) -> Result<Vec<String>, String> {
    let entries = parse_patch_text(patch).map_err(|e| format!("patch 解析失败: {e}"))?;
    let fallback_mount_id = match default_mount_id {
        Some(id) if !id.trim().is_empty() => id.to_string(),
        _ => resolve_mount_id(vfs, None)?,
    };

    let mut keys = BTreeSet::new();
    for entry in entries {
        collect_patch_entry_mutation_keys(&mut keys, &entry, &fallback_mount_id)?;
    }
    Ok(keys.into_iter().collect())
}

fn collect_patch_entry_mutation_keys(
    keys: &mut BTreeSet<String>,
    entry: &PatchEntry,
    fallback_mount_id: &str,
) -> Result<(), String> {
    let raw_path = entry.path().to_string_lossy();
    let (mount_id, relative_path) = mutation_key_parts(&raw_path, fallback_mount_id)?;
    keys.insert(format!("{mount_id}://{relative_path}"));

    if let PatchEntry::UpdateFile {
        move_path: Some(move_path),
        ..
    } = entry
    {
        let raw_move_path = move_path.to_string_lossy();
        let (move_mount_id, move_relative_path) = mutation_key_parts(&raw_move_path, &mount_id)?;
        keys.insert(format!("{move_mount_id}://{move_relative_path}"));
    }

    Ok(())
}

fn mutation_key_parts(raw: &str, fallback_mount_id: &str) -> Result<(String, String), String> {
    if let Some((mount_id, relative)) = raw.split_once("://") {
        let mount_id = mount_id.trim();
        if mount_id.is_empty() {
            return Err("patch 路径的 mount ID 不能为空".to_string());
        }
        return Ok((
            mount_id.to_string(),
            normalize_mount_relative_path(relative.trim_start_matches('/'), false)?,
        ));
    }

    Ok((
        fallback_mount_id.to_string(),
        normalize_mount_relative_path(raw, false)?,
    ))
}

// ---------------------------------------------------------------------------
// fs_glob  (formerly fs_list)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FsGlobTool {
    service: Arc<RelayVfsService>,
    vfs: SharedRuntimeVfs,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
}
impl FsGlobTool {
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
pub struct FsGlobParams {
    /// Directory path in `mount_id://relative/path` format. The mount prefix may be omitted when the session has exactly one mount. Defaults to the mount root if omitted.
    pub path: Option<String>,
    /// If true, list contents recursively. Defaults to false.
    pub recursive: Option<bool>,
    /// Glob pattern to filter entries (e.g., "*.rs", "**/*.json", "src/**/*.{ts,tsx}"). Supports wildcards (*, ?), recursive (**), character classes ([abc]), and alternation ({a,b}). If the pattern contains no glob characters, it is treated as a substring filter.
    pub pattern: Option<String>,
}

#[async_trait]
impl AgentTool for FsGlobTool {
    fn name(&self) -> &str {
        "fs_glob"
    }
    fn description(&self) -> &str {
        "Fast file pattern matching across all mount types.\n\
         \n\
         Usage:\n\
         - Supports glob patterns like \"**/*.rs\", \"src/**/*.ts\", or \"*.{ts,tsx}\".\n\
         - When pattern is omitted, lists direct children of the given path.\n\
         - Set recursive to true to list all nested contents; combine with pattern for deep matching.\n\
         - Returns entries tagged as [dir] or [file] with forward-slash separated paths.\n\
         - Use this tool to understand project structure before reading or editing files.\n\
         - For text content search (grep-style), use fs_grep instead."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsGlobParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsGlobParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;
        let vfs = self.vfs.snapshot().await;
        let target = resolve_uri_path(&vfs, params.path.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let result = self
            .service
            .list(
                &vfs,
                &target.mount_id,
                ListOptions {
                    path: if target.path.is_empty() {
                        ".".to_string()
                    } else {
                        target.path
                    },
                    pattern: params.pattern,
                    recursive: params.recursive.unwrap_or(false),
                },
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let lines = result
            .entries
            .into_iter()
            .map(|e| {
                let kind = if e.is_dir { "dir" } else { "file" };
                format!("[{}] {}", kind, e.path.replace('\\', "/"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ok_text(if lines.is_empty() {
            "(empty directory)".to_string()
        } else {
            lines
        }))
    }
}

// ---------------------------------------------------------------------------
// fs_grep  (formerly fs_search)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FsGrepTool {
    service: Arc<RelayVfsService>,
    vfs: SharedRuntimeVfs,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsGrepTool {
    pub fn new(
        service: Arc<RelayVfsService>,
        vfs: SharedRuntimeVfs,
        overlay: Option<Arc<InlineContentOverlay>>,
        _identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        Self {
            service,
            vfs,
            overlay,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FsGrepParams {
    /// The search query string. Interpreted as literal text by default; set regex to true for regex syntax.
    pub query: String,
    /// Root path to search under, in `mount_id://relative/path` format. The mount prefix may be omitted when the session has exactly one mount. Defaults to the mount root.
    pub path: Option<String>,
    /// If true, interpret query as a regular expression. Defaults to false.
    #[serde(default)]
    pub regex: bool,
    /// Glob pattern to filter which files are searched (e.g., "*.rs", "src/**/*.ts").
    pub include: Option<String>,
    /// Maximum number of matches to return. Defaults to 50.
    pub max_results: Option<usize>,
    /// Number of context lines to show around each match. Defaults to 0.
    pub context_lines: Option<usize>,
}

#[async_trait]
impl AgentTool for FsGrepTool {
    fn name(&self) -> &str {
        "fs_grep"
    }
    fn description(&self) -> &str {
        "Search for text content within files on a mount.\n\
         \n\
         Usage:\n\
         - ALWAYS use fs_grep for text search. NEVER use shell_exec with grep/rg.\n\
         - Supports literal text and regex patterns (set regex to true for regex mode).\n\
         - Use the include parameter to filter by file glob (e.g., \"*.rs\", \"src/**/*.ts\").\n\
         - Results show matching lines with file paths and line numbers.\n\
         - Defaults to max 50 results; adjust with max_results.\n\
         - Use context_lines to show surrounding lines for better understanding."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsGrepParams>()
    }
    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsGrepParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;
        let vfs = self.vfs.snapshot().await;
        let target = resolve_uri_path(&vfs, params.path.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let search_path = if target.path.is_empty() {
            ".".to_string()
        } else {
            target.path
        };
        let (hits, truncated) = self
            .service
            .search_text_extended(
                &vfs,
                &crate::vfs::TextSearchParams {
                    mount_id: &target.mount_id,
                    path: &search_path,
                    query: &params.query,
                    is_regex: params.regex,
                    include_glob: params.include.as_deref(),
                    max_results: params.max_results.unwrap_or(50).max(1),
                    context_lines: params.context_lines.unwrap_or(0),
                    overlay: self.overlay.as_ref().map(|arc| arc.as_ref()),
                },
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let mut output = if hits.is_empty() {
            "no matches found".to_string()
        } else {
            hits.join("\n")
        };
        if truncated {
            output.push_str("\n(results truncated; narrow your search to see more)");
        }
        Ok(ok_text(output))
    }
}

// ---------------------------------------------------------------------------
// shell_exec
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ShellExecTool {
    service: Arc<RelayVfsService>,
    vfs: SharedRuntimeVfs,
    shell_output_registry: Option<Arc<agentdash_relay::ShellOutputRegistry>>,
    materialization: Option<Arc<VfsMaterializationService>>,
    session_id: String,
    turn_id: Option<String>,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
}
impl ShellExecTool {
    pub fn new(service: Arc<RelayVfsService>, vfs: SharedRuntimeVfs) -> Self {
        Self {
            service,
            vfs,
            shell_output_registry: None,
            materialization: None,
            session_id: "session".to_string(),
            turn_id: None,
            overlay: None,
            identity: None,
        }
    }

    pub fn with_shell_output_registry(
        mut self,
        registry: Arc<agentdash_relay::ShellOutputRegistry>,
    ) -> Self {
        self.shell_output_registry = Some(registry);
        self
    }

    pub fn with_materialization_context(
        mut self,
        materialization: Option<Arc<VfsMaterializationService>>,
        session_id: String,
        turn_id: Option<String>,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        self.materialization = materialization;
        self.session_id = session_id;
        self.turn_id = turn_id;
        self.overlay = overlay;
        self.identity = identity;
        self
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellExecParams {
    /// Working directory in `mount_id://relative/path` format. The mount prefix may be omitted when the session has exactly one mount. Defaults to the mount root.
    pub cwd: Option<String>,
    /// The shell command to execute.
    pub command: String,
    /// Command timeout in seconds. If omitted, the system default timeout applies.
    pub timeout_secs: Option<u64>,
}

#[async_trait]
impl AgentTool for ShellExecTool {
    fn name(&self) -> &str {
        "shell_exec"
    }
    fn description(&self) -> &str {
        "Execute a shell command on a mount.\n\
         \n\
         Usage:\n\
         - Commands run in the shell environment of the target mount.\n\
         - Use the cwd parameter to set the working directory (defaults to mount root).\n\
         - stdout and stderr are returned separately, labeled as [stdout] and [stderr].\n\
         - The exit code is included in the output; non-zero exit codes are flagged as errors.\n\
         - Use timeout_secs to limit execution time for long-running commands.\n\
         - Prefer dedicated tools (fs_read, fs_glob, fs_grep) over shell equivalents when possible."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<ShellExecParams>()
    }
    async fn execute(
        &self,
        _tool_call_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        on_update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ShellExecParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;
        let vfs = self.vfs.snapshot().await;
        let target = resolve_uri_path(&vfs, params.cwd.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let cwd = if target.path.is_empty() {
            ".".to_string()
        } else {
            target.path
        };
        let exec_mount =
            resolve_mount(&vfs, &target.mount_id, agentdash_spi::MountCapability::Exec)
                .map_err(AgentToolError::ExecutionFailed)?;

        let rewrite_output = if let Some(materialization) = &self.materialization {
            materialization
                .rewrite_shell_command(crate::vfs::RewriteShellCommandInput {
                    vfs: &vfs,
                    exec_mount_id: &target.mount_id,
                    command: &params.command,
                    session_id: &self.session_id,
                    turn_id: self.turn_id.as_deref(),
                    tool_call_id: Some(_tool_call_id),
                    overlay: self.overlay.as_ref().map(|arc| arc.as_ref()),
                    identity: self.identity.as_ref(),
                })
                .await
                .map_err(AgentToolError::ExecutionFailed)?
        } else {
            RewriteShellCommandOutput {
                command: params.command.clone(),
                rewrites: Vec::new(),
            }
        };
        if !rewrite_output.rewrites.is_empty() {
            tracing::info!(
                exec_mount_id = %exec_mount.id,
                rewrite_count = rewrite_output.rewrites.len(),
                "shell_exec command 中的 VFS URI 已物化并重写"
            );
            if let Some(on_update) = &on_update {
                on_update(vfs_uri_rewrite_notice(
                    &params.command,
                    &rewrite_output.command,
                    &rewrite_output.rewrites,
                ));
            }
        }
        let rewritten_command = rewrite_output.command.clone();
        if let Some(message) = unresolved_vfs_uri_message(&rewritten_command, &vfs) {
            return Err(AgentToolError::ExecutionFailed(message));
        }

        let streaming_call_id = self
            .shell_output_registry
            .as_ref()
            .map(|_| agentdash_relay::RelayMessage::new_id("stream-call"));

        // 注册流式输出通道 + 转发任务
        let forward_handle = if let (Some(registry), Some(call_id), Some(on_update)) =
            (&self.shell_output_registry, &streaming_call_id, &on_update)
        {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            registry.register(call_id, tx);
            let cb = on_update.clone();
            Some(tokio::spawn(async move {
                while let Some(chunk) = rx.recv().await {
                    cb(AgentToolResult {
                        content: vec![ContentPart::text(chunk.delta)],
                        is_error: false,
                        details: Some(serde_json::json!({
                            "type": "shell_output",
                            "stream": chunk.stream,
                        })),
                    });
                }
            }))
        } else {
            None
        };

        let result = self
            .service
            .exec(
                &vfs,
                &ExecRequest {
                    mount_id: target.mount_id.clone(),
                    cwd: cwd.clone(),
                    command: rewritten_command.clone(),
                    timeout_ms: params.timeout_secs.map(|s| s.saturating_mul(1000)),
                    streaming_call_id: streaming_call_id.clone(),
                },
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;

        // 清理通道
        if let Some(ref call_id) = streaming_call_id {
            if let Some(registry) = &self.shell_output_registry {
                registry.unregister(call_id);
            }
        }
        if let Some(handle) = forward_handle {
            handle.abort();
        }

        let merged = if result.stderr.trim().is_empty() {
            result.stdout
        } else if result.stdout.trim().is_empty() {
            format!("[stderr]\n{}", result.stderr)
        } else {
            format!("[stdout]\n{}\n\n[stderr]\n{}", result.stdout, result.stderr)
        };
        Ok(AgentToolResult {
            content: vec![ContentPart::text(shell_exec_result_text(
                &params.command,
                &rewritten_command,
                &target.mount_id,
                &cwd,
                result.exit_code,
                &merged,
                !rewrite_output.rewrites.is_empty(),
            ))],
            is_error: result.exit_code != 0,
            details: shell_exec_result_details(
                &params.command,
                &rewritten_command,
                &rewrite_output.rewrites,
            ),
        })
    }
}

fn vfs_uri_rewrite_notice(
    original_command: &str,
    rewritten_command: &str,
    rewrites: &[MaterializationRewrite],
) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(format_vfs_uri_rewrite_notice(
            rewritten_command,
            rewrites,
        ))],
        is_error: false,
        details: Some(vfs_uri_rewrite_details(
            original_command,
            rewritten_command,
            rewrites,
        )),
    }
}

fn format_vfs_uri_rewrite_notice(
    rewritten_command: &str,
    rewrites: &[MaterializationRewrite],
) -> String {
    let mut lines = vec![format!(
        "vfs_uri_rewrite: {} URI(s) materialized",
        rewrites.len()
    )];
    for rewrite in rewrites {
        lines.push(format!("{} -> {}", rewrite.source_uri, rewrite.local_path));
    }
    lines.push(format!("executed_command: {rewritten_command}"));
    lines.join("\n")
}

fn vfs_uri_rewrite_details(
    original_command: &str,
    rewritten_command: &str,
    rewrites: &[MaterializationRewrite],
) -> serde_json::Value {
    serde_json::json!({
        "type": "vfs_uri_rewrite",
        "original_command": original_command,
        "executed_command": rewritten_command,
        "rewritten_command": rewritten_command,
        "rewrite_count": rewrites.len(),
        "rewrites": rewrites.iter().map(|rewrite| {
            serde_json::json!({
                "source_uri": rewrite.source_uri,
                "local_path": rewrite.local_path,
            })
        }).collect::<Vec<_>>(),
    })
}

fn shell_exec_result_text(
    original_command: &str,
    rewritten_command: &str,
    mount_id: &str,
    cwd: &str,
    exit_code: i32,
    merged_output: &str,
    has_rewrite: bool,
) -> String {
    if has_rewrite {
        format!(
            "command: {original_command}\nexecuted_command: {rewritten_command}\ncwd: {mount_id}://{cwd}\nexit_code: {exit_code}\n{merged_output}"
        )
    } else {
        format!(
            "command: {original_command}\ncwd: {mount_id}://{cwd}\nexit_code: {exit_code}\n{merged_output}"
        )
    }
}

fn shell_exec_result_details(
    original_command: &str,
    rewritten_command: &str,
    rewrites: &[MaterializationRewrite],
) -> Option<serde_json::Value> {
    (!rewrites.is_empty()).then(|| {
        serde_json::json!({
            "type": "shell_exec",
            "original_command": original_command,
            "executed_command": rewritten_command,
            "rewrite": vfs_uri_rewrite_details(original_command, rewritten_command, rewrites),
        })
    })
}

fn unresolved_vfs_uri_message(command: &str, vfs: &agentdash_spi::Vfs) -> Option<String> {
    let mut unresolved = unresolved_current_mount_uris(command, vfs);
    unresolved.extend(unresolved_reserved_vfs_uris(command));
    unresolved.sort();
    unresolved.dedup();
    if unresolved.is_empty() {
        return None;
    }

    Some(format!(
        "shell_exec 拒绝执行：命令中仍包含未物化的 VFS URI: {}。这类 URI 不能直接交给本机 shell 执行，否则会被当作普通路径/参数并可能超时；请确认当前 session VFS 包含对应 mount，且物化 rewrite 已在下发前成功。",
        unresolved.join(", ")
    ))
}

fn unresolved_current_mount_uris(command: &str, vfs: &agentdash_spi::Vfs) -> Vec<String> {
    let mount_ids = vfs
        .mounts
        .iter()
        .map(|mount| mount.id.clone())
        .collect::<Vec<_>>();
    find_mount_uri_candidates(command, &mount_ids)
        .into_iter()
        .map(|candidate| candidate.value)
        .collect()
}

fn unresolved_reserved_vfs_uris(command: &str) -> Vec<String> {
    const RESERVED_VFS_SCHEMES: &[&str] = &["skill-assets", "lifecycle"];
    let mount_ids = RESERVED_VFS_SCHEMES
        .iter()
        .map(|scheme| scheme.to_string())
        .collect::<Vec<_>>();
    find_mount_uri_candidates(command, &mount_ids)
        .into_iter()
        .map(|candidate| candidate.value)
        .collect()
}

#[cfg(test)]
mod fs_apply_patch_mutation_tests {
    use super::*;
    use agentdash_spi::{Mount, MountCapability, Vfs};

    fn vfs() -> Vfs {
        Vfs {
            mounts: vec![
                Mount {
                    id: "workspace".to_string(),
                    provider: crate::vfs::PROVIDER_RELAY_FS.to_string(),
                    backend_id: "local-dev-1".to_string(),
                    root_ref: "D:\\workspace".to_string(),
                    capabilities: vec![MountCapability::Write],
                    default_write: true,
                    display_name: "workspace".to_string(),
                    metadata: serde_json::Value::Null,
                },
                Mount {
                    id: "canvas".to_string(),
                    provider: crate::vfs::PROVIDER_INLINE_FS.to_string(),
                    backend_id: String::new(),
                    root_ref: "inline://canvas".to_string(),
                    capabilities: vec![MountCapability::Write],
                    default_write: false,
                    display_name: "canvas".to_string(),
                    metadata: serde_json::Value::Null,
                },
            ],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    #[test]
    fn apply_patch_mutation_keys_include_default_mount_and_move_target() {
        let keys = fs_apply_patch_mutation_keys(
            &vfs(),
            None,
            r#"*** Begin Patch
*** Update File: src/old.rs
*** Move to: src/new.rs
@@
 old
*** End Patch"#,
        )
        .expect("keys should parse");

        assert_eq!(
            keys,
            vec!["workspace://src/new.rs", "workspace://src/old.rs"]
        );
    }

    #[test]
    fn apply_patch_mutation_keys_preserve_explicit_mount_prefix() {
        let keys = fs_apply_patch_mutation_keys(
            &vfs(),
            Some("workspace"),
            r#"*** Begin Patch
*** Add File: canvas://src/view.tsx
+export const value = 1;
*** Delete File: src/old.rs
*** End Patch"#,
        )
        .expect("keys should parse");

        assert_eq!(
            keys,
            vec!["canvas://src/view.tsx", "workspace://src/old.rs"]
        );
    }
}

#[cfg(test)]
mod shell_exec_rewrite_tests {
    use super::*;
    use agentdash_spi::{Mount, Vfs};

    fn rewrite() -> MaterializationRewrite {
        MaterializationRewrite {
            source_uri: "skill-assets://skills/abc-user-lookup/scripts/lookup.py".to_string(),
            local_path: "C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py".to_string(),
        }
    }

    #[test]
    fn rewrite_notice_exposes_mapping_and_rewritten_command() {
        let rewrites = vec![rewrite()];
        let result = vfs_uri_rewrite_notice(
            "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
            "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao",
            &rewrites,
        );

        assert!(!result.is_error);
        let text = result.content[0].extract_text().expect("text content");
        assert!(text.contains("vfs_uri_rewrite"));
        assert!(text.contains("skill-assets://skills/abc-user-lookup/scripts/lookup.py"));
        assert!(text.contains("executed_command:"));
        let details = result.details.expect("details");
        assert_eq!(details["type"], "vfs_uri_rewrite");
        assert_eq!(
            details["executed_command"],
            "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao"
        );
        assert_eq!(details["rewrite_count"], 1);
        assert_eq!(
            details["rewrites"][0]["source_uri"],
            "skill-assets://skills/abc-user-lookup/scripts/lookup.py"
        );
    }

    #[test]
    fn shell_exec_result_shows_rewritten_command_only_when_rewritten() {
        let rewritten = shell_exec_result_text(
            "python skill-assets://skills/foo/scripts/run.py",
            "python \"C:\\agentdash\\materialized\\readonly\\skill-assets\\skills\\foo\\scripts\\run.py\"",
            "main",
            ".",
            0,
            "ok",
            true,
        );
        assert!(rewritten.contains("executed_command:"));

        let plain = shell_exec_result_text("echo ok", "echo ok", "main", ".", 0, "ok", false);
        assert!(!plain.contains("executed_command:"));
    }

    #[test]
    fn shell_exec_result_details_are_absent_without_rewrite() {
        assert!(shell_exec_result_details("echo ok", "echo ok", &[]).is_none());

        let rewrites = vec![rewrite()];
        let details = shell_exec_result_details(
            "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
            "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao",
            &rewrites,
        )
        .expect("rewrite details");
        assert_eq!(details["type"], "shell_exec");
        assert_eq!(
            details["executed_command"],
            "python \"C:\\Users\\yihao.liao\\AppData\\Local\\agentdash\\materialized\\readonly\\skill-assets\\skills\\abc-user-lookup\\scripts\\lookup.py\" yihao.liao"
        );
        assert_eq!(details["rewrite"]["type"], "vfs_uri_rewrite");
    }

    #[test]
    fn unresolved_vfs_uri_is_rejected_before_shell_execution() {
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: crate::vfs::PROVIDER_RELAY_FS.to_string(),
                backend_id: "local-dev-1".to_string(),
                root_ref: "D:\\workspace".to_string(),
                capabilities: vec![agentdash_spi::MountCapability::Exec],
                default_write: true,
                display_name: "main".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let message = unresolved_vfs_uri_message(
            "python skill-assets://skills/abc-user-lookup/scripts/lookup.py yihao.liao",
            &vfs,
        )
        .expect("unresolved VFS URI should be rejected");

        assert!(message.contains("未物化的 VFS URI"));
        assert!(message.contains("skill-assets://skills/abc-user-lookup/scripts/lookup.py"));
    }
}
