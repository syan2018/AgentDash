use agentdash_agent_protocol::ContextFrameSection;

use super::ProjectedSurfaceDimension;
use crate::context_projection::surface_state::NormalizedContextSurfaceDelta;

pub(super) fn project(
    delta: &NormalizedContextSurfaceDelta,
    phase_node: Option<&str>,
) -> Option<ProjectedSurfaceDimension> {
    if delta.mcp_servers.is_empty()
        && delta.unavailable_mcp_servers.is_empty()
        && !delta.mcp_server_readiness_changed
    {
        return None;
    }
    let mut lines = vec![super::surface_update_heading(
        "MCP Server Changes",
        phase_node,
    )];
    append_lines(
        &mut lines,
        "Added MCP servers",
        &delta.mcp_servers.added,
        "已注入",
    );
    append_lines(
        &mut lines,
        "Removed MCP servers",
        &delta.mcp_servers.removed,
        "已移除",
    );
    append_lines(
        &mut lines,
        "Changed MCP servers",
        &delta.mcp_servers.changed,
        "已变更",
    );
    if !delta.unavailable_mcp_servers.is_empty() {
        lines.push(String::new());
        lines.push("### Unavailable MCP servers (connection failed)".to_string());
        lines.push("The following MCP tool sources failed to connect. Their tools are NOT available this session. If the user's request requires these tools, inform them that the MCP environment needs to be fixed before proceeding.".to_string());
        lines.extend(delta.unavailable_mcp_servers.iter().map(|source| {
            format!(
                "- `{}` — `{}`: {}",
                source.name, source.reason_code, source.message
            )
        }));
    }
    Some(ProjectedSurfaceDimension {
        section: ContextFrameSection::McpServerDelta {
            added_mcp_servers: delta.mcp_servers.added.clone(),
            removed_mcp_servers: delta.mcp_servers.removed.clone(),
            changed_mcp_servers: delta.mcp_servers.changed.clone(),
        },
        rendered_text: lines.join("\n"),
    })
}

fn append_lines(lines: &mut Vec<String>, title: &str, values: &[String], suffix: &str) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("- {title}:"));
    lines.extend(
        values
            .iter()
            .map(|value| format!("  - `{value}` — {suffix}")),
    );
}
