use agentdash_domain::companion::COMPANION_SYSTEM_SKILL_NAME;
use agentdash_spi::Vfs;
use uuid::Uuid;

use crate::ApplicationError;
use crate::lifecycle::ActivityActivation;
use crate::repository_set::RepositorySet;
use crate::skill_asset::SkillAssetService;
use crate::vfs::{PROVIDER_LIFECYCLE_VFS, append_lifecycle_skill_asset_projection};

pub(crate) async fn ensure_companion_system_skill_asset(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<(), ApplicationError> {
    SkillAssetService::new(repos.skill_asset_repo.as_ref())
        .bootstrap_builtins(project_id, Some(COMPANION_SYSTEM_SKILL_NAME))
        .await
        .map(|_| ())
        .map_err(|error| ApplicationError::Internal(error.to_string()))
}

pub(crate) fn has_lifecycle_mount(vfs: &Vfs) -> bool {
    vfs.mounts
        .iter()
        .any(|mount| mount.id == "lifecycle" && mount.provider == PROVIDER_LIFECYCLE_VFS)
}

pub(crate) fn append_companion_system_skill_key(keys: &mut Vec<String>) {
    if keys
        .iter()
        .any(|key| key.trim() == COMPANION_SYSTEM_SKILL_NAME)
    {
        return;
    }
    keys.push(COMPANION_SYSTEM_SKILL_NAME.to_string());
}

pub(crate) fn append_lifecycle_companion_system_projection(
    vfs: &mut Vfs,
    project_id: Uuid,
    skill_asset_keys: &[String],
) -> bool {
    if !has_lifecycle_mount(vfs) {
        return false;
    }
    append_lifecycle_skill_asset_projection(vfs, project_id, skill_asset_keys)
}

pub(crate) async fn project_companion_system_skill_to_activation(
    repos: &RepositorySet,
    project_id: Uuid,
    activation: &mut ActivityActivation,
) -> Result<(), ApplicationError> {
    ensure_companion_system_skill_asset(repos, project_id).await?;
    let mut skill_asset_keys = Vec::new();
    append_companion_system_skill_key(&mut skill_asset_keys);
    append_lifecycle_companion_system_projection(
        &mut activation.lifecycle_vfs,
        project_id,
        &skill_asset_keys,
    );
    if let Some(mount) = activation
        .lifecycle_vfs
        .mounts
        .iter()
        .find(|mount| mount.id == "lifecycle" && mount.provider == PROVIDER_LIFECYCLE_VFS)
        .cloned()
    {
        activation.lifecycle_mount = mount;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::{Mount, MountCapability};

    fn lifecycle_vfs() -> Vfs {
        Vfs {
            mounts: vec![Mount {
                id: "lifecycle".to_string(),
                provider: PROVIDER_LIFECYCLE_VFS.to_string(),
                backend_id: String::new(),
                root_ref: "lifecycle://run/test".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "Lifecycle".to_string(),
                metadata: serde_json::json!({ "run_id": Uuid::new_v4().to_string() }),
            }],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    #[test]
    fn companion_system_key_is_appended_once() {
        let mut keys = vec!["writer".to_string()];
        append_companion_system_skill_key(&mut keys);
        append_companion_system_skill_key(&mut keys);

        assert_eq!(keys, vec!["writer", COMPANION_SYSTEM_SKILL_NAME]);
    }

    #[test]
    fn lifecycle_projection_writes_companion_system_to_mount_metadata() {
        let mut vfs = lifecycle_vfs();
        let project_id = Uuid::new_v4();
        let keys = vec![COMPANION_SYSTEM_SKILL_NAME.to_string()];

        assert!(append_lifecycle_companion_system_projection(
            &mut vfs, project_id, &keys,
        ));

        let lifecycle = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == "lifecycle")
            .expect("lifecycle mount");
        assert_eq!(
            lifecycle
                .metadata
                .get("skill_asset_project_id")
                .and_then(serde_json::Value::as_str),
            Some(project_id.to_string().as_str())
        );
        assert_eq!(
            lifecycle
                .metadata
                .get("skill_asset_keys")
                .and_then(serde_json::Value::as_array)
                .and_then(|items| items.first())
                .and_then(serde_json::Value::as_str),
            Some(COMPANION_SYSTEM_SKILL_NAME)
        );
    }
}
