use std::sync::Arc;

use agentdash_spi::AddressSpace;
use agentdash_spi::schema::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::address_space::build_canvas_mount;
use crate::address_space::inline_persistence::InlineContentOverlay;
use crate::address_space::relay_service::RelayAddressSpaceService;
use crate::address_space::{
    ExecRequest, ListOptions, ResourceRef, capability_name, parse_mount_uri,
};

/// Resolve a tool parameter path into a `ResourceRef`.
///
/// Rules:
/// 1. Contains `://` -> split into mount_id and relative path by URI syntax
/// 2. No `://` and the address space has exactly one mount -> use that mount implicitly
/// 3. Otherwise -> error, require explicit mount prefix
pub fn resolve_uri_path(address_space: &AddressSpace, path: &str) -> Result<ResourceRef, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path must not be empty".to_string());
    }

    if trimmed.contains("://") {
        return parse_mount_uri(trimmed, address_space);
    }

    if address_space.mounts.len() == 1 {
        let mount_id = address_space.mounts[0].id.clone();
        return Ok(ResourceRef {
            mount_id,
            path: trimmed.to_string(),
        });
    }

    Err(format!(
        "path `{trimmed}` is missing a mount prefix (format: mount_id://path). \
        The current session has {} mount(s); call mounts_list to see available mounts, \
        then use a fully qualified URI.",
        address_space.mounts.len(),
    ))
}

#[derive(Clone)]
pub struct SharedRuntimeAddressSpace {
    inner: Arc<RwLock<AddressSpace>>,
}

impl SharedRuntimeAddressSpace {
    pub fn new(address_space: AddressSpace) -> Self {
        Self {
            inner: Arc::new(RwLock::new(address_space)),
        }
    }

    pub async fn snapshot(&self) -> AddressSpace {
        self.inner.read().await.clone()
    }

    pub async fn append_canvas_mount(&self, canvas: &agentdash_domain::canvas::Canvas) {
        let mut guard = self.inner.write().await;
        let mount = build_canvas_mount(canvas);
        guard.mounts.retain(|existing| existing.id != mount.id);
        guard.mounts.push(mount);
    }
}

pub fn ok_text(text: String) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(text)],
        is_error: false,
        details: None,
    }
}

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
// mounts_list
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MountsListTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: SharedRuntimeAddressSpace,
}

impl MountsListTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: SharedRuntimeAddressSpace,
    ) -> Self {
        Self {
            service,
            address_space,
        }
    }
}

#[async_trait]
impl AgentTool for MountsListTool {
    fn name(&self) -> &str {
        "mounts_list"
    }
    fn description(&self) -> &str {
        "List all available mounts and their capabilities in the current session.\n\
         \n\
         Usage:\n\
         - Call this tool first to discover which mounts (file systems) are accessible.\n\
         - Each mount exposes a set of capabilities (read, write, exec, etc.).\n\
         - Use the returned mount IDs as prefixes in paths for other tools (e.g., `main://src/lib.rs`).\n\
         - If only one mount exists, the prefix can be omitted in other tool calls."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {}, "required": [], "additionalProperties": false })
    }
    async fn execute(
        &self,
        _: &str,
        _: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let address_space = self.address_space.snapshot().await;
        let mounts = self.service.list_mounts(&address_space);
        let body = mounts
            .iter()
            .map(|mount| {
                let capabilities = mount
                    .capabilities
                    .iter()
                    .map(capability_name)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "- {}:// — {} (capabilities=[{}])",
                    mount.id, mount.display_name, capabilities
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ok_text(if body.is_empty() {
            "No mounts available in the current session.".to_string()
        } else {
            format!(
                "Path format: mount_id://relative/path (prefix may be omitted when only one mount exists)\n\n{}",
                body
            )
        }))
    }
}

