pub use agentdash_application_skill::skill_asset::*;

use agentdash_application_shared_library::{
    InstallLibraryAssetInput, InstallLibraryAssetOutput, SharedLibraryRepositorySet,
    install_library_asset_to_project,
};
use agentdash_application_skill::skill_asset::{
    ImportRemoteSkillAssetInput as RemoteImportInput,
    SkillAssetApplicationError as SkillAssetError,
    map_shared_library_domain_error as map_skill_asset_domain_error,
    prepare_remote_skill_import as prepare_remote_skill_import_asset,
};
use agentdash_domain::shared_library::LibraryAssetRepository;
use agentdash_domain::skill_asset::SkillAsset;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_spi::RemoteSkillSource;

pub struct ImportRemoteSkillUrlDeps<'a> {
    pub skill_asset_repo: &'a dyn SkillAssetRepository,
    pub shared_library_repo: &'a dyn LibraryAssetRepository,
    pub shared_library_repos: &'a SharedLibraryRepositorySet,
}

pub async fn import_remote_skill_url_to_project(
    deps: ImportRemoteSkillUrlDeps<'_>,
    input: RemoteImportInput,
    source: &dyn RemoteSkillSource,
) -> Result<SkillAsset, SkillAssetError> {
    let library_asset = prepare_remote_skill_import_asset(
        deps.skill_asset_repo,
        deps.shared_library_repo,
        &input,
        source,
    )
    .await?;

    let output = install_library_asset_to_project(
        deps.shared_library_repos,
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
    deps.skill_asset_repo
        .get(id)
        .await
        .map_err(map_skill_asset_domain_error)?
        .ok_or_else(|| SkillAssetError::NotFound(format!("skill_asset 不存在: {id}")))
}
