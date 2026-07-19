use std::sync::Arc;

use agentdash_platform_spi::{
    AgentTool, AgentToolError, AgentToolResult, MountCapability, RuntimeVfsAccessPolicy,
    RuntimeVfsOperation, ToolUpdateCallback,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::runtime_tool_execution::{VfsToolExecutionError, VfsToolExecutionResult};
use crate::{VfsService, capability_name};

use super::common::SharedRuntimeVfs;
use super::{legacy_error, legacy_result};

// ---------------------------------------------------------------------------
// mounts_list
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MountsListExecutor {
    service: Arc<VfsService>,
    vfs: SharedRuntimeVfs,
}

impl MountsListExecutor {
    pub fn new(service: Arc<VfsService>, vfs: SharedRuntimeVfs) -> Self {
        Self { service, vfs }
    }

    pub async fn execute(
        &self,
        _: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<VfsToolExecutionResult, VfsToolExecutionError> {
        if cancel.is_cancelled() {
            return Err(VfsToolExecutionError::Cancelled);
        }
        let state = self.vfs.snapshot_state().await;
        let mounts = self.service.list_mounts(&state.vfs);
        let body = mounts
            .iter()
            .map(|mount| {
                let capabilities = mount
                    .capabilities
                    .iter()
                    .copied()
                    .filter_map(|capability| {
                        capability_label_by_policy(&state.access_policy, &mount.id, capability)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "- {}:// — {} (capabilities=[{}])",
                    mount.id, mount.display_name, capabilities
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(VfsToolExecutionResult::text(if body.is_empty() {
            "No mounts available in the current session.".to_string()
        } else {
            format!(
                "Path format: mount_id://relative/path (prefix may be omitted when only one mount exists)\n\n{}",
                body
            )
        }))
    }
}

#[derive(Clone)]
pub struct MountsListTool {
    executor: MountsListExecutor,
}

impl MountsListTool {
    pub fn new(service: Arc<VfsService>, vfs: SharedRuntimeVfs) -> Self {
        Self {
            executor: MountsListExecutor::new(service, vfs),
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
    fn protocol_projector(&self) -> Option<agentdash_agent_types::ToolProtocolProjector> {
        Some(agentdash_agent_types::ToolProtocolProjector::Dynamic { namespace: None })
    }
    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_vfs_mounts_dynamic_lifecycle".to_string())
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

fn capability_label_by_policy(
    policy: &RuntimeVfsAccessPolicy,
    mount_id: &str,
    capability: MountCapability,
) -> Option<String> {
    let operation = match capability {
        MountCapability::Read => RuntimeVfsOperation::Read,
        MountCapability::List => RuntimeVfsOperation::List,
        MountCapability::Search => RuntimeVfsOperation::Search,
        MountCapability::Write => RuntimeVfsOperation::Write,
        MountCapability::Exec => RuntimeVfsOperation::Exec,
        MountCapability::Watch => return None,
    };
    let name = capability_name(&capability);
    let mut has_scoped_rule = false;
    for rule in &policy.rules {
        if rule.mount_id != mount_id || !rule.operations.contains(&operation) {
            continue;
        }
        if rule.path_pattern.matches_normalized_path("") {
            return Some(name.to_string());
        }
        has_scoped_rule = true;
    }
    has_scoped_rule.then(|| format!("{name}(scoped)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{collections::BTreeSet, sync::Arc};

    use agentdash_platform_spi::{
        AgentTool, Mount, RuntimeVfsAccessRule, RuntimeVfsAccessSource, RuntimeVfsPathPattern, Vfs,
    };

    use crate::MountProviderRegistry;

    fn mount() -> Mount {
        Mount {
            id: "main".to_string(),
            provider: "memory".to_string(),
            backend_id: String::new(),
            root_ref: "memory://main".to_string(),
            capabilities: vec![
                MountCapability::Read,
                MountCapability::List,
                MountCapability::Search,
            ],
            default_write: false,
            display_name: "Main".to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn mounts_list_reports_runtime_effective_capabilities() {
        let vfs = Vfs {
            mounts: vec![mount()],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let policy = RuntimeVfsAccessPolicy {
            rules: vec![RuntimeVfsAccessRule {
                mount_id: "main".to_string(),
                path_pattern: RuntimeVfsPathPattern::All,
                operations: BTreeSet::from([RuntimeVfsOperation::Read]),
                source: RuntimeVfsAccessSource::ProjectPreset,
            }],
        };
        let tool = MountsListTool::new(
            Arc::new(VfsService::new(Arc::new(MountProviderRegistry::default()))),
            SharedRuntimeVfs::new_with_policy(vfs, policy),
        );

        let result = tool
            .execute(
                "call-1",
                serde_json::json!({}),
                tokio_util::sync::CancellationToken::new(),
                None,
            )
            .await
            .expect("mounts_list should succeed");
        let text = result.content[0].extract_text().expect("text result");

        assert!(text.contains("capabilities=[read]"));
        assert!(!text.contains("list"));
        assert!(!text.contains("search"));
    }

    #[tokio::test]
    async fn mounts_list_reports_path_scoped_capabilities() {
        let vfs = Vfs {
            mounts: vec![mount()],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let policy = RuntimeVfsAccessPolicy {
            rules: vec![RuntimeVfsAccessRule {
                mount_id: "main".to_string(),
                path_pattern: RuntimeVfsPathPattern::Prefix("docs".to_string()),
                operations: BTreeSet::from([RuntimeVfsOperation::Read]),
                source: RuntimeVfsAccessSource::ProjectPreset,
            }],
        };
        let tool = MountsListTool::new(
            Arc::new(VfsService::new(Arc::new(MountProviderRegistry::default()))),
            SharedRuntimeVfs::new_with_policy(vfs, policy),
        );

        let result = tool
            .execute(
                "call-1",
                serde_json::json!({}),
                tokio_util::sync::CancellationToken::new(),
                None,
            )
            .await
            .expect("mounts_list should succeed");
        let text = result.content[0].extract_text().expect("text result");

        assert!(text.contains("capabilities=[read(scoped)]"));
    }
}
