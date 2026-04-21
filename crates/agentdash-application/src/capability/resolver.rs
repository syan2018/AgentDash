//! CapabilityResolver 实现

use std::collections::{BTreeMap, BTreeSet};

use agent_client_protocol::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
};
use agentdash_domain::mcp_preset::McpServerDecl;
use agentdash_domain::session_binding::SessionOwnerCtx;
use agentdash_domain::workflow::CapabilityDirective;
use agentdash_mcp::injection::McpInjectionConfig;
use agentdash_spi::tool_capability::{
    self, PlatformMcpScope, ToolCapability, WELL_KNOWN_KEYS, cluster_tools, expand_alias,
};
use agentdash_spi::{FlowCapabilities, ToolCluster};

use crate::capability::SessionWorkflowContext;
use crate::platform_config::PlatformConfig;

/// 调用方预展开的 project 级 MCP Preset 字典。
///
/// key 为 preset `name`（同 `mcp:<name>` 中的 `<name>`），value 为对应 `McpServerDecl`。
/// resolver 内部保持纯函数；查询 Preset 的 IO 由调用方（例如 `SessionRequestAssembler`）完成,
/// 结果以 map 形式塞进 [`CapabilityResolverInput::available_presets`]。
pub type AvailableMcpPresets = BTreeMap<String, McpServerDecl>;

/// Resolver 输入 — 纯粹的 session 上下文描述，不含基础设施配置。
#[derive(Debug, Clone)]
pub struct CapabilityResolverInput {
    /// session 归属上下文（owner_type + 关联 ID 组合成的 sum type）。
    ///
    /// 合法组合被类型系统约束：
    /// - [`SessionOwnerCtx::Project`] — project 级 session
    /// - [`SessionOwnerCtx::Story`] — story 级 session（含 project_id）
    /// - [`SessionOwnerCtx::Task`] — task 级 session（含 project_id + story_id）
    pub owner_ctx: SessionOwnerCtx,
    /// agent config 中显式声明的 capability key 列表。
    /// None 表示 agent 未声明（使用默认可见能力），空 vec 表示显式声明为空。
    pub agent_declared_capabilities: Option<Vec<String>>,
    /// Workflow 上下文（是否活跃 + 标准能力指令集合）。
    ///
    /// - `has_active_workflow=false, workflow_capability_directives=None`
    ///   ([`SessionWorkflowContext::NONE`])：使用默认 visibility 规则
    /// - `has_active_workflow=true, workflow_capability_directives=Some(vec)`：
    ///   在默认能力基线上按 `CapabilityDirective` 做标准增删（推荐）
    /// - `has_active_workflow=true, workflow_capability_directives=None`：
    ///   仅激活 `workflow_can_grant` 授予路径，不覆盖能力集
    pub workflow_ctx: SessionWorkflowContext,
    /// agent config 中的 `mcp_servers` 配置 — 用于兼容旧的 inline 声明链路。
    /// `mcp:<X>` 解析优先查 `available_presets`，未命中时 fallback 到此列表。
    pub agent_mcp_servers: Vec<AgentMcpServerEntry>,
    /// project 级 MCP Preset 预展开字典 — `mcp:<name>` 的首选查源。
    /// 由调用方在 builder 入口处从 `McpPresetRepository` 批量查出并展开。
    pub available_presets: AvailableMcpPresets,
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

        let mut effective_caps =
            default_visible_capabilities(input, agent_declares_set.as_ref());
        let mut custom_mcp_servers = Vec::<agent_client_protocol::McpServer>::new();
        let mut seen_custom_mcp_names = BTreeSet::<String>::new();
        // 收集工具级裁剪（include/exclude）→ 最终注入 FlowCapabilities.excluded_tools
        let mut tool_filters: Vec<ResolvedToolFilter> = Vec::new();

