use agentdash_domain::{project::Project, workspace::Workspace};
use agentdash_spi::{ContextFragment, FragmentScope, FragmentScopeSet, MergeStrategy};

use crate::context::{Contribution, trim_or_dash, workspace_context_fragment};

pub struct ProjectContextBuildInput<'a> {
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub preset_name: Option<&'a str>,
    pub agent_display_name: &'a str,
}

/// 把 Project owner session 的业务上下文聚合为一个 `Contribution`。
///
/// 不包含 SessionPlan fragments；运行时画像由外层 composer 显式追加。
pub fn contribute_project_context(input: ProjectContextBuildInput<'_>) -> Contribution {
    let mut fragments = Vec::new();

    fragments.push(ContextFragment {
        slot: "project".to_string(),
        label: "project_core".to_string(),
        order: 10,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "project_context".to_string(),
        content: format!(
            "## Project\n- id: {}\n- name: {}\n- description: {}",
            input.project.id,
            trim_or_dash(&input.project.name),
            trim_or_dash(&input.project.description),
        ),
    });

    fragments.push(ContextFragment {
        slot: "agent_identity".to_string(),
        label: "project_agent_identity".to_string(),
        order: 20,
        strategy: MergeStrategy::Append,
        scope: FragmentScopeSet::only(FragmentScope::Audit),
        source: "project_context".to_string(),
        content: format!(
            "## Agent Identity\n- display_name: {}\n- preset_name: {}\n- default_agent_type: {}",
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

    Contribution::fragments_only(fragments)
}

#[cfg(test)]
mod tests {
    use agentdash_domain::project::Project;
    use agentdash_spi::FragmentScope;

    use super::*;

    #[test]
    fn project_agent_identity_is_audit_only() {
        let mut project = Project::new("AgentDash".to_string(), "我开发我自己".to_string());
        project.config.default_agent_type = Some("PI_AGENT".to_string());

        let contribution = contribute_project_context(ProjectContextBuildInput {
            project: &project,
            workspace: None,
            preset_name: Some("pi_agent_general"),
            agent_display_name: "Pi Agent General",
        });

        let project_core = contribution
            .fragments
            .iter()
            .find(|fragment| fragment.label == "project_core")
            .expect("project core fragment");
        assert_eq!(project_core.slot, "project");
        assert!(project_core.scope.contains(FragmentScope::RuntimeAgent));

        let agent_identity = contribution
            .fragments
            .iter()
            .find(|fragment| fragment.label == "project_agent_identity")
            .expect("agent identity fragment");
        assert_eq!(agent_identity.slot, "agent_identity");
        assert!(agent_identity.scope.contains(FragmentScope::Audit));
        assert!(!agent_identity.scope.contains(FragmentScope::RuntimeAgent));
        assert!(agent_identity.content.contains("## Agent Identity"));
        assert!(agent_identity.content.contains("Pi Agent General"));
    }
}
