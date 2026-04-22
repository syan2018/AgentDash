//! `activate_step` — 单 step 激活的唯一计算入口。
//!
//! 把分散在 `plan_builder` / `session_runtime_inputs` / `turn_context` /
//! `orchestrator` / `advance_node` 五处的"查 workflow → 算 capabilities →
//! 调 Resolver → 拼 MCP list → 拼 kickoff prompt → 构建 lifecycle mount"收敛
//! 到同一纯函数,消费者通过 applier 把产物写入不同目标(`PromptSessionRequest` /
//! 新 session bootstrap / 热更新运行中 session)。
//!
//! ## 设计原则
//!
//! - **纯计算**:`activate_step` 本身不做 IO;所有外部状态(workflow 定义 /
//!   agent MCP servers / available presets / baseline caps)都通过 input 传入。
//! - **port 前驱状态剥离**:kickoff prompt 构造需要知道 "前驱 output port 是否就绪",
//!   这部分 IO 由调用方先查好,以 `BTreeSet<String>` 形式塞进 `kickoff_context`。
//! - **baseline 可覆盖**:默认 baseline = `workflow.contract.capability_directives`;
//!   PhaseNode 热更新路径可传 `baseline_override = Some(hook_runtime.current_caps())`,
//!   再叠加 directive 得到新能力集。

use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::session_binding::SessionOwnerCtx;
use agentdash_domain::workflow::{
    CapabilityDirective, LifecycleEdge, LifecycleStepDefinition, WorkflowDefinition,
};
use agentdash_spi::hooks::{CapabilityDelta, SharedHookSessionRuntime};
use agentdash_spi::{FlowCapabilities, Vfs};
use uuid::Uuid;

use crate::capability::{
    AgentMcpServerEntry, AvailableMcpPresets, CapabilityResolver, CapabilityResolverInput,
    CompanionSliceMode, SessionWorkflowContext,
};
use crate::platform_config::PlatformConfig;
use crate::session::SessionHub;
use crate::vfs::build_lifecycle_mount_with_ports;

/// 激活一个 lifecycle step 所需的全部纯计算输入。
///
/// 构造方不应在这里做 IO;`workflow` 等来自 `ActiveWorkflowProjection` 或
/// `LifecycleDefinitionRepository::get_by_project_and_key` 的 cached 结果。
#[derive(Debug, Clone)]
pub struct StepActivationInput<'a> {
    /// session 所属的 owner sum type。
    pub owner_ctx: SessionOwnerCtx,
    /// 当前激活的 step 定义;提供 output/input ports、node_type、workflow_key。
    pub active_step: &'a LifecycleStepDefinition,
    /// step 绑定的 workflow 定义(若有);提供 `contract.capability_directives` baseline 与
    /// injection/hook_rules/constraints/completion。
    pub workflow: Option<&'a WorkflowDefinition>,
    /// lifecycle 的 run_id,用于构建 `lifecycle://<run_id>/artifacts/...` mount。
    pub run_id: Uuid,
    /// lifecycle key,lifecycle mount 路径的一部分。
    pub lifecycle_key: &'a str,
    /// lifecycle 全部 edges,kickoff prompt 生成前驱 port 引用时用。
    pub edges: &'a [LifecycleEdge],
    /// agent config 中显式声明的 capability key 列表(非 workflow 路径才生效)。
    pub agent_declared_capabilities: Option<Vec<String>>,
    /// agent config 内联 MCP server(向前兼容 `mcp:<name>` 解析)。
    pub agent_mcp_servers: Vec<AgentMcpServerEntry>,
    /// project 级 MCP Preset 预展开字典。
    pub available_presets: AvailableMcpPresets,
    /// Companion 子 session 的 slice 裁剪模式。
    pub companion_slice_mode: Option<CompanionSliceMode>,
    /// capability baseline 覆盖:PhaseNode 热更新时传入当前 hook runtime 的能力指令序列,
    /// 会取代 `workflow.contract.capability_directives`。
    /// None → 使用 `workflow.contract.capability_directives`。
    pub baseline_override: Option<Vec<CapabilityDirective>>,
    /// 运行时 capability 指令(PhaseNode 热更新场景);追加到 baseline 后由 slot 归约。
    pub capability_directives: &'a [CapabilityDirective],
    /// 已就绪的前驱 output port key 集合,kickoff prompt 标注状态时使用。
    /// 调用方提前通过 `load_port_output_map` 查好,activate_step 不做 IO。
    pub ready_port_keys: BTreeSet<String>,
}

