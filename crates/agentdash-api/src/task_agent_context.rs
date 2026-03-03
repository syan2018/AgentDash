use std::collections::HashMap;

use agentdash_domain::{project::Project, story::Story, task::Task, workspace::Workspace};
use serde_json::{Value, json};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskExecutionPhase {
    Start,
    Continue,
}

pub struct TaskAgentBuildInput<'a> {
    pub task: &'a Task,
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub phase: TaskExecutionPhase,
    pub override_prompt: Option<&'a str>,
    pub additional_prompt: Option<&'a str>,
}

pub struct BuiltTaskAgentContext {
    pub prompt_blocks: Vec<Value>,
    pub working_dir: Option<String>,
    pub source_summary: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MergeStrategy {
    Append,
    Override,
}

#[derive(Debug)]
struct ContextFragment {
    slot: &'static str,
    label: &'static str,
    order: i32,
    strategy: MergeStrategy,
    content: String,
}

#[derive(Default)]
struct ContextComposer {
    fragments: Vec<ContextFragment>,
}

impl ContextComposer {
    fn push(
        &mut self,
        slot: &'static str,
        label: &'static str,
        order: i32,
        strategy: MergeStrategy,
        content: impl Into<String>,
    ) {
        let content = content.into();
        if content.trim().is_empty() {
            return;
        }
        self.fragments.push(ContextFragment {
            slot,
            label,
            order,
            strategy,
            content,
        });
    }

