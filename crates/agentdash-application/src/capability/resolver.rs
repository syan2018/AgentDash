//! CapabilityResolver 实现
//!
//! 负责把各来源 contributions（agent / workflow）+ MCP 候选 归约为 session
//! 的有效能力状态：`CapabilityState`。

use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::mcp_preset::McpPreset;
use agentdash_spi::{CapabilityScope, CapabilityScopeCtx};
use agentdash_domain::workflow::{
    ToolCapabilityDirective, ToolCapabilityReduction, ToolCapabilitySlotState,
    reduce_tool_capability_directives,
};
use agentdash_mcp::injection::McpInjectionConfig;
use agentdash_spi::context::capability::CompanionAgentEntry;
use agentdash_spi::platform::tool_capability::{
    self, CAP_COLLABORATION, CAP_WORKFLOW, PlatformMcpScope, ToolCapability, WELL_KNOWN_KEYS,
};
use agentdash_spi::{CapabilityState, CompanionSliceMode, ToolCapabilityFilter, ToolCluster};

use crate::platform_config::PlatformConfig;

// ── 公共类型定义 ──────────────────────────────────────────────────────

/// 调用方预展开的 project 级 MCP Preset 字典。
pub type AvailableMcpPresets = BTreeMap<String, McpPreset>;

/// Tool 维度的 contribution（来自单个来源）。
#[derive(Debug, Clone, Default)]
pub struct ToolContribution {
    /// 该来源产出的 capability directives。
    pub directives: Vec<ToolCapabilityDirective>,
    /// 标记来源中是否存在活跃 workflow（影响 visibility 判定）。
    pub has_active_workflow: bool,
}

/// MCP server 候选数据源（独立于 contribution）。
#[derive(Debug, Clone, Default)]
pub struct McpCandidates {
    /// project 级 MCP Preset 预展开字典。
    pub presets: AvailableMcpPresets,
    /// agent 内联 MCP servers。
    pub agent_servers: Vec<AgentMcpServerEntry>,
}

/// Companion 维度的 contribution。
#[derive(Debug, Clone, Default)]
pub struct CompanionContribution {
    /// 可用 companion 候选列表。
    pub available: Vec<CompanionAgentEntry>,
}

/// contribution 的来源，用于区分 agent 声明、workflow 声明与资源候选。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextContributionSource {
    Agent,
    Workflow,
    Resource,
}

/// 各来源对各维度的贡献汇总。
#[derive(Debug, Clone)]
pub struct ContextContributions {
    pub source: ContextContributionSource,
    pub tool: Option<ToolContribution>,
    pub companion: Option<CompanionContribution>,
}

/// 增强型能力解析上下文 — 包含 session owner 与 run link 两条解析路径。
///
/// `owner_ctx` 为传统 Session-based visibility 路径（保留兼容）；
/// `run_context` 为新的 LifecycleRunLink-based 路径（后续 AgentPermission 在此扩展）。
#[derive(Debug, Clone, Default)]
pub struct CapabilityContext {
    /// Run 关联的 subject kinds（由 LifecycleRunLink 投影）。
    /// 例如 run 关联 Story → 此处含 `RunLinkSubjectKind::Story`。
    pub run_subject_kinds: Vec<agentdash_domain::workflow::RunLinkSubjectKind>,
    /// Permission Grant 授予的 capability keys（由 active grants 解析）。
    /// 任何出现在此集合中的 well-known key 视为已授权可见，绕过静态规则。
    pub granted_capability_keys: BTreeSet<String>,
}

/// Resolver 输入 — 纯粹的 session 上下文描述。
#[derive(Debug, Clone)]
pub struct CapabilityResolverInput {
    /// session 归属上下文（决定 visibility 基线 + platform MCP scope）。
    pub owner_ctx: CapabilityScopeCtx,
    /// 各来源按 directive 应用顺序排列的 contributions；授权语义由 `source` 显式决定。
    pub contributions: Vec<ContextContributions>,
    /// MCP server 候选数据源。
    pub mcp_candidates: McpCandidates,
    /// LifecycleRunLink-based 解析上下文（可选，新路径）。
    #[allow(dead_code)]
    pub capability_context: Option<CapabilityContext>,
}

