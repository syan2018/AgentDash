//! 工具 Schema 维度 — 追踪真正新增给 Agent 的工具 schema。
//!
//! 路径级的屏蔽 / 恢复 / 移除归 `ToolPathDelta`，此处不冗余。

use agentdash_agent_types::{DynAgentTool, ToolDefinition};
use agentdash_spi::hooks::{ContextFrameSection, RuntimeToolSchemaEntry};
use agentdash_spi::platform::tool_capability::{
    PlatformMcpScope, ToolDescriptor, ToolSource, platform_tool_descriptors,
};

use super::DimensionDelta;
use crate::session::CapabilityStateDelta;

#[derive(Debug, Clone)]
pub(crate) struct ToolSchemaDimensionDelta {
    pub added_tools: Vec<RuntimeToolSchemaEntry>,
}

impl ToolSchemaDimensionDelta {
    pub fn from_tools_and_state_delta(
        tools: &[DynAgentTool],
        state_delta: Option<&CapabilityStateDelta>,
    ) -> Option<Box<dyn DimensionDelta>> {
        let state_delta = state_delta?;
        let entries = runtime_tool_schema_entries_from_tools(tools);
        let restored_paths = state_delta
            .excluded_tool_paths
            .removed
            .iter()
            .chain(state_delta.included_tool_paths.added.iter())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let added_capabilities = state_delta
            .tool_capabilities
            .added
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();

        let added_tools = entries
            .into_iter()
            .filter(|entry| {
                entry
                    .capability_key
                    .as_ref()
                    .is_some_and(|capability| added_capabilities.contains(capability))
                    || entry
                        .tool_path
                        .as_ref()
                        .is_some_and(|path| restored_paths.contains(path))
            })
            .collect::<Vec<_>>();

        if added_tools.is_empty() {
            return None;
        }
        Some(Box::new(Self { added_tools }))
    }
}

impl DimensionDelta for ToolSchemaDimensionDelta {
    fn has_changes(&self) -> bool {
        !self.added_tools.is_empty()
    }

    fn to_section(&self) -> ContextFrameSection {
        ContextFrameSection::ToolSchemaDelta {
            added_tools: self.added_tools.clone(),
        }
    }

    fn render_text(&self, phase_node: Option<&str>) -> String {
        let mut lines = vec![
            match phase_node {
                Some(node) => format!("## Tool Schema Delta — Step Transition: {node}"),
                None => "## Tool Schema Delta".to_string(),
            },
            "以下只列出本次 capability state delta 真正新增给 Agent 的工具 schema；provider 的完整工具集合以实际 tool list 为准。".to_string(),
        ];
        if !self.added_tools.is_empty() {
            lines.push("### Added / Restored Tool Schemas".to_string());
            for tool in &self.added_tools {
                lines.push(format_tool_schema_entry(tool));
            }
        }
        lines.join("\n\n")
    }
}

// ── 工具 schema 构造辅助 ──────────────────────────────────────────────────────

pub(crate) fn runtime_tool_schema_entries_from_tools(
    tools: &[DynAgentTool],
) -> Vec<RuntimeToolSchemaEntry> {
    if tools.is_empty() {
        return Vec::new();
    }
    let mut definitions = tools
        .iter()
        .map(|tool| ToolDefinition::from_tool(tool.as_ref()))
        .collect::<Vec<_>>();
    definitions.sort_by(|left, right| left.name.cmp(&right.name));
    definitions.dedup_by(|left, right| left.name == right.name);
    runtime_tool_schema_entries(definitions)
}

fn format_tool_schema_entry(entry: &RuntimeToolSchemaEntry) -> String {
    let description = entry.description.trim();
    let parameters = serde_json::to_string_pretty(&entry.parameters_schema)
        .unwrap_or_else(|_| entry.parameters_schema.to_string());

    let mut lines = vec![format!("### `{}`", entry.name)];
    let mut meta = Vec::new();
    if let Some(capability_key) = entry.capability_key.as_deref() {
        meta.push(format!("capability: `{capability_key}`"));
    }
    if let Some(source) = entry.source.as_deref() {
        meta.push(format!("source: `{source}`"));
    }
    if let Some(tool_path) = entry.tool_path.as_deref() {
        meta.push(format!("path: `{tool_path}`"));
    }
    if !meta.is_empty() {
        lines.push(meta.join("；"));
    }
    if !description.is_empty() {
        lines.push(description.to_string());
    }
    lines.push("参数 schema：".to_string());
    lines.push(format!("```json\n{parameters}\n```"));
    lines.join("\n\n")
}

