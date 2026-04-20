//! CapabilityResolver 实现

use std::collections::BTreeSet;

use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_mcp::injection::McpInjectionConfig;
use agentdash_spi::tool_capability::{
    self, PlatformMcpScope, ToolCapability, WELL_KNOWN_KEYS,
};
use agentdash_spi::{FlowCapabilities, ToolCluster};
use uuid::Uuid;

use crate::capability::SessionWorkflowContext;
use crate::platform_config::PlatformConfig;

/// Resolver 输入 — 纯粹的 session 上下文描述，不含基础设施配置。
#[derive(Debug, Clone)]
pub struct CapabilityResolverInput {
    /// session 归属实体类型
    pub owner_type: SessionOwnerType,
    /// session 关联的 Project ID
    pub project_id: Uuid,
    /// session 关联的 Story ID（story/task session 必填）
    pub story_id: Option<Uuid>,
    /// session 关联的 Task ID（task session 必填）
    pub task_id: Option<Uuid>,
    /// agent config 中显式声明的 capability key 列表。
    /// None 表示 agent 未声明（使用默认可见能力），空 vec 表示显式声明为空。
    pub agent_declared_capabilities: Option<Vec<String>>,
    /// Workflow 上下文（是否活跃 + 显式目标能力集合）。
    ///
    /// - `has_active_workflow=false, workflow_capabilities=None`
    ///   ([`SessionWorkflowContext::NONE`])：使用默认 visibility 规则
    /// - `has_active_workflow=true, workflow_capabilities=Some(vec)`：
    ///   直接按给定 key 集合构建最终能力集（全量替换，可覆盖 visibility）
    /// - `has_active_workflow=true, workflow_capabilities=None`：
    ///   仅激活 `workflow_can_grant` 授予路径，不覆盖能力集
    pub workflow_ctx: SessionWorkflowContext,
    /// agent config 中的 `mcp_servers` 配置 — 用于解析 `mcp:*` key。
    /// 存储为 (server_name, McpServer ACP 对象) 对。
    pub agent_mcp_servers: Vec<AgentMcpServerEntry>,
    /// Companion sub-session 模式 — 设置时，对最终 FlowCapabilities 施加 slice 裁剪。
    pub companion_slice_mode: Option<CompanionSliceMode>,
}

/// Companion sub-session 的能力裁剪模式。
///
/// 控制 companion 继承父 session 能力时保留的范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanionSliceMode {
    Full,
    Compact,
    WorkflowOnly,
    ConstraintsOnly,
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
    /// 需注入的自定义 MCP server 列表（由 `mcp:*` key 解析得到）
    pub custom_mcp_servers: Vec<agent_client_protocol::McpServer>,
    /// 已解析通过的 capability key 集合（供调试 / 日志）
    pub effective_capabilities: BTreeSet<ToolCapability>,
}

/// 统一工具能力解析器。
///
/// 无状态、纯函数式 — session 上下文通过 `CapabilityResolverInput` 传入，
/// 基础设施配置通过 `&PlatformConfig` 传入。
pub struct CapabilityResolver;

impl CapabilityResolver {
    /// Companion sub-session 的快捷方法 — 仅按 slice_mode 裁剪 FlowCapabilities。
    ///
    /// Companion 继承父 session 的 MCP/VFS，不需要独立解析平台 MCP。
    pub fn resolve_companion_caps(slice_mode: CompanionSliceMode) -> FlowCapabilities {
        apply_companion_slice(FlowCapabilities::all(), slice_mode)
    }