/// kickoff prompt 的结构化片段 — 不组装成最终 prompt 文本,由 applier 决定拼接方式。
#[derive(Debug, Clone, Default)]
pub struct KickoffPromptFragment {
    /// "你正在执行 lifecycle X 的 node Y (描述)" 的主标题行。
    pub title_line: String,
    /// output port 交付要求块(含 `lifecycle://artifacts/...` 路径)。
    pub output_section: String,
    /// input port 引用块(含前驱 "已就绪/未就绪" 状态)。
    pub input_section: String,
}

impl KickoffPromptFragment {
    /// 默认拼接方式:主标题 + 完成 tool 提示 + output + input。
    /// applier 可自行替换。
    pub fn to_default_prompt(&self) -> String {
        format!(
            "{}\n请先完成当前阶段工作,并在完成后调用 `complete_lifecycle_node` 工具提交总结与产物。{}{}",
            self.title_line, self.output_section, self.input_section
        )
    }
}

/// activate_step 的完整产出 — 各 applier 从中摘取所需字段。
#[derive(Debug, Clone)]
pub struct StepActivation {
    /// 内置工具簇(PiAgent 内部使用)。
    pub flow_capabilities: FlowCapabilities,
    /// 合并并去重后的 MCP server 列表(platform + custom)。
    pub mcp_servers: Vec<agent_client_protocol::McpServer>,
    /// 已解析通过的 capability key 集合(供 hook runtime 初始化、日志、delta 对比)。
    pub capability_keys: BTreeSet<String>,
    /// kickoff prompt 结构化片段;若 step 没有 port/workflow,字段可能全为空。
    pub kickoff_prompt: KickoffPromptFragment,
    /// 带 output port 写入权限的 lifecycle mount。
    pub lifecycle_mount: agentdash_domain::common::Mount,
    /// 完整 Vfs(仅 lifecycle mount;applier 若需要更多 mount,自行扩展)。
    pub lifecycle_vfs: Vfs,
}

/// 单 step 激活的计算核心。纯函数,不做 IO。
///
/// 当前实现需要 `&PlatformConfig` 才能走 Resolver;对外入口走
/// [`activate_step_with_platform`]。保留此函数签名作为未来把 platform
/// 内联进 `StepActivationInput` 时的占位。
#[deprecated(note = "use activate_step_with_platform instead")]
pub fn activate_step(_input: &StepActivationInput<'_>) -> StepActivation {
    unreachable!("activate_step must be called via activate_step_with_platform");
}

/// 单 step 激活 — 显式接收 `&PlatformConfig`(resolver 需要)。
///
/// 这是对外的真实入口;`activate_step(...)` 是未来若能把 platform
/// 内联进 input 时的无参版本,当前为占位。
pub fn activate_step_with_platform(
    input: &StepActivationInput<'_>,
    platform: &PlatformConfig,
) -> StepActivation {
    // ── 1. baseline + override + directive → 合并 directive 序列 ──
    //
    // 合并策略：baseline (workflow contract 或 override) 在前，运行时 delta 在后；
    // `CapabilityResolver` 内部的 slot 归约「后来者胜」，保证运行时增删覆盖 baseline。
    let baseline_directives: Vec<CapabilityDirective> =
        input.baseline_override.clone().unwrap_or_else(|| {
            input
                .workflow
                .map(|w| w.contract.capability_directives.clone())
                .unwrap_or_default()
        });

    let mut combined_directives = baseline_directives;
    combined_directives.extend(input.capability_directives.iter().cloned());

    let has_active_workflow = input.workflow.is_some();
    let workflow_ctx = if has_active_workflow {
        SessionWorkflowContext {
            has_active_workflow: true,
            workflow_capability_directives: Some(combined_directives),
        }
    } else {
        SessionWorkflowContext::NONE
    };

    // ── 2. 调 Resolver ──
    let cap_input = CapabilityResolverInput {
        owner_ctx: input.owner_ctx,
        agent_declared_capabilities: input.agent_declared_capabilities.clone(),
        workflow_ctx,
        agent_mcp_servers: input.agent_mcp_servers.clone(),
        available_presets: input.available_presets.clone(),
        companion_slice_mode: input.companion_slice_mode,
    };
    let cap_output = CapabilityResolver::resolve(&cap_input, platform);

    // ── 3. 汇总 MCP server 列表(platform + custom),去重 ──
    let mut mcp_servers: Vec<agent_client_protocol::McpServer> = cap_output
        .platform_mcp_configs
        .iter()
        .map(|c| c.to_acp_mcp_server())
        .collect();
    mcp_servers.extend(cap_output.custom_mcp_servers);
    dedupe_mcp_servers(&mut mcp_servers);

    let capability_keys: BTreeSet<String> = cap_output
        .effective_capabilities
        .iter()
        .map(|c| c.key().to_string())
        .collect();

    // ── 4. lifecycle mount + Vfs ──
    let writable_port_keys: Vec<String> = input
        .active_step
        .output_ports
        .iter()
        .map(|p| p.key.clone())
        .collect();
    let lifecycle_mount =
        build_lifecycle_mount_with_ports(input.run_id, input.lifecycle_key, &writable_port_keys);
    let lifecycle_vfs = Vfs {
        mounts: vec![lifecycle_mount.clone()],
        default_mount_id: None,
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    };

    // ── 5. kickoff prompt fragment ──
    let kickoff_prompt = build_kickoff_prompt_fragment(input);

    StepActivation {
        flow_capabilities: cap_output.flow_capabilities,
        mcp_servers,
        capability_keys,
        kickoff_prompt,
        lifecycle_mount,
        lifecycle_vfs,
    }
}

