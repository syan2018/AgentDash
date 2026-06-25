use agentdash_domain::common::{Mount, MountCapability};
use uuid::Uuid;

use super::mount::PROVIDER_ROUTINE_VFS;

pub fn build_routine_mount(
    routine_id: Uuid,
    execution_id: Uuid,
    trigger_source: &str,
    entity_key: Option<&str>,
) -> Mount {
    Mount {
        id: "routine".to_string(),
        provider: PROVIDER_ROUTINE_VFS.to_string(),
        backend_id: String::new(),
        root_ref: format!("routine://routine/{routine_id}"),
        capabilities: vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
            MountCapability::Search,
        ],
        default_write: false,
        display_name: "Routine Memory".to_string(),
        metadata: serde_json::json!({
            "routine_id": routine_id.to_string(),
            "execution_id": execution_id.to_string(),
            "trigger_source": trigger_source,
            "entity_key": entity_key,
            "directory_hint": [
                "current/trigger.json",
                "current/execution.json",
                "current/resolved-prompt.md",
                "memory/brief.md",
                "memory/facts.md",
                "memory/decisions.md",
                "memory/open-items.md",
                "memory/changelog.md",
                "entities/{entity_key}/brief.md",
                "entities/{entity_key}/facts.md",
                "entities/{entity_key}/open-items.md",
                "entities/{entity_key}/last-run.md"
            ]
        }),
    }
}
