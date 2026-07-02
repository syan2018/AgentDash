//! MCP Server 维度 — 追踪 MCP server 的增删与变更。

use agentdash_spi::hooks::ContextFrameSection;

use super::DimensionDelta;
use agentdash_spi::CapabilityStateDelta;

#[derive(Debug, Clone)]
pub(crate) struct McpServerDimensionDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub unavailable: Vec<String>,
}

impl McpServerDimensionDelta {
    pub fn from_state_delta(
        state_delta: Option<&CapabilityStateDelta>,
    ) -> Option<Box<dyn DimensionDelta>> {
        let state_delta = state_delta?;
        let delta = Self {
            added: state_delta.mcp_servers.added.clone(),
            removed: state_delta.mcp_servers.removed.clone(),
            changed: state_delta.mcp_servers.changed.clone(),
            unavailable: state_delta.unavailable_mcp_servers.clone(),
        };
        if !delta.has_changes() {
            return None;
        }
        Some(Box::new(delta))
    }
}

impl DimensionDelta for McpServerDimensionDelta {
    fn has_changes(&self) -> bool {
        !self.added.is_empty()
            || !self.removed.is_empty()
            || !self.changed.is_empty()
            || !self.unavailable.is_empty()
    }

    fn to_section(&self) -> ContextFrameSection {
        ContextFrameSection::McpServerDelta {
            added_mcp_servers: self.added.clone(),
            removed_mcp_servers: self.removed.clone(),
            changed_mcp_servers: self.changed.clone(),
        }
    }

    fn render_text(&self, phase_node: Option<&str>) -> String {
        let mut lines = vec![match phase_node {
            Some(node) => format!("## MCP Server Changes — Step Transition: {node}"),
            None => "## MCP Server Changes".to_string(),
        }];

        append_lines(&mut lines, "Added MCP servers", &self.added, "已注入");
        append_lines(&mut lines, "Removed MCP servers", &self.removed, "已移除");
        append_lines(&mut lines, "Changed MCP servers", &self.changed, "已变更");

        if !self.unavailable.is_empty() {
            lines.push(String::new());
            lines.push("### Unavailable MCP servers (connection failed)".to_string());
            lines.push("The following MCP tool sources failed to connect. Their tools are NOT available this session. If the user's request requires these tools, inform them that the MCP environment needs to be fixed before proceeding.".to_string());
            for desc in &self.unavailable {
                lines.push(format!("- {desc}"));
            }
        }

        lines.join("\n")
    }
}

fn append_lines(lines: &mut Vec<String>, title: &str, values: &[String], suffix: &str) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("- {title}:"));
    for value in values {
        lines.push(format!("  - `{value}` — {suffix}"));
    }
}