    /// 根据 session 上下文计算有效工具集。
    pub fn resolve(
        input: &CapabilityResolverInput,
        platform: &PlatformConfig,
    ) -> CapabilityResolverOutput {
        let agent_declares_set: Option<BTreeSet<&str>> = input
            .agent_declared_capabilities
            .as_ref()
            .map(|caps| caps.iter().map(|s| s.as_str()).collect());

        let mut effective_caps = if input.workflow_ctx.workflow_capabilities.is_some() {
            BTreeSet::<ToolCapability>::new()
        } else {
            default_visible_capabilities(input, agent_declares_set.as_ref())
        };
        let mut custom_mcp_servers = Vec::<agent_client_protocol::McpServer>::new();
        let mut seen_custom_mcp_names = BTreeSet::<String>::new();

        // ── 1. workflow/step 覆盖能力集（支持 well-known + mcp:*） ──
        for key in input.workflow_ctx.workflow_capabilities.iter().flatten() {
            let cap = ToolCapability::new(key.clone());

            if cap.is_well_known() {
                // step 可覆盖 visibility：well-known 在此直接按声明生效。
                effective_caps.insert(cap);
                continue;
            }

            if cap.is_custom_mcp() {
                let Some(server_name) =
                    cap.custom_mcp_server_name().map(str::to_string)
                else {
                    continue;
                };
                if let Some(entry) = input
                    .agent_mcp_servers
                    .iter()
                    .find(|e| e.name == server_name)
                {
                    effective_caps.insert(cap);
                    if seen_custom_mcp_names.insert(server_name.clone()) {
                        custom_mcp_servers.push(entry.server.clone());
                    }
                } else {
                    tracing::warn!(
                        key = %key,
                        server_name = %server_name,
                        "workflow 声明了 mcp:{server_name}，但 agent config 中未注册该 MCP server"
                    );
                }
            }
            // 未知前缀的 key 静默忽略（未来扩展预留）
        }

        let mut tool_clusters = BTreeSet::<ToolCluster>::new();
        let mut platform_mcp_configs = Vec::<McpInjectionConfig>::new();
        for cap in &effective_caps {
            for cluster in tool_capability::capability_to_tool_clusters(cap) {
                tool_clusters.insert(cluster);
            }
            if let Some(scope) = tool_capability::capability_to_platform_mcp_scope(cap) {
                if let Some(config) =
                    build_platform_mcp_config(scope, platform.mcp_base_url.as_deref(), input)
                {
                    platform_mcp_configs.push(config);
                }
            }
        }

        let flow_capabilities = if let Some(slice_mode) = input.companion_slice_mode {
            apply_companion_slice(FlowCapabilities::from_clusters(tool_clusters), slice_mode)
        } else {
            FlowCapabilities::from_clusters(tool_clusters)
        };

        CapabilityResolverOutput {
            flow_capabilities,
            platform_mcp_configs,
            custom_mcp_servers,
            effective_capabilities: effective_caps,
        }
    }
}

fn default_visible_capabilities(
    input: &CapabilityResolverInput,
    agent_declares_set: Option<&BTreeSet<&str>>,
) -> BTreeSet<ToolCapability> {
    let mut effective = BTreeSet::new();
    for &key in WELL_KNOWN_KEYS {
        let cap = ToolCapability::new(key);
        let agent_declares_this = agent_declares_set.is_some_and(|set| set.contains(key));
        if tool_capability::is_capability_visible(
            &cap,
            input.owner_type,
            agent_declares_this,
            input.workflow_ctx.has_active_workflow,
        ) {
            effective.insert(cap);
        }
    }
    effective
}

/// Companion slice mode → FlowCapabilities 约束。
fn apply_companion_slice(
    base: FlowCapabilities,
    mode: CompanionSliceMode,
) -> FlowCapabilities {
    match mode {
        CompanionSliceMode::Full => base,
        CompanionSliceMode::Compact => base.intersect(&FlowCapabilities::from_clusters([
            ToolCluster::Read,
            ToolCluster::Execute,
            ToolCluster::Collaboration,
        ])),
        CompanionSliceMode::WorkflowOnly | CompanionSliceMode::ConstraintsOnly => {
            base.intersect(&FlowCapabilities::from_clusters([
                ToolCluster::Read,
                ToolCluster::Collaboration,
            ]))
        }
    }
}

