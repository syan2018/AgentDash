use agentdash_domain::workspace_module::WORKSPACE_MODULE_SYSTEM_SKILL_NAME;
use agentdash_spi::Vfs;
use uuid::Uuid;

use crate::ApplicationError;
use crate::repository_set::RepositorySet;
use crate::skill_asset::SkillAssetService;
use crate::vfs::{PROVIDER_LIFECYCLE_VFS, append_lifecycle_skill_asset_projection};

pub(crate) async fn ensure_workspace_module_system_skill_asset(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<(), ApplicationError> {
    SkillAssetService::new(repos.skill_asset_repo.as_ref())
        .bootstrap_builtins(project_id, Some(WORKSPACE_MODULE_SYSTEM_SKILL_NAME))
        .await
        .map(|_| ())
        .map_err(|error| ApplicationError::Internal(error.to_string()))
}

pub(crate) fn append_workspace_module_system_skill_key(keys: &mut Vec<String>) {
    if keys
        .iter()
        .any(|key| key.trim() == WORKSPACE_MODULE_SYSTEM_SKILL_NAME)
    {
        return;
    }
    keys.push(WORKSPACE_MODULE_SYSTEM_SKILL_NAME.to_string());
}

pub(crate) async fn project_workspace_module_system_skill_to_vfs(
    repos: &RepositorySet,
    project_id: Uuid,
    vfs: &mut Option<Vfs>,
) -> Result<bool, ApplicationError> {
    let Some(space) = vfs.as_mut() else {
        return Ok(false);
    };
    if !space
        .mounts
        .iter()
        .any(|mount| mount.id == "lifecycle" && mount.provider == PROVIDER_LIFECYCLE_VFS)
    {
        return Ok(false);
    }
    ensure_workspace_module_system_skill_asset(repos, project_id).await?;
    let mut skill_asset_keys = Vec::new();
    append_workspace_module_system_skill_key(&mut skill_asset_keys);
    Ok(append_lifecycle_skill_asset_projection(
        space,
        project_id,
        &skill_asset_keys,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_module_system_key_is_appended_once() {
        let mut keys = vec!["writer".to_string()];
        append_workspace_module_system_skill_key(&mut keys);
        append_workspace_module_system_skill_key(&mut keys);

        assert_eq!(keys, vec!["writer", WORKSPACE_MODULE_SYSTEM_SKILL_NAME]);
    }
}
