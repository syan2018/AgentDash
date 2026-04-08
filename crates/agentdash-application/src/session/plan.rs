use agentdash_domain::session_composition::{SessionComposition, SessionRequiredContextBlock};
use agentdash_domain::story::Story;
use agentdash_injection::{ContextFragment, MergeStrategy};
use serde::Serialize;

use crate::runtime::{AddressSpace, Mount, MountCapability, RuntimeMcpServer};

pub use agentdash_domain::session_binding::SessionOwnerType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPlanPhase {
    ProjectAgent,
    TaskStart,
    TaskContinue,
    StoryOwner,
}

pub trait SessionOwnerTypeExt {
    fn default_plan_phase(self, is_continuation: bool) -> SessionPlanPhase;
}

impl SessionOwnerTypeExt for SessionOwnerType {
    fn default_plan_phase(self, is_continuation: bool) -> SessionPlanPhase {
        match self {
            SessionOwnerType::Project => SessionPlanPhase::ProjectAgent,
            SessionOwnerType::Story => SessionPlanPhase::StoryOwner,
            SessionOwnerType::Task if is_continuation => SessionPlanPhase::TaskContinue,
            SessionOwnerType::Task => SessionPlanPhase::TaskStart,
        }
    }
}

pub struct SessionAddressSpaceSummary {
    pub markdown: String,
    pub default_mount_id: Option<String>,
    pub mount_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMcpServerSummary {
    pub name: String,
    pub transport: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionToolVisibilitySummary {
    pub markdown: String,
    pub resolved: bool,
    pub toolset_label: String,
    pub tool_names: Vec<String>,
    pub mcp_servers: Vec<SessionMcpServerSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionRuntimePolicySummary {
    pub markdown: String,
    pub workspace_attached: bool,
    pub address_space_attached: bool,
    pub mcp_enabled: bool,
    pub visible_mounts: Vec<String>,
    pub visible_tools: Vec<String>,
    pub writable_mounts: Vec<String>,
    pub exec_mounts: Vec<String>,
    pub path_policy: String,
}

pub struct SessionPlanInput<'a> {
    pub owner_type: SessionOwnerType,
    pub phase: SessionPlanPhase,
    pub address_space: Option<&'a AddressSpace>,
    pub mcp_servers: &'a [RuntimeMcpServer],
    pub session_composition: Option<&'a SessionComposition>,
    pub agent_type: Option<&'a str>,
    pub preset_name: Option<&'a str>,
    pub has_custom_prompt_template: bool,
    pub has_initial_context: bool,
    pub workspace_attached: bool,
}

pub struct SessionPlanFragments {
    pub fragments: Vec<ContextFragment>,
}

pub fn resolve_story_session_composition(story: Option<&Story>) -> Option<SessionComposition> {
    story.and_then(|item| item.context.session_composition.clone())
}

pub fn build_session_plan_fragments(input: SessionPlanInput<'_>) -> SessionPlanFragments {
    let mut fragments = Vec::new();

    if let Some(address_space) = input.address_space {
        let summary = summarize_address_space(address_space);
        fragments.push(ContextFragment {
            slot: "address_space",
            label: "address_space_summary",
            order: 35,
            strategy: MergeStrategy::Append,
            content: summary.markdown,
        });
    }

    let tool_visibility = summarize_tool_visibility_with_context(
        input.address_space,
        input.mcp_servers,
        Some(input.owner_type),
    );
    let tool_names = tool_visibility.tool_names.clone();
    fragments.push(ContextFragment {
        slot: "tools",
        label: "tool_visibility_summary",
        order: 36,
        strategy: MergeStrategy::Append,
        content: tool_visibility.markdown,
    });

    fragments.push(ContextFragment {
        slot: "persona",
        label: "persona_summary",
        order: 37,
        strategy: MergeStrategy::Append,
        content: build_persona_markdown(&input),
    });

    for (index, block) in input
        .session_composition
        .map(|item| item.required_context_blocks.as_slice())
        .unwrap_or(&[])
        .iter()
        .enumerate()
    {
        fragments.push(ContextFragment {
            slot: "required_context",
            label: "required_context_block",
            order: 38 + index as i32,
            strategy: MergeStrategy::Append,
            content: build_required_context_block_markdown(block),
        });
    }

    fragments.push(ContextFragment {
        slot: "workflow",
        label: "workflow_summary",
        order: 48,
        strategy: MergeStrategy::Append,
        content: build_workflow_markdown(&input),
    });

    let runtime_policy = summarize_runtime_policy(
        input.workspace_attached,
        input.address_space,
        input.mcp_servers,
        &tool_names,
    );
    fragments.push(ContextFragment {
        slot: "runtime_policy",
        label: "runtime_policy_summary",
        order: 49,
        strategy: MergeStrategy::Append,
        content: runtime_policy.markdown,
    });

    SessionPlanFragments { fragments }
}

pub fn summarize_address_space(address_space: &AddressSpace) -> SessionAddressSpaceSummary {
    let default_mount_id = address_space.default_mount_id.clone();
    let mount_ids = address_space
        .mounts
        .iter()
        .map(|mount| mount.id.clone())
        .collect::<Vec<_>>();

    let markdown = if address_space.mounts.is_empty() {
        "## Address Space\n- 当前会话未挂载可访问的 mount".to_string()
    } else {
        let default_mount = default_mount_id.as_deref().unwrap_or("-");
        let mount_lines = address_space
            .mounts
            .iter()
            .map(render_mount_summary)
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "## Address Space\n- default_mount: `{default_mount}`\n- mount_count: {}\n- usage: 访问文件时优先使用 `mount + 相对路径`；如无特殊说明，默认 mount 为 `{default_mount}`。\n\n### Mounts\n{}",
            address_space.mounts.len(),
            mount_lines
        )
    };

    SessionAddressSpaceSummary {
        markdown,
        default_mount_id,
        mount_ids,
    }
}

pub fn summarize_tool_visibility(
    address_space: Option<&AddressSpace>,
    mcp_servers: &[RuntimeMcpServer],
) -> SessionToolVisibilitySummary {
    summarize_tool_visibility_with_context(address_space, mcp_servers, None)
}

pub fn summarize_tool_visibility_with_context(
    address_space: Option<&AddressSpace>,
    mcp_servers: &[RuntimeMcpServer],
    owner_type: Option<SessionOwnerType>,
) -> SessionToolVisibilitySummary {
    let resolved = address_space.is_some();
    let mut tool_names = address_space
        .map(runtime_address_space_tools)
        .unwrap_or_default();

    // 流程工具：按 session owner 类型条件注入
    if resolved {
        let flow_tools = conditional_flow_tools(owner_type);
        tool_names.extend(flow_tools);
    }

    let toolset_label = if resolved {
        "address_space_runtime".to_string()
    } else {
        "runtime_unresolved".to_string()
    };
    let mcp_server_summaries = summarize_mcp_servers(mcp_servers);

    let mut sections = vec![if resolved {
        format!(
            "## Tool Visibility\n- resolved: yes\n- toolset: `{toolset_label}`\n- tools: {}\n- guidance: 优先使用当前会话声明的工具访问上下文，不要臆测文件内容、工具能力或 mounts。",
            if tool_names.is_empty() {
                "-".to_string()
            } else {
                tool_names
                    .iter()
                    .map(|tool| format!("`{tool}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        )
    } else {
        "## Tool Visibility\n- resolved: no\n- toolset: `runtime_unresolved`\n- tools: -\n- guidance: 当前未解析出最终运行时工具面，不要把推测值包装成当前会话已确认可见的工具。".to_string()
    }];

    if !mcp_server_summaries.is_empty() {
        sections.push(format!(
            "### MCP Servers\n{}",
            mcp_server_summaries
                .iter()
                .map(render_mcp_server_summary)
                .collect::<Vec<_>>()
                .join("\n")
        ));
        // 展开 MCP 服务器名，不再使用 "mcp_tools" 占位
        for server in &mcp_server_summaries {
            tool_names.push(format!("mcp:{}", server.name));
        }
    }

    SessionToolVisibilitySummary {
        markdown: sections.join("\n\n"),
        resolved,
        toolset_label,
        tool_names,
        mcp_servers: mcp_server_summaries,
    }
}

/// 根据 session owner 类型返回应注入的流程工具名。
/// - `report_workflow_artifact`：所有 task/story session 可用
/// - `companion_request` / `companion_respond`：统一 companion 信道工具
fn conditional_flow_tools(owner_type: Option<SessionOwnerType>) -> Vec<String> {
    let mut tools = Vec::new();
    match owner_type {
        Some(SessionOwnerType::Task) => {
            tools.push("report_workflow_artifact".to_string());
            tools.push("companion_respond".to_string());
            tools.push("canvases_list".to_string());
            tools.push("canvas_start".to_string());
            tools.push("bind_canvas_data".to_string());
            tools.push("present_canvas".to_string());
        }
        Some(SessionOwnerType::Story) => {
            tools.push("report_workflow_artifact".to_string());
            tools.push("companion_request".to_string());
            tools.push("companion_respond".to_string());
            tools.push("canvases_list".to_string());
            tools.push("canvas_start".to_string());
            tools.push("bind_canvas_data".to_string());
            tools.push("present_canvas".to_string());
        }
        Some(SessionOwnerType::Project) => {
            tools.push("companion_request".to_string());
            tools.push("companion_respond".to_string());
            tools.push("canvases_list".to_string());
            tools.push("canvas_start".to_string());
            tools.push("bind_canvas_data".to_string());
            tools.push("present_canvas".to_string());
        }
        None => {
            tools.push("report_workflow_artifact".to_string());
            tools.push("companion_request".to_string());
            tools.push("companion_respond".to_string());
            tools.push("canvases_list".to_string());
            tools.push("canvas_start".to_string());
            tools.push("bind_canvas_data".to_string());
            tools.push("present_canvas".to_string());
        }
    }
    tools
}

fn build_persona_markdown(input: &SessionPlanInput<'_>) -> String {
    let role_label = match input.owner_type {
        SessionOwnerType::Project => "project_agent",
        SessionOwnerType::Task => "task_execution",
        SessionOwnerType::Story => "story_owner",
    };
    let role_description = match input.owner_type {
        SessionOwnerType::Project => {
            "Project 级协作代理，负责维护项目共享上下文、整理资料、沉淀决策并辅助后续 Story 准备"
        }
        SessionOwnerType::Task => "执行单元代理，负责完成当前 Task 的实现、验证与结果汇报",
        SessionOwnerType::Story => "Story 主代理，负责整理上下文、推进 Story、拆解并创建 Task",
    };
    let identity = input
        .agent_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unspecified");
    let preset_name = input
        .preset_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-");
    let configured_persona_label = input
        .session_composition
        .and_then(|composition| composition.persona_label.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-");
    let configured_persona_prompt = input
        .session_composition
        .and_then(|composition| composition.persona_prompt.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut markdown = format!(
        "## Persona\n- role: `{role_label}`\n- identity: `{identity}`\n- preset: `{preset_name}`\n- custom_prompt_template: {}\n- initial_context: {}\n- responsibility: {}",
        yes_no(input.has_custom_prompt_template),
        yes_no(input.has_initial_context),
        role_description
    );

    markdown.push_str(&format!(
        "\n- configured_persona: `{configured_persona_label}`"
    ));

    if let Some(persona_prompt) = configured_persona_prompt {
        markdown.push_str(&format!("\n\n### Persona Prompt\n{persona_prompt}"));
    }

    markdown
}

fn build_workflow_markdown(input: &SessionPlanInput<'_>) -> String {
    let (phase_label, default_actions) = match input.phase {
        SessionPlanPhase::ProjectAgent => (
            "project_agent",
            vec![
                "先理解项目目标、共享上下文与当前可见 mounts".to_string(),
                "优先把项目资料组织为用户可理解的资料目录，而不是暴露底层运行时细节".to_string(),
                "需要沉淀共享上下文时，明确说明写入位置、内容结构与后续复用方式".to_string(),
            ],
        ),
        SessionPlanPhase::TaskStart => (
            "task_start",
            vec![
                "先理解任务、Story、上下文容器与工具边界".to_string(),
                "优先用声明的 mounts 和 tools 读取信息，不要猜测".to_string(),
                "完成实现后明确说明验证结果与剩余风险".to_string(),
            ],
        ),
        SessionPlanPhase::TaskContinue => (
            "task_continue",
            vec![
                "延续现有会话上下文，优先收敛未完成项".to_string(),
                "先核对已有 mounts、tools、上次执行状态，再继续推进".to_string(),
                "输出本轮新增进展与下一步建议".to_string(),
            ],
        ),
        SessionPlanPhase::StoryOwner => (
            "story_owner",
            vec![
                "围绕 Story 目标持续补全上下文".to_string(),
                "按需利用 MCP 与虚拟容器维护 Story/Task 编排".to_string(),
                "拆解任务时明确每个 Task 需要的上下文、工具与工作目录".to_string(),
            ],
        ),
    };
    let configured_actions = input
        .session_composition
        .map(|composition| composition.workflow_steps.as_slice())
        .unwrap_or(&[]);
    let required_actions = if configured_actions.is_empty() {
        default_actions
    } else {
        configured_actions.to_vec()
    };

    format!(
        "## Workflow\n- phase: `{phase_label}`\n{}\n- invariant: 必须把当前会话可见的 mounts、tools、workflow 约束视为显式输入，而不是隐式假设。",
        required_actions
            .iter()
            .map(|item| format!("- action: {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn build_required_context_block_markdown(block: &SessionRequiredContextBlock) -> String {
    format!("## {}\n{}", block.title.trim(), block.content.trim())
}

pub fn summarize_runtime_policy(
    workspace_attached: bool,
    address_space: Option<&AddressSpace>,
    mcp_servers: &[RuntimeMcpServer],
    tool_names: &[String],
) -> SessionRuntimePolicySummary {
    let mount_ids = address_space
        .map(|item| {
            item.mounts
                .iter()
                .map(|mount| mount.id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let writable_mounts = address_space
        .map(|address_space| {
            address_space
                .mounts
                .iter()
                .filter(|mount| mount.supports(MountCapability::Write))
                .map(|mount| format!("`{}`", mount.id))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let exec_mounts = address_space
        .map(|address_space| {
            address_space
                .mounts
                .iter()
                .filter(|mount| mount.supports(MountCapability::Exec))
                .map(|mount| format!("`{}`", mount.id))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let path_policy = if address_space.is_some() {
        "使用 `mount + 相对路径` 访问资源".to_string()
    } else if workspace_attached {
        "使用相对工作空间路径访问资源".to_string()
    } else {
        "运行时路径规则尚未解析".to_string()
    };

    let markdown = format!(
        "## Runtime Policy\n- workspace_attached: {}\n- address_space_attached: {}\n- mcp_enabled: {}\n- visible_mounts: {}\n- visible_tools: {}\n- writable_mounts: {}\n- exec_mounts: {}\n- path_policy: {}",
        yes_no(workspace_attached),
        yes_no(address_space.is_some()),
        yes_no(!mcp_servers.is_empty()),
        display_list(&mount_ids),
        display_list(tool_names),
        display_list(&writable_mounts),
        display_list(&exec_mounts),
        path_policy
    );

    SessionRuntimePolicySummary {
        markdown,
        workspace_attached,
        address_space_attached: address_space.is_some(),
        mcp_enabled: !mcp_servers.is_empty(),
        visible_mounts: mount_ids,
        visible_tools: tool_names.to_vec(),
        writable_mounts,
        exec_mounts,
        path_policy,
    }
}

fn render_mount_summary(mount: &Mount) -> String {
    let capabilities = mount
        .capabilities
        .iter()
        .map(render_capability)
        .collect::<Vec<_>>()
        .join(", ");
    let mut lines = vec![format!(
        "- `{}`: {}（provider={}, capabilities=[{}]）",
        mount.id,
        fallback_display_name(mount),
        mount.provider,
        capabilities
    )];

    if !mount.root_ref.trim().is_empty() {
        lines.push(format!("  - root_ref: `{}`", mount.root_ref));
    }
    if !mount.backend_id.trim().is_empty() {
        lines.push(format!("  - backend_id: `{}`", mount.backend_id));
    }
    if mount.default_write {
        lines.push("  - default_write: true".to_string());
    }

    if let Some(service_id) = mount
        .metadata
        .get("service_id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("  - service_id: `{service_id}`"));
    }

    lines.join("\n")
}

fn fallback_display_name(mount: &Mount) -> &str {
    let trimmed = mount.display_name.trim();
    if trimmed.is_empty() {
        mount.id.as_str()
    } else {
        trimmed
    }
}

fn render_capability(capability: &MountCapability) -> &'static str {
    match capability {
        MountCapability::Read => "read",
        MountCapability::Write => "write",
        MountCapability::List => "list",
        MountCapability::Search => "search",
        MountCapability::Exec => "exec",
    }
}

fn runtime_address_space_tools(_address_space: &AddressSpace) -> Vec<String> {
    vec![
        "mounts_list".to_string(),
        "fs_read".to_string(),
        "fs_glob".to_string(),
        "fs_grep".to_string(),
        "fs_apply_patch".to_string(),
        "shell_exec".to_string(),
    ]
}

fn summarize_mcp_servers(mcp_servers: &[RuntimeMcpServer]) -> Vec<SessionMcpServerSummary> {
    mcp_servers
        .iter()
        .map(|server| SessionMcpServerSummary {
            name: server.name().to_string(),
            transport: server.transport_label().to_string(),
            target: server.target(),
        })
        .collect()
}

fn render_mcp_server_summary(server: &SessionMcpServerSummary) -> String {
    format!(
        "- `{}`: {} `{}`",
        server.name, server.transport, server.target
    )
}

fn display_list<T: AsRef<str>>(items: &[T]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items
            .iter()
            .map(|item| item.as_ref().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::project::Project;
    use agentdash_domain::session_composition::{SessionComposition, SessionRequiredContextBlock};
    use agentdash_domain::story::Story;
    use serde_json::json;

    #[test]
    fn summarize_address_space_includes_mount_details() {
        let address_space = AddressSpace {
            mounts: vec![
                Mount {
                    id: "main".to_string(),
                    provider: "relay_fs".to_string(),
                    backend_id: "backend-a".to_string(),
                    root_ref: "/workspace/repo".to_string(),
                    capabilities: vec![
                        MountCapability::Read,
                        MountCapability::List,
                        MountCapability::Exec,
                    ],
                    default_write: false,
                    display_name: "主工作空间".to_string(),
                    metadata: serde_json::Value::Null,
                },
                Mount {
                    id: "km".to_string(),
                    provider: "external_service".to_string(),
                    backend_id: String::new(),
                    root_ref: "tenant://project/km".to_string(),
                    capabilities: vec![MountCapability::Read, MountCapability::Search],
                    default_write: false,
                    display_name: "知识库".to_string(),
                    metadata: json!({ "service_id": "km-gateway" }),
                },
            ],
            default_mount_id: Some("main".to_string()),
            ..Default::default()
        };

        let summary = summarize_address_space(&address_space);

        assert_eq!(summary.default_mount_id.as_deref(), Some("main"));
        assert_eq!(
            summary.mount_ids,
            vec!["main".to_string(), "km".to_string()]
        );
        assert!(summary.markdown.contains("default_mount: `main`"));
        assert!(summary.markdown.contains("`main`: 主工作空间"));
        assert!(summary.markdown.contains("service_id: `km-gateway`"));
    }

    #[test]
    fn summarize_tool_visibility_includes_runtime_and_mcp_tools() {
        let address_space = AddressSpace {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-a".to_string(),
                root_ref: "/workspace/repo".to_string(),
                capabilities: vec![
                    MountCapability::Read,
                    MountCapability::List,
                    MountCapability::Search,
                    MountCapability::Write,
                    MountCapability::Exec,
                ],
                default_write: true,
                display_name: "主工作空间".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            ..Default::default()
        };
        let mcp_servers = vec![RuntimeMcpServer::Http {
            name: "agentdash-story-tools".to_string(),
            url: "http://127.0.0.1:3001/mcp/story/123".to_string(),
        }];

        let summary = summarize_tool_visibility(Some(&address_space), &mcp_servers);

        assert!(summary.resolved);
        assert!(summary.tool_names.contains(&"mounts_list".to_string()));
        assert!(summary.tool_names.contains(&"fs_apply_patch".to_string()));
        assert!(summary.tool_names.contains(&"shell_exec".to_string()));
        assert!(
            summary
                .tool_names
                .contains(&"mcp:agentdash-story-tools".to_string())
        );
        assert!(summary.markdown.contains("## Tool Visibility"));
        assert!(summary.markdown.contains("`agentdash-story-tools`"));
    }

    #[test]
    fn build_session_plan_fragments_includes_persona_workflow_and_runtime_policy() {
        let address_space = AddressSpace {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "backend-a".to_string(),
                root_ref: "/workspace/repo".to_string(),
                capabilities: vec![
                    MountCapability::Read,
                    MountCapability::List,
                    MountCapability::Search,
                ],
                default_write: false,
                display_name: "主工作空间".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            ..Default::default()
        };

        let built = build_session_plan_fragments(SessionPlanInput {
            owner_type: SessionOwnerType::Task,
            phase: SessionPlanPhase::TaskStart,
            address_space: Some(&address_space),
            mcp_servers: &[],
            session_composition: Some(&SessionComposition {
                persona_label: Some("实现代理".to_string()),
                persona_prompt: Some("优先核对 mount 摘要，再动手实现".to_string()),
                workflow_steps: vec![
                    "先读取项目容器摘要".to_string(),
                    "完成实现后回报验证结果".to_string(),
                ],
                required_context_blocks: vec![SessionRequiredContextBlock {
                    title: "必读约束".to_string(),
                    content: "所有路径必须通过 mount + 相对路径访问".to_string(),
                }],
            }),
            agent_type: Some("PI_AGENT"),
            preset_name: Some("default"),
            has_custom_prompt_template: true,
            has_initial_context: true,
            workspace_attached: true,
        });

        let merged = built
            .fragments
            .iter()
            .map(|fragment| fragment.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        assert!(merged.contains("## Persona"));
        assert!(merged.contains("identity: `PI_AGENT`"));
        assert!(merged.contains("configured_persona: `实现代理`"));
        assert!(merged.contains("优先核对 mount 摘要，再动手实现"));
        assert!(merged.contains("## 必读约束"));
        assert!(merged.contains("## Workflow"));
        assert!(merged.contains("先读取项目容器摘要"));
        assert!(merged.contains("## Runtime Policy"));
        assert!(merged.contains("workspace_attached: yes"));
    }

    #[test]
    fn summarize_tool_visibility_marks_runtime_as_unresolved_without_fake_tools() {
        let summary = summarize_tool_visibility(None, &[]);

        assert!(!summary.resolved);
        assert_eq!(summary.toolset_label, "runtime_unresolved");
        assert!(summary.tool_names.is_empty());
        assert!(summary.markdown.contains("resolved: no"));
    }

    #[test]
    fn summarize_tool_visibility_with_only_mcp_keeps_runtime_unresolved() {
        let mcp_servers = vec![RuntimeMcpServer::Http {
            name: "agentdash-project-tools".to_string(),
            url: "http://127.0.0.1:3001/mcp/project/123".to_string(),
        }];

        let summary = summarize_tool_visibility(None, &mcp_servers);

        assert!(!summary.resolved);
        assert_eq!(summary.toolset_label, "runtime_unresolved");
        assert_eq!(
            summary.tool_names,
            vec!["mcp:agentdash-project-tools".to_string()]
        );
        assert!(!summary.tool_names.iter().any(|tool| tool == "read_file"));
        assert!(!summary.tool_names.iter().any(|tool| tool == "write_file"));
        assert!(
            !summary
                .tool_names
                .iter()
                .any(|tool| tool == "list_directory")
        );
        assert!(!summary.tool_names.iter().any(|tool| tool == "search"));
        assert!(!summary.tool_names.iter().any(|tool| tool == "shell"));
        assert!(summary.markdown.contains("resolved: no"));
        assert!(summary.markdown.contains("`agentdash-project-tools`"));
    }

    #[test]
    fn resolve_story_session_composition_reads_story_level_config() {
        let project = Project::new("demo".to_string(), "desc".to_string());
        let mut story = Story::new(project.id, "story".to_string(), "desc".to_string());
        story.context.session_composition = Some(SessionComposition {
            persona_label: Some("故事级角色".to_string()),
            persona_prompt: Some("先读 Story 约束".to_string()),
            workflow_steps: vec!["故事级步骤".to_string()],
            required_context_blocks: vec![SessionRequiredContextBlock {
                title: "故事级上下文".to_string(),
                content: "故事补充说明".to_string(),
            }],
        });

        let effective =
            resolve_story_session_composition(Some(&story)).expect("story session composition");

        assert_eq!(effective.persona_label.as_deref(), Some("故事级角色"));
        assert_eq!(effective.persona_prompt.as_deref(), Some("先读 Story 约束"));
        assert_eq!(effective.workflow_steps, vec!["故事级步骤".to_string()]);
        assert_eq!(
            effective.required_context_blocks,
            vec![SessionRequiredContextBlock {
                title: "故事级上下文".to_string(),
                content: "故事补充说明".to_string(),
            }]
        );
    }
}