        for directive in input
            .workflow_ctx
            .workflow_capability_directives
            .iter()
            .flatten()
        {
            match directive {
                CapabilityDirective::Add(entry) => {
                    let key = entry.key();

                    // 别名展开：file_system → file_read + file_write + shell_execute
                    if let Some(expanded_keys) = expand_alias(key) {
                        for &sub_key in expanded_keys {
                            effective_caps.insert(ToolCapability::new(sub_key));
                        }
                        // 别名级别的 tool filter 分配到所有展开 key
                        if entry.has_tool_filter() {
                            for &sub_key in expanded_keys {
                                tool_filters.push(ResolvedToolFilter {
                                    capability_key: sub_key.to_string(),
                                    include_tools: entry.include_tools().to_vec(),
                                    exclude_tools: entry.exclude_tools().to_vec(),
                                });
                            }
                        }
                        continue;
                    }

                    let cap = ToolCapability::new(key);
                    if cap.is_well_known() {
                        effective_caps.insert(cap);
                        if entry.has_tool_filter() {
                            tool_filters.push(ResolvedToolFilter {
                                capability_key: key.to_string(),
                                include_tools: entry.include_tools().to_vec(),
                                exclude_tools: entry.exclude_tools().to_vec(),
                            });
                        }
                        continue;
                    }

                    if cap.is_custom_mcp() {
                        let Some(server_name) =
                            cap.custom_mcp_server_name().map(str::to_string)
                        else {
                            continue;
                        };

                        if let Some(decl) = input.available_presets.get(&server_name) {
                            effective_caps.insert(cap);
                            if seen_custom_mcp_names.insert(server_name.clone()) {
                                custom_mcp_servers.push(mcp_server_decl_to_acp(decl));
                            }
                            continue;
                        }

                        if let Some(agent_entry) = input
                            .agent_mcp_servers
                            .iter()
                            .find(|e| e.name == server_name)
                        {
                            effective_caps.insert(cap);
                            if seen_custom_mcp_names.insert(server_name.clone()) {
                                custom_mcp_servers.push(agent_entry.server.clone());
                            }
                        } else {
                            tracing::warn!(
                                key = %cap.key(),
                                server_name = %server_name,
                                "workflow directive 声明了 mcp:{server_name}，但 project 级 McpPreset 和 agent 内联 mcp_servers 都未注册该 server"
                            );
                        }
                    }
                }
                CapabilityDirective::Remove(key) => {
                    // 别名展开移除
                    if let Some(expanded_keys) = expand_alias(key) {
                        for &sub_key in expanded_keys {
                            effective_caps.remove(&ToolCapability::new(sub_key));
                        }
                    } else {
                        let cap = ToolCapability::new(key.clone());
                        effective_caps.remove(&cap);
                        if cap.is_custom_mcp() {
                            if let Some(server_name) = cap.custom_mcp_server_name() {
                                seen_custom_mcp_names.remove(server_name);
                                custom_mcp_servers.retain(|server| {
                                    mcp_server_name(server) != Some(server_name)
                                });
                            }
                        }
                    }
                }
            }
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

        // 构建工具级排除集
        let excluded_tools = compute_excluded_tools(&tool_filters);

        let mut flow_capabilities = FlowCapabilities {
            enabled_clusters: tool_clusters,
            excluded_tools,
        };

        if let Some(slice_mode) = input.companion_slice_mode {
            flow_capabilities = apply_companion_slice(flow_capabilities, slice_mode);
        }

        CapabilityResolverOutput {
            flow_capabilities,
            platform_mcp_configs,
            custom_mcp_servers,
            effective_capabilities: effective_caps,
        }
    }
}

/// 单个 capability 的工具级裁剪描述。
#[derive(Debug)]
struct ResolvedToolFilter {
    capability_key: String,
    include_tools: Vec<String>,
    exclude_tools: Vec<String>,
}

/// 把所有 capability 的 include/exclude 合并为统一的 excluded_tools 集合。
///
/// - `include_tools` 非空时：该 capability 对应 cluster 下不在白名单内的工具被排除
/// - `exclude_tools` 直接添加到排除集
fn compute_excluded_tools(filters: &[ResolvedToolFilter]) -> BTreeSet<String> {
    let mut excluded = BTreeSet::<String>::new();
    for filter in filters {
        let clusters = tool_capability::capability_to_tool_clusters(
            &ToolCapability::new(&filter.capability_key),
        );

        if !filter.include_tools.is_empty() {
            for cluster in &clusters {
                for &tool in cluster_tools(*cluster) {
                    if !filter.include_tools.iter().any(|t| t == tool) {
                        excluded.insert(tool.to_string());
                    }
                }
            }
        }

        for tool_name in &filter.exclude_tools {
            excluded.insert(tool_name.clone());
        }
    }
    excluded
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
            input.owner_ctx.owner_type(),
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
        PlatformMcpScope::Relay => {
            McpInjectionConfig::for_relay(base_url, input.owner_ctx.project_id())
        }
        PlatformMcpScope::Story => {
            let story_id = input.owner_ctx.story_id()?;
            McpInjectionConfig::for_story(base_url, input.owner_ctx.project_id(), story_id)
        }
        PlatformMcpScope::Task => {
            let task_id = input.owner_ctx.task_id()?;
            let story_id = input.owner_ctx.story_id()?;
            McpInjectionConfig::for_task(
                base_url,
                input.owner_ctx.project_id(),
                story_id,
                task_id,
            )
        }
        PlatformMcpScope::Workflow => {
            McpInjectionConfig::for_workflow(base_url, input.owner_ctx.project_id())
        }
    })
}