/// agent config 中注册的 MCP server 条目（用于 `mcp:*` key 解析）
#[derive(Debug, Clone)]
pub struct AgentMcpServerEntry {
    pub name: String,
    pub server: agentdash_spi::SessionMcpServer,
}

// ── Resolver 内部合并中间态 ──────────────────────────────────────────

/// 从 contributions 合并产出的 Tool 维度中间态。
struct MergedToolInput {
    /// 从 agent 来源的 Add directives 提取的 key 集合（用于 visibility 判定）。
    agent_declared_keys: BTreeSet<String>,
    /// 按 directive 顺序归约后，仍由合法 source 授权启用的 well-known key。
    source_grantable_keys: BTreeSet<String>,
    /// 合并后的全部 directives（按 contributions 顺序 concat）。
    directives: Vec<ToolCapabilityDirective>,
    /// 任一来源标记了 has_active_workflow。
    has_active_workflow: bool,
}

/// 从 contributions 合并产出 MergedToolInput。
fn merge_contributions(
    owner_ctx: &CapabilityScopeCtx,
    contributions: &[ContextContributions],
) -> MergedToolInput {
    let mut agent_declared_keys = BTreeSet::new();
    let mut source_grantable_keys = BTreeSet::new();
    let mut directives = Vec::new();
    let mut has_active_workflow = false;

    for contrib in contributions {
        if let Some(tool) = &contrib.tool {
            if tool.has_active_workflow {
                has_active_workflow = true;
            }
            for d in &tool.directives {
                if let ToolCapabilityDirective::Add(path) = d {
                    if contrib.source == ContextContributionSource::Agent {
                        agent_declared_keys.insert(path.capability.clone());
                    }
                    let source_can_enable = source_can_enable_capability(
                        owner_ctx,
                        contrib.source,
                        tool.has_active_workflow,
                        path.capability.as_str(),
                    );
                    match &path.tool {
                        None => {
                            if source_can_enable {
                                source_grantable_keys.insert(path.capability.clone());
                            } else {
                                source_grantable_keys.remove(path.capability.as_str());
                            }
                        }
                        Some(_) => {
                            if source_can_enable {
                                source_grantable_keys.insert(path.capability.clone());
                            }
                        }
                    }
                } else if let ToolCapabilityDirective::Remove(path) = d
                    && path.tool.is_none()
                {
                    source_grantable_keys.remove(path.capability.as_str());
                }
            }
            directives.extend(tool.directives.iter().cloned());
        }
    }

    MergedToolInput {
        agent_declared_keys,
        source_grantable_keys,
        directives,
        has_active_workflow,
    }
}

/// 从 contributions 聚合 companion 候选。
fn merge_companion_candidates(contributions: &[ContextContributions]) -> Vec<CompanionAgentEntry> {
    contributions
        .iter()
        .filter_map(|c| c.companion.as_ref())
        .flat_map(|c| c.available.iter().cloned())
        .collect()
}

/// Resolver 输出 = CapabilityState（唯一运行态能力容器）。
pub type CapabilityResolverOutput = CapabilityState;

/// 统一工具能力解析器。
///
/// 无状态、纯函数式 — session 上下文通过 `CapabilityResolverInput` 传入，
/// 基础设施配置通过 `&PlatformConfig` 传入。
pub struct CapabilityResolver;

impl CapabilityResolver {
    /// Companion sub-session 的快捷方法 — 仅按 slice_mode 裁剪 CapabilityState。
    pub fn resolve_companion_caps(slice_mode: CompanionSliceMode) -> CapabilityState {
        apply_companion_slice(CapabilityState::all(), slice_mode)
    }

