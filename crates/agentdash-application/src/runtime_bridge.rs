use std::collections::BTreeMap;

use agentdash_spi::{McpTransportConfig, SessionMcpServer};

use crate::runtime::RuntimeMcpServer;

pub fn session_mcp_server_to_runtime(server: &SessionMcpServer) -> RuntimeMcpServer {
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
            command, args, env, ..
        } => RuntimeMcpServer::Stdio {
            name: server.name.clone(),
            command: command.clone(),
            args: args.clone(),
            env: env
                .iter()
                .map(|item| (item.name.clone(), item.value.clone()))
                .collect::<BTreeMap<_, _>>(),
            cwd: None,
        },
    }
}

pub fn session_mcp_servers_to_runtime(servers: &[SessionMcpServer]) -> Vec<RuntimeMcpServer> {
    servers.iter().map(session_mcp_server_to_runtime).collect()
}

pub fn runtime_mcp_server_to_session(server: &RuntimeMcpServer) -> Option<SessionMcpServer> {
    match server {
        RuntimeMcpServer::Http { name, url } => Some(SessionMcpServer {
            name: name.clone(),
            transport: McpTransportConfig::Http { url: url.clone(), headers: vec![] },
            uses_relay: false,
        }),
        RuntimeMcpServer::Sse { name, url } => Some(SessionMcpServer {
            name: name.clone(),
            transport: McpTransportConfig::Sse { url: url.clone(), headers: vec![] },
            uses_relay: false,
        }),
        RuntimeMcpServer::Stdio { name, command, args, env, .. } => Some(SessionMcpServer {
            name: name.clone(),
            transport: McpTransportConfig::Stdio {
                command: command.clone(),
                args: args.clone(),
                env: env.iter().map(|(k, v)| agentdash_spi::McpEnvVar {
                    name: k.clone(), value: v.clone(),
                }).collect(),
            },
            uses_relay: false,
        }),
        RuntimeMcpServer::Unsupported { .. } => None,
    }
}

pub fn runtime_mcp_servers_to_session(servers: &[RuntimeMcpServer]) -> Vec<SessionMcpServer> {
    servers.iter().filter_map(runtime_mcp_server_to_session).collect()
}
