use agentdash_relay::{
    McpEnvVarRelay, McpHttpHeaderRelay, McpServerRelay, McpTransportConfigRelay,
};
use agentdash_spi::{McpEnvVar, McpHttpHeader, McpTransportConfig, RuntimeMcpServer};

pub fn runtime_mcp_server_to_relay(server: &RuntimeMcpServer) -> McpServerRelay {
    mcp_server_parts_to_relay(&server.name, &server.transport)
}

pub fn relay_mcp_server_to_runtime(server: &McpServerRelay, uses_relay: bool) -> RuntimeMcpServer {
    RuntimeMcpServer {
        name: server.name.clone(),
        transport: relay_transport_to_mcp_transport(&server.transport),
        uses_relay,
    }
}

pub fn mcp_server_parts_to_relay(name: &str, transport: &McpTransportConfig) -> McpServerRelay {
    McpServerRelay {
        name: name.to_string(),
        transport: mcp_transport_to_relay(transport),
    }
}

pub fn mcp_transport_to_relay(transport: &McpTransportConfig) -> McpTransportConfigRelay {
    match transport {
        McpTransportConfig::Http { url, headers } => McpTransportConfigRelay::Http {
            url: url.clone(),
            headers: headers_to_relay(headers),
        },
        McpTransportConfig::Sse { url, headers } => McpTransportConfigRelay::Sse {
            url: url.clone(),
            headers: headers_to_relay(headers),
        },
        McpTransportConfig::Stdio {
            command,
            args,
            env,
            cwd,
        } => McpTransportConfigRelay::Stdio {
            command: command.clone(),
            args: args.clone(),
            env: env_to_relay(env),
            cwd: cwd.clone(),
        },
    }
}

pub fn relay_transport_to_mcp_transport(transport: &McpTransportConfigRelay) -> McpTransportConfig {
    match transport {
        McpTransportConfigRelay::Http { url, headers } => McpTransportConfig::Http {
            url: url.clone(),
            headers: headers_from_relay(headers),
        },
        McpTransportConfigRelay::Sse { url, headers } => McpTransportConfig::Sse {
            url: url.clone(),
            headers: headers_from_relay(headers),
        },
        McpTransportConfigRelay::Stdio {
            command,
            args,
            env,
            cwd,
        } => McpTransportConfig::Stdio {
            command: command.clone(),
            args: args.clone(),
            env: env_from_relay(env),
            cwd: cwd.clone(),
        },
    }
}

fn headers_to_relay(headers: &[McpHttpHeader]) -> Vec<McpHttpHeaderRelay> {
    headers
        .iter()
        .map(|header| McpHttpHeaderRelay {
            name: header.name.clone(),
            value: header.value.clone(),
        })
        .collect()
}

fn env_to_relay(env: &[McpEnvVar]) -> Vec<McpEnvVarRelay> {
    env.iter()
        .map(|var| McpEnvVarRelay {
            name: var.name.clone(),
            value: var.value.clone(),
        })
        .collect()
}

fn headers_from_relay(headers: &[McpHttpHeaderRelay]) -> Vec<McpHttpHeader> {
    headers
        .iter()
        .map(|header| McpHttpHeader {
            name: header.name.clone(),
            value: header.value.clone(),
        })
        .collect()
}

fn env_from_relay(env: &[McpEnvVarRelay]) -> Vec<McpEnvVar> {
    env.iter()
        .map(|var| McpEnvVar {
            name: var.name.clone(),
            value: var.value.clone(),
        })
        .collect()
}