    /// 根据 session 上下文计算有效工具集。
    ///
    /// 核心流程：
    /// 1. 合并 contributions → MergedToolInput
    /// 2. baseline = auto_granted + agent declared 可见能力集合
    /// 3. 对全部 directives 执行 slot 归约，对 baseline 做覆盖
    /// 4. 解析自定义 MCP → 映射到 cluster / platform MCP scope
    pub fn resolve(
        input: &CapabilityResolverInput,
        platform: &PlatformConfig,
    ) -> CapabilityResolverOutput {
        let merged = merge_contributions(&input.owner_ctx, &input.contributions);

        // baseline：只包含 well-known key 的 agent-level 能力
        let granted_keys = input
            .capability_context
            .as_ref()
            .map(|ctx| &ctx.granted_capability_keys);
        let mut effective_caps =
            default_visible_capabilities(&input.owner_ctx, &merged, granted_keys);

        let mut resolved_mcp_servers = Vec::<agentdash_spi::SessionMcpServer>::new();
        let mut seen_custom_mcp_names = BTreeSet::<String>::new();

        // ── 按 directive 序列执行 slot 归约 ──
        let reduction: ToolCapabilityReduction =
            reduce_tool_capability_directives(&merged.directives);

        // ── 按 reduction 调整 effective_caps ──
        for (key, state) in &reduction.slots {
            let cap = ToolCapability::new(key);
            match state {
                ToolCapabilitySlotState::Blocked => {
                    effective_caps.remove(&cap);
                }
                ToolCapabilitySlotState::FullCapability
                | ToolCapabilitySlotState::ToolWhitelist(_) => {
                    if cap.is_well_known() {
                        if can_enable_well_known_capability(&cap, &input.owner_ctx, &merged) {
                            effective_caps.insert(cap);
                        } else {
                            effective_caps.remove(&cap);
                        }
                    } else if cap.is_custom_mcp() {
                        if let Some(server_name) = cap.custom_mcp_server_name().map(str::to_string)
                        {
                            if let Some(preset) = input.mcp_candidates.presets.get(&server_name) {
                                effective_caps.insert(cap.clone());
                                if seen_custom_mcp_names.insert(server_name.clone()) {
                                    resolved_mcp_servers.push(
                                        crate::mcp_preset::preset_to_session_mcp_server(preset),
                                    );
                                }
                            } else if let Some(agent_entry) = input
                                .mcp_candidates
                                .agent_servers
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
                                    "directive 声明了 mcp:{server_name}，但 McpPreset 和 agent 内联都未注册"
                                );
                            }
                        }
                    }
                }
                ToolCapabilitySlotState::NotDeclared => {}
            }
        }

        // ── effective_caps → ToolCluster / platform MCP scope ──
        let mut enabled_clusters = BTreeSet::<ToolCluster>::new();
        for cap in &effective_caps {
            for cluster in tool_capability::capability_to_tool_clusters(cap) {
                enabled_clusters.insert(cluster);
            }
            if let Some(scope) = tool_capability::capability_to_platform_mcp_scope(cap) {
                if let Some(config) = build_platform_mcp_config(
                    scope,
                    platform.mcp_base_url.as_deref(),
                    &input.owner_ctx,
                ) {
                    resolved_mcp_servers.push(config.to_session_mcp_server());
                }
            }
        }

        let tool_policy = compute_tool_policy(&reduction, &effective_caps);

        let companion_candidates = merge_companion_candidates(&input.contributions);
        let companion = if effective_caps.contains(&ToolCapability::new(CAP_COLLABORATION)) {
            agentdash_spi::CompanionDimension {
                agents: companion_candidates,
            }
        } else {
            agentdash_spi::CompanionDimension::default()
        };

        CapabilityState {
            tool: agentdash_spi::ToolDimension {
                capabilities: effective_caps.clone(),
                enabled_clusters,
                tool_policy,
                mcp_servers: resolved_mcp_servers,
            },
            companion,
            ..Default::default()
        }
    }

    /// resolve 后对 CapabilityState 施加 companion slice 裁剪。
    ///
    /// companion_slice_mode 是 session 上下文管理概念，不在 resolver 输入中。
    /// 调用方在 resolve() 之后按需调用此方法。
    pub fn apply_companion_slice(
        state: CapabilityState,
        mode: CompanionSliceMode,
    ) -> CapabilityState {
        apply_companion_slice(state, mode)
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
    owner_ctx: &CapabilityScopeCtx,
    merged: &MergedToolInput,
    granted_keys: Option<&BTreeSet<String>>,
) -> BTreeSet<ToolCapability> {
    let mut effective = BTreeSet::new();
    for &key in WELL_KNOWN_KEYS {
        let cap = ToolCapability::new(key);

        // Permission Grant override: 如果 key 在 active grants 中，直接可见
        if granted_keys.is_some_and(|gk| gk.contains(key)) {
            effective.insert(cap);
            continue;
        }

        let agent_declares_this = merged.agent_declared_keys.contains(key);
        let workflow_declares_this = key == CAP_WORKFLOW && merged.has_active_workflow;
        if tool_capability::is_capability_visible(
            &cap,
            owner_ctx.owner_type(),
            agent_declares_this,
            workflow_declares_this,
        ) {
            effective.insert(cap);
        }
    }
    effective
}

