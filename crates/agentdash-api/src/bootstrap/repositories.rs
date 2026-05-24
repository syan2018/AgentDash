use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;

use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::session::SessionPersistence;
use agentdash_application::shared_library::{PluginEmbeddedLibraryAssetSeed, SharedLibraryService};
use agentdash_infrastructure::{
    PostgresAuthSessionRepository, PostgresBackendRepository, PostgresCanvasRepository,
    PostgresInlineFileRepository, PostgresLlmProviderRepository, PostgresMcpPresetRepository,
    PostgresProjectAgentRepository, PostgresProjectBackendAccessRepository,
    PostgresProjectExtensionInstallationRepository, PostgresProjectRepository,
    PostgresProjectVfsMountRepository, PostgresRoutineExecutionRepository,
    PostgresRoutineRepository, PostgresRuntimeHealthRepository, PostgresSessionBindingRepository,
    PostgresSessionRepository, PostgresSettingsRepository, PostgresSharedLibraryRepository,
    PostgresSkillAssetRepository, PostgresStateChangeRepository, PostgresStoryRepository,
    PostgresUserDirectoryRepository, PostgresWorkflowRepository, PostgresWorkspaceRepository,
};

pub(crate) struct RepositoryBootstrapOutput {
    pub repos: RepositorySet,
    pub session_persistence: Arc<dyn SessionPersistence>,
    pub auth_session_service: Arc<AuthSessionService>,
}

pub(crate) async fn build_repositories(
    pool: PgPool,
    plugin_library_asset_seeds: Vec<PluginEmbeddedLibraryAssetSeed>,
) -> Result<RepositoryBootstrapOutput> {
    agentdash_infrastructure::migration::assert_postgres_schema_ready(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let project_repo = Arc::new(PostgresProjectRepository::new(pool.clone()));

    let canvas_repo = Arc::new(PostgresCanvasRepository::new(pool.clone()));

    let workspace_repo = Arc::new(PostgresWorkspaceRepository::new(pool.clone()));

    let story_repo = Arc::new(PostgresStoryRepository::new(pool.clone()));
    let state_change_repo = Arc::new(PostgresStateChangeRepository::new(pool.clone()));

    let session_binding_repo = Arc::new(PostgresSessionBindingRepository::new(pool.clone()));
    let session_repo = Arc::new(PostgresSessionRepository::new(pool.clone()));

    let backend_repo = Arc::new(PostgresBackendRepository::new(pool.clone()));
    let runtime_health_repo = Arc::new(PostgresRuntimeHealthRepository::new(pool.clone()));
    let project_backend_access_repo =
        Arc::new(PostgresProjectBackendAccessRepository::new(pool.clone()));

    let user_directory_repo = Arc::new(PostgresUserDirectoryRepository::new(pool.clone()));

    let settings_repo = Arc::new(PostgresSettingsRepository::new(pool.clone()));

    let shared_library_repo = Arc::new(PostgresSharedLibraryRepository::new(pool.clone()));
    {
        let service = SharedLibraryService::new(shared_library_repo.as_ref());
        let seeded = service
            .seed_builtin_assets(Default::default())
            .await
            .map_err(|e| anyhow::anyhow!("builtin Shared Library assets 初始化失败: {e}"))?;
        tracing::info!(
            seeded = seeded.len(),
            "已同步 builtin Shared Library assets"
        );
    }

    let project_extension_installation_repo = Arc::new(
        PostgresProjectExtensionInstallationRepository::new(pool.clone()),
    );

    let project_agent_repo = Arc::new(PostgresProjectAgentRepository::new(pool.clone()));

    let project_vfs_mount_repo = Arc::new(PostgresProjectVfsMountRepository::new(pool.clone()));

    let routine_repo = Arc::new(PostgresRoutineRepository::new(pool.clone()));
    let routine_execution_repo = Arc::new(PostgresRoutineExecutionRepository::new(pool.clone()));

    let llm_provider_repo = Arc::new(PostgresLlmProviderRepository::new(pool.clone()));

    let auth_session_repo = Arc::new(PostgresAuthSessionRepository::new(pool.clone()));
    let auth_session_service = Arc::new(AuthSessionService::new(auth_session_repo.clone()));

    let workflow_repo = Arc::new(PostgresWorkflowRepository::new(pool.clone()));

    let mcp_preset_repo = Arc::new(PostgresMcpPresetRepository::new(pool.clone()));

    let skill_asset_repo = Arc::new(PostgresSkillAssetRepository::new(pool.clone()));

    let inline_file_repo = Arc::new(PostgresInlineFileRepository::new(pool));

    let repos = RepositorySet {
        project_repo: project_repo.clone(),
        canvas_repo: canvas_repo.clone(),
        workspace_repo: workspace_repo.clone(),
        story_repo: story_repo.clone(),
        state_change_repo: state_change_repo.clone(),
        session_binding_repo: session_binding_repo.clone(),
        backend_repo: backend_repo.clone(),
        runtime_health_repo: runtime_health_repo.clone(),
        project_backend_access_repo: project_backend_access_repo.clone(),
        backend_workspace_inventory_repo: project_backend_access_repo.clone(),
        auth_session_repo: auth_session_repo.clone(),
        user_directory_repo: user_directory_repo.clone(),
        settings_repo: settings_repo.clone(),
        shared_library_repo: shared_library_repo.clone(),
        project_extension_installation_repo: project_extension_installation_repo.clone(),
        llm_provider_repo: llm_provider_repo.clone(),
        mcp_preset_repo: mcp_preset_repo.clone(),
        skill_asset_repo: skill_asset_repo.clone(),
        project_agent_repo: project_agent_repo.clone(),
        project_vfs_mount_repo: project_vfs_mount_repo.clone(),
        workflow_definition_repo: workflow_repo.clone(),
        workflow_template_install_repo: workflow_repo.clone(),
        lifecycle_definition_repo: workflow_repo.clone(),
        activity_lifecycle_definition_repo: workflow_repo.clone(),
        activity_execution_claim_repo: workflow_repo.clone(),
        lifecycle_run_repo: workflow_repo.clone(),
        routine_repo: routine_repo.clone(),
        routine_execution_repo: routine_execution_repo.clone(),
        inline_file_repo: inline_file_repo.clone(),
    };

    let plugin_asset_count = plugin_library_asset_seeds.len();
    if plugin_asset_count > 0 {
        let service = SharedLibraryService::new(shared_library_repo.as_ref());
        let seeded = service
            .seed_plugin_embedded_assets(plugin_library_asset_seeds)
            .await
            .map_err(|e| anyhow::anyhow!("plugin embedded library assets 初始化失败: {e}"))?;
        tracing::info!(
            declared = plugin_asset_count,
            seeded = seeded.len(),
            "已同步 plugin embedded Shared Library assets"
        );
    }

    Ok(RepositoryBootstrapOutput {
        repos,
        session_persistence: session_repo,
        auth_session_service,
    })
}
