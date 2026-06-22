//! `activate_activity` — 单 activity 激活的唯一计算入口。
//!
//! 把分散在 `plan_builder` / `session_runtime_inputs` / `turn_context` /
//! `orchestrator` / `advance_node` 五处的"查 workflow → 算 capabilities →
//! 调 Resolver → 拼 MCP list → 拼 kickoff prompt → 构建 lifecycle mount"收敛
//! 到同一纯函数,消费者通过 applier 把产物写入不同目标(新 session bootstrap /
//! 热更新运行中 session)。
//!
//! ## 设计原则
//!
//! - **纯计算**:`activate_activity` 本身不做 IO;所有外部状态(workflow 定义 /
//!   available presets / baseline caps)都通过 input 传入。
//! - **port 前驱状态剥离**:kickoff prompt 构造需要知道 "前驱 output port 是否就绪",
//!   这部分 IO 由调用方先查好,以 `BTreeSet<String>` 形式塞进 `kickoff_context`。
//! - **baseline 可覆盖**:默认 baseline = `workflow.contract.capability_config.tool_directives`;
//!   PhaseNode 热更新路径可传 `baseline_override = Some(hook_runtime.current_caps())`,
//!   再叠加 directive 得到新能力集。

use std::collections::BTreeSet;

use agentdash_domain::workflow::{
    ActivityDefinition, AgentProcedureContract, MountDirective, ToolCapabilityDirective,
};
use agentdash_spi::CapabilityScopeCtx;
use agentdash_spi::{CapabilityState, Vfs};
use uuid::Uuid;

use crate::capability::{
    AuthorityState, AvailableMcpPresets, CapabilityResolver, CapabilityResolverInput,
    CompanionContribution, CompanionSliceMode, ContextContributionSource, ContextContributions,
    McpCandidates, ToolContribution,
};
use crate::platform_config::PlatformConfig;
use crate::vfs::build_lifecycle_mount_with_node_scope;

/// 激活一个 lifecycle activity 所需的全部纯计算输入。
///
/// 构造方不应在这里做 IO;`workflow` 等来自 `ActiveWorkflowProjection` 或
/// `LifecycleDefinitionRepository::get_by_project_and_key` 的 cached 结果。
#[derive(Debug, Clone)]
pub struct ActivityActivationInput<'a> {
    /// Session 的能力作用域（包含 entity ID，用于 MCP scope 注入）。
    pub owner_ctx: CapabilityScopeCtx,
    /// 当前激活的 activity 定义;提供 output/input ports、key、description。
    /// node_type / procedure_key 由 executor 推导,激活计算本身不需要。
    pub active_activity: &'a ActivityDefinition,
    /// activity 绑定的 AgentProcedure 执行合同(若有);提供 capability baseline 与 mount overlay。
    pub workflow_contract: Option<&'a AgentProcedureContract>,
    /// AgentFrame 当前 revision 的 VFS。运行时 MCP binding 以 base VFS + lifecycle overlay
    /// 组成的有效 VFS 为事实源，而不是以 RuntimeSession 记录作为事实源。
    pub base_vfs: Option<&'a Vfs>,
    /// lifecycle 的 run_id,用于构建 `lifecycle://<run_id>/artifacts/...` mount。
    pub run_id: Uuid,
    /// 当前 Activity 所属 orchestration instance，用于把 lifecycle VFS 绑定到运行态事实源。
    pub orchestration_id: Uuid,
    /// 当前 Activity 在 orchestration runtime 中的稳定 node path。
    pub node_path: &'a str,
    /// 当前 runtime node attempt，用于把 lifecycle VFS artifact 写入绑定到精确 attempt。
    pub attempt: u32,
    /// lifecycle key,lifecycle mount 路径的一部分。
    pub lifecycle_key: &'a str,
    /// project 级 MCP Preset 预展开字典。
    pub available_presets: AvailableMcpPresets,
    /// 当前运行身份对应的 authority；resolver 以它裁剪能力与工具入口。
    pub authority_state: AuthorityState,
    /// Agent 侧 capability directives。Companion child 选择 ProjectAgent 时通过此字段
    /// 保留 selected Agent preset 与 workflow contract 的来源边界。
    pub agent_tool_directives: Vec<ToolCapabilityDirective>,
    /// Companion 子 session 的 slice 裁剪模式（resolve 后应用，不混入 resolver 输入）。
    pub companion_slice_mode: Option<CompanionSliceMode>,
    /// capability baseline 覆盖:PhaseNode 热更新时传入当前 hook runtime 的能力指令序列,
    /// 会取代 `workflow.contract.capability_config.tool_directives`。
    /// None → 使用 `workflow.contract.capability_config.tool_directives`。
    pub baseline_override: Option<Vec<ToolCapabilityDirective>>,
    /// 运行时 capability 指令(PhaseNode 热更新场景);追加到 baseline 后由 slot 归约。
    pub tool_directives: &'a [ToolCapabilityDirective],
    /// 已就绪的前驱 output port key 集合,kickoff prompt 标注状态时使用。
    /// 调用方提前按 runtime node scope 查好，activate_step 不做 IO。
    pub ready_port_keys: BTreeSet<String>,
    /// Companion agent 候选列表（workflow/lifecycle 路径通常为空）。
    pub available_companions: Vec<agentdash_spi::context::capability::CompanionAgentEntry>,
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

