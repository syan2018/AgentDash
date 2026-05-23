use std::sync::Arc;

use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::vfs::ListOptions;
use crate::vfs::inline_persistence::InlineContentOverlay;
use crate::vfs::relay_service::RelayVfsService;
use crate::vfs::tools::common::{SharedRuntimeVfs, ok_text, resolve_uri_path};

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
