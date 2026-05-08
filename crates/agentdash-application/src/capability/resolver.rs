//! CapabilityResolver 实现
//!
//! 负责把 workflow + agent baseline + `ToolCapabilityDirective` 序列归约为 session
//! 的有效能力状态：`CapabilityState`。

use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::mcp_preset::McpPreset;
use agentdash_domain::session_binding::SessionOwnerCtx;
use agentdash_domain::workflow::{
    ToolCapabilityDirective, ToolCapabilityReduction, ToolCapabilitySlotState,
    reduce_tool_capability_directives,
};
use agentdash_mcp::injection::McpInjectionConfig;
use agentdash_spi::platform::tool_capability::{
    self, PlatformMcpScope, ToolCapability, WELL_KNOWN_KEYS,
};
use agentdash_spi::context::capability::CompanionAgentEntry;
use agentdash_spi::platform::tool_capability::CAP_COLLABORATION;
use agentdash_spi::{CapabilityState, CompanionSliceMode, ToolCapabilityFilter, ToolCluster};

use crate::capability::SessionWorkflowContext;
use crate::platform_config::PlatformConfig;

/// 调用方预展开的 project 级 MCP Preset 字典。
///
/// key 为 preset `key`（同 `mcp:<key>` 中的 `<key>`），value 为对应 `McpPreset`。
/// resolver 内部保持纯函数；查询 Preset 的 IO 由调用方（例如 `SessionRequestAssembler`）完成,
/// 结果以 map 形式塞进 [`CapabilityResolverInput::available_presets`]。
pub type AvailableMcpPresets = BTreeMap<String, McpPreset>;

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
    /// - `has_active_workflow=false, workflow_tool_directives=None`
    ///   ([`SessionWorkflowContext::NONE`])：使用默认 visibility 规则
    /// - `has_active_workflow=true, workflow_tool_directives=Some(vec)`：
    ///   在默认能力基线上按 `ToolCapabilityDirective` 做标准增删（推荐）
    /// - `has_active_workflow=true, workflow_tool_directives=None`：
    ///   仅激活 `workflow_can_grant` 授予路径，不覆盖能力集
    pub workflow_ctx: SessionWorkflowContext,
    /// 已解析成运行时条目的 Agent MCP 列表。
    /// `mcp:<X>` 解析优先查 `available_presets`，未命中时 fallback 到此列表。
    pub agent_mcp_servers: Vec<AgentMcpServerEntry>,
    /// project 级 MCP Preset 预展开字典 — `mcp:<name>` 的首选查源。
    /// 由调用方在 builder 入口处从 `McpPresetRepository` 批量查出并展开。
    pub available_presets: AvailableMcpPresets,
    /// Companion sub-session 模式 — 设置时，对最终 CapabilityState 施加 slice 裁剪。
    pub companion_slice_mode: Option<CompanionSliceMode>,
    /// 当前 project 中可供调用的 companion agent 候选列表。
    /// Resolver 按 `CAP_COLLABORATION` 可见性决定是否写入 `state.companion.agents`。
    pub available_companions: Vec<CompanionAgentEntry>,
}


/// agent config 中注册的 MCP server 条目（用于 `mcp:*` key 解析）
#[derive(Debug, Clone)]
pub struct AgentMcpServerEntry {
    pub name: String,
    pub server: agentdash_spi::SessionMcpServer,
}

/// Resolver 输出 — session 的有效工具集
#[derive(Debug, Clone)]
pub struct CapabilityResolverOutput {
    /// Resolver 唯一产出的运行态能力状态。
    pub state: CapabilityState,
}

/// 统一工具能力解析器。
///
/// 无状态、纯函数式 — session 上下文通过 `CapabilityResolverInput` 传入，
/// 基础设施配置通过 `&PlatformConfig` 传入。
pub struct CapabilityResolver;

impl CapabilityResolver {
    /// Companion sub-session 的快捷方法 — 仅按 slice_mode 裁剪 CapabilityState。
    ///
    /// Companion 继承父 session 的 MCP/VFS，不需要独立解析平台 MCP。
    pub fn resolve_companion_caps(slice_mode: CompanionSliceMode) -> CapabilityState {
        apply_companion_slice(CapabilityState::all(), slice_mode)
    }

