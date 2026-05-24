use std::sync::Arc;

use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::vfs::inline_persistence::InlineContentOverlay;
use crate::vfs::relay_service::RelayVfsService;
use crate::vfs::tools::common::{SharedRuntimeVfs, ok_text, resolve_uri_path};

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