#[derive(Debug, Clone)]
struct ToolRuntimeMetadata {
    runtime_name: String,
    capability_key: String,
    source: String,
    tool_path: String,
}

fn runtime_tool_schema_entries(definitions: Vec<ToolDefinition>) -> Vec<RuntimeToolSchemaEntry> {
    let metadata = tool_runtime_metadata(&definitions);
    definitions
        .into_iter()
        .map(|definition| {
            let metadata = metadata_for_tool(&definition.name, &metadata);
            RuntimeToolSchemaEntry {
                name: definition.name,
                description: definition.description,
                parameters_schema: definition.parameters,
                capability_key: metadata
                    .as_ref()
                    .map(|metadata| metadata.capability_key.clone()),
                source: metadata.as_ref().map(|metadata| metadata.source.clone()),
                tool_path: metadata.as_ref().map(|metadata| metadata.tool_path.clone()),
            }
        })
        .collect()
}

fn tool_runtime_metadata(definitions: &[ToolDefinition]) -> Vec<ToolRuntimeMetadata> {
    let runtime_names = definitions
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    platform_tool_descriptors()
        .into_iter()
        .flat_map(|descriptor| {
            runtime_names_for_descriptor(&descriptor).into_iter().map(
                move |(runtime_name, tool_path)| ToolRuntimeMetadata {
                    runtime_name,
                    capability_key: descriptor.capability_key.clone(),
                    source: format_tool_source(&descriptor.source),
                    tool_path,
                },
            )
        })
        .filter(|metadata| runtime_names.contains(metadata.runtime_name.as_str()))
        .collect()
}

fn metadata_for_tool(
    tool_name: &str,
    metadata: &[ToolRuntimeMetadata],
) -> Option<ToolRuntimeMetadata> {
    metadata
        .iter()
        .find(|candidate| candidate.runtime_name == tool_name)
        .cloned()
}

fn runtime_names_for_descriptor(descriptor: &ToolDescriptor) -> Vec<(String, String)> {
    let tool_path = format!("{}::{}", descriptor.capability_key, descriptor.name);
    match descriptor.source {
        ToolSource::Platform { .. } => vec![(descriptor.name.clone(), tool_path)],
        ToolSource::PlatformMcp { scope } => {
            vec![(
                platform_mcp_runtime_name(scope, &descriptor.name),
                tool_path,
            )]
        }
        ToolSource::Mcp { .. } => vec![(descriptor.name.clone(), tool_path)],
    }
}

fn platform_mcp_runtime_name(scope: PlatformMcpScope, tool_name: &str) -> String {
    let server_name = match scope {
        PlatformMcpScope::Relay => "agentdash-relay-tools",
        PlatformMcpScope::Story => "agentdash-story-tools",
        PlatformMcpScope::Task => "agentdash-task-tools",
        PlatformMcpScope::Workflow => "agentdash-workflow-tools",
    };
    format!(
        "mcp_{}_{}",
        sanitize_identifier(server_name),
        sanitize_identifier(tool_name)
    )
}

fn sanitize_identifier(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('_').to_string()
}

fn format_tool_source(source: &ToolSource) -> String {
    match source {
        ToolSource::Platform { cluster } => format!("platform:{}", tool_cluster_key(*cluster)),
        ToolSource::PlatformMcp { scope } => {
            format!("platform_mcp:{}", platform_mcp_scope_key(*scope))
        }
        ToolSource::Mcp { server_name } => format!("mcp:{server_name}"),
    }
}

fn tool_cluster_key(cluster: agentdash_spi::ToolCluster) -> &'static str {
    match cluster {
        agentdash_spi::ToolCluster::Read => "read",
        agentdash_spi::ToolCluster::Write => "write",
        agentdash_spi::ToolCluster::Execute => "execute",
        agentdash_spi::ToolCluster::Workflow => "workflow",
        agentdash_spi::ToolCluster::Collaboration => "collaboration",
        agentdash_spi::ToolCluster::Canvas => "canvas",
    }
}

fn platform_mcp_scope_key(scope: PlatformMcpScope) -> &'static str {
    match scope {
        PlatformMcpScope::Relay => "relay",
        PlatformMcpScope::Story => "story",
        PlatformMcpScope::Task => "task",
        PlatformMcpScope::Workflow => "workflow",
    }
}