    fn compose(mut self) -> (String, Vec<String>) {
        self.fragments.sort_by_key(|item| item.order);

        let mut slot_order: Vec<&'static str> = Vec::new();
        let mut slot_chunks: HashMap<&'static str, Vec<String>> = HashMap::new();
        let mut source_summary: Vec<String> = Vec::new();

        for fragment in self.fragments {
            if !slot_chunks.contains_key(fragment.slot) {
                slot_order.push(fragment.slot);
            }
            source_summary.push(format!("{}({})", fragment.label, fragment.slot));

            match fragment.strategy {
                MergeStrategy::Append => {
                    slot_chunks
                        .entry(fragment.slot)
                        .or_default()
                        .push(fragment.content);
                }
                MergeStrategy::Override => {
                    slot_chunks.insert(fragment.slot, vec![fragment.content]);
                }
            }
        }

        let mut sections = Vec::new();
        for slot in slot_order {
            if let Some(chunks) = slot_chunks.remove(slot) {
                let merged = chunks
                    .into_iter()
                    .filter(|chunk| !chunk.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if !merged.trim().is_empty() {
                    sections.push(merged);
                }
            }
        }

        (sections.join("\n\n"), source_summary)
    }
}

pub fn build_task_agent_context(
    input: TaskAgentBuildInput<'_>,
) -> Result<BuiltTaskAgentContext, String> {
    let mut context_composer = ContextComposer::default();
    let mut instruction_composer = ContextComposer::default();

    let working_dir = input.workspace.map(|w| w.container_ref.clone());
    let workspace_path = working_dir.clone().unwrap_or_else(|| ".".to_string());

    context_composer.push(
        "task",
        "task_core",
        10,
        MergeStrategy::Append,
        format!(
            "## Task\n- id: {}\n- title: {}\n- description: {}\n- status: {:?}",
            input.task.id,
            trim_or_dash(&input.task.title),
            trim_or_dash(&input.task.description),
            input.task.status
        ),
    );

    context_composer.push(
        "story",
        "story_core",
        20,
        MergeStrategy::Append,
        format!(
            "## Story\n- id: {}\n- title: {}\n- description: {}",
            input.story.id,
            trim_or_dash(&input.story.title),
            trim_or_dash(&input.story.description),
        ),
    );

    if let Some(prd) = clean_text(input.story.context.prd_doc.as_deref()) {
        context_composer.push(
            "story_context",
            "story_prd",
            30,
            MergeStrategy::Append,
            format!("## Story PRD\n{prd}"),
        );
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
        context_composer.push(
            "story_context",
            "story_spec_refs",
            31,
            MergeStrategy::Append,
            format!("## Spec Refs\n{refs}"),
        );
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
        context_composer.push(
            "story_context",
            "story_resources",
            32,
            MergeStrategy::Append,
            format!("## Resources\n{resources}"),
        );
    }

    context_composer.push(
        "project",
        "project_config",
        40,
        MergeStrategy::Append,
        format!(
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
    );

    if let Some(workspace) = input.workspace {
        context_composer.push(
            "workspace",
            "workspace_context",
            50,
            MergeStrategy::Append,
            format!(
                "## Workspace\n- id: {}\n- name: {}\n- path: {}\n- type: {:?}\n- status: {:?}",
                workspace.id,
                trim_or_dash(&workspace.name),
                workspace.container_ref,
                workspace.workspace_type,
                workspace.status,
            ),
        );
    }

    if let Some(initial_context) = clean_text(input.task.agent_binding.initial_context.as_deref()) {
        context_composer.push(
            "initial_context",
            "binding_initial_context",
            80,
            MergeStrategy::Append,
            format!("## Initial Context\n{initial_context}"),
        );
    }

    let template = resolve_instruction_template(&input);
    let rendered_instruction = render_template(
        &template,
        &template_vars(
            &input.task.title,
            &input.task.description,
            &input.story.title,
            &workspace_path,
        ),
    );
    instruction_composer.push(
        "instruction",
        "binding_template",
        90,
        MergeStrategy::Override,
        format!("## Instruction\n{rendered_instruction}"),
    );

    if input.phase == TaskExecutionPhase::Continue {
        if let Some(additional) = clean_text(input.additional_prompt) {
            instruction_composer.push(
                "instruction",
                "user_additional_prompt",
                100,
                MergeStrategy::Append,
                format!("## Additional Prompt\n{additional}"),
            );
        }
    }

    let (context_prompt, mut source_summary) = context_composer.compose();
    let (instruction_prompt, instruction_sources) = instruction_composer.compose();
    source_summary.extend(instruction_sources);

    let combined_prompt = [context_prompt.as_str(), instruction_prompt.as_str()]
        .iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("\n\n");

    if combined_prompt.trim().is_empty() {
        return Err("构建执行上下文失败：最终 prompt 为空".to_string());
    }

    let mut prompt_blocks = Vec::new();
    if !context_prompt.trim().is_empty() {
        prompt_blocks.push(build_task_context_resource_block(
            input.task.id.to_string(),
            input.phase,
            context_prompt,
        ));
    }
    if !instruction_prompt.trim().is_empty() {
        prompt_blocks.push(json!({
            "type": "text",
            "text": instruction_prompt,
        }));
    }

    Ok(BuiltTaskAgentContext {
        prompt_blocks,
        working_dir,
        source_summary,
    })
}

fn build_task_context_resource_block(
    task_id: String,
    phase: TaskExecutionPhase,
    markdown: String,
) -> Value {
    let phase_label = match phase {
        TaskExecutionPhase::Start => "start",
        TaskExecutionPhase::Continue => "continue",
    };

    json!({
        "type": "resource",
        "resource": {
            "uri": format!("agentdash://task-context/{task_id}?phase={phase_label}"),
            "mimeType": "text/markdown",
            "text": markdown,
        }
    })
}

fn resolve_instruction_template(input: &TaskAgentBuildInput<'_>) -> String {
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
        TaskExecutionPhase::Continue => {
            input
                .task
                .agent_binding
                .prompt_template
                .clone()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_CONTINUE_TEMPLATE.to_string())
        }
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

fn trim_or_dash(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() { "-" } else { trimmed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_keeps_initial_context_when_instruction_slot_is_override() {
        let mut composer = ContextComposer::default();
        composer.push(
            "initial_context",
            "binding_initial_context",
            80,
            MergeStrategy::Append,
            "## Initial Context\ncontext from binding",
        );
        composer.push(
            "instruction",
            "binding_template",
            90,
            MergeStrategy::Override,
            "## Instruction\nrun task",
        );

        let (prompt, _) = composer.compose();
        assert!(prompt.contains("context from binding"));
        assert!(prompt.contains("## Instruction"));
    }
}
