use agentdash_domain::agent::ProjectAgent;

pub(crate) const MAX_AGENT_LINEAGE_DEPTH: usize = 16;

pub(crate) fn project_agent_label(agent: &ProjectAgent) -> String {
    agent
        .preset_config()
        .ok()
        .and_then(|config| config.display_name)
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| agent.name.clone())
}
