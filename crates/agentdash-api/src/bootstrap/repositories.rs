use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;

use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::skill_asset::SkillAssetService;
use agentdash_application_shared_library::{
    BuiltinLibrarySeedProviderInput, IntegrationEmbeddedLibraryAssetSeed,
    SeedBuiltinLibraryAssetsInput, SharedLibraryService,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_infrastructure::{
    FilesystemExtensionPackageArtifactStorage, PostgresAgentFrameRepository,
    PostgresAgentLineageRepository, PostgresAgentRunLineageRepository,
    PostgresAuthSessionRepository, PostgresBackendExecutionLeaseRepository,
    PostgresBackendRepository, PostgresCanvasRepository, PostgresCanvasRuntimeStateRepository,
    PostgresExtensionPackageArtifactRepository, PostgresInlineFileRepository,
    PostgresLifecycleAgentRepository, PostgresLifecycleGateRepository,
    PostgresLifecycleSubjectAssociationRepository, PostgresLlmProviderCredentialRepository,
    PostgresLlmProviderRepository, PostgresMcpPresetRepository, PostgresProjectAgentRepository,
    PostgresProjectBackendAccessRepository, PostgresProjectExtensionInstallationRepository,
    PostgresProjectRepository, PostgresProjectVfsMountRepository,
    PostgresRoutineExecutionRepository, PostgresRoutineRepository,
    PostgresRunnerRegistrationTokenRepository, PostgresRuntimeHealthRepository,
    PostgresSettingsRepository, PostgresSharedLibraryRepository, PostgresSkillAssetRepository,
    PostgresStateChangeRepository, PostgresStoryRepository, PostgresUserDirectoryRepository,
    PostgresWorkflowRepository, PostgresWorkspaceRepository,
};
use agentdash_platform_spi::extension_package::ExtensionPackageArtifactStorage;

pub(crate) struct RepositoryBootstrapOutput {
    pub repos: RepositorySet,
    pub auth_session_service: Arc<AuthSessionService>,
    pub extension_package_artifact_storage: Arc<dyn ExtensionPackageArtifactStorage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectBuiltinSkillProvisioningSummary {
    projects: usize,
    assets: usize,
}

async fn reconcile_project_builtin_skill_assets(
    project_repo: &dyn ProjectRepository,
    skill_asset_repo: &dyn SkillAssetRepository,
) -> Result<ProjectBuiltinSkillProvisioningSummary> {
    let projects = project_repo.list_all().await.map_err(|error| {
        anyhow::anyhow!("读取 Project builtin Skill provisioning 范围失败: {error}")
    })?;
    let service = SkillAssetService::new(skill_asset_repo);
    let mut provisioned = 0usize;
    for project in &projects {
        let assets = service
            .provision_project_builtins(project.id, None)
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "Project {} builtin Skill provisioning 失败: {error}",
                    project.id
                )
            })?;
        provisioned += assets.len();
    }
    Ok(ProjectBuiltinSkillProvisioningSummary {
        projects: projects.len(),
        assets: provisioned,
    })
}

