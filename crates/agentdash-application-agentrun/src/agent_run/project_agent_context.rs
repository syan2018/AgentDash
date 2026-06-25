use agentdash_domain::{
    agent::ProjectAgent,
    common::{AgentConfig, AgentPresetConfig},
    project::Project,
    workspace::Workspace,
};

use crate::repository_set::RepositorySet;

pub const PROJECT_AGENT_BINDING_LABEL_PREFIX: &str = "project_agent:";

#[derive(Debug, Clone)]
pub struct ResolvedProjectAgentContext {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub executor_config: agentdash_spi::AgentConfig,
    pub preset_config: AgentPresetConfig,
    pub preset_name: Option<String>,
    pub source: String,
    pub project_agent: ProjectAgent,
}

pub async fn resolve_project_workspace(
    repos: &RepositorySet,
    project: &Project,
) -> Result<Option<Workspace>, String> {
    if let Some(workspace_id) = project.config.default_workspace_id {
        return repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(|error| error.to_string());
    }
    Ok(None)
}

pub async fn build_project_agent_context(
    _repos: &RepositorySet,
    agent: &ProjectAgent,
) -> Result<ResolvedProjectAgentContext, String> {
    let preset = agent.preset_config().map_err(|error| error.to_string())?;
    let executor_config: AgentConfig = preset.to_agent_config(&agent.agent_type);
    let display_name = preset
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&agent.name)
        .to_string();
    let description = preset
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("Agent `{}`，执行器 {}。", agent.name, agent.agent_type));

    Ok(ResolvedProjectAgentContext {
        key: agent.id.to_string(),
        display_name,
        description,
        executor_config,
        preset_config: preset,
        preset_name: Some(agent.name.clone()),
        source: format!("project_agents[{}]", agent.id),
        project_agent: agent.clone(),
    })
}