fn build_kickoff_prompt_fragment(input: &StepActivationInput<'_>) -> KickoffPromptFragment {
    let node_key = &input.active_step.key;
    let desc = input.active_step.description.trim();
    let node_title = if desc.is_empty() {
        format!("`{node_key}`")
    } else {
        format!("`{node_key}`({desc})")
    };
    let title_line = format!(
        "你正在执行 lifecycle `{}` 的 node {}。",
        input.lifecycle_key, node_title
    );

    let output_section = render_output_section(&input.active_step.output_ports);
    let input_section = render_input_section(
        &input.active_step.input_ports,
        node_key,
        input.edges,
        &input.ready_port_keys,
    );

    KickoffPromptFragment {
        title_line,
        output_section,
        input_section,
    }
}

fn render_output_section(ports: &[agentdash_domain::workflow::OutputPortDefinition]) -> String {
    if ports.is_empty() {
        return String::new();
    }
    let items: Vec<String> = ports
        .iter()
        .map(|p| format!("- `lifecycle://artifacts/{}` — {}", p.key, p.description))
        .collect();
    format!(
        "\n\n## 必须交付的产出\n请将以下产出通过 `write_file` 写入对应路径:\n{}\n\n**所有 output port 写入完成后**再调用 `complete_lifecycle_node`。",
        items.join("\n")
    )
}

fn render_input_section(
    ports: &[agentdash_domain::workflow::InputPortDefinition],
    node_key: &str,
    edges: &[LifecycleEdge],
    ready_port_keys: &BTreeSet<String>,
) -> String {
    if ports.is_empty() {
        return String::new();
    }
    let mut items = Vec::new();
    for ip in ports {
        // Port 级输入只匹配 artifact edge；flow edge 不承载 port 关系
        let source_edges: Vec<_> = edges
            .iter()
            .filter(|e| e.to_node == *node_key && e.to_port.as_deref() == Some(ip.key.as_str()))
            .collect();
        if source_edges.is_empty() {
            items.push(format!("- **{}**({}) — 无前驱连接", ip.key, ip.description));
        } else {
            for edge in source_edges {
                let from_port = edge.from_port.as_deref().unwrap_or("");
                let status = if ready_port_keys.contains(from_port) {
                    "已就绪"
                } else {
                    "未就绪"
                };
                items.push(format!(
                    "- **{}**({}) ← `lifecycle://artifacts/{from_port}` [{status}]",
                    ip.key, ip.description
                ));
            }
        }
    }
    format!(
        "\n\n## 输入上下文\n以下是来自前驱节点的产出,可通过 `read_file` 读取:\n{}",
        items.join("\n")
    )
}

fn dedupe_mcp_servers(servers: &mut Vec<agent_client_protocol::McpServer>) {
    let mut seen = BTreeSet::<String>::new();
    servers.retain(|server| {
        let Some(name) = mcp_server_name(server) else {
            return true;
        };
        seen.insert(name.to_string())
    });
}

fn mcp_server_name(server: &agent_client_protocol::McpServer) -> Option<&str> {
    match server {
        agent_client_protocol::McpServer::Http(http) => Some(http.name.as_str()),
        agent_client_protocol::McpServer::Sse(sse) => Some(sse.name.as_str()),
        agent_client_protocol::McpServer::Stdio(stdio) => Some(stdio.name.as_str()),
        _ => None,
    }
}

