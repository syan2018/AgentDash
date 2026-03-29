use std::collections::HashMap;

use crate::address_space::selected_workspace_binding;
use agentdash_domain::context_source::ContextSourceKind;
use agentdash_injection::{
    ContextFragment, MergeStrategy, ResolveSourcesRequest, resolve_declared_sources,
};

use crate::runtime::{RuntimeMcpBinding, RuntimeToolScope};

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

// ─── 内置 Contributor 实现 ──────────────────────────────────

/// Task/Story/Project/Workspace 核心上下文
pub(crate) struct CoreContextContributor;

impl ContextContributor for CoreContextContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        let mut fragments = Vec::new();

        fragments.push(ContextFragment {
            slot: "task",
            label: "task_core",
            order: 10,
            strategy: MergeStrategy::Append,
            content: format!(
                "## Task\n- id: {}\n- title: {}\n- description: {}\n- status: {:?}",
                input.task.id,
                trim_or_dash(&input.task.title),
                trim_or_dash(&input.task.description),
                input.task.status
            ),
        });

        fragments.push(ContextFragment {
            slot: "story",
            label: "story_core",
            order: 20,
            strategy: MergeStrategy::Append,
            content: format!(
                "## Story\n- id: {}\n- title: {}\n- description: {}",
                input.story.id,
                trim_or_dash(&input.story.title),
                trim_or_dash(&input.story.description),
            ),
        });

        if let Some(prd) = clean_text(input.story.context.prd_doc.as_deref()) {
            fragments.push(ContextFragment {
                slot: "story_context",
                label: "story_prd",
                order: 30,
                strategy: MergeStrategy::Append,
                content: format!("## Story PRD\n{prd}"),
            });
        }

        if !input.story.context.spec_refs.is_empty() {
            let refs = input
                .story
                .context
                .spec_refs
                .iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n");
            fragments.push(ContextFragment {
                slot: "story_context",
                label: "story_spec_refs",
                order: 31,
                strategy: MergeStrategy::Append,
                content: format!("## Spec Refs\n{refs}"),
            });
        }

        if !input.story.context.resource_list.is_empty() {
            let resources = input
                .story
                .context
                .resource_list
                .iter()
                .map(|res| format!("- [{}] {} ({})", res.resource_type, res.name, res.uri))
                .collect::<Vec<_>>()
                .join("\n");
            fragments.push(ContextFragment {
                slot: "story_context",
                label: "story_resources",
                order: 32,
                strategy: MergeStrategy::Append,
                content: format!("## Resources\n{resources}"),
            });
        }

        fragments.push(ContextFragment {
            slot: "project",
            label: "project_config",
            order: 40,
            strategy: MergeStrategy::Append,
            content: format!(
                "## Project\n- id: {}\n- name: {}\n- default_agent_type: {}",
                input.project.id,
                trim_or_dash(&input.project.name),
                input
                    .project
                    .config
                    .default_agent_type
                    .as_deref()
                    .unwrap_or("-")
            ),
        });

        if let Some(workspace) = input.workspace {
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
                slot: "workspace",
                label: "workspace_context",
                order: 50,
                strategy: MergeStrategy::Append,
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
}

/// Agent 绑定上下文（initial_context）
pub(crate) struct BindingContextContributor;

impl ContextContributor for BindingContextContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        let mut fragments = Vec::new();

        if let Some(initial_context) =
            clean_text(input.task.agent_binding.initial_context.as_deref())
        {
            fragments.push(ContextFragment {
                slot: "initial_context",
                label: "binding_initial_context",
                order: 80,
                strategy: MergeStrategy::Append,
                content: format!("## Initial Context\n{initial_context}"),
            });
        }

        Contribution::fragments_only(fragments)
    }
}

/// 声明式来源注入 Contributor（Story + Task 的非工作空间来源）
pub(crate) struct DeclaredSourcesContributor;

impl ContextContributor for DeclaredSourcesContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        let mut sources = input.story.context.source_refs.clone();
        sources.extend(input.task.agent_binding.context_sources.clone());

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
            workspace_root: None,
            base_order: 82,
        }) {
            Ok(result) => {
                fragments.extend(result.fragments);
                if !result.warnings.is_empty() {
                    fragments.push(ContextFragment {
                        slot: "references",
                        label: "declared_source_warnings",
                        order: 89,
                        strategy: MergeStrategy::Append,
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
                    slot: "references",
                    label: "declared_source_error",
                    order: 89,
                    strategy: MergeStrategy::Append,
                    content: format!("## Injection Error\n- 声明式上下文来源解析失败：{}", err),
                });
                Contribution::fragments_only(fragments)
            }
        }
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
        let mut fragments = Vec::new();

        let workspace_path = input
            .workspace
            .map(|_| ".".to_string())
            .unwrap_or_else(|| ".".to_string());

        let template = resolve_instruction_template(input);
        let rendered = render_template(
            &template,
            &template_vars(
                &input.task.title,
                &input.task.description,
                &input.story.title,
                &workspace_path,
            ),
        );
        fragments.push(ContextFragment {
            slot: "instruction",
            label: "binding_template",
            order: 90,
            strategy: MergeStrategy::Override,
            content: format!("## Instruction\n{rendered}"),
        });

        if input.phase == TaskExecutionPhase::Continue
            && let Some(additional) = clean_text(input.additional_prompt)
        {
            fragments.push(ContextFragment {
                slot: "instruction",
                label: "user_additional_prompt",
                order: 100,
                strategy: MergeStrategy::Append,
                content: format!("## Additional Prompt\n{additional}"),
            });
        }

        Contribution::fragments_only(fragments)
    }
}

/// MCP 能力注入 Contributor — 通过 ACP 协议类型声明 MCP Server，并在上下文中附加简要说明
pub struct McpContextContributor {
    pub config: RuntimeMcpBinding,
}

impl McpContextContributor {
    pub fn new(config: RuntimeMcpBinding) -> Self {
        Self { config }
    }
}

impl ContextContributor for McpContextContributor {
    fn contribute(&self, _input: &ContributorInput<'_>) -> Contribution {
        let label: &'static str = match self.config.scope {
            RuntimeToolScope::Relay => "mcp_relay_tools",
            RuntimeToolScope::Story => "mcp_story_tools",
            RuntimeToolScope::Task => "mcp_task_tools",
        };

        Contribution {
            context_fragments: vec![ContextFragment {
                slot: "mcp_config",
                label,
                order: 85,
                strategy: MergeStrategy::Append,
                content: self.config.to_context_content(),
            }],
            mcp_servers: vec![self.config.to_runtime_server()],
        }
    }
}
