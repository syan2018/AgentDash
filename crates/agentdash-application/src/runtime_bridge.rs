use std::collections::BTreeMap;

use agentdash_spi::{McpTransportConfig, RuntimeMcpServerDeclaration};

use crate::runtime::RuntimeMcpServer;

pub fn mcp_declaration_to_runtime_server(server: &RuntimeMcpServerDeclaration) -> RuntimeMcpServer {
    match &server.transport {
        McpTransportConfig::Http { url, .. } => RuntimeMcpServer::Http {
            name: server.name.clone(),
            url: url.clone(),
        },
        McpTransportConfig::Sse { url, .. } => RuntimeMcpServer::Sse {
            name: server.name.clone(),
            url: url.clone(),
        },
        McpTransportConfig::Stdio {
            command,
            args,
            env,
            cwd,
        } => RuntimeMcpServer::Stdio {
            name: server.name.clone(),
            command: command.clone(),
            args: args.clone(),
            env: env
                .iter()
                .map(|item| (item.name.clone(), item.value.clone()))
                .collect::<BTreeMap<_, _>>(),
            cwd: cwd.clone(),
        },
    }
}

pub fn mcp_declarations_to_runtime_servers(
    servers: &[RuntimeMcpServerDeclaration],
) -> Vec<RuntimeMcpServer> {
    servers
        .iter()
        .map(mcp_declaration_to_runtime_server)
        .collect()
}

pub fn runtime_server_to_mcp_declaration(
    server: &RuntimeMcpServer,
) -> Option<RuntimeMcpServerDeclaration> {
    match server {
        RuntimeMcpServer::Http { name, url } => Some(RuntimeMcpServerDeclaration {
            name: name.clone(),
            transport: McpTransportConfig::Http {
                url: url.clone(),
                headers: vec![],
            },
            uses_relay: false,
        }),
        RuntimeMcpServer::Sse { name, url } => Some(RuntimeMcpServerDeclaration {
            name: name.clone(),
            transport: McpTransportConfig::Sse {
                url: url.clone(),
                headers: vec![],
            },
            uses_relay: false,
        }),
        RuntimeMcpServer::Stdio {
            name,
            command,
            args,
            env,
            cwd,
            ..
        } => Some(RuntimeMcpServerDeclaration {
            name: name.clone(),
            transport: McpTransportConfig::Stdio {
                command: command.clone(),
                args: args.clone(),
                env: env
                    .iter()
                    .map(|(k, v)| agentdash_spi::McpEnvVar {
                        name: k.clone(),
                        value: v.clone(),
                    })
                    .collect(),
                cwd: cwd.clone(),
            },
            uses_relay: false,
        }),
        RuntimeMcpServer::Unsupported { .. } => None,
    }
}

pub fn runtime_servers_to_mcp_declarations(
    servers: &[RuntimeMcpServer],
) -> Vec<RuntimeMcpServerDeclaration> {
    servers
        .iter()
        .filter_map(runtime_server_to_mcp_declaration)
        .collect()
}
