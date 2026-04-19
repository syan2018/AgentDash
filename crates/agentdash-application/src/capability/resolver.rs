//! CapabilityResolver 实现

use std::collections::BTreeSet;

use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_mcp::injection::McpInjectionConfig;
use agentdash_spi::tool_capability::{
    self, PlatformMcpScope, ToolCapability, WELL_KNOWN_KEYS,
};
use agentdash_spi::{FlowCapabilities, ToolCluster};
use uuid::Uuid;

/// Resolver 输入 — 描述 session 上下文
#[derive(Debug, Clone)]
pub struct CapabilityResolverInput {
    /// session 归属实体类型
    pub owner_type: SessionOwnerType,
    /// 平台 MCP server 基础 URL（如 `http://localhost:3001`）
    pub mcp_base_url: Option<String>,
    /// session 关联的 Project ID
    pub project_id: Uuid,
    /// session 关联的 Story ID（story/task session 必填）
    pub story_id: Option<Uuid>,
    /// session 关联的 Task ID（task session 必填）
    pub task_id: Option<Uuid>,
    /// agent config 中显式声明的 capability key 列表。
    /// None 表示 agent 未声明（使用默认可见能力），空 vec 表示显式声明为空。
    pub agent_declared_capabilities: Option<Vec<String>>,
    /// 是否有活跃的 workflow lifecycle run
    pub has_active_workflow: bool,
    /// workflow 或 step 级额外声明的 capability key（Phase 2 扩展用）
    pub workflow_capabilities: Vec<String>,
    /// agent config 中的 `mcp_servers` 配置 — 用于解析 `mcp:*` key。
    /// 存储为 (server_name, McpServer ACP 对象) 对。
    pub agent_mcp_servers: Vec<AgentMcpServerEntry>,
}

/// agent config 中注册的 MCP server 条目（用于 `mcp:*` key 解析）
#[derive(Debug, Clone)]
pub struct AgentMcpServerEntry {
    pub name: String,
    pub server: agent_client_protocol::McpServer,
}

/// Resolver 输出 — session 的有效工具集
#[derive(Debug, Clone)]
pub struct CapabilityResolverOutput {
    /// 内置工具簇（PiAgent 内部使用）
    pub flow_capabilities: FlowCapabilities,
    /// 需注入的平台 MCP server 列表
    pub platform_mcp_configs: Vec<McpInjectionConfig>,
    /// 已解析通过的 capability key 集合（供调试 / 日志）
    pub effective_capabilities: BTreeSet<ToolCapability>,
}

/// 统一工具能力解析器。
///
/// 无状态、纯函数式 — 所有依赖通过 `CapabilityResolverInput` 传入。
pub struct CapabilityResolver;

impl CapabilityResolver {
    /// 根据 session 上下文计算有效工具集。
    pub fn resolve(input: &CapabilityResolverInput) -> CapabilityResolverOutput {
        let mut effective_caps = BTreeSet::<ToolCapability>::new();
        let mut tool_clusters = BTreeSet::<ToolCluster>::new();
        let mut platform_mcp_configs = Vec::<McpInjectionConfig>::new();
        let mut custom_mcp_servers = Vec::<agent_client_protocol::McpServer>::new();

        let agent_declares_set: Option<BTreeSet<&str>> = input
            .agent_declared_capabilities
            .as_ref()
            .map(|caps| caps.iter().map(|s| s.as_str()).collect());

        // ── 1. 平台 well-known 能力 ──
        for &key in WELL_KNOWN_KEYS {
            let cap = ToolCapability::new(key);

            let agent_declares_this = agent_declares_set
                .as_ref()
                .is_some_and(|set| set.contains(key));

            if !tool_capability::is_capability_visible(
                &cap,
                input.owner_type,
                agent_declares_this,
                input.has_active_workflow,
            ) {
                continue;
            }

            effective_caps.insert(cap.clone());

            // ToolCluster 映射
            for cluster in tool_capability::capability_to_tool_clusters(&cap) {
                tool_clusters.insert(cluster);
            }

            // 平台 MCP 映射
            if let Some(scope) = tool_capability::capability_to_platform_mcp_scope(&cap) {
                if let Some(config) =
                    build_platform_mcp_config(scope, input)
                {
                    platform_mcp_configs.push(config);
                }
            }
        }

        // ── 2. workflow/step 声明的额外能力 ──
        for key in &input.workflow_capabilities {
            let cap = ToolCapability::new(key.clone());

            if cap.is_well_known() {
                // well-known 在上面已处理（按 visibility rule），此处跳过避免重复
                continue;
            }

            if cap.is_custom_mcp() {
                if let Some(server_name) = cap.custom_mcp_server_name() {
                    if let Some(entry) = input
                        .agent_mcp_servers
                        .iter()
                        .find(|e| e.name == server_name)
                    {
                        custom_mcp_servers.push(entry.server.clone());
                        effective_caps.insert(cap);
                    } else {
                        tracing::warn!(
                            key = %key,
                            server_name = %server_name,
                            "workflow 声明了 mcp:{server_name}，但 agent config 中未注册该 MCP server"
                        );
                    }
                }
            }
            // 未知前缀的 key 静默忽略（未来扩展预留）
        }

        CapabilityResolverOutput {
            flow_capabilities: FlowCapabilities::from_clusters(tool_clusters),
            platform_mcp_configs,
            effective_capabilities: effective_caps,
        }
    }
}