/// 把领域层的 `McpServerDecl` 转换为 ACP `McpServer`（供 session 注入用）。
///
/// Preset 里 stdio / http / sse 三种 transport 各自映射到 ACP 对应类型,
/// relay 标志在当前 resolver 层不消费（由下游 transport 决定）。
fn mcp_server_decl_to_acp(decl: &McpServerDecl) -> agent_client_protocol::McpServer {
    match decl {
        McpServerDecl::Http { name, url, headers, .. } => {
            let mapped_headers: Vec<HttpHeader> = headers
                .iter()
                .map(|h| HttpHeader::new(h.name.clone(), h.value.clone()))
                .collect();
            McpServer::Http(McpServerHttp::new(name.clone(), url.clone()).headers(mapped_headers))
        }
        McpServerDecl::Sse { name, url, headers, .. } => {
            let mapped_headers: Vec<HttpHeader> = headers
                .iter()
                .map(|h| HttpHeader::new(h.name.clone(), h.value.clone()))
                .collect();
            McpServer::Sse(McpServerSse::new(name.clone(), url.clone()).headers(mapped_headers))
        }
        McpServerDecl::Stdio { name, command, args, env, .. } => {
            let mapped_env: Vec<EnvVariable> = env
                .iter()
                .map(|e| EnvVariable::new(e.name.clone(), e.value.clone()))
                .collect();
            McpServer::Stdio(
                McpServerStdio::new(name.clone(), command.clone())
                    .args(args.clone())
                    .env(mapped_env),
            )
        }
    }
}

