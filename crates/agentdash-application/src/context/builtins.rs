use std::collections::HashMap;

use crate::vfs::selected_workspace_binding;
use agentdash_domain::context_source::ContextSourceKind;
use agentdash_spi::{ContextFragment, MergeStrategy, ResolveSourcesRequest};

use super::resolve_declared_sources;
use serde_json::{Value, json};

use super::contributor::{ContextContributor, Contribution, ContributorInput, TaskExecutionPhase};

// ─── 文本工具 ────────────────────────────────────────────────

pub(crate) fn clean_text(input: Option<&str>) -> Option<&str> {
    input.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

pub(crate) fn trim_or_dash(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() { "-" } else { trimmed }
}

// ─── Workspace Context Fragment ──────────────────────────────

/// Project / Story owner 路径共享的 workspace context fragment 构建。
///
/// Task owner 路径在 `StaticFragmentsContributor` 中有自己的版本（包含 status 字段），
/// 此函数仅供 project / story context builder 使用以消除重复。
pub(crate) fn workspace_context_fragment(
    workspace: &agentdash_domain::workspace::Workspace,
) -> ContextFragment {
    let binding_summary = selected_workspace_binding(workspace)
        .map(|binding| {
            format!(
                "{} @ {}",
                trim_or_dash(&binding.backend_id),
                trim_or_dash(&binding.root_ref)
            )
        })
        .unwrap_or_else(|| "-".to_string());

    ContextFragment {
        slot: "workspace".to_string(),
        label: "workspace_context".to_string(),
        order: 30,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:contributor:workspace".to_string(),
        content: format!(
            "## Workspace\n- id: {}\n- identity_kind: {:?}\n- name: {}\n- binding: {}\n- working_dir: .",
            workspace.id,
            workspace.identity_kind,
            trim_or_dash(&workspace.name),
            binding_summary,
        ),
    }
}

// ─── Owner Context Resource Block ───────────────────────────

/// 将 context markdown 封装为 ACP resource content block。
///
/// 所有 owner 类型（Project / Story / Task）的 context 都需要以
/// `{ "type": "resource", "resource": { uri, mimeType, text } }` 结构
/// 注入到 prompt blocks 中，此函数统一了该构建逻辑。
pub fn build_owner_context_resource_block(uri: &str, markdown: &str) -> Value {
    json!({
        "type": "resource",
        "resource": {
            "uri": uri,
            "mimeType": "text/markdown",
            "text": markdown,
        }
    })
}


// ─── 指令模板 ────────────────────────────────────────────────

const DEFAULT_START_TEMPLATE: &str = r#"你是该任务的执行 Agent。
请结合任务上下文完成实现，并在完成后给出关键变更与验证结果。

任务标题：{{task_title}}
任务描述：{{task_description}}
Story：{{story_title}}
工作目录：{{workspace_path}}"#;

const DEFAULT_CONTINUE_TEMPLATE: &str = r#"请在当前会话上下文基础上继续推进该任务。
优先完成未完成项，并说明下一步建议。

任务标题：{{task_title}}
任务描述：{{task_description}}
Story：{{story_title}}
工作目录：{{workspace_path}}"#;

fn resolve_instruction_template(input: &ContributorInput<'_>) -> String {
    match input.phase {
        TaskExecutionPhase::Start => {
            if let Some(override_prompt) = clean_text(input.override_prompt) {
                return override_prompt.to_string();
            }
            input
                .task
                .agent_binding
                .prompt_template
                .clone()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_START_TEMPLATE.to_string())
        }
        TaskExecutionPhase::Continue => input
            .task
            .agent_binding
            .prompt_template
            .clone()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CONTINUE_TEMPLATE.to_string()),
    }
}

fn template_vars(
    task_title: &str,
    task_description: &str,
    story_title: &str,
    workspace_path: &str,
) -> HashMap<&'static str, String> {
    let mut vars = HashMap::new();
    vars.insert("task_title", trim_or_dash(task_title).to_string());
    vars.insert(
        "task_description",
        trim_or_dash(task_description).to_string(),
    );
    vars.insert("story_title", trim_or_dash(story_title).to_string());
    vars.insert("workspace_path", trim_or_dash(workspace_path).to_string());
    vars
}

fn render_template(template: &str, vars: &HashMap<&'static str, String>) -> String {
    let mut rendered = template.to_string();
    for (key, value) in vars {
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
    }
    rendered
}

// ─── 领域自治 contribute_* 纯函数 ─────────────────────────────
//
// 每个 contribute_* 函数把 domain 对象解包成 Contribution，和 trait impl 产出的
// Markdown 完全等价。Step 3 先引入这些函数；Step 9 调用方切换过来后，trait impl
// 可以直接调用它们以 DRY，或整体下线。