/// 根据平台 MCP scope 和 session 上下文构建 `McpInjectionConfig`。
fn build_platform_mcp_config(
    scope: PlatformMcpScope,
    mcp_base_url: Option<&str>,
    input: &CapabilityResolverInput,
) -> Option<McpInjectionConfig> {
    let base_url = mcp_base_url?;

    Some(match scope {
        PlatformMcpScope::Relay => McpInjectionConfig::for_relay(base_url, input.project_id),
        PlatformMcpScope::Story => {
            let story_id = input.story_id?;
            McpInjectionConfig::for_story(base_url, input.project_id, story_id)
        }
        PlatformMcpScope::Task => {
            let task_id = input.task_id?;
            let story_id = input.story_id?;
            McpInjectionConfig::for_task(base_url, input.project_id, story_id, task_id)
        }
        PlatformMcpScope::Workflow => {
            McpInjectionConfig::for_workflow(base_url, input.project_id)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_platform() -> PlatformConfig {
        PlatformConfig {
            mcp_base_url: Some("http://localhost:3001".to_string()),
        }
    }

    fn base_input() -> CapabilityResolverInput {
        CapabilityResolverInput {
            owner_type: SessionOwnerType::Project,
            project_id: Uuid::new_v4(),
            story_id: None,
            task_id: None,
            agent_declared_capabilities: None,
            workflow_ctx: SessionWorkflowContext::NONE,
            agent_mcp_servers: vec![],
            companion_slice_mode: None,
        }
    }

    #[test]
    fn project_session_gets_expected_clusters() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(output.flow_capabilities.has(ToolCluster::Read));
        assert!(output.flow_capabilities.has(ToolCluster::Write));
        assert!(output.flow_capabilities.has(ToolCluster::Execute));
        assert!(output.flow_capabilities.has(ToolCluster::Canvas));
        assert!(output.flow_capabilities.has(ToolCluster::Collaboration));
        assert!(!output.flow_capabilities.has(ToolCluster::Workflow));
    }

    #[test]
    fn project_session_gets_relay_mcp() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input, &test_platform());

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

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let has_workflow_mcp = output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/workflow/"));
        assert!(has_workflow_mcp, "应注入 WorkflowMcpServer");
    }

    #[test]
    fn project_session_without_workflow_declaration_no_workflow_mcp() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input, &test_platform());

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
            project_id,
            story_id: Some(story_id),
            task_id: Some(task_id),
            agent_declared_capabilities: None,
            workflow_ctx: SessionWorkflowContext::NONE,
            agent_mcp_servers: vec![],
            companion_slice_mode: None,
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let has_task_mcp = output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/task/"));
        assert!(has_task_mcp, "task session 应注入 TaskMcpServer");

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
            project_id,
            story_id: Some(story_id),
            task_id: None,
            agent_declared_capabilities: None,
            workflow_ctx: SessionWorkflowContext::NONE,
            agent_mcp_servers: vec![],
            companion_slice_mode: None,
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let has_story_mcp = output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/story/"));
        assert!(has_story_mcp, "story session 应注入 StoryMcpServer");
    }

    #[test]
    fn workflow_cluster_requires_active_workflow() {
        let mut input = base_input();
        let platform = test_platform();
        let output_no_workflow = CapabilityResolver::resolve(&input, &platform);
        assert!(!output_no_workflow.flow_capabilities.has(ToolCluster::Workflow));

        input.workflow_ctx.has_active_workflow = true;
        let output_with_workflow = CapabilityResolver::resolve(&input, &platform);
        assert!(output_with_workflow.flow_capabilities.has(ToolCluster::Workflow));
    }

    #[test]
    fn no_mcp_base_url_produces_no_platform_mcp() {
        let input = base_input();
        let platform = PlatformConfig { mcp_base_url: None };
        let output = CapabilityResolver::resolve(&input, &platform);
        assert!(output.platform_mcp_configs.is_empty());
    }

    #[test]
    fn custom_mcp_from_workflow_resolved() {
        let mut input = base_input();
        input.workflow_ctx.workflow_capabilities = Some(vec!["mcp:code_analyzer".to_string()]);
        input.agent_mcp_servers = vec![AgentMcpServerEntry {
            name: "code_analyzer".to_string(),
            server: agent_client_protocol::McpServer::Http(
                agent_client_protocol::McpServerHttp::new(
                    "code_analyzer",
                    "http://external:8080/mcp",
                ),
            ),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(output
            .effective_capabilities
            .contains(&ToolCapability::custom_mcp("code_analyzer")));
    }

    #[test]
    fn custom_mcp_missing_server_not_resolved() {
        let mut input = base_input();
        input.workflow_ctx.workflow_capabilities = Some(vec!["mcp:nonexistent".to_string()]);

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(!output
            .effective_capabilities
            .contains(&ToolCapability::custom_mcp("nonexistent")));
    }

    #[test]
    fn workflow_well_known_can_override_visibility() {
        let mut input = base_input();
        input.workflow_ctx.has_active_workflow = false;
        input.workflow_ctx.workflow_capabilities = Some(vec!["workflow".to_string()]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.flow_capabilities.has(ToolCluster::Workflow));
    }

    #[test]
    fn workflow_override_empty_caps_removes_default_clusters() {
        let mut input = base_input();
        input.workflow_ctx.workflow_capabilities = Some(vec![]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.flow_capabilities.enabled_clusters.is_empty());
        assert!(output.platform_mcp_configs.is_empty());
        assert!(output.effective_capabilities.is_empty());
    }

    #[test]
    fn workflow_custom_mcp_is_included_in_output() {
        let mut input = base_input();
        input.workflow_ctx.workflow_capabilities = Some(vec!["mcp:code_analyzer".to_string()]);
        input.agent_mcp_servers = vec![AgentMcpServerEntry {
            name: "code_analyzer".to_string(),
            server: agent_client_protocol::McpServer::Http(
                agent_client_protocol::McpServerHttp::new(
                    "code_analyzer",
                    "http://external:8080/mcp",
                ),
            ),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert_eq!(output.custom_mcp_servers.len(), 1);
        assert!(output
            .effective_capabilities
            .contains(&ToolCapability::custom_mcp("code_analyzer")));
    }
}