// ---------------------------------------------------------------------------
// fs_read
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FsReadTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: SharedRuntimeAddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::auth::AuthIdentity>,
}
impl FsReadTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: SharedRuntimeAddressSpace,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_spi::auth::AuthIdentity>,
    ) -> Self {
        Self {
            service,
            address_space,
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
        "Read the contents of a text file from a mount.\n\
         \n\
         Usage:\n\
         - The path parameter uses `mount_id://relative/path` format (e.g., `main://src/lib.rs`).\n\
         - When the session has only one mount, the prefix may be omitted.\n\
         - By default reads the entire file; use start_line / end_line for partial reads.\n\
         - Results are returned with numbered lines in `cat -n` style (line_number | content).\n\
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
        let address_space = self.address_space.snapshot().await;
        let target = resolve_uri_path(&address_space, &params.path)
            .map_err(AgentToolError::ExecutionFailed)?;
        let result = self
            .service
            .read_text(
                &address_space,
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

// ---------------------------------------------------------------------------
// fs_apply_patch
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FsApplyPatchTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: SharedRuntimeAddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::auth::AuthIdentity>,
}
impl FsApplyPatchTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: SharedRuntimeAddressSpace,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_spi::auth::AuthIdentity>,
    ) -> Self {
        Self {
            service,
            address_space,
            overlay,
            identity,
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
        let address_space = self.address_space.snapshot().await;
        let result = self
            .service
            .apply_patch_multi(
                &address_space,
                params.mount.as_deref(),
                &params.patch,
                self.overlay.as_ref().map(|arc| arc.as_ref()),
                self.identity.as_ref(),
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

// ---------------------------------------------------------------------------
// fs_glob  (formerly fs_list)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FsGlobTool {
    service: Arc<RelayAddressSpaceService>,
    address_space: SharedRuntimeAddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::auth::AuthIdentity>,
}
impl FsGlobTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: SharedRuntimeAddressSpace,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_spi::auth::AuthIdentity>,
    ) -> Self {
        Self {
            service,
            address_space,
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
        let address_space = self.address_space.snapshot().await;
        let target = resolve_uri_path(&address_space, params.path.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let result = self
            .service
            .list(
                &address_space,
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
    service: Arc<RelayAddressSpaceService>,
    address_space: SharedRuntimeAddressSpace,
    overlay: Option<Arc<InlineContentOverlay>>,
}
impl FsGrepTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: SharedRuntimeAddressSpace,
        overlay: Option<Arc<InlineContentOverlay>>,
        _identity: Option<agentdash_spi::auth::AuthIdentity>,
    ) -> Self {
        Self {
            service,
            address_space,
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
        let address_space = self.address_space.snapshot().await;
        let target = resolve_uri_path(&address_space, params.path.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let search_path = if target.path.is_empty() {
            ".".to_string()
        } else {
            target.path
        };
        let (hits, truncated) = self
            .service
            .search_text_extended(
                &address_space,
                &crate::address_space::TextSearchParams {
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
    service: Arc<RelayAddressSpaceService>,
    address_space: SharedRuntimeAddressSpace,
}
impl ShellExecTool {
    pub fn new(
        service: Arc<RelayAddressSpaceService>,
        address_space: SharedRuntimeAddressSpace,
    ) -> Self {
        Self {
            service,
            address_space,
        }
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
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: ShellExecParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;
        let address_space = self.address_space.snapshot().await;
        let target = resolve_uri_path(&address_space, params.cwd.as_deref().unwrap_or("."))
            .map_err(AgentToolError::ExecutionFailed)?;
        let cwd = if target.path.is_empty() {
            ".".to_string()
        } else {
            target.path
        };
        let result = self
            .service
            .exec(
                &address_space,
                &ExecRequest {
                    mount_id: target.mount_id.clone(),
                    cwd: cwd.clone(),
                    command: params.command.clone(),
                    timeout_ms: params.timeout_secs.map(|s| s.saturating_mul(1000)),
                },
            )
            .await
            .map_err(AgentToolError::ExecutionFailed)?;
        let merged = if result.stderr.trim().is_empty() {
            result.stdout
        } else if result.stdout.trim().is_empty() {
            format!("[stderr]\n{}", result.stderr)
        } else {
            format!("[stdout]\n{}\n\n[stderr]\n{}", result.stdout, result.stderr)
        };
        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "command: {}\ncwd: {}://{}\nexit_code: {}\n{}",
                params.command, target.mount_id, cwd, result.exit_code, merged
            ))],
            is_error: result.exit_code != 0,
            details: None,
        })
    }
}
