use agentdash_domain::{project::Project, workspace::Workspace};
use agentdash_spi::{ContextFragment, MergeStrategy};

use crate::context::{Contribution, trim_or_dash, workspace_context_fragment};
use crate::runtime::{RuntimeMcpServer, Vfs};
use crate::session::plan::{SessionPlanInput, SessionPlanPhase, build_session_plan_fragments};

pub struct ProjectContextBuildInput<'a> {
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub vfs: Option<&'a Vfs>,
    pub mcp_servers: &'a [RuntimeMcpServer],
    pub effective_agent_type: Option<&'a str>,
    pub preset_name: Option<&'a str>,
    pub agent_display_name: &'a str,
}

/// 把 Project owner session 的业务上下文聚合为一个 `Contribution`。
pub fn contribute_project_context(input: ProjectContextBuildInput<'_>) -> Contribution {
    let mut fragments = Vec::new();

    fragments.push(ContextFragment {
        slot: "project".to_string(),
        label: "project_core".to_string(),
        order: 10,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:project_context".to_string(),
        content: format!(
            "## Project\n- id: {}\n- name: {}\n- description: {}",
            input.project.id,
            trim_or_dash(&input.project.name),
            trim_or_dash(&input.project.description),
        ),
    });

    fragments.push(ContextFragment {
        slot: "project".to_string(),
        label: "project_agent_identity".to_string(),
        order: 20,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:project_context".to_string(),
        content: format!(
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
    });

    if let Some(workspace) = input.workspace {
        fragments.push(workspace_context_fragment(workspace));
    }

    let session_plan = build_session_plan_fragments(SessionPlanInput {
        owner_ctx: agentdash_domain::session_binding::SessionOwnerCtx::Project {
            project_id: input.project.id,
        },
        phase: SessionPlanPhase::ProjectAgent,
        vfs: input.vfs,
        mcp_servers: input.mcp_servers,
        session_composition: None,
        agent_type: input.effective_agent_type,
        preset_name: input.preset_name,
        has_custom_prompt_template: false,
        has_initial_context: false,
        workspace_attached: input.workspace.is_some(),
    });
    fragments.extend(session_plan.fragments);

    Contribution::fragments_only(fragments)
}