/// 根据平台 MCP scope 和 session 上下文构建 `McpInjectionConfig`。
fn build_platform_mcp_config(
    scope: PlatformMcpScope,
    input: &CapabilityResolverInput,
) -> Option<McpInjectionConfig> {
    let base_url = input.mcp_base_url.as_ref()?;

    Some(match scope {
        PlatformMcpScope::Relay => McpInjectionConfig::for_relay(base_url.clone(), input.project_id),
        PlatformMcpScope::Story => {
            let story_id = input.story_id?;
            McpInjectionConfig::for_story(base_url.clone(), input.project_id, story_id)
        }
        PlatformMcpScope::Task => {
            let task_id = input.task_id?;
            let story_id = input.story_id?;
            McpInjectionConfig::for_task(base_url.clone(), input.project_id, story_id, task_id)
        }
        PlatformMcpScope::Workflow => {
            McpInjectionConfig::for_workflow(base_url.clone(), input.project_id)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> CapabilityResolverInput {
        CapabilityResolverInput {
            owner_type: SessionOwnerType::Project,
            mcp_base_url: Some("http://localhost:3001".to_string()),
            project_id: Uuid::new_v4(),
            story_id: None,
            task_id: None,
            agent_declared_capabilities: None,
            has_active_workflow: false,
            workflow_capabilities: vec![],
            agent_mcp_servers: vec![],
        }
    }

    #[test]
    fn project_session_gets_expected_clusters() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input);

        assert!(output.flow_capabilities.has(ToolCluster::Read));
        assert!(output.flow_capabilities.has(ToolCluster::Write));
        assert!(output.flow_capabilities.has(ToolCluster::Execute));
        assert!(output.flow_capabilities.has(ToolCluster::Canvas));
        assert!(output.flow_capabilities.has(ToolCluster::Collaboration));
        // Workflow 需要 has_active_workflow
        assert!(!output.flow_capabilities.has(ToolCluster::Workflow));
    }

    #[test]
    fn project_session_gets_relay_mcp() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input);

        assert_eq!(output.platform_mcp_configs.len(), 1);
        assert!(output
            .platform_mcp_configs[0]
            .endpoint_url()
            .contains("/mcp/relay"));
    }

    #[test]
    fn project_session_with_workflow_management() {
        let mut input = base_input();
        input.agent_declared_capabilities = Some(vec!["workflow_management".to_string()]);

        let output = CapabilityResolver::resolve(&input);

        let has_workflow_mcp = output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/workflow/"));
        assert!(has_workflow_mcp, "应注入 WorkflowMcpServer");
    }

    #[test]
    fn project_session_without_workflow_declaration_no_workflow_mcp() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input);

        let has_workflow_mcp = output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/workflow/"));
        assert!(
            !has_workflow_mcp,
            "未声明 workflow_management 的 agent 不应有 WorkflowMcpServer"
        );
    }

    #[test]
    fn task_session_gets_task_mcp() {
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();

        let input = CapabilityResolverInput {
            owner_type: SessionOwnerType::Task,
            mcp_base_url: Some("http://localhost:3001".to_string()),
            project_id,
            story_id: Some(story_id),
            task_id: Some(task_id),
            agent_declared_capabilities: None,
            has_active_workflow: false,
            workflow_capabilities: vec![],
            agent_mcp_servers: vec![],
        };

        let output = CapabilityResolver::resolve(&input);

        let has_task_mcp = output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/task/"));
        assert!(has_task_mcp, "task session 应注入 TaskMcpServer");

        // task session 不应有 relay MCP
        let has_relay_mcp = output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/relay"));
        assert!(!has_relay_mcp, "task session 不应注入 RelayMcpServer");
    }

    #[test]
    fn story_session_gets_story_mcp() {
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();

        let input = CapabilityResolverInput {
            owner_type: SessionOwnerType::Story,
            mcp_base_url: Some("http://localhost:3001".to_string()),
            project_id,
            story_id: Some(story_id),
            task_id: None,
            agent_declared_capabilities: None,
            has_active_workflow: false,
            workflow_capabilities: vec![],
            agent_mcp_servers: vec![],
        };

        let output = CapabilityResolver::resolve(&input);

        let has_story_mcp = output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/story/"));
        assert!(has_story_mcp, "story session 应注入 StoryMcpServer");
    }

    #[test]
    fn workflow_cluster_requires_active_workflow() {
        let mut input = base_input();
        let output_no_workflow = CapabilityResolver::resolve(&input);
        assert!(!output_no_workflow.flow_capabilities.has(ToolCluster::Workflow));

        input.has_active_workflow = true;
        let output_with_workflow = CapabilityResolver::resolve(&input);
        assert!(output_with_workflow.flow_capabilities.has(ToolCluster::Workflow));
    }

    #[test]
    fn no_mcp_base_url_produces_no_platform_mcp() {
        let mut input = base_input();
        input.mcp_base_url = None;
        let output = CapabilityResolver::resolve(&input);
        assert!(output.platform_mcp_configs.is_empty());
    }

    #[test]
    fn custom_mcp_from_workflow_resolved() {
        let mut input = base_input();
        input.workflow_capabilities = vec!["mcp:code_analyzer".to_string()];
        input.agent_mcp_servers = vec![AgentMcpServerEntry {
            name: "code_analyzer".to_string(),
            server: agent_client_protocol::McpServer::Http(
                agent_client_protocol::McpServerHttp::new(
                    "code_analyzer",
                    "http://external:8080/mcp",
                ),
            ),
        }];

        let output = CapabilityResolver::resolve(&input);

        assert!(output
            .effective_capabilities
            .contains(&ToolCapability::custom_mcp("code_analyzer")));
    }

    #[test]
    fn custom_mcp_missing_server_not_resolved() {
        let mut input = base_input();
        input.workflow_capabilities = vec!["mcp:nonexistent".to_string()];

        let output = CapabilityResolver::resolve(&input);

        assert!(!output
            .effective_capabilities
            .contains(&ToolCapability::custom_mcp("nonexistent")));
    }
}
