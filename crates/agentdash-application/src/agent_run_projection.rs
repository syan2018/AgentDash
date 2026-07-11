use std::collections::HashMap;

use agentdash_domain::{agent::ProjectAgent, workflow::LifecycleAgent};
use uuid::Uuid;

pub(crate) const MAX_AGENT_LINEAGE_DEPTH: usize = 16;

pub(crate) fn project_agent_label(agent: &ProjectAgent) -> String {
    agent
        .preset_config()
        .ok()
        .and_then(|config| config.display_name)
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| agent.name.clone())
}

pub(crate) fn lifecycle_agent_title(
    agent: &LifecycleAgent,
    project_agent_labels: &HashMap<Uuid, String>,
) -> String {
    agent
        .workspace_title
        .clone()
        .or_else(|| {
            agent
                .project_agent_id
                .and_then(|id| project_agent_labels.get(&id).cloned())
        })
        .unwrap_or_else(|| agent.source.as_str().to_string())
}
