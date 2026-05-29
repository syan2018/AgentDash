use std::sync::Arc;

use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ToolUpdateCallback};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::vfs::{VfsService, capability_name};

use super::common::{SharedRuntimeVfs, ok_text};

// ---------------------------------------------------------------------------
// mounts_list
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MountsListTool {
    service: Arc<VfsService>,
    vfs: SharedRuntimeVfs,
}

impl MountsListTool {
    pub fn new(service: Arc<VfsService>, vfs: SharedRuntimeVfs) -> Self {
        Self { service, vfs }
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
        let vfs = self.vfs.snapshot().await;
        let mounts = self.service.list_mounts(&vfs);
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
