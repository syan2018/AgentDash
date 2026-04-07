use agentdash_domain::{project::Project, workspace::Workspace};
use agentdash_injection::{ContextComposer, MergeStrategy};
use serde_json::json;

use crate::address_space::selected_workspace_binding;
use crate::context::trim_or_dash;
use crate::runtime::{AddressSpace, RuntimeMcpServer};
use crate::session_plan::{
    SessionOwnerType, SessionPlanInput, SessionPlanPhase, build_session_plan_fragments,
};

pub struct ProjectContextBuildInput<'a> {
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub address_space: Option<&'a AddressSpace>,
    pub mcp_servers: &'a [RuntimeMcpServer],
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
        let binding_summary = selected_workspace_binding(workspace)
            .map(|binding| {
                format!(
                    "{} @ {}",
                    trim_or_dash(&binding.backend_id),
                    trim_or_dash(&binding.root_ref)
                )
            })
            .unwrap_or_else(|| "-".to_string());
        composer.push(
            "workspace",
            "workspace_context",
            30,
            MergeStrategy::Append,
            format!(
                "## Workspace\n- id: {}\n- identity_kind: {:?}\n- name: {}\n- binding: {}\n- working_dir: .",
                workspace.id,
                workspace.identity_kind,
                trim_or_dash(&workspace.name),
                binding_summary,
            ),
        );
    }

    let session_plan = build_session_plan_fragments(SessionPlanInput {
        owner_type: SessionOwnerType::Project,
        phase: SessionPlanPhase::ProjectAgent,
        address_space: input.address_space,
        mcp_servers: input.mcp_servers,
        session_composition: None,
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
    user_prompt_blocks: Vec<serde_json::Value>,
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

    prefix_blocks.extend(user_prompt_blocks);
    prefix_blocks
}

