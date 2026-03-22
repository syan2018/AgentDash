use std::fs;
use std::path::{Path, PathBuf};

use agentdash_domain::project::Project;
use agentdash_domain::story::Story;
use agentdash_domain::task::Task;
use agentdash_domain::workflow::{
    WorkflowContextBinding, WorkflowContextBindingKind, WorkflowTargetKind,
};
use agentdash_domain::workspace::Workspace;
use serde::Serialize;

const MAX_WORKFLOW_DOCUMENT_CHARS: usize = 6_000;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Snapshot of a single resolved binding, serialisable and safe to expose to
/// the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowResolvedBindingSnapshot {
    pub kind: WorkflowContextBindingKind,
    pub locator: String,
    pub reason: String,
    pub required: bool,
    pub title: Option<String>,
    pub resolved: bool,
    pub summary: String,
}

/// Internal resolution result carrying both the snapshot and optional rendered
/// markdown content for injection into agent context.
#[derive(Debug, Clone)]
pub struct ResolvedWorkflowBinding {
    pub snapshot: WorkflowResolvedBindingSnapshot,
    pub content_markdown: Option<String>,
}

/// Input context required by binding resolution.  The fields mirror the
/// entities available at the session bootstrap / hook snapshot building call
/// sites.
pub struct BindingResolutionContext<'a> {
    pub target_kind: WorkflowTargetKind,
    pub project: &'a Project,
    pub story: Option<&'a Story>,
    pub task: Option<&'a Task>,
    pub workspace: Option<&'a Workspace>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn resolve_binding(
    binding: &WorkflowContextBinding,
    context: &BindingResolutionContext<'_>,
) -> ResolvedWorkflowBinding {
    match binding.kind {
        WorkflowContextBindingKind::DocumentPath => {
            resolve_document_binding(binding, context.workspace)
        }
        WorkflowContextBindingKind::RuntimeContext => resolve_runtime_binding(binding, context),
        WorkflowContextBindingKind::Checklist => resolve_checklist_binding(binding, context),
        WorkflowContextBindingKind::JournalTarget => {
            resolve_journal_binding(binding, context.workspace)
        }
        WorkflowContextBindingKind::ActionRef => resolve_action_binding(binding),
    }
}