/// Task/Story/Project/Workspace 核心上下文（对应 `CoreContextContributor`）
pub fn contribute_core_context(
    task: &agentdash_domain::task::Task,
    story: &agentdash_domain::story::Story,
    project: &agentdash_domain::project::Project,
    workspace: Option<&agentdash_domain::workspace::Workspace>,
) -> Contribution {
    let mut fragments = Vec::new();

    fragments.push(ContextFragment {
        slot: "task".to_string(),
        label: "task_core".to_string(),
        order: 10,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:contributor:core".to_string(),
        content: format!(
            "## Task\n- id: {}\n- title: {}\n- description: {}\n- status: {:?}",
            task.id,
            trim_or_dash(&task.title),
            trim_or_dash(&task.description),
            task.status()
        ),
    });

    fragments.push(ContextFragment {
        slot: "story".to_string(),
        label: "story_core".to_string(),
        order: 20,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:contributor:core".to_string(),
        content: format!(
            "## Story\n- id: {}\n- title: {}\n- description: {}",
            story.id,
            trim_or_dash(&story.title),
            trim_or_dash(&story.description),
        ),
    });

    fragments.push(ContextFragment {
        slot: "project".to_string(),
        label: "project_config".to_string(),
        order: 40,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:contributor:core".to_string(),
        content: format!(
            "## Project\n- id: {}\n- name: {}\n- default_agent_type: {}",
            project.id,
            trim_or_dash(&project.name),
            project
                .config
                .default_agent_type
                .as_deref()
                .unwrap_or("-")
        ),
    });

    if let Some(workspace) = workspace {
        let binding_summary = selected_workspace_binding(workspace)
            .map(|binding| {
                format!(
                    "{} @ {}",
                    trim_or_dash(&binding.backend_id),
                    trim_or_dash(&binding.root_ref)
                )
            })
            .unwrap_or_else(|| "-".to_string());
        fragments.push(ContextFragment {
            slot: "workspace".to_string(),
            label: "workspace_context".to_string(),
            order: 50,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "legacy:contributor:core".to_string(),
            content: format!(
                "## Workspace\n- id: {}\n- identity_kind: {:?}\n- name: {}\n- working_dir: .\n- binding: {}\n- status: {:?}",
                workspace.id,
                workspace.identity_kind,
                trim_or_dash(&workspace.name),
                binding_summary,
                workspace.status,
            ),
        });
    }

    Contribution::fragments_only(fragments)
}

/// Agent 绑定的 initial_context 片段（对应 `BindingContextContributor`）
pub fn contribute_binding_initial_context(task: &agentdash_domain::task::Task) -> Contribution {
    let mut fragments = Vec::new();
    if let Some(initial_context) = clean_text(task.agent_binding.initial_context.as_deref()) {
        fragments.push(ContextFragment {
            slot: "initial_context".to_string(),
            label: "binding_initial_context".to_string(),
            order: 80,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "legacy:contributor:binding".to_string(),
            content: format!("## Initial Context\n{initial_context}"),
        });
    }
    Contribution::fragments_only(fragments)
}