/// activate_activity 的完整产出 — 各 applier 从中摘取所需字段。
#[derive(Debug, Clone)]
pub struct ActivityActivation {
    /// 内置工具簇(PiAgent 内部使用)。
    pub capability_state: CapabilityState,
    /// 合并并去重后的 MCP server 列表(platform + custom)。
    pub mcp_servers: Vec<agentdash_spi::RuntimeMcpServer>,
    /// 已解析通过的 capability key 集合(供 hook runtime 初始化、日志、delta 对比)。
    pub capability_keys: BTreeSet<String>,
    /// kickoff prompt 结构化片段;若 activity 没有 port/workflow,字段可能全为空。
    pub kickoff_prompt: KickoffPromptFragment,
    /// 带 output port 写入权限的 lifecycle mount。
    pub lifecycle_mount: agentdash_domain::common::Mount,
    /// 完整 Vfs(仅 lifecycle mount;applier 若需要更多 mount,自行扩展)。
    pub lifecycle_vfs: Vfs,
    /// workflow contract + activity 级 mount overlay 指令。
    pub mount_directives: Vec<MountDirective>,
}

/// 单 activity 激活的计算核心。纯函数,不做 IO。
/// 单 activity 激活 — 显式接收 `&PlatformConfig`(resolver 需要)。
pub fn activate_activity_with_platform(
    input: &ActivityActivationInput<'_>,
    platform: &PlatformConfig,
) -> Result<ActivityActivation, String> {
    // ── 1. baseline + override + directive → 合并 directive 序列 ──
    //
    // 合并策略：baseline (workflow contract 或 override) 在前，运行时 delta 在后；
    // `CapabilityResolver` 内部的 slot 归约「后来者胜」，保证运行时增删覆盖 baseline。
    let baseline_directives: Vec<ToolCapabilityDirective> =
        input.baseline_override.clone().unwrap_or_else(|| {
            input
                .workflow_contract
                .map(|contract| contract.capability_config.tool_directives.clone())
                .unwrap_or_default()
        });

    let mut combined_directives = baseline_directives;
    combined_directives.extend(input.tool_directives.iter().cloned());

    let has_active_workflow = input.workflow_contract.is_some();

    // ── 2. 先构造本次 AgentRun/AgentFrame 有效 VFS,再调 Resolver ──
    let writable_port_keys: Vec<String> = input
        .active_activity
        .output_ports
        .iter()
        .map(|p| p.key.clone())
        .collect();
    let lifecycle_mount = build_lifecycle_mount_with_node_scope(
        input.run_id,
        input.orchestration_id,
        input.node_path,
        input.lifecycle_key,
        &writable_port_keys,
        Some(input.attempt),
    );
    let lifecycle_vfs = Vfs {
        mounts: vec![lifecycle_mount.clone()],
        default_mount_id: None,
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    };
    // Activity 没有 step 级 capability_config;mount overlay 全部来自 workflow contract。
    let mount_directives = input
        .workflow_contract
        .map(|contract| contract.capability_config.mount_directives.clone())
        .unwrap_or_default();
    let effective_vfs = crate::session::capability_state::compose_vfs_with_overlay_and_directives(
        input.base_vfs,
        &lifecycle_vfs,
        &mount_directives,
    );

    // ── 3. 调 Resolver ──
    let mut contributions = Vec::new();
    if !input.agent_tool_directives.is_empty() {
        contributions.push(ContextContributions {
            source: ContextContributionSource::Agent,
            tool: Some(ToolContribution {
                directives: input.agent_tool_directives.clone(),
                has_active_workflow: false,
            }),
            companion: None,
        });
    }
    contributions.push(ContextContributions {
        source: ContextContributionSource::Workflow,
        tool: Some(ToolContribution {
            directives: combined_directives.clone(),
            has_active_workflow,
        }),
        companion: if !input.available_companions.is_empty() {
            Some(CompanionContribution {
                available: input.available_companions.clone(),
            })
        } else {
            None
        },
    });
    let cap_input = CapabilityResolverInput {
        owner_ctx: input.owner_ctx.clone(),
        contributions,
        mcp_candidates: McpCandidates {
            presets: input.available_presets.clone(),
        },
        mcp_runtime_context: Some(crate::mcp_preset::McpRuntimeBindingContext {
            vfs: Some(&effective_vfs),
            backend_anchor: None,
        }),
        capability_context: None,
        authority_state: input.authority_state.clone(),
    };
    let mut cap_output = CapabilityResolver::resolve_checked(&cap_input, platform)?;
    if let Some(slice_mode) = input.companion_slice_mode {
        cap_output = CapabilityResolver::apply_companion_slice(cap_output, slice_mode);
    }

    // ── 4. 汇总 MCP server 列表(platform + custom),去重 ──
    let mut mcp_servers: Vec<agentdash_spi::RuntimeMcpServer> = cap_output.tool.mcp_servers.clone();
    dedupe_runtime_mcp_servers(&mut mcp_servers);

    let capability_keys = cap_output.capability_keys();

    // ── 5. kickoff prompt fragment ──
    let kickoff_prompt = build_kickoff_prompt_fragment(input);

    Ok(ActivityActivation {
        capability_state: cap_output,
        mcp_servers,
        capability_keys,
        kickoff_prompt,
        lifecycle_mount,
        lifecycle_vfs,
        mount_directives,
    })
}