/// Build a rendered markdown section for an already-resolved list of bindings.
pub fn build_bindings_markdown(resolved: &[ResolvedWorkflowBinding]) -> Option<String> {
    if resolved.is_empty() {
        return None;
    }
    let body = resolved
        .iter()
        .map(|b| {
            let header = format!(
                "- {} [{}] {}",
                binding_display_title_from_snapshot(&b.snapshot),
                if b.snapshot.resolved {
                    "resolved"
                } else if b.snapshot.required {
                    "missing-required"
                } else {
                    "missing-optional"
                },
                b.snapshot.summary
            );
            match b.content_markdown.as_deref() {
                Some(content) => format!("{header}\n\n{content}"),
                None => header,
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(format!("## Phase Bindings\n{body}"))
}

// ---------------------------------------------------------------------------
// Individual resolvers
// ---------------------------------------------------------------------------

fn resolve_document_binding(
    binding: &WorkflowContextBinding,
    workspace: Option<&Workspace>,
) -> ResolvedWorkflowBinding {
    let roots = candidate_roots(workspace);
    for root in roots {
        let path = root.join(binding.locator.trim());
        if !path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let trimmed = truncate_text(content.trim().to_string(), MAX_WORKFLOW_DOCUMENT_CHARS);
        let display_path = normalize_path(&path);
        let title = binding_display_title(binding);
        return ResolvedWorkflowBinding {
            snapshot: WorkflowResolvedBindingSnapshot {
                kind: binding.kind,
                locator: binding.locator.clone(),
                reason: binding.reason.clone(),
                required: binding.required,
                title: binding.title.clone(),
                resolved: true,
                summary: format!("已注入文档 `{display_path}`"),
            },
            content_markdown: Some(format!(
                "### {title}\n- source: `{display_path}`\n\n{trimmed}"
            )),
        };
    }

    unresolved_binding(binding, "未找到可读取的文档路径")
}

fn resolve_runtime_binding(
    binding: &WorkflowContextBinding,
    context: &BindingResolutionContext<'_>,
) -> ResolvedWorkflowBinding {
    let content = match binding.locator.trim() {
        "project_session_context" => Some(render_project_runtime_context(context.project)),
        "story_prd" => context
            .story
            .and_then(|story| clean_text(story.context.prd_doc.as_deref()))
            .map(|prd| format!("### {}\n{prd}", binding_display_title(binding))),
        "story_context_snapshot" => context.story.map(render_story_runtime_context),
        "task_execution_context" => context
            .task
            .map(|task| render_task_runtime_context(task, context.story)),
        _ => None,
    };

    match content {
        Some(content_markdown) => ResolvedWorkflowBinding {
            snapshot: WorkflowResolvedBindingSnapshot {
                kind: binding.kind,
                locator: binding.locator.clone(),
                reason: binding.reason.clone(),
                required: binding.required,
                title: binding.title.clone(),
                resolved: true,
                summary: "已注入运行时上下文".to_string(),
            },
            content_markdown: Some(content_markdown),
        },
        None => unresolved_binding(binding, "当前目标缺少可解析的运行时上下文"),
    }
}

fn resolve_checklist_binding(
    binding: &WorkflowContextBinding,
    context: &BindingResolutionContext<'_>,
) -> ResolvedWorkflowBinding {
    let items = match binding.locator.trim() {
        "project_review_checklist" => vec![
            "确认项目级默认 Agent、上下文容器与挂载策略是否仍然一致。",
            "确认当前流程约束已沉淀到共享文档，而不是只停留在对话里。",
            "确认后续 Story/Task 能直接消费当前沉淀的上下文。",
        ],
        "story_review_checklist" => vec![
            "确认 Story PRD、验收条件与关键引用已经完整。",
            "确认 Story 拆解出的 Task 与当前目标边界一致。",
            "确认后续执行所需上下文已经能被任务会话消费。",
        ],
        "task_review_checklist" => vec![
            "确认实现结果覆盖当前 Task 与 Story 的目标。",
            "确认验证步骤、风险说明和剩余问题已经写清楚。",
            "确认记录产物足以支持下一位协作者继续接手。",
        ],
        _ => checklist_fallback(context.target_kind),
    };
    let title = binding_display_title(binding);
    ResolvedWorkflowBinding {
        snapshot: WorkflowResolvedBindingSnapshot {
            kind: binding.kind,
            locator: binding.locator.clone(),
            reason: binding.reason.clone(),
            required: binding.required,
            title: binding.title.clone(),
            resolved: true,
            summary: "已注入检查清单".to_string(),
        },
        content_markdown: Some(format!(
            "### {title}\n{}",
            items
                .iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
}

fn resolve_journal_binding(
    binding: &WorkflowContextBinding,
    workspace: Option<&Workspace>,
) -> ResolvedWorkflowBinding {
    let roots = candidate_roots(workspace);
    for root in roots {
        let path = root.join(".trellis/workspace");
        if path.is_dir() {
            let display_path = normalize_path(&path);
            return ResolvedWorkflowBinding {
                snapshot: WorkflowResolvedBindingSnapshot {
                    kind: binding.kind,
                    locator: binding.locator.clone(),
                    reason: binding.reason.clone(),
                    required: binding.required,
                    title: binding.title.clone(),
                    resolved: true,
                    summary: format!("记录目标位于 `{display_path}`"),
                },
                content_markdown: Some(format!(
                    "### {}\n- journal_root: `{display_path}`\n- guidance: 当前阶段输出应沉淀为可复用记录，方便后续会话恢复上下文。",
                    binding_display_title(binding)
                )),
            };
        }
    }

    ResolvedWorkflowBinding {
        snapshot: WorkflowResolvedBindingSnapshot {
            kind: binding.kind,
            locator: binding.locator.clone(),
            reason: binding.reason.clone(),
            required: binding.required,
            title: binding.title.clone(),
            resolved: false,
            summary: "未发现 `.trellis/workspace`，改为提示记录目标语义".to_string(),
        },
        content_markdown: Some(format!(
            "### {}\n- guidance: 当前阶段应输出可沉淀的记录内容（阶段总结、后续建议、归档说明）。",
            binding_display_title(binding)
        )),
    }
}

fn resolve_action_binding(binding: &WorkflowContextBinding) -> ResolvedWorkflowBinding {
    let guidance = match binding.locator.trim() {
        "workflow_archive_action" => {
            "可在当前阶段给出归档建议或下一步归档动作，但不要把归档本身伪装成已经完成的事实。"
        }
        _ => "当前阶段可以输出与该动作相关的建议或操作说明。",
    };

    ResolvedWorkflowBinding {
        snapshot: WorkflowResolvedBindingSnapshot {
            kind: binding.kind,
            locator: binding.locator.clone(),
            reason: binding.reason.clone(),
            required: binding.required,
            title: binding.title.clone(),
            resolved: true,
            summary: "已注入动作语义提示".to_string(),
        },
        content_markdown: Some(format!(
            "### {}\n- action: `{}`\n- guidance: {}",
            binding_display_title(binding),
            binding.locator.trim(),
            guidance
        )),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unresolved_binding(binding: &WorkflowContextBinding, summary: &str) -> ResolvedWorkflowBinding {
    let title = binding_display_title(binding);
    ResolvedWorkflowBinding {
        snapshot: WorkflowResolvedBindingSnapshot {
            kind: binding.kind,
            locator: binding.locator.clone(),
            reason: binding.reason.clone(),
            required: binding.required,
            title: binding.title.clone(),
            resolved: false,
            summary: summary.to_string(),
        },
        content_markdown: Some(format!(
            "### {title}\n- locator: `{}`\n- status: {}\n- note: 当前阶段仍会保留该约束语义，请谨慎说明缺失上下文。",
            binding.locator.trim(),
            summary
        )),
    }
}

fn render_project_runtime_context(project: &Project) -> String {
    format!(
        "### Project Session Context\n- project: {}\n- default_agent_type: {}\n- context_containers: {}\n- workflow_steps: {}\n- required_context_blocks: {}",
        clean_text(Some(project.name.as_str())).unwrap_or("-"),
        clean_text(project.config.default_agent_type.as_deref()).unwrap_or("-"),
        project.config.context_containers.len(),
        project.config.session_composition.workflow_steps.len(),
        project
            .config
            .session_composition
            .required_context_blocks
            .len(),
    )
}

fn render_story_runtime_context(story: &Story) -> String {
    format!(
        "### Story Context Snapshot\n- story: {}\n- prd: {}\n- spec_refs: {}\n- resources: {}\n- source_refs: {}\n- context_containers: {}\n- workflow_override_steps: {}",
        clean_text(Some(story.title.as_str())).unwrap_or("-"),
        yes_no(clean_text(story.context.prd_doc.as_deref()).is_some()),
        story.context.spec_refs.len(),
        story.context.resource_list.len(),
        story.context.source_refs.len(),
        story.context.context_containers.len(),
        story
            .context
            .session_composition_override
            .as_ref()
            .map(|item| item.workflow_steps.len())
            .unwrap_or(0),
    )
}

fn render_task_runtime_context(task: &Task, story: Option<&Story>) -> String {
    format!(
        "### Task Execution Context\n- task: {}\n- status: {:?}\n- story: {}\n- prompt_template: {}\n- initial_context: {}\n- declared_sources: {}\n- has_session: {}",
        clean_text(Some(task.title.as_str())).unwrap_or("-"),
        task.status,
        story
            .and_then(|item| clean_text(Some(item.title.as_str())))
            .unwrap_or("-"),
        yes_no(clean_text(task.agent_binding.prompt_template.as_deref()).is_some()),
        yes_no(clean_text(task.agent_binding.initial_context.as_deref()).is_some()),
        task.agent_binding.context_sources.len(),
        yes_no(task.session_id.is_some()),
    )
}

fn checklist_fallback(target_kind: WorkflowTargetKind) -> Vec<&'static str> {
    match target_kind {
        WorkflowTargetKind::Project => vec![
            "确认项目级上下文与流程约束仍然一致。",
            "确认共享资料对后续协作者仍然可消费。",
        ],
        WorkflowTargetKind::Story => vec![
            "确认 Story 目标、约束和拆解仍然清晰。",
            "确认执行所需上下文已经准备完成。",
        ],
        WorkflowTargetKind::Task => vec![
            "确认当前实现与验证结果一致。",
            "确认交接说明和记录产物足够清楚。",
        ],
    }
}

pub fn binding_display_title(binding: &WorkflowContextBinding) -> &str {
    binding
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(binding.locator.as_str())
}

pub fn binding_display_title_from_snapshot(binding: &WorkflowResolvedBindingSnapshot) -> &str {
    binding
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(binding.locator.as_str())
}

fn candidate_roots(workspace: Option<&Workspace>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(workspace) = workspace {
        let trimmed = workspace.container_ref.trim();
        if !trimmed.is_empty() {
            roots.push(PathBuf::from(trimmed));
        }
    }
    roots
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn truncate_text(content: String, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content;
    }
    let truncated = content.chars().take(max_chars).collect::<String>();
    format!("{truncated}\n\n> 内容已截断")
}

fn clean_text(input: Option<&str>) -> Option<&str> {
    input.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
