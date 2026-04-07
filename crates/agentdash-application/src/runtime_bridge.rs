use std::collections::BTreeMap;

use agent_client_protocol::{EnvVariable, McpServer, McpServerHttp, McpServerSse, McpServerStdio};

use crate::runtime::RuntimeMcpServer;

pub fn acp_mcp_server_to_runtime(server: &McpServer) -> RuntimeMcpServer {
    match server {
        McpServer::Http(server) => RuntimeMcpServer::Http {
            name: server.name.clone(),
            url: server.url.clone(),
        },
        McpServer::Sse(server) => RuntimeMcpServer::Sse {
            name: server.name.clone(),
            url: server.url.clone(),
        },
        McpServer::Stdio(server) => RuntimeMcpServer::Stdio {
            name: server.name.clone(),
            command: server.command.to_string_lossy().to_string(),
            args: server.args.clone(),
            env: server
                .env
                .iter()
                .map(|item| (item.name.clone(), item.value.clone()))
                .collect::<BTreeMap<_, _>>(),
            cwd: None,
        },
        _ => RuntimeMcpServer::Unsupported {
            name: "unsupported".to_string(),
            transport: "unsupported".to_string(),
            target: String::new(),
        },
    }
}

pub fn runtime_mcp_server_to_acp(server: &RuntimeMcpServer) -> Option<McpServer> {
    match server {
        RuntimeMcpServer::Http { name, url } => Some(McpServer::Http(McpServerHttp::new(
            name.clone(),
            url.clone(),
        ))),
        RuntimeMcpServer::Sse { name, url } => {
            Some(McpServer::Sse(McpServerSse::new(name.clone(), url.clone())))
        }
        RuntimeMcpServer::Stdio {
            name,
            command,
            args,
            env,
            ..
        } => Some(McpServer::Stdio(
            McpServerStdio::new(name.clone(), command.clone())
                .args(args.clone())
                .env(
                    env.iter()
                        .map(|(name, value)| EnvVariable::new(name.clone(), value.clone()))
                        .collect(),
                ),
        )),
        RuntimeMcpServer::Unsupported { .. } => None,
    }
}

pub fn acp_mcp_servers_to_runtime(servers: &[McpServer]) -> Vec<RuntimeMcpServer> {
    servers.iter().map(acp_mcp_server_to_runtime).collect()
}

pub fn runtime_mcp_servers_to_acp(servers: &[RuntimeMcpServer]) -> Vec<McpServer> {
    servers
        .iter()
        .filter_map(runtime_mcp_server_to_acp)
        .collect()
}
