use agentdash_application_ports::mcp_discovery::DiscoveredMcpTool;
use agentdash_platform_spi::{AgentToolError, DynAgentTool, sanitize_tool_schema};

use super::naming::namespaced_tool_name;

const DEFAULT_MCP_TOOL_DESCRIPTION: &str = "MCP 工具";

#[derive(Clone)]
pub(crate) struct McpToolSurface {
    pub runtime_name: String,
    pub server_name: String,
    pub tool_name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
}

impl McpToolSurface {
    pub fn new(
        server_name: impl Into<String>,
        tool_name: impl Into<String>,
        description: Option<&str>,
        parameters_schema: serde_json::Value,
    ) -> Self {
        let server_name = server_name.into();
        let tool_name = tool_name.into();
        Self {
            runtime_name: namespaced_tool_name(&server_name, &tool_name),
            server_name,
            tool_name,
            description: normalize_description(description),
            parameters_schema: sanitize_tool_schema(parameters_schema),
        }
    }
}

pub(crate) fn normalize_description(description: Option<&str>) -> String {
    description
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_MCP_TOOL_DESCRIPTION)
        .to_string()
}

pub(crate) fn normalize_args_object(
    args: serde_json::Value,
) -> Result<Option<serde_json::Map<String, serde_json::Value>>, AgentToolError> {
    match args {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Object(map) => Ok(Some(map)),
        other => Err(AgentToolError::InvalidArguments(format!(
            "MCP 工具参数必须是 JSON object，实际为: {}",
            other
        ))),
    }
}

pub(crate) fn build_discovered_entry(
    surface: &McpToolSurface,
    uses_relay: bool,
    tool: DynAgentTool,
) -> DiscoveredMcpTool {
    DiscoveredMcpTool {
        runtime_name: surface.runtime_name.clone(),
        server_name: surface.server_name.clone(),
        tool_name: surface.tool_name.clone(),
        uses_relay,
        description: surface.description.clone(),
        parameters_schema: surface.parameters_schema.clone(),
        tool,
    }
}
