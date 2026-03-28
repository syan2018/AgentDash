use std::collections::BTreeMap;

use agent_client_protocol::{EnvVariable, McpServer, McpServerHttp, McpServerSse, McpServerStdio};
use agentdash_mcp::{injection::McpInjectionConfig, scope::ToolScope};

use crate::runtime::{
    RuntimeMcpBinding, RuntimeMcpServer, RuntimeToolScope,
};

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
        RuntimeMcpServer::Http { name, url } => {
            Some(McpServer::Http(McpServerHttp::new(name.clone(), url.clone())))
        }
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

pub fn mcp_injection_config_to_runtime_binding(
    config: &McpInjectionConfig,
) -> RuntimeMcpBinding {
    let scope = match config.scope {
        ToolScope::Relay => RuntimeToolScope::Relay,
        ToolScope::Story => RuntimeToolScope::Story,
        ToolScope::Task => RuntimeToolScope::Task,
    };

    RuntimeMcpBinding {
        base_url: config.base_url.clone(),
        scope,
        project_id: config.project_id,
        story_id: config.story_id,
        task_id: config.task_id,
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::common::{AddressSpace, Mount, MountCapability};

    #[test]
    fn address_space_roundtrip_preserves_mount_shape() {
        let address_space = AddressSpace {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-a".to_string(),
                root_ref: "/workspace".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::Write],
                default_write: true,
                display_name: "主工作区".to_string(),
                metadata: serde_json::json!({ "kind": "workspace" }),
            }],
            default_mount_id: Some("main".to_string()),
            source_project_id: Some("project-1".to_string()),
            source_story_id: Some("story-1".to_string()),
        };

        let cloned = address_space.clone();
        assert_eq!(cloned, address_space);
    }
}
