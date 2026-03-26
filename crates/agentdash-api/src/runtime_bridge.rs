use std::collections::BTreeMap;

use agent_client_protocol::{EnvVariable, McpServer, McpServerHttp, McpServerSse, McpServerStdio};
use agentdash_application::runtime::{
    ExecutorConfig, RuntimeAddressSpace, RuntimeFileEntry, RuntimeMcpBinding, RuntimeMcpServer,
    RuntimeMount, RuntimeToolScope, ThinkingLevel,
};
use agentdash_executor::{AgentDashExecutorConfig, ExecutionAddressSpace, ExecutionMount};
use agentdash_mcp::{injection::McpInjectionConfig, scope::ToolScope};
use agentdash_relay::FileEntryRelay;

pub fn connector_executor_config_to_runtime(config: &AgentDashExecutorConfig) -> ExecutorConfig {
    ExecutorConfig {
        executor: config.executor.clone(),
        variant: config.variant.clone(),
        provider_id: config.provider_id.clone(),
        model_id: config.model_id.clone(),
        agent_id: config.agent_id.clone(),
        thinking_level: config.thinking_level.map(connector_thinking_level_to_runtime),
        permission_policy: config.permission_policy.clone(),
    }
}

pub fn runtime_executor_config_to_connector(config: &ExecutorConfig) -> AgentDashExecutorConfig {
    AgentDashExecutorConfig {
        executor: config.executor.clone(),
        variant: config.variant.clone(),
        provider_id: config.provider_id.clone(),
        model_id: config.model_id.clone(),
        agent_id: config.agent_id.clone(),
        thinking_level: config.thinking_level.map(runtime_thinking_level_to_connector),
        permission_policy: config.permission_policy.clone(),
    }
}

pub fn connector_thinking_level_to_runtime(level: agentdash_executor::ThinkingLevel) -> ThinkingLevel {
    match level {
        agentdash_executor::ThinkingLevel::Off => ThinkingLevel::Off,
        agentdash_executor::ThinkingLevel::Minimal => ThinkingLevel::Minimal,
        agentdash_executor::ThinkingLevel::Low => ThinkingLevel::Low,
        agentdash_executor::ThinkingLevel::Medium => ThinkingLevel::Medium,
        agentdash_executor::ThinkingLevel::High => ThinkingLevel::High,
        agentdash_executor::ThinkingLevel::Xhigh => ThinkingLevel::Xhigh,
    }
}

pub fn runtime_thinking_level_to_connector(level: ThinkingLevel) -> agentdash_executor::ThinkingLevel {
    match level {
        ThinkingLevel::Off => agentdash_executor::ThinkingLevel::Off,
        ThinkingLevel::Minimal => agentdash_executor::ThinkingLevel::Minimal,
        ThinkingLevel::Low => agentdash_executor::ThinkingLevel::Low,
        ThinkingLevel::Medium => agentdash_executor::ThinkingLevel::Medium,
        ThinkingLevel::High => agentdash_executor::ThinkingLevel::High,
        ThinkingLevel::Xhigh => agentdash_executor::ThinkingLevel::Xhigh,
    }
}

pub fn execution_mount_to_runtime(mount: &ExecutionMount) -> RuntimeMount {
    RuntimeMount {
        id: mount.id.clone(),
        provider: mount.provider.clone(),
        backend_id: mount.backend_id.clone(),
        root_ref: mount.root_ref.clone(),
        capabilities: mount.capabilities.clone(),
        default_write: mount.default_write,
        display_name: mount.display_name.clone(),
        metadata: mount.metadata.clone(),
    }
}

pub fn runtime_mount_to_execution(mount: &RuntimeMount) -> ExecutionMount {
    ExecutionMount {
        id: mount.id.clone(),
        provider: mount.provider.clone(),
        backend_id: mount.backend_id.clone(),
        root_ref: mount.root_ref.clone(),
        capabilities: mount.capabilities.clone(),
        default_write: mount.default_write,
        display_name: mount.display_name.clone(),
        metadata: mount.metadata.clone(),
    }
}