// ─── Appliers ─────────────────────────────────────────────
//
// 三个 applier 对应三条激活路径:
//   A. Bootstrap 新 session —— apply_to_prompt_request
//   B. Orchestrator 创建 AgentNode session —— apply_to_new_lifecycle_session (PR4 实现)
//   C. PhaseNode / advance tool 热更新 —— apply_to_running_session
//
// 当前已提供 A / C；B 仍待后续把 orchestrator 的 session 创建流程进一步收口。

/// Applier A:把 `StepActivation` 的产物合入一份新构造的 `PromptSessionRequest`。
///
/// 调用方负责提供 base `req`(携带 user input + executor_config 等);本函数只写
/// `vfs / flow_capabilities / effective_capability_keys / mcp_servers` 字段。
/// kickoff_prompt 由调用方按需调 `activation.kickoff_prompt.to_default_prompt()` 拼进 user input。
pub fn apply_to_prompt_request(
    activation: &StepActivation,
    req: &mut crate::session::PromptSessionRequest,
) {
    req.vfs = Some(activation.lifecycle_vfs.clone());
    req.flow_capabilities = Some(activation.flow_capabilities.clone());
    req.effective_capability_keys = Some(activation.capability_keys.iter().cloned().collect());
    // mcp_servers 是 Vec,调用方若已有值则 extend + dedupe;此处全量覆盖作为默认策略。
    req.mcp_servers = activation.mcp_servers.clone();
}

/// Applier C:把 `StepActivation` 的 capability / MCP 结果应用到运行中的 session。
///
/// 返回 capability delta；若无能力变化则返回 `Ok(None)`，并保持与旧实现一致：
/// 不触发 MCP 替换、steering 注入或 capability changed hook。
pub async fn apply_to_running_session(
    activation: &StepActivation,
    hook_session: &SharedHookSessionRuntime,
    session_hub: &SessionHub,
    turn_id: Option<&str>,
    phase_node_key: &str,
) -> Result<Option<CapabilityDelta>, String> {
    let Some(delta) = hook_session.update_capabilities(activation.capability_keys.clone()) else {
        return Ok(None);
    };

    session_hub
        .replace_runtime_mcp_servers(hook_session.session_id(), activation.mcp_servers.clone())
        .await
        .map_err(|error| format!("Phase node MCP 热更新失败: {error}"))?;

    let delta_md = crate::capability::build_capability_delta_markdown(
        phase_node_key,
        &delta,
        &activation.capability_keys,
    );
    session_hub
        .push_session_notification(hook_session.session_id(), delta_md)
        .await
        .map_err(|error| format!("Phase node capability delta 消息注入失败: {error}"))?;

    session_hub
        .emit_capability_changed_hook(
            hook_session.session_id(),
            turn_id,
            serde_json::json!({
                "phase_node": phase_node_key,
                "added": delta.added.clone(),
                "removed": delta.removed.clone(),
                "capabilities": activation.capability_keys.iter().cloned().collect::<Vec<_>>(),
                "mcp_server_count": activation.mcp_servers.len(),
            }),
        )
        .await;

    Ok(Some(delta))
}

/// 便捷函数:把 CapabilityResolver 的 effective capability key 集转成有序 Vec,
/// 供 hook runtime 初始化时注入。
pub fn capability_keys_sorted(activation: &StepActivation) -> Vec<String> {
    activation.capability_keys.iter().cloned().collect()
}

/// 把 capability key 集转换成 `Add(simple)` 指令序列。
pub fn capability_directives_from_keys(keys: &BTreeSet<String>) -> Vec<CapabilityDirective> {
    keys.iter()
        .cloned()
        .map(CapabilityDirective::add_simple)
        .collect()
}

/// 计算两组 capability key 集之间的指令化 delta（added/removed）。
pub fn capability_delta_directives(
    old_keys: &BTreeSet<String>,
    new_keys: &BTreeSet<String>,
) -> Vec<CapabilityDirective> {
    let mut directives: Vec<CapabilityDirective> = new_keys
        .difference(old_keys)
        .cloned()
        .map(CapabilityDirective::add_simple)
        .collect();
    directives.extend(
        old_keys
            .difference(new_keys)
            .cloned()
            .map(CapabilityDirective::remove_simple),
    );
    directives
}