fn can_enable_well_known_capability(
    cap: &ToolCapability,
    owner_ctx: &CapabilityScopeCtx,
    merged: &MergedToolInput,
) -> bool {
    let workflow_declares_this = cap.key() == CAP_WORKFLOW && merged.has_active_workflow;
    tool_capability::is_capability_visible(
        cap,
        owner_ctx.owner_type(),
        false,
        workflow_declares_this,
    ) || merged.source_grantable_keys.contains(cap.key())
}

fn source_can_enable_capability(
    owner_ctx: &CapabilityScopeCtx,
    source: ContextContributionSource,
    has_active_workflow: bool,
    capability_key: &str,
) -> bool {
    let cap = ToolCapability::new(capability_key);
    if !cap.is_well_known() {
        return true;
    }
    match source {
        ContextContributionSource::Agent => {
            tool_capability::is_capability_visible(&cap, owner_ctx.owner_type(), true, false)
        }
        ContextContributionSource::Workflow => tool_capability::is_capability_visible(
            &cap,
            owner_ctx.owner_type(),
            false,
            has_active_workflow,
        ),
        ContextContributionSource::Resource => false,
    }
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

fn build_platform_mcp_config(
    scope: PlatformMcpScope,
    mcp_base_url: Option<&str>,
    owner_ctx: &CapabilityScopeCtx,
) -> Option<McpInjectionConfig> {
    let base_url = mcp_base_url?;

    Some(match scope {
        PlatformMcpScope::Relay => McpInjectionConfig::for_relay(base_url, owner_ctx.project_id()),
        PlatformMcpScope::Story => {
            let story_id = owner_ctx.story_id()?;
            McpInjectionConfig::for_story(base_url, owner_ctx.project_id(), story_id)
        }
        PlatformMcpScope::Task => {
            let task_id = owner_ctx.task_id()?;
            let story_id = owner_ctx.story_id()?;
            McpInjectionConfig::for_task(base_url, owner_ctx.project_id(), story_id, task_id)
        }
        PlatformMcpScope::Workflow => {
            McpInjectionConfig::for_workflow(base_url, owner_ctx.project_id())
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
            owner_ctx: CapabilityScopeCtx::Project {
                project_id: Uuid::new_v4(),
            },
            contributions: Vec::new(),
            mcp_candidates: McpCandidates::default(),
            capability_context: None,
        }
    }

    /// 向 input 追加 workflow 维度的 tool contribution。
    fn with_workflow_directives(
        input: &mut CapabilityResolverInput,
        directives: Vec<ToolCapabilityDirective>,
        has_active_workflow: bool,
    ) {
        input.contributions.push(ContextContributions {
            source: ContextContributionSource::Workflow,
            tool: Some(ToolContribution {
                directives,
                has_active_workflow,
            }),
            companion: None,
        });
    }

    fn state_has_mcp_url(output: &CapabilityResolverOutput, needle: &str) -> bool {
        output.tool.mcp_servers.iter().any(|server| {
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
            .tool
            .mcp_servers
            .iter()
            .find(|server| server.name == name)
    }

    #[test]
    fn project_session_gets_expected_clusters() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(output.has(ToolCluster::Read), "file_read auto-granted");
        assert!(output.has(ToolCluster::Write), "file_write auto-granted");
        assert!(
            output.has(ToolCluster::Execute),
            "shell_execute auto-granted"
        );
        assert!(output.has(ToolCluster::Canvas));
        assert!(output.has(ToolCluster::Collaboration));
        assert!(!output.has(ToolCluster::Workflow));
    }

    #[test]
    fn project_session_gets_relay_mcp() {
        let input = base_input();
        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert_eq!(output.tool.mcp_servers.len(), 1);
        assert!(state_has_mcp_url(&output, "/mcp/relay"));
    }

    #[test]
    fn project_session_with_workflow_management() {
        let mut input = base_input();
        input.contributions.push(ContextContributions {
            source: ContextContributionSource::Agent,
            tool: Some(ToolContribution {
                directives: vec![ToolCapabilityDirective::add_simple("workflow_management")],
                has_active_workflow: false,
            }),
            companion: None,
        });

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
            owner_ctx: CapabilityScopeCtx::Task {
                project_id,
                story_id,
                task_id,
            },
            contributions: Vec::new(),
            mcp_candidates: McpCandidates::default(),
            capability_context: None,
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
            owner_ctx: CapabilityScopeCtx::Story {
                project_id,
                story_id,
            },
            contributions: Vec::new(),
            mcp_candidates: McpCandidates::default(),
            capability_context: None,
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let has_story_mcp = state_has_mcp_url(&output, "/mcp/story/");
        assert!(has_story_mcp, "story session 应注入 StoryMcpServer");
    }

    #[test]
    fn workflow_cluster_requires_active_workflow() {
        let input = base_input();
        let platform = test_platform();
        let output_no_workflow = CapabilityResolver::resolve(&input, &platform);
        assert!(!output_no_workflow.has(ToolCluster::Workflow));

        let mut input_with = base_input();
        with_workflow_directives(&mut input_with, vec![], true);
        let output_with_workflow = CapabilityResolver::resolve(&input_with, &platform);
        assert!(output_with_workflow.has(ToolCluster::Workflow));
    }

    #[test]
    fn active_workflow_without_management_directive_does_not_inject_workflow_mcp() {
        let mut input = base_input();
        with_workflow_directives(&mut input, vec![], true);

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            !output
                .tool
                .capabilities
                .contains(&ToolCapability::new("workflow_management")),
            "active workflow 本身只授予 workflow 运行能力，不应隐式授予 workflow_management"
        );
        assert!(
            !state_has_mcp_url(&output, "/mcp/workflow/"),
            "缺少 workflow_management directive 时不应注入 Workflow MCP"
        );
    }

    #[test]
    fn resource_contribution_cannot_grant_well_known_capability() {
        let mut input = base_input();
        input.contributions.push(ContextContributions {
            source: ContextContributionSource::Resource,
            tool: Some(ToolContribution {
                directives: vec![ToolCapabilityDirective::add_simple("workflow_management")],
                has_active_workflow: false,
            }),
            companion: None,
        });

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            !output
                .tool
                .capabilities
                .contains(&ToolCapability::new("workflow_management")),
            "Resource 来源只提供候选资源，不能授权 well-known capability"
        );
    }

    #[test]
    fn no_mcp_base_url_produces_no_platform_mcp() {
        let input = base_input();
        let platform = PlatformConfig { mcp_base_url: None };
        let output = CapabilityResolver::resolve(&input, &platform);
        assert!(output.tool.mcp_servers.is_empty());
    }

    #[test]
    fn custom_mcp_from_workflow_resolved() {
        let mut input = base_input();
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::add_simple("mcp:code_analyzer")],
            true,
        );
        input.mcp_candidates.agent_servers = vec![AgentMcpServerEntry {
            name: "code_analyzer".to_string(),
            server: test_session_mcp("code_analyzer", "http://external:8080/mcp"),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            output
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("code_analyzer"))
        );
    }

    #[test]
    fn custom_mcp_missing_server_not_resolved() {
        let mut input = base_input();
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::add_simple("mcp:nonexistent")],
            true,
        );

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            !output
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
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::add_simple("mcp:code_analyzer")],
            true,
        );
        input.mcp_candidates.presets.insert(
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
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::add_simple("mcp:shared")],
            true,
        );
        input.mcp_candidates.presets.insert(
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
        input.mcp_candidates.agent_servers = vec![AgentMcpServerEntry {
            name: "shared".to_string(),
            server: test_session_mcp("shared", "http://inline/mcp"),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert_eq!(
            output
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
    fn workflow_well_known_respects_source_visibility() {
        let mut input = base_input();
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::add_simple("workflow")],
            false,
        );

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(!output.has(ToolCluster::Workflow));

        let mut input = base_input();
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::add_simple("workflow")],
            true,
        );
        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.has(ToolCluster::Workflow));
    }

    #[test]
    fn workflow_empty_caps_keeps_default_clusters() {
        let mut input = base_input();
        with_workflow_directives(&mut input, vec![], true);

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.has(ToolCluster::Read));
        assert!(output.has(ToolCluster::Write));
        assert!(output.has(ToolCluster::Execute));
        assert!(output.has(ToolCluster::Canvas));
        assert!(output.has(ToolCluster::Collaboration));
    }

    #[test]
    fn workflow_custom_mcp_is_included_in_output() {
        let mut input = base_input();
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::add_simple("mcp:code_analyzer")],
            true,
        );
        input.mcp_candidates.agent_servers = vec![AgentMcpServerEntry {
            name: "code_analyzer".to_string(),
            server: test_session_mcp("code_analyzer", "http://external:8080/mcp"),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(state_mcp_server(&output, "code_analyzer").is_some());
        assert!(
            output
                .tool
                .capabilities
                .contains(&ToolCapability::custom_mcp("code_analyzer"))
        );
    }

    #[test]
    fn workflow_directive_can_remove_default_well_known_capability() {
        let mut input = base_input();
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::remove_simple("collaboration")],
            true,
        );

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(!output.has(ToolCluster::Collaboration));
        assert!(output.has(ToolCluster::Read));
    }

    #[test]
    fn workflow_directive_can_remove_shell_execute() {
        // PRD 关键场景：workflow 声明 Remove("shell_execute") 必须能屏蔽 baseline
        // 中 auto_granted 的能力。
        let mut input = base_input();
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::remove_simple("shell_execute")],
            true,
        );

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(
            !output.has(ToolCluster::Execute),
            "Remove(shell_execute) 应屏蔽 Execute cluster"
        );
        assert!(output.has(ToolCluster::Read), "Read cluster 不应受影响");
    }

    #[test]
    fn workflow_directive_remove_tool_keeps_capability_but_excludes_tool() {
        // Remove(file_read::fs_grep) 应保留 file_read 能力，但屏蔽对应 capability 下的 fs_grep
        let mut input = base_input();
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::remove_tool("file_read", "fs_grep")],
            true,
        );

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.has(ToolCluster::Read), "file_read 能力整体仍可见");
        assert!(
            output.is_tool_path_excluded("file_read", "fs_grep"),
            "fs_grep 应进入 file_read 的工具过滤策略"
        );
    }

    #[test]
    fn workflow_management_plan_keeps_mcp_but_blocks_upsert_tools() {
        let mut input = base_input();
        input.contributions.push(ContextContributions {
            source: ContextContributionSource::Workflow,
            tool: Some(ToolContribution {
                directives: vec![
                    ToolCapabilityDirective::add_simple("workflow_management"),
                    ToolCapabilityDirective::remove_tool(
                        "workflow_management",
                        "upsert_workflow_tool",
                    ),
                    ToolCapabilityDirective::remove_tool(
                        "workflow_management",
                        "upsert_lifecycle_tool",
                    ),
                ],
                has_active_workflow: true,
            }),
            companion: None,
        });

        let output = CapabilityResolver::resolve(&input, &test_platform());

        assert!(
            output
                .tool
                .capabilities
                .contains(&ToolCapability::new("workflow_management")),
            "Plan 阶段仍需要 workflow_management 的只读工具"
        );
        assert!(
            state_has_mcp_url(&output, "/mcp/workflow/"),
            "workflow_management capability 应继续注入 Workflow MCP server"
        );
        assert!(output.is_capability_tool_enabled("workflow_management", "get_workflow", None));
        assert!(!output.is_capability_tool_enabled(
            "workflow_management",
            "upsert_workflow_tool",
            None
        ));
        assert!(!output.is_capability_tool_enabled(
            "workflow_management",
            "upsert_lifecycle_tool",
            None
        ));
        assert_eq!(
            output.excluded_tool_paths(),
            BTreeSet::from([
                "workflow_management::upsert_lifecycle_tool".to_string(),
                "workflow_management::upsert_workflow_tool".to_string(),
            ])
        );
    }

    #[test]
    fn workflow_directive_can_remove_custom_mcp_capability() {
        let mut input = base_input();
        with_workflow_directives(
            &mut input,
            vec![
                ToolCapabilityDirective::add_simple("mcp:code_analyzer"),
                ToolCapabilityDirective::remove_simple("mcp:code_analyzer"),
            ],
            true,
        );
        input.mcp_candidates.agent_servers = vec![AgentMcpServerEntry {
            name: "code_analyzer".to_string(),
            server: test_session_mcp("code_analyzer", "http://external:8080/mcp"),
        }];

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(state_mcp_server(&output, "code_analyzer").is_none());
        assert!(
            !output
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
        with_workflow_directives(
            &mut input,
            vec![ToolCapabilityDirective::add_tool("file_read", "fs_read")],
            true,
        );

        let output = CapabilityResolver::resolve(&input, &test_platform());
        assert!(output.has(ToolCluster::Read));
        assert!(output.is_capability_tool_enabled("file_read", "fs_read", Some(ToolCluster::Read)));
        assert!(!output.is_capability_tool_enabled(
            "file_read",
            "fs_grep",
            Some(ToolCluster::Read)
        ));
        assert!(!output.is_capability_tool_enabled(
            "file_read",
            "fs_glob",
            Some(ToolCluster::Read)
        ));
        assert!(!output.is_capability_tool_enabled(
            "file_read",
            "mounts_list",
            Some(ToolCluster::Read)
        ));
    }

    // ── CapabilityScope 变体 × MCP 注入边界回归 ──────────────────────────────

    #[test]
    fn project_owner_ctx_injects_relay_with_project_id() {
        let project_id = Uuid::new_v4();
        let input = CapabilityResolverInput {
            owner_ctx: CapabilityScopeCtx::Project { project_id },
            contributions: Vec::new(),
            mcp_candidates: McpCandidates::default(),
            capability_context: None,
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let relay = output
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
            !output.tool.mcp_servers.iter().any(|server| matches!(
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
            owner_ctx: CapabilityScopeCtx::Story {
                project_id,
                story_id,
            },
            contributions: Vec::new(),
            mcp_candidates: McpCandidates::default(),
            capability_context: None,
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let story = output
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
            !output.tool.mcp_servers.iter().any(|server| matches!(
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
            owner_ctx: CapabilityScopeCtx::Task {
                project_id,
                story_id,
                task_id,
            },
            contributions: Vec::new(),
            mcp_candidates: McpCandidates::default(),
            capability_context: None,
        };

        let output = CapabilityResolver::resolve(&input, &test_platform());

        let task = output
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
