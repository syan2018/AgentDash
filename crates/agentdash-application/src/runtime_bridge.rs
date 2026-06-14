use agentdash_spi::{McpTransportConfig, RuntimeMcpServer};

use crate::runtime::McpServerSummary;

pub fn runtime_mcp_server_to_summary(server: &RuntimeMcpServer) -> McpServerSummary {
    let (transport, target) = match &server.transport {
        McpTransportConfig::Http { url, .. } => ("http", url.clone()),
        McpTransportConfig::Sse { url, .. } => ("sse", url.clone()),
        McpTransportConfig::Stdio { command, .. } => ("stdio", command.clone()),
    };

    McpServerSummary {
        name: server.name.clone(),
        transport: transport.to_string(),
        target,
    }
}

pub fn runtime_mcp_servers_to_summaries(servers: &[RuntimeMcpServer]) -> Vec<McpServerSummary> {
    servers.iter().map(runtime_mcp_server_to_summary).collect()
}