pub(crate) async fn build_repositories(
    pool: PgPool,
    integration_library_asset_seeds: Vec<IntegrationEmbeddedLibraryAssetSeed>,
) -> Result<RepositoryBootstrapOutput> {
    agentdash_infrastructure::migration::assert_postgres_schema_ready(&pool)
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    let project_repo = Arc::new(PostgresProjectRepository::new(pool.clone()));
    let canvas_repo = Arc::new(PostgresCanvasRepository::new(pool.clone()));
    let canvas_runtime_state_repo =
        Arc::new(PostgresCanvasRuntimeStateRepository::new(pool.clone()));
    let workspace_repo = Arc::new(PostgresWorkspaceRepository::new(pool.clone()));
    let story_repo = Arc::new(PostgresStoryRepository::new(pool.clone()));
    let state_change_repo = Arc::new(PostgresStateChangeRepository::new(pool.clone()));
    let backend_repo = Arc::new(PostgresBackendRepository::new(pool.clone()));
    let runtime_health_repo = Arc::new(PostgresRuntimeHealthRepository::new(pool.clone()));
    let backend_execution_lease_repo =
        Arc::new(PostgresBackendExecutionLeaseRepository::new(pool.clone()));
    let project_backend_access_repo =
        Arc::new(PostgresProjectBackendAccessRepository::new(pool.clone()));
    let runner_registration_token_repo =
        Arc::new(PostgresRunnerRegistrationTokenRepository::new(pool.clone()));
    let auth_session_repo = Arc::new(PostgresAuthSessionRepository::new(pool.clone()));
    let auth_session_service = Arc::new(AuthSessionService::new(auth_session_repo.clone()));
    let user_directory_repo = Arc::new(PostgresUserDirectoryRepository::new(pool.clone()));
    let settings_repo = Arc::new(PostgresSettingsRepository::new(pool.clone()));
    let shared_library_repo = Arc::new(PostgresSharedLibraryRepository::new(pool.clone()));
    let extension_package_artifact_repo = Arc::new(
        PostgresExtensionPackageArtifactRepository::new(pool.clone()),
    );
    let project_extension_installation_repo = Arc::new(
        PostgresProjectExtensionInstallationRepository::new(pool.clone()),
    );
    let llm_provider_repo = Arc::new(PostgresLlmProviderRepository::new(pool.clone()));
    let llm_provider_credential_repo =
        Arc::new(PostgresLlmProviderCredentialRepository::new(pool.clone()));
    let mcp_preset_repo = Arc::new(PostgresMcpPresetRepository::new(pool.clone()));
    let skill_asset_repo = Arc::new(PostgresSkillAssetRepository::new(pool.clone()));
    let project_agent_repo = Arc::new(PostgresProjectAgentRepository::new(pool.clone()));
    let project_vfs_mount_repo = Arc::new(PostgresProjectVfsMountRepository::new(pool.clone()));
    let workflow_repo = Arc::new(PostgresWorkflowRepository::new(pool.clone()));
    let lifecycle_agent_repo = Arc::new(PostgresLifecycleAgentRepository::new(pool.clone()));
    let agent_frame_repo = Arc::new(PostgresAgentFrameRepository::new(pool.clone()));
    let lifecycle_subject_association_repo = Arc::new(
        PostgresLifecycleSubjectAssociationRepository::new(pool.clone()),
    );
    let lifecycle_gate_repo = Arc::new(PostgresLifecycleGateRepository::new(pool.clone()));
    let agent_lineage_repo = Arc::new(PostgresAgentLineageRepository::new(pool.clone()));
    let agent_run_lineage_repo = Arc::new(PostgresAgentRunLineageRepository::new(pool.clone()));
    let routine_repo = Arc::new(PostgresRoutineRepository::new(pool.clone()));
    let routine_execution_repo = Arc::new(PostgresRoutineExecutionRepository::new(pool.clone()));
    let inline_file_repo = Arc::new(PostgresInlineFileRepository::new(pool));

    {
        let service = SharedLibraryService::new(shared_library_repo.as_ref());
        let seeded = service
            .seed_builtin_assets(SeedBuiltinLibraryAssetsInput {
                asset_type: None,
                key: None,
                seed_provider: builtin_seed_provider_input()?,
            })
            .await
            .map_err(|error| {
                anyhow::anyhow!("builtin Shared Library assets 初始化失败: {error}")
            })?;
        diag!(
            Info,
            Subsystem::Api,
            seeded = seeded.len(),
            "已同步 builtin Shared Library assets"
        );
        if !integration_library_asset_seeds.is_empty() {
            let declared = integration_library_asset_seeds.len();
            let seeded = service
                .seed_integration_embedded_assets(integration_library_asset_seeds)
                .await
                .map_err(|error| {
                    anyhow::anyhow!(
                        "integration embedded Shared Library assets 初始化失败: {error}"
                    )
                })?;
            diag!(
                Info,
                Subsystem::Api,
                declared,
                seeded = seeded.len(),
                "已同步 integration embedded Shared Library assets"
            );
        }
    }

    let skill_summary =
        reconcile_project_builtin_skill_assets(project_repo.as_ref(), skill_asset_repo.as_ref())
            .await?;
    diag!(
        Info,
        Subsystem::Api,
        projects = skill_summary.projects,
        assets = skill_summary.assets,
        "已同步 Project builtin Skill assets"
    );

    let repos = RepositorySet {
        project_repo,
        canvas_repo,
        canvas_runtime_state_repo,
        workspace_repo,
        story_repo,
        state_change_repo,
        backend_repo,
        runtime_health_repo,
        backend_execution_lease_repo,
        project_backend_access_repo: project_backend_access_repo.clone(),
        backend_workspace_inventory_repo: project_backend_access_repo,
        runner_registration_token_repo,
        auth_session_repo,
        user_directory_repo,
        settings_repo,
        shared_library_repo,
        extension_package_artifact_repo,
        project_extension_installation_repo,
        llm_provider_repo,
        llm_provider_credential_repo,
        mcp_preset_repo,
        skill_asset_repo,
        project_agent_repo,
        project_vfs_mount_repo,
        agent_procedure_repo: workflow_repo.clone(),
        workflow_template_install_repo: workflow_repo.clone(),
        workflow_graph_repo: workflow_repo.clone(),
        lifecycle_run_repo: workflow_repo,
        lifecycle_agent_repo,
        agent_frame_repo,
        lifecycle_subject_association_repo,
        lifecycle_gate_repo: lifecycle_gate_repo.clone(),
        gate_result_delivery_marker_repo: lifecycle_gate_repo,
        agent_lineage_repo,
        agent_run_lineage_repo,
        routine_repo,
        routine_execution_repo,
        inline_file_repo,
    };

    Ok(RepositoryBootstrapOutput {
        repos,
        auth_session_service,
        extension_package_artifact_storage: Arc::new(
            FilesystemExtensionPackageArtifactStorage::default(),
        ),
    })
}

fn builtin_seed_provider_input() -> Result<BuiltinLibrarySeedProviderInput> {
    Ok(BuiltinLibrarySeedProviderInput {
        workflow_templates: agentdash_application_workflow::list_builtin_workflow_templates()
            .map_err(|error| anyhow::anyhow!("{error}"))?,
    })
}