/// Story + Task 的声明式来源片段（对应 `DeclaredSourcesContributor`）
pub fn contribute_declared_sources(
    task: &agentdash_domain::task::Task,
    story: &agentdash_domain::story::Story,
) -> Contribution {
    let mut sources = story.context.source_refs.clone();
    sources.extend(task.agent_binding.context_sources.clone());

    if sources.is_empty() {
        return Contribution::fragments_only(Vec::new());
    }

    let mut fragments = Vec::new();
    let resolvable_sources = sources
        .iter()
        .filter(|source| {
            !matches!(
                source.kind,
                ContextSourceKind::File | ContextSourceKind::ProjectSnapshot
            )
        })
        .cloned()
        .collect::<Vec<_>>();

    match resolve_declared_sources(ResolveSourcesRequest {
        sources: &resolvable_sources,
        base_order: 82,
    }) {
        Ok(result) => {
            fragments.extend(result.fragments);
            if !result.warnings.is_empty() {
                fragments.push(ContextFragment {
                    slot: "references".to_string(),
                    label: "declared_source_warnings".to_string(),
                    order: 89,
                    strategy: MergeStrategy::Append,
                    scope: ContextFragment::default_scope(),
                    source: "legacy:contributor:declared_source".to_string(),
                    content: format!(
                        "## Injection Notes\n{}",
                        result
                            .warnings
                            .iter()
                            .map(|item| format!("- {item}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ),
                });
            }
            Contribution::fragments_only(fragments)
        }
        Err(err) => {
            fragments.push(ContextFragment {
                slot: "references".to_string(),
                label: "declared_source_error".to_string(),
                order: 89,
                strategy: MergeStrategy::Append,
                scope: ContextFragment::default_scope(),
                source: "legacy:contributor:declared_source".to_string(),
                content: format!("## Injection Error\n- 声明式上下文来源解析失败：{}", err),
            });
            Contribution::fragments_only(fragments)
        }
    }
}

/// 指令模板片段（对应 `InstructionContributor`）
pub fn contribute_instruction(
    task: &agentdash_domain::task::Task,
    story: &agentdash_domain::story::Story,
    _workspace: Option<&agentdash_domain::workspace::Workspace>,
    phase: TaskExecutionPhase,
    override_prompt: Option<&str>,
    additional_prompt: Option<&str>,
) -> Contribution {
    // 复用现有的 `ContributorInput`-based 模板解析器：构造一个临时的 dummy
    // ContributorInput 仅供 `resolve_instruction_template` 读取 phase / override_prompt /
    // prompt_template 字段。project/workspace 字段不会被读，保持 trait impl 行为逐字一致。
    let dummy_project = agentdash_domain::project::Project::new(String::new(), String::new());
    let input = ContributorInput {
        task,
        story,
        project: &dummy_project,
        workspace: None,
        phase,
        override_prompt,
        additional_prompt,
    };
    let template = resolve_instruction_template(&input);
    let workspace_path = ".".to_string();
    let rendered = render_template(
        &template,
        &template_vars(&task.title, &task.description, &story.title, &workspace_path),
    );

    let mut fragments = Vec::new();
    fragments.push(ContextFragment {
        slot: "instruction".to_string(),
        label: "binding_template".to_string(),
        order: 90,
        strategy: MergeStrategy::Override,
        scope: ContextFragment::default_scope(),
        source: "legacy:contributor:instruction".to_string(),
        content: format!("## Instruction\n{rendered}"),
    });

    if phase == TaskExecutionPhase::Continue
        && let Some(additional) = clean_text(additional_prompt)
    {
        fragments.push(ContextFragment {
            slot: "instruction".to_string(),
            label: "user_additional_prompt".to_string(),
            order: 100,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "legacy:contributor:instruction".to_string(),
            content: format!("## Additional Prompt\n{additional}"),
        });
    }

    Contribution::fragments_only(fragments)
}

/// MCP 能力注入（对应 `McpContextContributor`）
pub fn contribute_mcp(config: &agentdash_mcp::injection::McpInjectionConfig) -> Contribution {
    let label: &'static str = match config.scope {
        agentdash_mcp::scope::ToolScope::Relay => "mcp_relay_tools",
        agentdash_mcp::scope::ToolScope::Story => "mcp_story_tools",
        agentdash_mcp::scope::ToolScope::Task => "mcp_task_tools",
        agentdash_mcp::scope::ToolScope::Workflow => "mcp_workflow_tools",
    };

    let server = config.to_acp_mcp_server();
    let runtime_server = crate::runtime_bridge::acp_mcp_server_to_runtime(&server);

    Contribution {
        fragments: vec![ContextFragment {
            slot: "mcp_config".to_string(),
            label: label.to_string(),
            order: 85,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "legacy:contributor:mcp".to_string(),
            content: config.to_context_content(),
        }],
        mcp_servers: vec![runtime_server],
    }
}

// ─── 内置 Contributor 实现（body 复用上方 contribute_* 保持 DRY） ──

/// Task/Story/Project/Workspace 核心上下文
pub(crate) struct CoreContextContributor;

impl ContextContributor for CoreContextContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        contribute_core_context(input.task, input.story, input.project, input.workspace)
    }
}

/// Agent 绑定上下文（initial_context）
pub(crate) struct BindingContextContributor;

impl ContextContributor for BindingContextContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        contribute_binding_initial_context(input.task)
    }
}

/// 声明式来源注入 Contributor（Story + Task 的非工作空间来源）
pub(crate) struct DeclaredSourcesContributor;

impl ContextContributor for DeclaredSourcesContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        contribute_declared_sources(input.task, input.story)
    }
}

/// 静态片段贡献者 — 用于将预解析的片段（如工作空间文件源）注入上下文
pub struct StaticFragmentsContributor {
    fragments: Vec<ContextFragment>,
}

impl StaticFragmentsContributor {
    pub fn new(fragments: Vec<ContextFragment>) -> Self {
        Self { fragments }
    }
}

impl ContextContributor for StaticFragmentsContributor {
    fn contribute(&self, _input: &ContributorInput<'_>) -> Contribution {
        Contribution::fragments_only(self.fragments.clone())
    }
}

/// 指令模板 Contributor
pub(crate) struct InstructionContributor;

impl ContextContributor for InstructionContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        contribute_instruction(
            input.task,
            input.story,
            input.workspace,
            input.phase,
            input.override_prompt,
            input.additional_prompt,
        )
    }
}

/// MCP 能力注入 Contributor — 通过 ACP 协议类型声明 MCP Server，并在上下文中附加简要说明。
///
/// 接受 `McpInjectionConfig`（由 CapabilityResolver 产出），支持所有平台 MCP scope。
pub struct McpContextContributor {
    pub config: agentdash_mcp::injection::McpInjectionConfig,
}

impl McpContextContributor {
    pub fn new(config: agentdash_mcp::injection::McpInjectionConfig) -> Self {
        Self { config }
    }
}

impl ContextContributor for McpContextContributor {
    fn contribute(&self, _input: &ContributorInput<'_>) -> Contribution {
        contribute_mcp(&self.config)
    }
}
