use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;

use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::repository_set::{
    LifecycleProjectAgentLaunchAdapter, LifecycleProjectAgentLaunchDeps, RepositorySet,
};
use agentdash_application_agentrun::agent_run::frame::{
    AgentRunLaunchAnchorFrameConstructionAdapter, AgentRunWorkflowNodeFrameMaterializationAdapter,
};
use agentdash_application_lifecycle::{
    AgentRunLifecycleSurfaceProjector, SessionMetaStoreRuntimeSessionCreator,
};
use agentdash_application_ports::project_projection_notification::ProjectProjectionNotificationPort;
use agentdash_application_runtime_session::session::SessionStoreSet;
use agentdash_application_shared_library::{
    BuiltinLibrarySeedProviderInput, IntegrationEmbeddedLibraryAssetSeed,
    SeedBuiltinLibraryAssetsInput, SharedLibraryService,
};
use agentdash_infrastructure::{
    FilesystemExtensionPackageArtifactStorage, PostgresAgentFrameRepository,
    PostgresAgentLineageRepository, PostgresAgentRunCommandReceiptRepository,
    PostgresAgentRunDeliveryBindingRepository, PostgresAgentRunForkMaterialization,
    PostgresAgentRunLineageRepository, PostgresAgentRunMailboxRepository,
    PostgresAuthSessionRepository, PostgresBackendExecutionLeaseRepository,
    PostgresBackendRepository, PostgresCanvasRepository, PostgresCanvasRuntimeStateRepository,
    PostgresExtensionPackageArtifactRepository, PostgresInlineFileRepository,
    PostgresInteractionRepository, PostgresLifecycleAgentRepository,
    PostgresLifecycleGateRepository, PostgresLifecycleSubjectAssociationRepository,
    PostgresLlmProviderCredentialRepository, PostgresLlmProviderRepository,
    PostgresManualContextCompactionRequestRepository, PostgresMcpPresetRepository,
    PostgresProjectAgentRepository, PostgresProjectBackendAccessRepository,
    PostgresProjectExtensionInstallationRepository, PostgresProjectRepository,
    PostgresProjectVfsMountRepository, PostgresRoutineExecutionRepository,
    PostgresRoutineRepository, PostgresRunnerRegistrationTokenRepository,
    PostgresRuntimeHealthRepository, PostgresSessionRepository, PostgresSettingsRepository,
    PostgresSharedLibraryRepository, PostgresSkillAssetRepository, PostgresStateChangeRepository,
    PostgresStoryRepository, PostgresUserDirectoryRepository, PostgresWorkflowRepository,
    PostgresWorkspaceRepository,
};
use agentdash_spi::extension_package::ExtensionPackageArtifactStorage;

pub(crate) struct RepositoryBootstrapOutput {
    pub repos: RepositorySet,
    pub session_stores: SessionStoreSet,
    pub auth_session_service: Arc<AuthSessionService>,
    pub extension_package_artifact_storage: Arc<dyn ExtensionPackageArtifactStorage>,
}