fn build_kickoff_prompt_fragment(input: &ActivityActivationInput<'_>) -> KickoffPromptFragment {
    let node_key = &input.active_activity.key;
    let desc = input.active_activity.description.trim();
    let node_title = if desc.is_empty() {
        format!("`{node_key}`")
    } else {
        format!("`{node_key}`({desc})")
    };
    let title_line = format!(
        "你正在执行 lifecycle `{}` 的 node {}。",
        input.lifecycle_key, node_title
    );

    let output_section = render_output_section(&input.active_activity.output_ports);
    let input_section =
        render_input_section(&input.active_activity.input_ports, &input.ready_port_keys);

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
    ready_port_keys: &BTreeSet<String>,
) -> String {
    if ports.is_empty() {
        return String::new();
    }
    let items: Vec<String> = ports
        .iter()
        .map(|ip| {
            let status = if ready_port_keys.contains(&ip.key) {
                "已就绪"
            } else {
                "未就绪"
            };
            format!("- **{}**({}) — {}", ip.key, ip.description, status)
        })
        .collect();
    format!(
        "\n\n## 输入上下文\n以下是来自前驱节点的产出,可通过 `read_file` 读取:\n{}",
        items.join("\n")
    )
}

fn dedupe_runtime_mcp_servers(servers: &mut Vec<agentdash_spi::RuntimeMcpServer>) {
    let mut seen = BTreeSet::<String>::new();
    servers.retain(|server| seen.insert(server.name.clone()));
}

