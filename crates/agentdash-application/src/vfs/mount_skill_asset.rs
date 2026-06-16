use std::collections::BTreeSet;

use crate::runtime::{Mount, MountCapability, Vfs};
use uuid::Uuid;

use super::mount::{
    PROVIDER_LIFECYCLE_VFS, PROVIDER_SKILL_ASSET_FS, SKILL_ASSET_KEYS_METADATA_KEY,
    SKILL_ASSET_PROJECT_ID_METADATA_KEY,
};

pub fn build_project_skill_asset_management_mount(
    project_id: Uuid,
    skill_asset_keys: &[String],
) -> Mount {
    Mount {
        id: "skill-assets".to_string(),
        provider: PROVIDER_SKILL_ASSET_FS.to_string(),
        backend_id: String::new(),
        root_ref: format!("skill-assets://project/{project_id}"),
        capabilities: vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
            MountCapability::Search,
        ],
        default_write: true,
        display_name: "Project Skill Assets".to_string(),
        metadata: serde_json::json!({
            SKILL_ASSET_PROJECT_ID_METADATA_KEY: project_id.to_string(),
            SKILL_ASSET_KEYS_METADATA_KEY: normalized_skill_asset_keys(skill_asset_keys),
        }),
    }
}

pub fn append_lifecycle_skill_asset_projection(
    vfs: &mut Vfs,
    project_id: Uuid,
    skill_asset_keys: &[String],
) -> bool {
    let new_keys = normalized_skill_asset_keys(skill_asset_keys);
    if new_keys.is_empty() {
        return true;
    }

    if let Some(lifecycle) = vfs
        .mounts
        .iter_mut()
        .find(|mount| mount.id == "lifecycle" && mount.provider == PROVIDER_LIFECYCLE_VFS)
    {
        let mut metadata = match std::mem::take(&mut lifecycle.metadata) {
            serde_json::Value::Object(object) => object,
            serde_json::Value::Null => serde_json::Map::new(),
            other => {
                let mut object = serde_json::Map::new();
                object.insert("raw_metadata".to_string(), other);
                object
            }
        };
        let mut keys = metadata
            .get(SKILL_ASSET_KEYS_METADATA_KEY)
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        keys.extend(new_keys);
        let keys = normalized_skill_asset_keys(&keys);
        metadata.insert(
            SKILL_ASSET_PROJECT_ID_METADATA_KEY.to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
        metadata.insert(
            SKILL_ASSET_KEYS_METADATA_KEY.to_string(),
            serde_json::Value::Array(
                keys.iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        lifecycle.metadata = serde_json::Value::Object(metadata);
        return true;
    }

    false
}

fn normalized_skill_asset_keys(skill_asset_keys: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    skill_asset_keys
        .iter()
        .map(|key| key.trim())
        .filter(|key| !key.is_empty())
        .filter_map(|key| {
            if seen.insert(key.to_string()) {
                Some(key.to_string())
            } else {
                None
            }
        })
        .collect()
}