fn mcp_server_name(server: &agent_client_protocol::McpServer) -> Option<&str> {
    match server {
        agent_client_protocol::McpServer::Http(http) => Some(http.name.as_str()),
        agent_client_protocol::McpServer::Sse(sse) => Some(sse.name.as_str()),
        agent_client_protocol::McpServer::Stdio(stdio) => Some(stdio.name.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_platform() -> PlatformConfig {
        PlatformConfig {
            mcp_base_url: Some("http://localhost:3001".to_string()),
        }
    }

    fn base_input() -> CapabilityResolverInput {
        CapabilityResolverInput {
            owner_ctx: SessionOwnerCtx::Project {
                project_id: Uuid::new_v4(),
            },
            agent_declared_capabilities: None,
            workflow_ctx: SessionWorkflowContext::NONE,
            agent_mcp_servers: vec![],
            available_presets: Default::default(),
            companion_slice_mode: None,
        }
    }

    #[test]
    fn project_session_gets_expected_clusters() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(output.flow_capabilities.has(ToolCluster::Read), "file_read auto-granted");
        assert!(output.flow_capabilities.has(ToolCluster::Write), "file_write auto-granted");
        assert!(output.flow_capabilities.has(ToolCluster::Execute), "shell_execute auto-granted");
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
            owner_ctx: SessionOwnerCtx::Task {
                project_id,
                story_id,
                task_id,
            },
            agent_declared_capabilities: None,
            workflow_ctx: SessionWorkflowContext::NONE,
            agent_mcp_servers: vec![],
            available_presets: Default::default(),
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
            owner_ctx: SessionOwnerCtx::Story {
                project_id,
                story_id,
            },
            agent_declared_capabilities: None,
            workflow_ctx: SessionWorkflowContext::NONE,
            agent_mcp_servers: vec![],
            available_presets: Default::default(),
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
        input.workflow_ctx.workflow_capability_directives = Some(vec![
            CapabilityDirective::add_simple("mcp:code_analyzer"),
        ]);
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
        input.workflow_ctx.workflow_capability_directives = Some(vec![
            CapabilityDirective::add_simple("mcp:nonexistent"),
        ]);

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(!output
            .effective_capabilities
            .contains(&ToolCapability::custom_mcp("nonexistent")));
    }

    /// Workflow 的 `mcp:<preset>` 可以从 `available_presets` 展开,
    /// 不再依赖 agent config 的 inline mcp_servers。
    #[test]
    fn workflow_mcp_capability_resolves_to_preset() {
        use agentdash_domain::mcp_preset::McpServerDecl;

        let mut input = base_input();
        input.workflow_ctx.workflow_capability_directives = Some(vec![
            CapabilityDirective::add_simple("mcp:code_analyzer"),
        ]);
        input.available_presets.insert(
            "code_analyzer".to_string(),
            McpServerDecl::Http {
                name: "code_analyzer".to_string(),
                url: "http://external:8080/mcp".to_string(),
                headers: vec![],
                relay: None,
            },
        );

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            output
                .effective_capabilities
                .contains(&ToolCapability::custom_mcp("code_analyzer")),
            "preset 命中后 effective_capabilities 应包含 mcp:code_analyzer"
        );
        assert_eq!(output.custom_mcp_servers.len(), 1);
        match &output.custom_mcp_servers[0] {
            agent_client_protocol::McpServer::Http(http) => {
                assert_eq!(http.name, "code_analyzer");
                assert_eq!(http.url, "http://external:8080/mcp");
            }
            other => panic!("期望 Http transport, 实际: {other:?}"),
        }
    }

    /// Preset 与 inline agent mcp_server 同名时以 Preset 为准（不重复注入）。
    #[test]
    fn preset_takes_precedence_over_inline_agent_mcp_server() {
        use agentdash_domain::mcp_preset::McpServerDecl;

        let mut input = base_input();
        input.workflow_ctx.workflow_capability_directives = Some(vec![
            CapabilityDirective::add_simple("mcp:shared"),
        ]);
        input.available_presets.insert(
            "shared".to_string(),
            McpServerDecl::Http {
                name: "shared".to_string(),
                url: "http://preset/mcp".to_string(),
                headers: vec![],
                relay: None,
            },
        );
        input.agent_mcp_servers = vec![AgentMcpServerEntry {
            name: "shared".to_string(),
            server: agent_client_protocol::McpServer::Http(
                agent_client_protocol::McpServerHttp::new("shared", "http://inline/mcp"),
            ),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert_eq!(output.custom_mcp_servers.len(), 1, "同名去重,只保留一条");
        match &output.custom_mcp_servers[0] {
            agent_client_protocol::McpServer::Http(http) => {
                assert_eq!(http.url, "http://preset/mcp", "应以 preset url 为准");
            }
            other => panic!("期望 Http transport, 实际: {other:?}"),
        }
    }

    #[test]
    fn workflow_well_known_can_override_visibility() {
        let mut input = base_input();
        input.workflow_ctx.has_active_workflow = false;
        input.workflow_ctx.workflow_capability_directives = Some(vec![
            CapabilityDirective::add_simple("workflow"),
        ]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.flow_capabilities.has(ToolCluster::Workflow));
    }

    #[test]
    fn workflow_empty_caps_keeps_default_clusters() {
        let mut input = base_input();
        input.workflow_ctx.workflow_capability_directives = Some(vec![]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.flow_capabilities.has(ToolCluster::Read));
        assert!(output.flow_capabilities.has(ToolCluster::Write));
        assert!(output.flow_capabilities.has(ToolCluster::Execute));
        assert!(output.flow_capabilities.has(ToolCluster::Canvas));
        assert!(output.flow_capabilities.has(ToolCluster::Collaboration));
    }

    #[test]
    fn workflow_custom_mcp_is_included_in_output() {
        let mut input = base_input();
        input.workflow_ctx.workflow_capability_directives = Some(vec![
            CapabilityDirective::add_simple("mcp:code_analyzer"),
        ]);
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

    #[test]
    fn workflow_directive_can_remove_default_well_known_capability() {
        let mut input = base_input();
        input.workflow_ctx.workflow_capability_directives = Some(vec![
            CapabilityDirective::Remove("collaboration".into()),
        ]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(!output.flow_capabilities.has(ToolCluster::Collaboration));
        assert!(output.flow_capabilities.has(ToolCluster::Read));
    }

    #[test]
    fn workflow_directive_can_remove_custom_mcp_capability() {
        let mut input = base_input();
        input.workflow_ctx.workflow_capability_directives = Some(vec![
            CapabilityDirective::add_simple("mcp:code_analyzer"),
            CapabilityDirective::Remove("mcp:code_analyzer".into()),
        ]);
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
        assert!(output.custom_mcp_servers.is_empty());
        assert!(!output
            .effective_capabilities
            .contains(&ToolCapability::custom_mcp("code_analyzer")));
    }

    // ── SessionOwnerCtx 变体 × MCP 注入边界回归 ──────────────────────────────
    //
    // 这一组测试固定"sum type 变体所携带的 project/story/task id 必须通过
    // `CapabilityResolver` 透传到最终的 `McpInjectionConfig`"的契约,守住
    // PR1 收口后"四字段 → owner_ctx"改造的核心行为。

    #[test]
    fn project_owner_ctx_injects_relay_with_project_id() {
        let project_id = Uuid::new_v4();
        let input = CapabilityResolverInput {
            owner_ctx: SessionOwnerCtx::Project { project_id },
            agent_declared_capabilities: None,
            workflow_ctx: SessionWorkflowContext::NONE,
            agent_mcp_servers: vec![],
            available_presets: Default::default(),
            companion_slice_mode: None,
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let relay = output
            .platform_mcp_configs
            .iter()
            .find(|c| c.endpoint_url().contains("/mcp/relay"))
            .expect("project owner 应注入 relay MCP");
        assert_eq!(
            relay.project_id, project_id,
            "relay config 应透传 owner_ctx.project_id"
        );
        assert!(relay.story_id.is_none());
        assert!(relay.task_id.is_none());
        assert!(
            !output
                .platform_mcp_configs
                .iter()
                .any(|c| c.endpoint_url().contains("/mcp/story/")
                    || c.endpoint_url().contains("/mcp/task/")),
            "project owner 不应注入 story/task scope"
        );
    }

    #[test]
    fn story_owner_ctx_injects_story_scope_with_story_id() {
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let input = CapabilityResolverInput {
            owner_ctx: SessionOwnerCtx::Story {
                project_id,
                story_id,
            },
            agent_declared_capabilities: None,
            workflow_ctx: SessionWorkflowContext::NONE,
            agent_mcp_servers: vec![],
            available_presets: Default::default(),
            companion_slice_mode: None,
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let story = output
            .platform_mcp_configs
            .iter()
            .find(|c| c.endpoint_url().contains("/mcp/story/"))
            .expect("story owner 应注入 story MCP");
        assert_eq!(story.project_id, project_id);
        assert_eq!(story.story_id, Some(story_id));
        assert!(story.task_id.is_none());
        assert!(
            story.endpoint_url().contains(&story_id.to_string()),
            "story endpoint URL 应包含 story_id, 实际: {}",
            story.endpoint_url()
        );
        assert!(
            !output
                .platform_mcp_configs
                .iter()
                .any(|c| c.endpoint_url().contains("/mcp/task/")),
            "story owner 不应注入 task scope"
        );
    }

    #[test]
    fn task_owner_ctx_injects_task_scope_with_story_and_task_ids() {
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let input = CapabilityResolverInput {
            owner_ctx: SessionOwnerCtx::Task {
                project_id,
                story_id,
                task_id,
            },
            agent_declared_capabilities: None,
            workflow_ctx: SessionWorkflowContext::NONE,
            agent_mcp_servers: vec![],
            available_presets: Default::default(),
            companion_slice_mode: None,
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let task = output
            .platform_mcp_configs
            .iter()
            .find(|c| c.endpoint_url().contains("/mcp/task/"))
            .expect("task owner 应注入 task MCP");
        assert_eq!(task.project_id, project_id);
        assert_eq!(task.story_id, Some(story_id), "task config 应透传 story_id");
        assert_eq!(task.task_id, Some(task_id), "task config 应透传 task_id");
        assert!(
            task.endpoint_url().contains(&task_id.to_string()),
            "task endpoint URL 应包含 task_id, 实际: {}",
            task.endpoint_url()
        );
    }
}