#[cfg(test)]
pub(crate) fn build_capability_state_for_activation(
    activation: &ActivityActivation,
    base_surface: Option<&CapabilityState>,
) -> CapabilityState {
    use crate::agent_run::frame::builder::{
        AgentFrameActivationSurfaceInput, build_lifecycle_activation_surface,
    };
    let surface = build_lifecycle_activation_surface(AgentFrameActivationSurfaceInput {
        activation,
        base_vfs: base_surface.and_then(|s| s.vfs.active.as_ref()),
        inherit_skills_from: base_surface,
    });
    surface.capability_state
}

// ─── available_presets 辅助 ────────────────────────────────

/// 构造空的 `AvailableMcpPresets`(调用方未预展开时的占位)。
#[cfg(test)]
fn empty_presets() -> AvailableMcpPresets {
    use std::collections::BTreeMap;

    BTreeMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::workflow::{
        ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, AgentProcedure,
        AgentProcedureContract, CapabilityConfig, DefinitionSource, MountDirective,
    };

    fn sample_activity(
        output_ports: Vec<agentdash_domain::workflow::OutputPortDefinition>,
    ) -> ActivityDefinition {
        ActivityDefinition {
            key: "implement".to_string(),
            description: "实现并记录结果".to_string(),
            executor: ActivityExecutorSpec::Agent(
                AgentActivityExecutorSpec::create_activity_agent("wf_impl"),
            ),
            input_ports: vec![],
            output_ports,
            completion_policy: Default::default(),
            iteration_policy: Default::default(),
            join_policy: Default::default(),
        }
    }

    fn sample_workflow(directives: Vec<ToolCapabilityDirective>) -> AgentProcedure {
        let contract = AgentProcedureContract {
            capability_config: CapabilityConfig {
                tool_directives: directives,
                ..Default::default()
            },
            ..AgentProcedureContract::default()
        };
        AgentProcedure::new(
            Uuid::new_v4(),
            "wf_impl",
            "Workflow Implement",
            "desc",
            DefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow")
    }

    fn test_platform() -> PlatformConfig {
        PlatformConfig {
            mcp_base_url: Some("http://localhost:3001".to_string()),
        }
    }

    fn mount(id: &str, provider: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: provider.to_string(),
            backend_id: "test-backend".to_string(),
            root_ref: format!("{provider}://{id}"),
            capabilities: vec![MountCapability::Read],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn activate_activity_no_workflow_uses_default_visibility() {
        let step = sample_activity(vec![]);
        let project_id = Uuid::new_v4();
        let story_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let input = ActivityActivationInput {
            owner_ctx: CapabilityScopeCtx::Task {
                project_id,
                story_id: Some(story_id),
                task_id,
            },
            active_activity: &step,
            workflow_contract: None,
            base_vfs: None,
            run_id: Uuid::new_v4(),
            orchestration_id: Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: "trellis_dev_task",
            available_presets: empty_presets(),
            authority_state: AuthorityState::main_project_agent(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: BTreeSet::new(),
            available_companions: Vec::new(),
        };

        let out = activate_activity_with_platform(&input, &test_platform()).expect("activate");
        // 无 workflow,走默认 visibility —— task scope 至少能拿到 Read/Write/Execute
        assert!(
            !out.capability_keys.is_empty(),
            "default visibility 应产出至少一个能力"
        );
    }

    #[test]
    fn activate_activity_with_workflow_uses_contract_capabilities_as_baseline() {
        let workflow = sample_workflow(vec![
            ToolCapabilityDirective::add_simple("file_read"),
            ToolCapabilityDirective::add_simple("file_write"),
            ToolCapabilityDirective::add_simple("shell_execute"),
            ToolCapabilityDirective::add_simple("workflow_management"),
        ]);
        let step = sample_activity(vec![]);
        let project_id = Uuid::new_v4();

        let input = ActivityActivationInput {
            owner_ctx: CapabilityScopeCtx::Project { project_id },
            active_activity: &step,
            workflow_contract: Some(&workflow.contract),
            base_vfs: None,
            run_id: Uuid::new_v4(),
            orchestration_id: Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: "lc_admin",
            available_presets: empty_presets(),
            authority_state: AuthorityState::main_project_agent(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: BTreeSet::new(),
            available_companions: Vec::new(),
        };

        let out = activate_activity_with_platform(&input, &test_platform()).expect("activate");
        assert!(out.capability_keys.contains("workflow_management"));
        // file_read/write/shell_execute 现在是独立 directive
        assert!(out.capability_keys.contains("file_read"));
        assert!(out.capability_keys.contains("file_write"));
        assert!(out.capability_keys.contains("shell_execute"));
    }

    #[test]
    fn activate_activity_companion_child_applies_authority_to_workflow_capabilities() {
        let workflow = sample_workflow(vec![
            ToolCapabilityDirective::add_simple("workflow_management"),
            ToolCapabilityDirective::add_simple("workspace_module"),
        ]);
        let step = sample_activity(vec![]);
        let project_id = Uuid::new_v4();

        let input = ActivityActivationInput {
            owner_ctx: CapabilityScopeCtx::Project { project_id },
            active_activity: &step,
            workflow_contract: Some(&workflow.contract),
            base_vfs: None,
            run_id: Uuid::new_v4(),
            orchestration_id: Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: "lc_child",
            available_presets: empty_presets(),
            authority_state: AuthorityState::companion_child(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: BTreeSet::new(),
            available_companions: Vec::new(),
        };

        let out = activate_activity_with_platform(&input, &test_platform()).expect("activate");

        assert!(out.capability_keys.contains("workflow"));
        assert!(out.capability_keys.contains("collaboration"));
        assert!(!out.capability_keys.contains("workspace_module"));
        assert!(!out.capability_keys.contains("workflow_management"));
        assert!(
            !out.mcp_servers
                .iter()
                .any(|server| server.name.contains("workflow")),
            "companion child 不应注入 workflow authoring MCP"
        );
    }

    #[test]
    fn phase_node_target_workflow_preserves_owner_default_baseline() {
        let workflow = sample_workflow(vec![ToolCapabilityDirective::add_simple(
            "workflow_management",
        )]);
        let step = sample_activity(vec![]);
        let project_id = Uuid::new_v4();

        let input = ActivityActivationInput {
            owner_ctx: CapabilityScopeCtx::Project { project_id },
            active_activity: &step,
            workflow_contract: Some(&workflow.contract),
            base_vfs: None,
            run_id: Uuid::new_v4(),
            orchestration_id: Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: "lc_phase",
            available_presets: empty_presets(),
            authority_state: AuthorityState::main_project_agent(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: BTreeSet::new(),
            available_companions: Vec::new(),
        };

        let out = activate_activity_with_platform(&input, &test_platform()).expect("activate");

        assert!(out.capability_keys.contains("workflow_management"));
        assert!(out.capability_keys.contains("file_read"));
        assert!(out.capability_keys.contains("file_write"));
        assert!(out.capability_keys.contains("shell_execute"));
        assert!(out.capability_keys.contains("workspace_module"));
        assert!(out.capability_keys.contains("collaboration"));
    }

    #[test]
    fn same_capability_key_tool_directive_changes_tool_state() {
        let step = sample_activity(vec![]);
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let full_read_workflow =
            sample_workflow(vec![ToolCapabilityDirective::add_simple("file_read")]);
        let restricted_read_workflow = sample_workflow(vec![
            ToolCapabilityDirective::add_simple("file_read"),
            ToolCapabilityDirective::remove_tool("file_read", "fs_grep"),
        ]);

        let base_input = ActivityActivationInput {
            owner_ctx: CapabilityScopeCtx::Project { project_id },
            active_activity: &step,
            workflow_contract: Some(&full_read_workflow.contract),
            base_vfs: None,
            run_id,
            orchestration_id: Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: "lc_phase",
            available_presets: empty_presets(),
            authority_state: AuthorityState::main_project_agent(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: BTreeSet::new(),
            available_companions: Vec::new(),
        };
        let restricted_input = ActivityActivationInput {
            workflow_contract: Some(&restricted_read_workflow.contract),
            ..base_input.clone()
        };

        let base =
            activate_activity_with_platform(&base_input, &test_platform()).expect("activate");
        let restricted =
            activate_activity_with_platform(&restricted_input, &test_platform()).expect("activate");

        assert_eq!(base.capability_keys, restricted.capability_keys);
        assert!(
            !base
                .capability_state
                .is_tool_path_excluded("file_read", "fs_grep")
        );
        assert!(
            restricted
                .capability_state
                .is_tool_path_excluded("file_read", "fs_grep")
        );
    }

    #[test]
    fn activity_mount_directives_change_capability_state_vfs() {
        // mount overlay 现统一来自 workflow contract（Activity 无 step 级 capability_config）。
        let contract = AgentProcedureContract {
            capability_config: CapabilityConfig {
                tool_directives: vec![ToolCapabilityDirective::add_simple("file_read")],
                mount_directives: vec![
                    MountDirective::RemoveMount {
                        mount_id: "secret".to_string(),
                    },
                    MountDirective::AddMount {
                        mount: mount("review", "inline_fs"),
                    },
                    MountDirective::SetDefaultMount {
                        mount_id: Some("review".to_string()),
                    },
                ],
            },
            ..AgentProcedureContract::default()
        };
        let workflow = AgentProcedure::new(
            Uuid::new_v4(),
            "wf_impl",
            "Workflow Implement",
            "desc",
            DefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow");
        let step = sample_activity(vec![]);
        let project_id = Uuid::new_v4();
        let input = ActivityActivationInput {
            owner_ctx: CapabilityScopeCtx::Project { project_id },
            active_activity: &step,
            workflow_contract: Some(&workflow.contract),
            base_vfs: None,
            run_id: Uuid::new_v4(),
            orchestration_id: Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: "lc_phase",
            available_presets: empty_presets(),
            authority_state: AuthorityState::main_project_agent(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: BTreeSet::new(),
            available_companions: Vec::new(),
        };
        let activation =
            activate_activity_with_platform(&input, &test_platform()).expect("activate");
        let base_surface = {
            let mut state = activation.capability_state.clone();
            state.tool.mcp_servers = activation.mcp_servers.clone();
            state.vfs.active = Some(Vfs {
                mounts: vec![mount("workspace", "relay_fs"), mount("secret", "inline_fs")],
                default_mount_id: Some("workspace".to_string()),
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            });
            state
        };

        let target = build_capability_state_for_activation(&activation, Some(&base_surface));
        let target_vfs = target.vfs.active.as_ref().expect("target vfs");
        let mount_ids = target_vfs
            .mounts
            .iter()
            .map(|mount| mount.id.as_str())
            .collect::<BTreeSet<_>>();

        assert!(mount_ids.contains("workspace"));
        assert!(mount_ids.contains("review"));
        assert!(mount_ids.contains("lifecycle"));
        assert!(!mount_ids.contains("secret"));
        assert_eq!(target_vfs.default_mount_id.as_deref(), Some("review"));
        assert_ne!(base_surface, target);

        let delta = crate::session::compute_capability_state_delta(
            Some(&base_surface),
            &target,
            &activation.capability_keys,
        );
        assert!(delta.vfs.mounts.added.contains(&"review".to_string()));
        assert!(delta.vfs.mounts.removed.contains(&"secret".to_string()));
        assert!(delta.vfs.default_mount.changed);
    }

    #[test]
    fn activate_activity_baseline_override_takes_precedence_over_contract() {
        let workflow = sample_workflow(vec![ToolCapabilityDirective::add_simple("file_read")]);
        let step = sample_activity(vec![]);
        let project_id = Uuid::new_v4();

        // PhaseNode 热更新场景:baseline 来自 hook_runtime.current_capabilities()
        let input = ActivityActivationInput {
            owner_ctx: CapabilityScopeCtx::Project { project_id },
            active_activity: &step,
            workflow_contract: Some(&workflow.contract),
            base_vfs: None,
            run_id: Uuid::new_v4(),
            orchestration_id: Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: "lc",
            available_presets: empty_presets(),
            authority_state: AuthorityState::main_project_agent(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: Some(vec![
                ToolCapabilityDirective::add_simple("workspace_module"),
                ToolCapabilityDirective::add_simple("collaboration"),
                // 显式屏蔽 workflow contract 原有的 file_read
                ToolCapabilityDirective::remove_simple("file_read"),
            ]),
            tool_directives: &[ToolCapabilityDirective::add_simple("workflow_management")],
            ready_port_keys: BTreeSet::new(),
            available_companions: Vec::new(),
        };

        let out = activate_activity_with_platform(&input, &test_platform()).expect("activate");
        // baseline_override = workspace_module + collaboration + Remove(file_read),
        // directive = +workflow_management
        // workflow.contract.capability_config.tool_directives = file_read 被 override 替代
        assert!(out.capability_keys.contains("workspace_module"));
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
        let step = sample_activity(ports);
        let project_id = Uuid::new_v4();

        let input = ActivityActivationInput {
            owner_ctx: CapabilityScopeCtx::Project { project_id },
            active_activity: &step,
            workflow_contract: None,
            base_vfs: None,
            run_id: Uuid::new_v4(),
            orchestration_id: Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: "lc",
            available_presets: empty_presets(),
            authority_state: AuthorityState::main_project_agent(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: BTreeSet::new(),
            available_companions: Vec::new(),
        };

        let out = activate_activity_with_platform(&input, &test_platform()).expect("activate");
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
        let step = ActivityDefinition {
            key: "b".to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Agent(
                AgentActivityExecutorSpec::create_activity_agent("wf_impl"),
            ),
            input_ports: vec![agentdash_domain::workflow::InputPortDefinition {
                key: "ctx".to_string(),
                description: "前驱上下文".to_string(),
                context_strategy: Default::default(),
                context_template: None,
                standalone_fulfillment: Default::default(),
            }],
            output_ports: vec![],
            completion_policy: Default::default(),
            iteration_policy: Default::default(),
            join_policy: Default::default(),
        };
        let project_id = Uuid::new_v4();
        let ready: BTreeSet<String> = ["out".to_string()].into_iter().collect();

        let input = ActivityActivationInput {
            owner_ctx: CapabilityScopeCtx::Project { project_id },
            active_activity: &step,
            workflow_contract: None,
            base_vfs: None,
            run_id: Uuid::new_v4(),
            orchestration_id: Uuid::new_v4(),
            node_path: "implement",
            attempt: 1,
            lifecycle_key: "lc",
            available_presets: empty_presets(),
            authority_state: AuthorityState::main_project_agent(),
            agent_tool_directives: Vec::new(),
            companion_slice_mode: None,
            baseline_override: None,
            tool_directives: &[],
            ready_port_keys: ready,
            available_companions: Vec::new(),
        };

        let out = activate_activity_with_platform(&input, &test_platform()).expect("activate");
        assert!(out.kickoff_prompt.input_section.contains("ctx"));
    }
}
