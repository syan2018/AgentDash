//! 工具 Schema 维度 — 追踪真正新增给 Agent 的工具 schema。
//!
//! 路径级的屏蔽 / 恢复 / 移除归 `ToolPathDelta`，此处不冗余。

use agentdash_agent_types::{DynAgentTool, ToolDefinition};
use agentdash_application_ports::mcp_discovery::DiscoveredMcpTool;
use agentdash_spi::context_usage_kind;
use agentdash_spi::hooks::{ContextFrameSection, RuntimeToolSchemaEntry};
use agentdash_spi::platform::tool_capability::{
    CAP_RELAY_MANAGEMENT, CAP_STORY_MANAGEMENT, CAP_WORKFLOW_MANAGEMENT, PlatformMcpScope,
    ToolDescriptor, ToolSource, platform_tool_descriptors,
};
use serde_json::Value;

use super::DimensionDelta;
use agentdash_spi::CapabilityStateDelta;

#[derive(Debug, Clone)]
pub(crate) struct ToolSchemaDimensionDelta {
    pub added_tools: Vec<RuntimeToolSchemaEntry>,
}

impl ToolSchemaDimensionDelta {
    pub fn from_schema_entries_and_state_delta(
        entries: &[RuntimeToolSchemaEntry],
        state_delta: Option<&CapabilityStateDelta>,
    ) -> Option<Box<dyn DimensionDelta>> {
        let state_delta = state_delta?;
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
        let added_or_changed_mcp_servers = state_delta
            .mcp_servers
            .added
            .iter()
            .chain(state_delta.mcp_servers.changed.iter())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();

        let added_tools = entries
            .iter()
            .filter(|entry| {
                entry
                    .capability_key
                    .as_ref()
                    .is_some_and(|capability| added_capabilities.contains(capability))
                    || entry
                        .tool_path
                        .as_ref()
                        .is_some_and(|path| restored_paths.contains(path))
                    || mcp_schema_matches_server(entry, &added_or_changed_mcp_servers)
            })
            .cloned()
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

pub(crate) fn runtime_tool_schema_entries_from_mcp_tools(
    entries: &[DiscoveredMcpTool],
) -> Vec<RuntimeToolSchemaEntry> {
    entries
        .iter()
        .map(|entry| {
            let metadata = metadata_for_tool(
                &entry.runtime_name,
                &tool_runtime_metadata_for_runtime_names(std::iter::once(
                    entry.runtime_name.as_str(),
                )),
            )
            .unwrap_or_else(|| mcp_runtime_metadata(entry));
            RuntimeToolSchemaEntry {
                name: entry.runtime_name.clone(),
                description: entry.description.clone(),
                parameters_schema: entry.parameters_schema.clone(),
                capability_key: Some(metadata.capability_key),
                context_usage_kind: Some(metadata.context_usage_kind.to_string()),
                source: Some(metadata.source),
                tool_path: Some(metadata.tool_path),
            }
        })
        .collect()
}

fn format_tool_schema_entry(entry: &RuntimeToolSchemaEntry) -> String {
    let description = entry.description.trim();

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
    lines.push("参数说明：".to_string());
    lines.extend(format_parameter_summary(&entry.parameters_schema));
    lines.join("\n\n")
}

fn format_parameter_summary(schema: &Value) -> Vec<String> {
    const MAX_FIELDS: usize = 48;
    const MAX_DEPTH: usize = 2;

    let mut lines = Vec::new();
    collect_schema_fields(schema, "", 0, MAX_DEPTH, MAX_FIELDS, &mut lines, &mut false);
    if lines.is_empty() {
        lines.push("- 无参数。".to_string());
    }
    lines
}

fn collect_schema_fields(
    schema: &Value,
    prefix: &str,
    depth: usize,
    max_depth: usize,
    max_fields: usize,
    lines: &mut Vec<String>,
    truncated: &mut bool,
) {
    if lines.len() >= max_fields {
        if !*truncated {
            lines.push(
                "- 其余嵌套字段已省略；完整机器 schema 已通过 provider tools 字段提供。"
                    .to_string(),
            );
            *truncated = true;
        }
        return;
    }

    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        if prefix.is_empty() {
            lines.push(format!("- 参数整体类型：{}", schema_type_summary(schema)));
        }
        return;
    };
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    let mut names = properties.keys().collect::<Vec<_>>();
    names.sort();

    for name in names {
        if lines.len() >= max_fields {
            if !*truncated {
                lines.push(
                    "- 其余嵌套字段已省略；完整机器 schema 已通过 provider tools 字段提供。"
                        .to_string(),
                );
                *truncated = true;
            }
            return;
        }

        let field_schema = &properties[name];
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        };
        let requirement = if required.contains(name.as_str()) {
            "required"
        } else {
            "optional"
        };
        let description = schema_description(field_schema);
        let suffix = if description.is_empty() {
            String::new()
        } else {
            format!(": {description}")
        };
        lines.push(format!(
            "- `{path}` ({requirement}, {}){suffix}",
            schema_type_summary(field_schema)
        ));

        if depth >= max_depth {
            continue;
        }

        if field_schema.get("properties").is_some() {
            collect_schema_fields(
                field_schema,
                &path,
                depth + 1,
                max_depth,
                max_fields,
                lines,
                truncated,
            );
        } else if let Some(items) = field_schema.get("items")
            && items.get("properties").is_some()
        {
            collect_schema_fields(
                items,
                &format!("{path}[]"),
                depth + 1,
                max_depth,
                max_fields,
                lines,
                truncated,
            );
        }
    }
}