pub(crate) async fn build_repositories(
    pool: PgPool,
    integration_library_asset_seeds: Vec<IntegrationEmbeddedLibraryAssetSeed>,
    project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
) -> Result<RepositoryBootstrapOutput> {
    agentdash_infrastructure::migration::assert_postgres_schema_ready(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let project_repo = Arc::new(PostgresProjectRepository::new(pool.clone()));

    let canvas_repo = Arc::new(PostgresCanvasRepository::new(pool.clone()));
    let canvas_runtime_state_repo =
        Arc::new(PostgresCanvasRuntimeStateRepository::new(pool.clone()));
    let interaction_repo = Arc::new(PostgresInteractionRepository::new(pool.clone()));

    let workspace_repo = Arc::new(PostgresWorkspaceRepository::new(pool.clone()));

    let story_repo = Arc::new(PostgresStoryRepository::new(pool.clone()));
    let state_change_repo = Arc::new(PostgresStateChangeRepository::new(pool.clone()));

    let session_repo = Arc::new(PostgresSessionRepository::new(pool.clone()));
    let session_stores = SessionStoreSet::new(
        session_repo.clone(),
        session_repo.clone(),
        session_repo.clone(),
        session_repo.clone(),
        session_repo.clone(),
        session_repo.clone(),
        session_repo.clone(),
    );
    let runtime_session_creator = Arc::new(SessionMetaStoreRuntimeSessionCreator::new(
        session_stores.meta.clone(),
    ));

    let backend_repo = Arc::new(PostgresBackendRepository::new(pool.clone()));
    let runtime_health_repo = Arc::new(PostgresRuntimeHealthRepository::new(pool.clone()));
    let backend_execution_lease_repo =
        Arc::new(PostgresBackendExecutionLeaseRepository::new(pool.clone()));
    let project_backend_access_repo =
        Arc::new(PostgresProjectBackendAccessRepository::new(pool.clone()));
    let runner_registration_token_repo =
        Arc::new(PostgresRunnerRegistrationTokenRepository::new(pool.clone()));

    let user_directory_repo = Arc::new(PostgresUserDirectoryRepository::new(pool.clone()));

    let settings_repo = Arc::new(PostgresSettingsRepository::new(pool.clone()));

    let shared_library_repo = Arc::new(PostgresSharedLibraryRepository::new(pool.clone()));
    {
        let service = SharedLibraryService::new(shared_library_repo.as_ref());
        let seeded = service
            .seed_builtin_assets(SeedBuiltinLibraryAssetsInput {
                asset_type: None,
                key: None,
                seed_provider: builtin_seed_provider_input()?,
            })
            .await
            .map_err(|e| anyhow::anyhow!("builtin Shared Library assets 初始化失败: {e}"))?;
        diag!(
            Info,
            Subsystem::Api,
            seeded = seeded.len(),
            "已同步 builtin Shared Library assets"
        );
    }

    let project_extension_installation_repo = Arc::new(
        PostgresProjectExtensionInstallationRepository::new(pool.clone()),
    );
    let extension_package_artifact_repo = Arc::new(
        PostgresExtensionPackageArtifactRepository::new(pool.clone()),
    );

    let project_agent_repo = Arc::new(PostgresProjectAgentRepository::new(pool.clone()));

    let project_vfs_mount_repo = Arc::new(PostgresProjectVfsMountRepository::new(pool.clone()));

    let routine_repo = Arc::new(PostgresRoutineRepository::new(pool.clone()));
    let routine_execution_repo = Arc::new(PostgresRoutineExecutionRepository::new(pool.clone()));

    let llm_provider_repo = Arc::new(PostgresLlmProviderRepository::new(pool.clone()));
    let llm_provider_credential_repo =
        Arc::new(PostgresLlmProviderCredentialRepository::new(pool.clone()));

    let auth_session_repo = Arc::new(PostgresAuthSessionRepository::new(pool.clone()));
    let auth_session_service = Arc::new(AuthSessionService::new(auth_session_repo.clone()));

    let workflow_repo = Arc::new(PostgresWorkflowRepository::new(pool.clone()));

    let mcp_preset_repo = Arc::new(PostgresMcpPresetRepository::new(pool.clone()));

    let skill_asset_repo = Arc::new(PostgresSkillAssetRepository::new(pool.clone()));

    let inline_file_repo = Arc::new(PostgresInlineFileRepository::new(pool.clone()));
    let lifecycle_agent_repo = Arc::new(PostgresLifecycleAgentRepository::new(pool.clone()));
    let agent_frame_repo = Arc::new(PostgresAgentFrameRepository::new(pool.clone()));
    let lifecycle_subject_association_repo = Arc::new(
        PostgresLifecycleSubjectAssociationRepository::new(pool.clone()),
    );
    let lifecycle_gate_repo = Arc::new(PostgresLifecycleGateRepository::new(pool.clone()));
    let agent_lineage_repo = Arc::new(PostgresAgentLineageRepository::new(pool.clone()));
    let agent_run_lineage_repo = Arc::new(PostgresAgentRunLineageRepository::new(pool.clone()));
    let execution_anchor_repo = Arc::new(
        agentdash_infrastructure::PostgresRuntimeSessionExecutionAnchorRepository::new(
            pool.clone(),
        ),
    );
    let agent_run_delivery_binding_repo =
        Arc::new(PostgresAgentRunDeliveryBindingRepository::new(pool.clone()));
    let agent_run_command_receipt_repo =
        Arc::new(PostgresAgentRunCommandReceiptRepository::new(pool.clone()));
    let manual_context_compaction_request_repo = Arc::new(
        PostgresManualContextCompactionRequestRepository::new(pool.clone()),
    );
    let agent_run_mailbox_repo = Arc::new(PostgresAgentRunMailboxRepository::new(pool.clone()));
    let agent_frame_construction = Arc::new(AgentRunLaunchAnchorFrameConstructionAdapter::new(
        agent_frame_repo.clone(),
    ));
    let agent_run_fork_materialization =
        Arc::new(PostgresAgentRunForkMaterialization::new(pool.clone()));
    let lifecycle_surface_projection = Arc::new(
        AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(skill_asset_repo.clone()),
    );
    let workflow_agent_frame_materialization =
        Arc::new(AgentRunWorkflowNodeFrameMaterializationAdapter::new(
            agent_frame_repo.clone(),
            lifecycle_surface_projection,
        ));
    let project_agent_lifecycle_launch = Arc::new(LifecycleProjectAgentLaunchAdapter::new(
        LifecycleProjectAgentLaunchDeps {
            run_repo: workflow_repo.clone(),
            workflow_graph_repo: workflow_repo.clone(),
            agent_repo: lifecycle_agent_repo.clone(),
            frame_repo: agent_frame_repo.clone(),
            association_repo: lifecycle_subject_association_repo.clone(),
            gate_repo: lifecycle_gate_repo.clone(),
            lineage_repo: agent_lineage_repo.clone(),
            anchor_repo: execution_anchor_repo.clone(),
            delivery_binding_repo: agent_run_delivery_binding_repo.clone(),
            runtime_session_creator: runtime_session_creator.clone(),
            frame_construction: agent_frame_construction.clone(),
        },
    ));

    let permission_grant_repo =
        Arc::new(agentdash_infrastructure::PostgresPermissionGrantRepository::new(pool));

    let repos = RepositorySet {
        project_repo: project_repo.clone(),
        canvas_repo: canvas_repo.clone(),
        canvas_runtime_state_repo: canvas_runtime_state_repo.clone(),
        interaction_definition_repo: interaction_repo.clone(),
        interaction_instance_repo: interaction_repo,
        workspace_repo: workspace_repo.clone(),
        story_repo: story_repo.clone(),
        state_change_repo: state_change_repo.clone(),
        backend_repo: backend_repo.clone(),
        runtime_health_repo: runtime_health_repo.clone(),
        backend_execution_lease_repo: backend_execution_lease_repo.clone(),
        project_backend_access_repo: project_backend_access_repo.clone(),
        backend_workspace_inventory_repo: project_backend_access_repo.clone(),
        runner_registration_token_repo: runner_registration_token_repo.clone(),
        auth_session_repo: auth_session_repo.clone(),
        user_directory_repo: user_directory_repo.clone(),
        settings_repo: settings_repo.clone(),
        shared_library_repo: shared_library_repo.clone(),
        extension_package_artifact_repo: extension_package_artifact_repo.clone(),
        project_extension_installation_repo: project_extension_installation_repo.clone(),
        llm_provider_repo: llm_provider_repo.clone(),
        llm_provider_credential_repo: llm_provider_credential_repo.clone(),
        mcp_preset_repo: mcp_preset_repo.clone(),
        skill_asset_repo: skill_asset_repo.clone(),
        project_agent_repo: project_agent_repo.clone(),
        project_vfs_mount_repo: project_vfs_mount_repo.clone(),
        agent_procedure_repo: workflow_repo.clone(),
        workflow_template_install_repo: workflow_repo.clone(),
        workflow_graph_repo: workflow_repo.clone(),
        lifecycle_run_repo: workflow_repo.clone(),
        lifecycle_agent_repo: lifecycle_agent_repo.clone(),
        agent_frame_repo: agent_frame_repo.clone(),
        lifecycle_subject_association_repo: lifecycle_subject_association_repo.clone(),
        lifecycle_gate_repo: lifecycle_gate_repo.clone(),
        gate_result_delivery_marker_repo: lifecycle_gate_repo.clone(),
        agent_lineage_repo: agent_lineage_repo.clone(),
        agent_run_lineage_repo: agent_run_lineage_repo.clone(),
        execution_anchor_repo: execution_anchor_repo.clone(),
        agent_run_delivery_binding_repo: agent_run_delivery_binding_repo.clone(),
        agent_run_command_receipt_repo: agent_run_command_receipt_repo.clone(),
        manual_context_compaction_request_repo: manual_context_compaction_request_repo.clone(),
        agent_run_mailbox_repo: agent_run_mailbox_repo.clone(),
        runtime_session_creator: runtime_session_creator.clone(),
        agent_frame_construction,
        agent_run_fork_materialization,
        workflow_agent_frame_materialization,
        project_agent_lifecycle_launch,
        routine_repo: routine_repo.clone(),
        routine_execution_repo: routine_execution_repo.clone(),
        inline_file_repo: inline_file_repo.clone(),
        permission_grant_repo: permission_grant_repo.clone(),
        project_projection_notifications: project_projection_notifications.clone(),
    };

    let integration_asset_count = integration_library_asset_seeds.len();
    if integration_asset_count > 0 {
        let service = SharedLibraryService::new(shared_library_repo.as_ref());
        let seeded = service
            .seed_integration_embedded_assets(integration_library_asset_seeds)
            .await
            .map_err(|e| anyhow::anyhow!("integration embedded library assets 初始化失败: {e}"))?;
        diag!(
            Info,
            Subsystem::Api,
            declared = integration_asset_count,
            seeded = seeded.len(),
            "已同步 integration embedded Shared Library assets"
        );
    }

    Ok(RepositoryBootstrapOutput {
        repos,
        session_stores,
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
