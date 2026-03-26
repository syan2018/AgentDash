use agentdash_injection::{ContextComposer, ContextFragment, MergeStrategy};
use serde_json::{Value, json};

use super::contributor::{
    BuiltTaskAgentContext, ContextContributorRegistry, ContributorInput, TaskAgentBuildInput,
    TaskExecutionPhase,
};
use crate::session_plan::{
    SessionPlanInput, SessionPlanOwnerKind, SessionPlanPhase, build_session_plan_fragments,
    resolve_story_session_composition,
};

pub fn build_task_agent_context(
    input: TaskAgentBuildInput<'_>,
    registry: &ContextContributorRegistry,
) -> Result<BuiltTaskAgentContext, String> {
    let contributor_input = ContributorInput {
        task: input.task,
        story: input.story,
        project: input.project,
        workspace: input.workspace,
        phase: input.phase,
        override_prompt: input.override_prompt,
        additional_prompt: input.additional_prompt,
    };

    let working_dir = input.workspace.map(|_| ".".to_string());

    let mut context_composer = ContextComposer::default();
    let mut mcp_servers = Vec::new();

    let all_contributors = registry
        .contributors
        .iter()
        .map(|c| c.as_ref())
        .chain(input.extra_contributors.iter().map(|c| c.as_ref()));

    for contributor in all_contributors {
        let contribution = contributor.contribute(&contributor_input);

        mcp_servers.extend(contribution.mcp_servers);

        for fragment in contribution.context_fragments {
            if !matches!(fragment.slot, "instruction" | "instruction_append") {
                context_composer.push_fragment(fragment);
            }
        }
    }

    let effective_session_composition = resolve_story_session_composition(Some(input.story));
    let session_plan = build_session_plan_fragments(SessionPlanInput {
        owner_kind: SessionPlanOwnerKind::TaskExecution,
        phase: match input.phase {
            TaskExecutionPhase::Start => SessionPlanPhase::TaskStart,
            TaskExecutionPhase::Continue => SessionPlanPhase::TaskContinue,
        },
        address_space: input.address_space,
        mcp_servers: &mcp_servers,
        session_composition: effective_session_composition.as_ref(),
        agent_type: input.effective_agent_type,
        preset_name: input.task.agent_binding.preset_name.as_deref(),
        has_custom_prompt_template: input
            .task
            .agent_binding
            .prompt_template
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        has_initial_context: input
            .task
            .agent_binding
            .initial_context
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        workspace_attached: input.workspace.is_some(),
    });
    for fragment in session_plan.fragments {
        context_composer.push_fragment(fragment);
    }

    let (context_prompt, source_summary) = context_composer.compose();

    if context_prompt.trim().is_empty() {
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

    Ok(BuiltTaskAgentContext {
        prompt_blocks,
        working_dir,
        source_summary,
        mcp_servers,
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

pub fn build_declared_source_warning_fragment(
    label: &'static str,
    order: i32,
    warnings: &[String],
) -> ContextFragment {
    ContextFragment {
        slot: "references",
        label,
        order,
        strategy: MergeStrategy::Append,
        content: format!(
            "## Injection Notes\n{}",
            warnings
                .iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}