fn schema_description(schema: &Value) -> String {
    schema
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            const MAX_DESCRIPTION_CHARS: usize = 140;
            let mut output = value
                .split('\n')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            if output.chars().count() > MAX_DESCRIPTION_CHARS {
                output = output
                    .chars()
                    .take(MAX_DESCRIPTION_CHARS)
                    .collect::<String>();
                output.push_str("...");
            }
            output
        })
        .unwrap_or_default()
}

fn schema_type_summary(schema: &Value) -> String {
    if let Some(any_of) = schema.get("anyOf").and_then(Value::as_array) {
        let mut variants = any_of.iter().map(schema_type_summary).collect::<Vec<_>>();
        variants.sort();
        variants.dedup();
        return variants.join(" | ");
    }

    let Some(schema_type) = schema.get("type") else {
        if schema.get("properties").is_some() {
            return "object".to_string();
        }
        if schema.get("items").is_some() {
            return "array".to_string();
        }
        if let Some(values) = schema.get("enum").and_then(Value::as_array) {
            return format!("enum{}", enum_values_summary(values));
        }
        return "any".to_string();
    };

    match schema_type {
        Value::String(value) if value == "array" => {
            let item = schema
                .get("items")
                .map(schema_type_summary)
                .unwrap_or_else(|| "any".to_string());
            format!("array<{item}>")
        }
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" | "),
        _ => "any".to_string(),
    }
}

fn enum_values_summary(values: &[Value]) -> String {
    let items = values
        .iter()
        .map(|value| match value {
            Value::String(text) => text.clone(),
            _ => value.to_string(),
        })
        .take(6)
        .collect::<Vec<_>>();
    format!("({})", items.join(" | "))
}

#[derive(Debug, Clone)]
struct ToolRuntimeMetadata {
    runtime_name: String,
    capability_key: String,
    source: String,
    tool_path: String,
    context_usage_kind: &'static str,
}

fn runtime_tool_schema_entries(definitions: Vec<ToolDefinition>) -> Vec<RuntimeToolSchemaEntry> {
    let metadata = tool_runtime_metadata(&definitions);
    definitions
        .into_iter()
        .map(|definition| {
            let metadata = metadata_for_tool(&definition.name, &metadata);
            let source = metadata.as_ref().map(|metadata| metadata.source.clone());
            RuntimeToolSchemaEntry {
                name: definition.name,
                description: definition.description,
                parameters_schema: definition.parameters,
                capability_key: metadata
                    .as_ref()
                    .map(|metadata| metadata.capability_key.clone()),
                context_usage_kind: metadata
                    .as_ref()
                    .map(|metadata| metadata.context_usage_kind.to_string()),
                source,
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
    tool_runtime_metadata_for_runtime_names(runtime_names)
}

fn tool_runtime_metadata_for_runtime_names<'a>(
    runtime_names: impl IntoIterator<Item = &'a str>,
) -> Vec<ToolRuntimeMetadata> {
    let runtime_names = runtime_names
        .into_iter()
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
                    context_usage_kind: tool_source_context_usage_kind(&descriptor.source),
                },
            )
        })
        .filter(|metadata| runtime_names.contains(metadata.runtime_name.as_str()))
        .collect()
}

fn mcp_runtime_metadata(entry: &DiscoveredMcpTool) -> ToolRuntimeMetadata {
    let server_name = agent_facing_mcp_server_name(&entry.server_name);
    let capability_key = capability_key_for_mcp_server_name(&server_name);
    ToolRuntimeMetadata {
        runtime_name: entry.runtime_name.clone(),
        capability_key: capability_key.clone(),
        source: format!("mcp:{server_name}"),
        tool_path: format!("{capability_key}::{}", entry.tool_name),
        context_usage_kind: context_usage_kind::MCP_TOOLS,
    }
}

