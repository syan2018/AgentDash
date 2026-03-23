use agent_client_protocol::McpServer;
use agentdash_domain::{project::Project, workspace::Workspace};
use agentdash_executor::ExecutionAddressSpace;
use agentdash_injection::{ContextComposer, MergeStrategy};
use serde_json::json;

use crate::session_plan::{
    SessionPlanInput, SessionPlanOwnerKind, SessionPlanPhase, build_session_plan_fragments,
};

pub struct ProjectContextBuildInput<'a> {
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub address_space: Option<&'a ExecutionAddressSpace>,
    pub mcp_servers: &'a [McpServer],
    pub effective_agent_type: Option<&'a str>,
    pub preset_name: Option<&'a str>,
    pub agent_display_name: &'a str,
}

pub fn build_project_context_markdown(
    input: ProjectContextBuildInput<'_>,
) -> (String, Vec<String>) {
    let mut composer = ContextComposer::default();

    composer.push(
        "project",
        "project_core",
        10,
        MergeStrategy::Append,
        format!(
            "## Project\n- id: {}\n- name: {}\n- description: {}",
            input.project.id,
            trim_or_dash(&input.project.name),
            trim_or_dash(&input.project.description),
        ),
    );

    composer.push(
        "project",
        "project_agent_identity",
        20,
        MergeStrategy::Append,
        format!(
            "## Project Agent\n- display_name: {}\n- preset_name: {}\n- default_agent_type: {}",
            trim_or_dash(input.agent_display_name),
            input.preset_name.unwrap_or("-"),
            input
                .project
                .config
                .default_agent_type
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("-"),
        ),
    );

    if let Some(workspace) = input.workspace {
        composer.push(
            "workspace",
            "workspace_context",
            30,
            MergeStrategy::Append,
            format!(
                "## Workspace\n- id: {}\n- backend_id: {}\n- name: {}\n- working_dir: .",
                workspace.id,
                trim_or_dash(&workspace.backend_id),
                trim_or_dash(&workspace.name),
            ),
        );
    }

    let session_plan = build_session_plan_fragments(SessionPlanInput {
        owner_kind: SessionPlanOwnerKind::ProjectAgent,
        phase: SessionPlanPhase::ProjectAgent,
        address_space: input.address_space,
        mcp_servers: input.mcp_servers,
        session_composition: Some(&input.project.config.session_composition),
        agent_type: input.effective_agent_type,
        preset_name: input.preset_name,
        has_custom_prompt_template: false,
        has_initial_context: false,
        workspace_attached: input.workspace.is_some(),
    });
    for fragment in session_plan.fragments {
        composer.push_fragment(fragment);
    }

    composer.compose()
}

pub fn build_project_owner_prompt_blocks(
    project_id: uuid::Uuid,
    context_markdown: String,
    source_summary: &[String],
    workflow_instruction: Option<String>,
    original_prompt: Option<String>,
    original_prompt_blocks: Option<Vec<serde_json::Value>>,
) -> Vec<serde_json::Value> {
    let mut prefix_blocks = Vec::new();
    if !context_markdown.trim().is_empty() {
        prefix_blocks.push(json!({
            "type": "resource",
            "resource": {
                "uri": format!("agentdash://project-context/{}", project_id),
                "mimeType": "text/markdown",
                "text": context_markdown,
            }
        }));
    }

    let project_instruction = format!(
        "## Instruction\n你是该 Project 下的共享协作 Agent。请围绕项目共享上下文、资料整理、决策沉淀和后续 Story 准备展开工作。\n\n默认应把上下文组织成用户可理解的资料目录，而不是向用户强调底层 provider、mount derivation 或 runtime capability 细节。\n\n当前来源摘要：{}",
        if source_summary.is_empty() {
            "-".to_string()
        } else {
            source_summary.join(", ")
        }
    );
    prefix_blocks.push(json!({
        "type": "text",
        "text": project_instruction,
    }));
    if let Some(workflow_instruction) = workflow_instruction
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        prefix_blocks.push(json!({
            "type": "text",
            "text": workflow_instruction,
        }));
    }

    let user_blocks = match (original_prompt, original_prompt_blocks) {
        (Some(prompt), None) => vec![json!({ "type": "text", "text": prompt })],
        (None, Some(blocks)) => blocks,
        (Some(prompt), Some(mut blocks)) => {
            let mut merged = vec![json!({ "type": "text", "text": prompt })];
            merged.append(&mut blocks);
            merged
        }
        (None, None) => Vec::new(),
    };

    prefix_blocks.extend(user_blocks);
    prefix_blocks
}

fn trim_or_dash(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() { "-" } else { trimmed }
}
