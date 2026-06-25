pub use agentdash_application_skill::skill_asset::*;

use agentdash_application_skill::skill_asset::{
    ImportRemoteSkillAssetInput as RemoteImportInput,
    SkillAssetApplicationError as SkillAssetError,
    map_shared_library_domain_error as map_skill_asset_domain_error,
    prepare_remote_skill_import as prepare_remote_skill_import_asset,
};
use agentdash_domain::skill_asset::SkillAsset;
use agentdash_spi::RemoteSkillSource;

use crate::repository_set::RepositorySet;
use agentdash_application_shared_library::{
    InstallLibraryAssetInput, InstallLibraryAssetOutput, install_library_asset_to_project,
};

pub async fn import_remote_skill_url_to_project(
    repos: &RepositorySet,
    input: RemoteImportInput,
    source: &dyn RemoteSkillSource,
) -> Result<SkillAsset, SkillAssetError> {
    let library_asset = prepare_remote_skill_import_asset(
        repos.skill_asset_repo.as_ref(),
        repos.shared_library_repo.as_ref(),
        &input,
        source,
    )
    .await?;

    let output = install_library_asset_to_project(
        &repos.to_shared_library_repository_set(),
        InstallLibraryAssetInput {
            project_id: input.project_id,
            library_asset_id: library_asset.id,
            target_key: None,
            overwrite: true,
            install_options: None,
        },
    )
    .await
    .map_err(map_skill_asset_domain_error)?;

    let InstallLibraryAssetOutput::SkillAsset { id } = output else {
        return Err(SkillAssetError::Internal(
            "skill_template 安装结果不是 SkillAsset".to_string(),
        ));
    };
    repos
        .skill_asset_repo
        .get(id)
        .await
        .map_err(map_skill_asset_domain_error)?
        .ok_or_else(|| SkillAssetError::NotFound(format!("skill_asset 不存在: {id}")))
}