    /// 根据 session 上下文计算有效工具集。
    ///
    /// 核心流程：
    /// 1. baseline = agent auto_granted + agent declared 可见能力集合
    /// 2. 对 workflow_tool_directives 执行 slot 归约（FullCapability /
    ///    ToolWhitelist / Blocked），对 baseline 做覆盖
    /// 3. 解析自定义 MCP (`mcp:<server>`) —— 优先查 preset，回退 agent inline
    /// 4. 映射到 cluster / platform MCP scope / capability-aware tool_policy
    pub fn resolve(
        input: &CapabilityResolverInput,
        platform: &PlatformConfig,
    ) -> CapabilityResolverOutput {
        let agent_declares_set: Option<BTreeSet<&str>> = input
            .agent_declared_capabilities
            .as_ref()
            .map(|caps| caps.iter().map(|s| s.as_str()).collect());

        // baseline：只包含 well-known key 的 agent-level 能力
        let mut effective_caps = default_visible_capabilities(input, agent_declares_set.as_ref());

        let mut resolved_mcp_servers = Vec::<agentdash_spi::SessionMcpServer>::new();
        let mut seen_custom_mcp_names = BTreeSet::<String>::new();

        // ── 按 directive 序列执行 slot 归约 ──
        let directives: &[ToolCapabilityDirective] = input
            .workflow_ctx
            .workflow_tool_directives
            .as_deref()
            .unwrap_or(&[]);
        let reduction: ToolCapabilityReduction = reduce_tool_capability_directives(directives);

        // ── 按 reduction 调整 effective_caps ──
        for (key, state) in &reduction.slots {
            let cap = ToolCapability::new(key);
            match state {
                ToolCapabilitySlotState::Blocked => {
                    // 硬屏蔽：即便 auto_granted 也要从集合剔除
                    effective_caps.remove(&cap);
                }
                ToolCapabilitySlotState::FullCapability
                | ToolCapabilitySlotState::ToolWhitelist(_) => {
                    // well-known 或 custom mcp 均通过此分支启用
                    if cap.is_well_known() {
                        effective_caps.insert(cap);
                    } else if cap.is_custom_mcp() {
                        if let Some(server_name) = cap.custom_mcp_server_name().map(str::to_string)
                        {
                            if let Some(preset) = input.available_presets.get(&server_name) {
                                effective_caps.insert(cap.clone());
                                if seen_custom_mcp_names.insert(server_name.clone()) {
                                    resolved_mcp_servers.push(
                                        crate::mcp_preset::preset_to_session_mcp_server(preset),
                                    );
                                }
                            } else if let Some(agent_entry) = input
                                .agent_mcp_servers
                                .iter()
                                .find(|e| e.name == server_name)
                            {
                                effective_caps.insert(cap.clone());
                                if seen_custom_mcp_names.insert(server_name.clone()) {
                                    resolved_mcp_servers.push(agent_entry.server.clone());
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
                }
                ToolCapabilitySlotState::NotDeclared => {
                    // 兜底；`reduce_tool_capability_directives` 不会产出此状态，留作防御
                }
            }
        }

        // ── 归约产出的 effective_caps 到 ToolCluster / platform MCP scope ──
        let mut tool_clusters = BTreeSet::<ToolCluster>::new();
        for cap in &effective_caps {
            for cluster in tool_capability::capability_to_tool_clusters(cap) {
                tool_clusters.insert(cluster);
            }
            if let Some(scope) = tool_capability::capability_to_platform_mcp_scope(cap) {
                if let Some(config) =
                    build_platform_mcp_config(scope, platform.mcp_base_url.as_deref(), input)
                {
                    resolved_mcp_servers.push(config.to_session_mcp_server());
                }
            }
        }

        // ── 编译工具级过滤策略（ToolWhitelist + Remove(tool) 合集）──
        let tool_policy = compute_tool_policy(&reduction, &effective_caps);

        let companion = if effective_caps.contains(&ToolCapability::new(CAP_COLLABORATION)) {
            agentdash_spi::CompanionDimension {
                agents: input.available_companions.clone(),
            }
        } else {
            agentdash_spi::CompanionDimension::default()
        };

        let mut state = CapabilityState {
            tool: agentdash_spi::ToolDimension {
                capabilities: effective_caps.clone(),
                tool_clusters,
                tool_policy,
                mcp_servers: resolved_mcp_servers,
            },
            companion,
            ..Default::default()
        };

        if let Some(slice_mode) = input.companion_slice_mode {
            state = apply_companion_slice(state, slice_mode);
        }

        CapabilityResolverOutput { state }
    }
}

/// 将 directive reduction 编译成运行态唯一工具过滤表。
fn compute_tool_policy(
    reduction: &ToolCapabilityReduction,
    effective_caps: &BTreeSet<ToolCapability>,
) -> BTreeMap<String, ToolCapabilityFilter> {
    let mut filters = BTreeMap::<String, ToolCapabilityFilter>::new();

    for (key, tools) in &reduction.excluded_tools {
        let cap = ToolCapability::new(key);
        if !effective_caps.contains(&cap) {
            continue;
        }
        let filter = filters.entry(key.clone()).or_default();
        for tool in tools {
            filter.exclude.insert(tool.clone());
        }
    }

    for (key, state) in &reduction.slots {
        if let ToolCapabilitySlotState::ToolWhitelist(whitelist) = state {
            let cap = ToolCapability::new(key);
            if !effective_caps.contains(&cap) {
                continue;
            }
            filters
                .entry(key.clone())
                .or_default()
                .include_only
                .extend(whitelist.iter().cloned());
        }
    }

    filters.retain(|_, filter| !filter.is_empty());
    filters
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

/// Companion slice mode → CapabilityState 约束。
fn apply_companion_slice(base: CapabilityState, mode: CompanionSliceMode) -> CapabilityState {
    match mode {
        CompanionSliceMode::Full => base,
        CompanionSliceMode::Compact => base.intersect(&CapabilityState::from_clusters([
            ToolCluster::Read,
            ToolCluster::Execute,
            ToolCluster::Collaboration,
        ])),
        CompanionSliceMode::WorkflowOnly | CompanionSliceMode::ConstraintsOnly => {
            base.intersect(&CapabilityState::from_clusters([
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
            McpInjectionConfig::for_task(base_url, input.owner_ctx.project_id(), story_id, task_id)
        }
        PlatformMcpScope::Workflow => {
            McpInjectionConfig::for_workflow(base_url, input.owner_ctx.project_id())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_session_mcp(name: &str, url: &str) -> agentdash_spi::SessionMcpServer {
        agentdash_spi::SessionMcpServer {
            name: name.to_string(),
            transport: agentdash_spi::McpTransportConfig::Http {
                url: url.to_string(),
                headers: vec![],
            },
            uses_relay: false,
        }
    }

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
            available_companions: Vec::new(),
        }
    }

    fn state_has_mcp_url(output: &CapabilityResolverOutput, needle: &str) -> bool {
        output.state.tool.mcp_servers.iter().any(|server| {
            matches!(
                &server.transport,
                agentdash_spi::McpTransportConfig::Http { url, .. } if url.contains(needle)
            )
        })
    }

    fn state_mcp_server<'a>(
        output: &'a CapabilityResolverOutput,
        name: &str,
    ) -> Option<&'a agentdash_spi::SessionMcpServer> {
        output
            .state
            .tool
            .mcp_servers
            .iter()
            .find(|server| server.name == name)
    }

    #[test]
    fn project_session_gets_expected_clusters() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            output.state.has(ToolCluster::Read),
            "file_read auto-granted"
        );
        assert!(
            output.state.has(ToolCluster::Write),
            "file_write auto-granted"
        );
        assert!(
            output.state.has(ToolCluster::Execute),
            "shell_execute auto-granted"
        );
        assert!(output.state.has(ToolCluster::Canvas));
        assert!(output.state.has(ToolCluster::Collaboration));
        assert!(!output.state.has(ToolCluster::Workflow));
    }

    #[test]
    fn project_session_gets_relay_mcp() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert_eq!(output.state.tool.mcp_servers.len(), 1);
        assert!(state_has_mcp_url(&output, "/mcp/relay"));
    }

    #[test]
    fn project_session_with_workflow_management() {
        let mut input = base_input();
        input.agent_declared_capabilities = Some(vec!["workflow_management".to_string()]);

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let has_workflow_mcp = state_has_mcp_url(&output, "/mcp/workflow/");
        assert!(has_workflow_mcp, "应注入 WorkflowMcpServer");
    }

    #[test]
    fn project_session_without_workflow_declaration_no_workflow_mcp() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input, &test_platform());

        let has_workflow_mcp = state_has_mcp_url(&output, "/mcp/workflow/");
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
            available_companions: Vec::new(),
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let has_task_mcp = state_has_mcp_url(&output, "/mcp/task/");
        assert!(has_task_mcp, "task session 应注入 TaskMcpServer");

        let has_relay_mcp = state_has_mcp_url(&output, "/mcp/relay");
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
            available_companions: Vec::new(),
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let has_story_mcp = state_has_mcp_url(&output, "/mcp/story/");
        assert!(has_story_mcp, "story session 应注入 StoryMcpServer");
    }

    #[test]
    fn workflow_cluster_requires_active_workflow() {
        let mut input = base_input();
        let platform = test_platform();
        let output_no_workflow = CapabilityResolver::resolve(&input, &platform);
        assert!(!output_no_workflow.state.has(ToolCluster::Workflow));

        input.workflow_ctx.has_active_workflow = true;
        let output_with_workflow = CapabilityResolver::resolve(&input, &platform);
        assert!(output_with_workflow.state.has(ToolCluster::Workflow));
    }

    #[test]
    fn no_mcp_base_url_produces_no_platform_mcp() {
        let input = base_input();
        let platform = PlatformConfig { mcp_base_url: None };
        let output = CapabilityResolver::resolve(&input, &platform);
        assert!(output.state.tool.mcp_servers.is_empty());
    }

    #[test]
    fn custom_mcp_from_workflow_resolved() {
        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::add_simple(
                "mcp:code_analyzer",
            )]);
        input.agent_mcp_servers = vec![AgentMcpServerEntry {
            name: "code_analyzer".to_string(),
            server: test_session_mcp("code_analyzer", "http://external:8080/mcp"),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            output
                .state
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("code_analyzer"))
        );
    }

    #[test]
    fn custom_mcp_missing_server_not_resolved() {
        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::add_simple("mcp:nonexistent")]);

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            !output
                .state
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("nonexistent"))
        );
    }

    /// Workflow 的 `mcp:<preset>` 可以从 `available_presets` 展开,
    /// 不再依赖 agent config 的 inline mcp_servers。
    #[test]
    fn workflow_mcp_capability_resolves_to_preset() {
        use agentdash_domain::mcp_preset::{McpPreset, McpRoutePolicy, McpTransportConfig};

        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::add_simple(
                "mcp:code_analyzer",
            )]);
        input.available_presets.insert(
            "code_analyzer".to_string(),
            McpPreset::new_user(
                Uuid::new_v4(),
                "code_analyzer",
                "Code Analyzer",
                None,
                McpTransportConfig::Http {
                    url: "http://external:8080/mcp".to_string(),
                    headers: vec![],
                },
                McpRoutePolicy::Direct,
            ),
        );

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            output
                .state
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("code_analyzer")),
            "preset 命中后 capabilities 应包含 mcp:code_analyzer"
        );
        let server = state_mcp_server(&output, "code_analyzer").expect("应注入 code_analyzer");
        match &server.transport {
            agentdash_spi::McpTransportConfig::Http { url, .. } => {
                assert_eq!(url, "http://external:8080/mcp");
            }
            other => panic!("期望 Http transport, 实际: {other:?}"),
        }
    }

    /// Preset 与 inline agent mcp_server 同名时以 Preset 为准（不重复注入）。
    #[test]
    fn preset_takes_precedence_over_inline_agent_mcp_server() {
        use agentdash_domain::mcp_preset::{McpPreset, McpRoutePolicy, McpTransportConfig};

        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::add_simple("mcp:shared")]);
        input.available_presets.insert(
            "shared".to_string(),
            McpPreset::new_user(
                Uuid::new_v4(),
                "shared",
                "Shared",
                None,
                McpTransportConfig::Http {
                    url: "http://preset/mcp".to_string(),
                    headers: vec![],
                },
                McpRoutePolicy::Direct,
            ),
        );
        input.agent_mcp_servers = vec![AgentMcpServerEntry {
            name: "shared".to_string(),
            server: test_session_mcp("shared", "http://inline/mcp"),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert_eq!(
            output
                .state
                .tool
                .mcp_servers
                .iter()
                .filter(|server| server.name == "shared")
                .count(),
            1,
            "同名去重,只保留一条"
        );
        let server = state_mcp_server(&output, "shared").expect("应注入 shared");
        match &server.transport {
            agentdash_spi::McpTransportConfig::Http { url, .. } => {
                assert_eq!(url, "http://preset/mcp", "应以 preset url 为准");
            }
            other => panic!("期望 Http transport, 实际: {other:?}"),
        }
    }

    #[test]
    fn workflow_well_known_can_override_visibility() {
        let mut input = base_input();
        input.workflow_ctx.has_active_workflow = false;
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::add_simple("workflow")]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.state.has(ToolCluster::Workflow));
    }

    #[test]
    fn workflow_empty_caps_keeps_default_clusters() {
        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives = Some(vec![]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.state.has(ToolCluster::Read));
        assert!(output.state.has(ToolCluster::Write));
        assert!(output.state.has(ToolCluster::Execute));
        assert!(output.state.has(ToolCluster::Canvas));
        assert!(output.state.has(ToolCluster::Collaboration));
    }

    #[test]
    fn workflow_custom_mcp_is_included_in_output() {
        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::add_simple(
                "mcp:code_analyzer",
            )]);
        input.agent_mcp_servers = vec![AgentMcpServerEntry {
            name: "code_analyzer".to_string(),
            server: test_session_mcp("code_analyzer", "http://external:8080/mcp"),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(state_mcp_server(&output, "code_analyzer").is_some());
        assert!(
            output
                .state
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("code_analyzer"))
        );
    }

    #[test]
    fn workflow_directive_can_remove_default_well_known_capability() {
        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::remove_simple(
                "collaboration",
            )]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(!output.state.has(ToolCluster::Collaboration));
        assert!(output.state.has(ToolCluster::Read));
    }

    #[test]
    fn workflow_directive_can_remove_shell_execute() {
        // PRD 关键场景：workflow 声明 Remove("shell_execute") 必须能屏蔽 baseline
        // 中 auto_granted 的能力。
        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::remove_simple(
                "shell_execute",
            )]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(
            !output.state.has(ToolCluster::Execute),
            "Remove(shell_execute) 应屏蔽 Execute cluster"
        );
        assert!(
            output.state.has(ToolCluster::Read),
            "Read cluster 不应受影响"
        );
    }

    #[test]
    fn workflow_directive_remove_tool_keeps_capability_but_excludes_tool() {
        // Remove(file_read::fs_grep) 应保留 file_read 能力，但屏蔽对应 capability 下的 fs_grep
        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::remove_tool(
                "file_read",
                "fs_grep",
            )]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(
            output.state.has(ToolCluster::Read),
            "file_read 能力整体仍可见"
        );
        assert!(
            output.state.is_tool_path_excluded("file_read", "fs_grep"),
            "fs_grep 应进入 file_read 的工具过滤策略"
        );
    }

    #[test]
    fn workflow_management_plan_keeps_mcp_but_blocks_upsert_tools() {
        let mut input = base_input();
        input.workflow_ctx = SessionWorkflowContext {
            has_active_workflow: true,
            workflow_tool_directives: Some(vec![
                ToolCapabilityDirective::add_simple("workflow_management"),
                ToolCapabilityDirective::remove_tool("workflow_management", "upsert_workflow_tool"),
                ToolCapabilityDirective::remove_tool(
                    "workflow_management",
                    "upsert_lifecycle_tool",
                ),
            ]),
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            output
                .state
                .tool
                .capabilities
                .contains(&ToolCapability::new("workflow_management")),
            "Plan 阶段仍需要 workflow_management 的只读工具"
        );
        assert!(
            state_has_mcp_url(&output, "/mcp/workflow/"),
            "workflow_management capability 应继续注入 Workflow MCP server"
        );
        assert!(output.state.is_capability_tool_enabled(
            "workflow_management",
            "get_workflow",
            None
        ));
        assert!(!output.state.is_capability_tool_enabled(
            "workflow_management",
            "upsert_workflow_tool",
            None
        ));
        assert!(!output.state.is_capability_tool_enabled(
            "workflow_management",
            "upsert_lifecycle_tool",
            None
        ));
        assert_eq!(
            output.state.excluded_tool_paths(),
            BTreeSet::from([
                "workflow_management::upsert_lifecycle_tool".to_string(),
                "workflow_management::upsert_workflow_tool".to_string(),
            ])
        );
    }

    #[test]
    fn workflow_directive_can_remove_custom_mcp_capability() {
        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives = Some(vec![
            ToolCapabilityDirective::add_simple("mcp:code_analyzer"),
            ToolCapabilityDirective::remove_simple("mcp:code_analyzer"),
        ]);
        input.agent_mcp_servers = vec![AgentMcpServerEntry {
            name: "code_analyzer".to_string(),
            server: test_session_mcp("code_analyzer", "http://external:8080/mcp"),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(state_mcp_server(&output, "code_analyzer").is_none());
        assert!(
            !output
                .state
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("code_analyzer"))
        );
    }

    #[test]
    fn workflow_directive_add_tool_whitelist_excludes_other_tools() {
        // Add(file_read::fs_read) → whitelist 只保留 fs_read，
        // 其他 read 工具（mounts_list/fs_glob/fs_grep）不再可见。
        let mut input = base_input();
        input.workflow_ctx.workflow_tool_directives =
            Some(vec![ToolCapabilityDirective::add_tool(
                "file_read",
                "fs_read",
            )]);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.state.has(ToolCluster::Read));
        assert!(output.state.is_capability_tool_enabled(
            "file_read",
            "fs_read",
            Some(ToolCluster::Read)
        ));
        assert!(!output.state.is_capability_tool_enabled(
            "file_read",
            "fs_grep",
            Some(ToolCluster::Read)
        ));
        assert!(!output.state.is_capability_tool_enabled(
            "file_read",
            "fs_glob",
            Some(ToolCluster::Read)
        ));
        assert!(!output.state.is_capability_tool_enabled(
            "file_read",
            "mounts_list",
            Some(ToolCluster::Read)
        ));
    }

    // ── SessionOwnerCtx 变体 × MCP 注入边界回归 ──────────────────────────────

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
            available_companions: Vec::new(),
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let relay = output
            .state
            .tool
            .mcp_servers
            .iter()
            .find(|server| {
                matches!(
                    &server.transport,
                    agentdash_spi::McpTransportConfig::Http { url, .. } if url.contains("/mcp/relay")
                )
            })
            .expect("project owner 应注入 relay MCP");
        assert_eq!(relay.name, "agentdash-relay-tools");
        assert!(
            !output.state.tool.mcp_servers.iter().any(|server| matches!(
                &server.transport,
                agentdash_spi::McpTransportConfig::Http { url, .. }
                    if url.contains("/mcp/story/") || url.contains("/mcp/task/")
            )),
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
            available_companions: Vec::new(),
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let story = output
            .state
            .tool
            .mcp_servers
            .iter()
            .find(|server| {
                matches!(
                    &server.transport,
                    agentdash_spi::McpTransportConfig::Http { url, .. } if url.contains("/mcp/story/")
                )
            })
            .expect("story owner 应注入 story MCP");
        let agentdash_spi::McpTransportConfig::Http { url, .. } = &story.transport else {
            panic!("story MCP 应使用 HTTP transport");
        };
        assert!(url.contains(&story_id.to_string()));
        assert!(
            !output.state.tool.mcp_servers.iter().any(|server| matches!(
                &server.transport,
                agentdash_spi::McpTransportConfig::Http { url, .. } if url.contains("/mcp/task/")
            )),
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
            available_companions: Vec::new(),
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let task = output
            .state
            .tool
            .mcp_servers
            .iter()
            .find(|server| {
                matches!(
                    &server.transport,
                    agentdash_spi::McpTransportConfig::Http { url, .. } if url.contains("/mcp/task/")
                )
            })
            .expect("task owner 应注入 task MCP");
        let agentdash_spi::McpTransportConfig::Http { url, .. } = &task.transport else {
            panic!("task MCP 应使用 HTTP transport");
        };
        assert!(url.contains(&task_id.to_string()));
    }
}