/// 从当前 runtime MCP server 列表构造 `AgentMcpServerEntry`，供 step activation
/// 在 phase 热更新路径里解析自定义 `mcp:<name>` 能力。
pub fn agent_mcp_entries_from_servers(
    servers: &[agent_client_protocol::McpServer],
) -> Vec<AgentMcpServerEntry> {
    servers
        .iter()
        .filter_map(|server| {
            let name = match server {
                agent_client_protocol::McpServer::Http(http) => http.name.clone(),
                agent_client_protocol::McpServer::Sse(sse) => sse.name.clone(),
                agent_client_protocol::McpServer::Stdio(stdio) => stdio.name.clone(),
                _ => return None,
            };
            Some(AgentMcpServerEntry {
                uses_relay: false,
                name,
                server: server.clone(),
            })
        })
        .collect()
}

// ─── available_presets 辅助 ────────────────────────────────

/// 构造空的 `AvailableMcpPresets`(调用方未预展开时的占位)。
pub fn empty_presets() -> AvailableMcpPresets {
    BTreeMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{
        LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, WorkflowBindingKind,
        WorkflowContract, WorkflowDefinition, WorkflowDefinitionSource,
    };

    fn sample_step(
        output_ports: Vec<agentdash_domain::workflow::OutputPortDefinition>,
    ) -> LifecycleStepDefinition {
        LifecycleStepDefinition {
            key: "implement".to_string(),
            description: "实现并记录结果".to_string(),
            workflow_key: Some("wf_impl".to_string()),
            node_type: Default::default(),
            output_ports,
            input_ports: vec![],
        }
    }

    fn sample_workflow(directives: Vec<CapabilityDirective>) -> WorkflowDefinition {
        let contract = WorkflowContract {
            capability_directives: directives,
            ..WorkflowContract::default()
        };
        WorkflowDefinition::new(
            Uuid::new_v4(),
            "wf_impl",
            "Workflow Implement",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow")
    }

    fn test_platform() -> PlatformConfig {
        PlatformConfig {
            mcp_base_url: Some("http://localhost:3001".to_string()),
        }
    }

    #[test]
    fn activate_step_no_workflow_uses_default_visibility() {
        let step = sample_step(vec![]);
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let input = StepActivationInput {
            owner_ctx: SessionOwnerCtx::Task {
                project_id,
                story_id,
                task_id,
            },
            active_step: &step,
            workflow: None,
            run_id: Uuid::new_v4(),
            lifecycle_key: "trellis_dev_task",
            edges: &[],
            agent_declared_capabilities: None,
            agent_mcp_servers: vec![],
            available_presets: empty_presets(),
            companion_slice_mode: None,
            baseline_override: None,
            capability_directives: &[],
            ready_port_keys: BTreeSet::new(),
        };

        let out = activate_step_with_platform(&input, &test_platform());
        // 无 workflow,走默认 visibility —— task scope 至少能拿到 Read/Write/Execute
        assert!(
            !out.capability_keys.is_empty(),
            "default visibility 应产出至少一个能力"
        );
    }

    #[test]
    fn activate_step_with_workflow_uses_contract_capabilities_as_baseline() {
        let workflow = sample_workflow(vec![
            CapabilityDirective::add_simple("file_read"),
            CapabilityDirective::add_simple("file_write"),
            CapabilityDirective::add_simple("shell_execute"),
            CapabilityDirective::add_simple("workflow_management"),
        ]);
        let step = sample_step(vec![]);
        let project_id = Uuid::new_v4();

        let input = StepActivationInput {
            owner_ctx: SessionOwnerCtx::Project { project_id },
            active_step: &step,
            workflow: Some(&workflow),
            run_id: Uuid::new_v4(),
            lifecycle_key: "lc_admin",
            edges: &[],
            agent_declared_capabilities: None,
            agent_mcp_servers: vec![],
            available_presets: empty_presets(),
            companion_slice_mode: None,
            baseline_override: None,
            capability_directives: &[],
            ready_port_keys: BTreeSet::new(),
        };

        let out = activate_step_with_platform(&input, &test_platform());
        assert!(out.capability_keys.contains("workflow_management"));
        // file_read/write/shell_execute 现在是独立 directive
        assert!(out.capability_keys.contains("file_read"));
        assert!(out.capability_keys.contains("file_write"));
        assert!(out.capability_keys.contains("shell_execute"));
    }

    #[test]
    fn activate_step_baseline_override_takes_precedence_over_contract() {
        let workflow = sample_workflow(vec![CapabilityDirective::add_simple("file_read")]);
        let step = sample_step(vec![]);
        let project_id = Uuid::new_v4();

        // PhaseNode 热更新场景:baseline 来自 hook_runtime.current_capabilities()
        let input = StepActivationInput {
            owner_ctx: SessionOwnerCtx::Project { project_id },
            active_step: &step,
            workflow: Some(&workflow),
            run_id: Uuid::new_v4(),
            lifecycle_key: "lc",
            edges: &[],
            agent_declared_capabilities: None,
            agent_mcp_servers: vec![],
            available_presets: empty_presets(),
            companion_slice_mode: None,
            baseline_override: Some(vec![
                CapabilityDirective::add_simple("canvas"),
                CapabilityDirective::add_simple("collaboration"),
                // 显式屏蔽 workflow contract 原有的 file_read
                CapabilityDirective::remove_simple("file_read"),
            ]),
            capability_directives: &[CapabilityDirective::add_simple("workflow_management")],
            ready_port_keys: BTreeSet::new(),
        };

        let out = activate_step_with_platform(&input, &test_platform());
        // baseline_override = canvas + collaboration + Remove(file_read),
        // directive = +workflow_management
        // workflow.contract.capability_directives = file_read 被 override 替代
        assert!(out.capability_keys.contains("canvas"));
        assert!(out.capability_keys.contains("collaboration"));
        assert!(out.capability_keys.contains("workflow_management"));
        // 注意：auto_granted baseline 会带入 file_read；但我们在 override 里写了 Remove(file_read)，
        // 所以最终不应包含 file_read。
        assert!(!out.capability_keys.contains("file_read"));
    }

    #[test]
    fn kickoff_prompt_fragment_contains_output_port_lines() {
        let ports = vec![agentdash_domain::workflow::OutputPortDefinition {
            key: "summary".to_string(),
            description: "本 step 的结论摘要".to_string(),
            gate_strategy: Default::default(),
            gate_params: None,
        }];
        let step = sample_step(ports);
        let project_id = Uuid::new_v4();

        let input = StepActivationInput {
            owner_ctx: SessionOwnerCtx::Project { project_id },
            active_step: &step,
            workflow: None,
            run_id: Uuid::new_v4(),
            lifecycle_key: "lc",
            edges: &[],
            agent_declared_capabilities: None,
            agent_mcp_servers: vec![],
            available_presets: empty_presets(),
            companion_slice_mode: None,
            baseline_override: None,
            capability_directives: &[],
            ready_port_keys: BTreeSet::new(),
        };

        let out = activate_step_with_platform(&input, &test_platform());
        assert!(
            out.kickoff_prompt
                .output_section
                .contains("lifecycle://artifacts/summary")
        );
        assert!(
            out.kickoff_prompt
                .output_section
                .contains("本 step 的结论摘要")
        );
        assert!(
            out.kickoff_prompt
                .output_section
                .contains("complete_lifecycle_node")
        );
    }

    #[test]
    fn kickoff_prompt_fragment_marks_ready_ports() {
        let step = LifecycleStepDefinition {
            key: "b".to_string(),
            description: String::new(),
            workflow_key: None,
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![agentdash_domain::workflow::InputPortDefinition {
                key: "ctx".to_string(),
                description: "前驱上下文".to_string(),
                context_strategy: Default::default(),
                context_template: None,
            }],
        };
        let edges = vec![LifecycleEdge::artifact("a", "out", "b", "ctx")];
        let project_id = Uuid::new_v4();
        let ready: BTreeSet<String> = ["out".to_string()].into_iter().collect();

        let input = StepActivationInput {
            owner_ctx: SessionOwnerCtx::Project { project_id },
            active_step: &step,
            workflow: None,
            run_id: Uuid::new_v4(),
            lifecycle_key: "lc",
            edges: &edges,
            agent_declared_capabilities: None,
            agent_mcp_servers: vec![],
            available_presets: empty_presets(),
            companion_slice_mode: None,
            baseline_override: None,
            capability_directives: &[],
            ready_port_keys: ready,
        };

        let out = activate_step_with_platform(&input, &test_platform());
        assert!(
            out.kickoff_prompt
                .input_section
                .contains("lifecycle://artifacts/out")
        );
        assert!(out.kickoff_prompt.input_section.contains("已就绪"));
    }

    // 保留未使用的 import 以免 clippy warning
    #[allow(dead_code)]
    fn _ref_unused_for_imports(_d: LifecycleDefinition, _r: LifecycleRun) {}
}
