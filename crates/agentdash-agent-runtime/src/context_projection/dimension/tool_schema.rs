use std::collections::BTreeSet;

use agentdash_agent_protocol::{ContextFrameSection, RuntimeToolSchemaEntry};

use super::ProjectedSurfaceDimension;
use crate::context_projection::surface_state::{
    NormalizedContextSurfaceDelta, NormalizedContextSurfaceState,
};

pub(super) fn project(
    delta: &NormalizedContextSurfaceDelta,
    target: &NormalizedContextSurfaceState,
    phase_node: Option<&str>,
) -> Option<ProjectedSurfaceDimension> {
    let added_capabilities = delta
        .capability_keys
        .added
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let restored_paths = delta
        .excluded_tool_paths
        .removed
        .iter()
        .chain(delta.included_tool_paths.added.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let added_or_changed_mcp_servers = delta
        .mcp_servers
        .added
        .iter()
        .chain(delta.mcp_servers.changed.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let directly_added_or_changed = delta
        .tool_schemas
        .added
        .iter()
        .chain(delta.tool_schemas.changed.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let added_tools = target
        .tool_schemas
        .iter()
        .filter(|(name, entry)| {
            directly_added_or_changed.contains(*name)
                || entry
                    .capability_key
                    .as_ref()
                    .is_some_and(|capability| added_capabilities.contains(capability))
                || entry
                    .tool_path
                    .as_ref()
                    .is_some_and(|path| restored_paths.contains(path))
                || mcp_schema_matches_server(entry, &added_or_changed_mcp_servers)
        })
        .map(|(_, entry)| entry)
        .cloned()
        .collect::<Vec<_>>();
    if added_tools.is_empty() {
        return None;
    }
    Some(ProjectedSurfaceDimension {
        section: ContextFrameSection::ToolSchemaDelta {
            added_tools: added_tools.clone(),
        },
        rendered_text: crate::context_projection::artifact::render_tool_schema_delta(
            phase_node,
            &added_tools,
        ),
    })
}

fn mcp_schema_matches_server(
    entry: &RuntimeToolSchemaEntry,
    server_names: &BTreeSet<String>,
) -> bool {
    let source_matches = entry.source.as_deref().is_some_and(|source| {
        let Some(source_server) = source.strip_prefix("mcp:") else {
            return false;
        };
        let source_server = agent_facing_mcp_server_name(source_server);
        server_names
            .iter()
            .any(|server| agent_facing_mcp_server_name(server) == source_server)
    });
    source_matches
        || entry.capability_key.as_ref().is_some_and(|capability| {
            server_names
                .iter()
                .any(|server| capability_key_for_mcp_server_name(server) == *capability)
        })
}

fn capability_key_for_mcp_server_name(server_name: &str) -> String {
    match agent_facing_mcp_server_name(server_name).as_str() {
        "agentdash-relay-tools" => "relay_management".to_string(),
        "agentdash-story-tools" => "story_management".to_string(),
        "agentdash-workflow-tools" => "workflow_management".to_string(),
        other => format!("mcp:{other}"),
    }
}

fn agent_facing_mcp_server_name(server_name: &str) -> String {
    for (prefix, stable) in [
        ("agentdash-story-tools-", "agentdash-story-tools"),
        ("agentdash-workflow-tools-", "agentdash-workflow-tools"),
    ] {
        if server_name.starts_with(prefix) {
            return stable.to_string();
        }
    }
    server_name.to_string()
}