pub fn execution_address_space_to_runtime(
    address_space: &ExecutionAddressSpace,
) -> RuntimeAddressSpace {
    RuntimeAddressSpace {
        mounts: address_space
            .mounts
            .iter()
            .map(execution_mount_to_runtime)
            .collect(),
        default_mount_id: address_space.default_mount_id.clone(),
        source_project_id: address_space.source_project_id.clone(),
        source_story_id: address_space.source_story_id.clone(),
    }
}

pub fn runtime_address_space_to_execution(
    address_space: &RuntimeAddressSpace,
) -> ExecutionAddressSpace {
    ExecutionAddressSpace {
        mounts: address_space
            .mounts
            .iter()
            .map(runtime_mount_to_execution)
            .collect(),
        default_mount_id: address_space.default_mount_id.clone(),
        source_project_id: address_space.source_project_id.clone(),
        source_story_id: address_space.source_story_id.clone(),
    }
}

pub fn relay_file_entry_to_runtime(entry: &FileEntryRelay) -> RuntimeFileEntry {
    RuntimeFileEntry {
        path: entry.path.clone(),
        size: entry.size,
        modified_at: entry.modified_at,
        is_dir: entry.is_dir,
    }
}

pub fn runtime_file_entry_to_relay(entry: &RuntimeFileEntry) -> FileEntryRelay {
    FileEntryRelay {
        path: entry.path.clone(),
        size: entry.size,
        modified_at: entry.modified_at,
        is_dir: entry.is_dir,
    }
}

pub fn relay_file_entries_to_runtime(entries: &[FileEntryRelay]) -> Vec<RuntimeFileEntry> {
    entries.iter().map(relay_file_entry_to_runtime).collect()
}

pub fn runtime_file_entries_to_relay(entries: &[RuntimeFileEntry]) -> Vec<FileEntryRelay> {
    entries.iter().map(runtime_file_entry_to_relay).collect()
}

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
    use super::*;

    #[test]
    fn executor_config_roundtrip_preserves_fields() {
        let connector = AgentDashExecutorConfig {
            executor: "PI_AGENT".to_string(),
            variant: Some("fast".to_string()),
            provider_id: Some("openai".to_string()),
            model_id: Some("gpt-5.4".to_string()),
            agent_id: Some("agent-1".to_string()),
            thinking_level: Some(agentdash_executor::ThinkingLevel::High),
            permission_policy: Some("AUTO".to_string()),
        };

        let runtime = connector_executor_config_to_runtime(&connector);
        let next = runtime_executor_config_to_connector(&runtime);

        assert_eq!(next.executor, connector.executor);
        assert_eq!(next.variant, connector.variant);
        assert_eq!(next.provider_id, connector.provider_id);
        assert_eq!(next.model_id, connector.model_id);
        assert_eq!(next.agent_id, connector.agent_id);
        assert_eq!(next.thinking_level, connector.thinking_level);
        assert_eq!(next.permission_policy, connector.permission_policy);
    }

    #[test]
    fn address_space_roundtrip_preserves_mount_shape() {
        let address_space = ExecutionAddressSpace {
            mounts: vec![ExecutionMount {
                id: "main".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-a".to_string(),
                root_ref: "/workspace".to_string(),
                capabilities: vec![
                    agentdash_executor::ExecutionMountCapability::Read,
                    agentdash_executor::ExecutionMountCapability::Write,
                ],
                default_write: true,
                display_name: "主工作区".to_string(),
                metadata: serde_json::json!({ "kind": "workspace" }),
            }],
            default_mount_id: Some("main".to_string()),
            source_project_id: Some("project-1".to_string()),
            source_story_id: Some("story-1".to_string()),
        };

        let runtime = execution_address_space_to_runtime(&address_space);
        let next = runtime_address_space_to_execution(&runtime);

        assert_eq!(next, address_space);
    }
}