fn mcp_schema_matches_server(
    entry: &RuntimeToolSchemaEntry,
    servers: &std::collections::BTreeSet<String>,
) -> bool {
    if servers.is_empty() {
        return false;
    }
    if entry.context_usage_kind.as_deref() == Some(context_usage_kind::MCP_TOOLS)
        && entry.capability_key.as_ref().is_some_and(|capability| {
            servers
                .iter()
                .any(|server| capability_key_for_mcp_server_name(server) == *capability)
        })
    {
        return true;
    }
    let Some(source) = entry.source.as_deref() else {
        return false;
    };
    let Some(server_name) = source.strip_prefix("mcp:") else {
        return false;
    };
    servers.contains(server_name)
        || servers
            .iter()
            .any(|server| agent_facing_mcp_server_name(server) == server_name)
}

fn tool_source_context_usage_kind(source: &ToolSource) -> &'static str {
    match source {
        ToolSource::Mcp { .. } | ToolSource::PlatformMcp { .. } => context_usage_kind::MCP_TOOLS,
        ToolSource::Platform { .. } => context_usage_kind::SYSTEM_TOOLS,
    }
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
        PlatformMcpScope::Workflow => "agentdash-workflow-tools",
    };
    format!(
        "mcp_{}_{}",
        sanitize_identifier(server_name),
        sanitize_identifier(tool_name)
    )
}

fn capability_key_for_mcp_server_name(server_name: &str) -> String {
    match agent_facing_mcp_server_name(server_name).as_str() {
        "agentdash-relay-tools" => CAP_RELAY_MANAGEMENT.to_string(),
        "agentdash-story-tools" => CAP_STORY_MANAGEMENT.to_string(),
        "agentdash-workflow-tools" => CAP_WORKFLOW_MANAGEMENT.to_string(),
        other => format!("mcp:{other}"),
    }
}

fn agent_facing_mcp_server_name(server_name: &str) -> String {
    const PLATFORM_SCOPED_PREFIXES: &[(&str, &str)] = &[
        ("agentdash-story-tools-", "agentdash-story-tools"),
        ("agentdash-workflow-tools-", "agentdash-workflow-tools"),
    ];

    for (prefix, stable_name) in PLATFORM_SCOPED_PREFIXES {
        if server_name.starts_with(prefix) {
            return (*stable_name).to_string();
        }
    }

    server_name.to_string()
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
        agentdash_spi::ToolCluster::Task => "task",
        agentdash_spi::ToolCluster::WorkspaceModule => "workspace_module",
    }
}

fn platform_mcp_scope_key(scope: PlatformMcpScope) -> &'static str {
    match scope {
        PlatformMcpScope::Relay => "relay",
        PlatformMcpScope::Story => "story",
        PlatformMcpScope::Workflow => "workflow",
    }
}

#[cfg(test)]
mod tests {
    use agentdash_spi::hooks::ContextFrameSection;

    use super::*;

    #[test]
    fn platform_mcp_schema_matches_changed_runtime_server_by_capability_key() {
        let state_delta = CapabilityStateDelta {
            mcp_servers: agentdash_spi::NamedEntityDelta {
                added: Vec::new(),
                removed: Vec::new(),
                changed: vec!["agentdash-workflow-tools-8de613e7".to_string()],
            },
            ..CapabilityStateDelta::default()
        };
        let entries = vec![RuntimeToolSchemaEntry {
            name: "mcp_agentdash_workflow_tools_get_lifecycle".to_string(),
            description: "读取 Lifecycle".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" }
                }
            }),
            capability_key: Some(CAP_WORKFLOW_MANAGEMENT.to_string()),
            source: Some("platform_mcp:workflow".to_string()),
            tool_path: Some("workflow_management::get_lifecycle".to_string()),
            context_usage_kind: Some(context_usage_kind::MCP_TOOLS.to_string()),
        }];

        let delta = ToolSchemaDimensionDelta::from_schema_entries_and_state_delta(
            &entries,
            Some(&state_delta),
        )
        .expect("platform MCP schema should match changed server");

        match delta.to_section() {
            ContextFrameSection::ToolSchemaDelta { added_tools } => {
                assert_eq!(added_tools.len(), 1);
                assert_eq!(
                    added_tools[0].source.as_deref(),
                    Some("platform_mcp:workflow")
                );
            }
            other => panic!("unexpected section: {other:?}"),
        }
    }
}
