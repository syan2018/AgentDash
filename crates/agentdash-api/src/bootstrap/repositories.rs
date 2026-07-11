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
use agentdash_application_lifecycle::AgentRunLifecycleSurfaceProjector;
use agentdash_application_ports::agent_frame_materialization::{
    AgentRunFrameConstructionPort, SharedAgentRunFrameConstructionHandle,
};
use agentdash_application_ports::lifecycle_surface_projection::LifecycleSurfaceProjectionPort;
use agentdash_application_ports::project_projection_notification::ProjectProjectionNotificationPort;
use agentdash_application_shared_library::{
    BuiltinLibrarySeedProviderInput, IntegrationEmbeddedLibraryAssetSeed,
    SeedBuiltinLibraryAssetsInput, SharedLibraryService,
};
use agentdash_infrastructure::{
    FilesystemExtensionPackageArtifactStorage, PostgresAgentFrameRepository,
    PostgresAgentLineageRepository, PostgresAgentRunLineageRepository,
    PostgresAgentRunMailboxRepository, PostgresAgentRuntimeCompositionRepository,
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
use agentdash_spi::extension_package::ExtensionPackageArtifactStorage;

pub(crate) struct RepositoryBootstrapOutput {
    pub repos: RepositorySet,
    pub auth_session_service: Arc<AuthSessionService>,
    pub extension_package_artifact_storage: Arc<dyn ExtensionPackageArtifactStorage>,
    pub lifecycle_surface_projection: Arc<dyn LifecycleSurfaceProjectionPort>,
}

pub(crate) async fn build_repositories(
    pool: PgPool,
    integration_library_asset_seeds: Vec<IntegrationEmbeddedLibraryAssetSeed>,
    project_projection_notifications: Option<Arc<dyn ProjectProjectionNotificationPort>>,
    runtime_provisioner_handle: agentdash_application_ports::agent_run_runtime::SharedAgentRunRuntimeProvisionerHandle,
    frame_construction_handle: SharedAgentRunFrameConstructionHandle,
) -> Result<RepositoryBootstrapOutput> {
    agentdash_infrastructure::migration::assert_postgres_schema_ready(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

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
    let agent_run_runtime_binding_repo =
        Arc::new(PostgresAgentRuntimeCompositionRepository::new(pool.clone()));
    let agent_run_mailbox_repo = Arc::new(PostgresAgentRunMailboxRepository::new(pool.clone()));
    let agent_frame_construction: Arc<dyn AgentRunFrameConstructionPort> = Arc::new(
        AgentRunLaunchAnchorFrameConstructionAdapter::new(agent_frame_repo.clone()),
    );
    let project_agent_frame_construction: Arc<dyn AgentRunFrameConstructionPort> =
        Arc::new(frame_construction_handle);
    let lifecycle_surface_projection: Arc<dyn LifecycleSurfaceProjectionPort> = Arc::new(
        AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(skill_asset_repo.clone()),
    );
    let workflow_agent_frame_materialization =
        Arc::new(AgentRunWorkflowNodeFrameMaterializationAdapter::new(
            agent_frame_repo.clone(),
            lifecycle_surface_projection.clone(),
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
            frame_construction: project_agent_frame_construction,
        },
    ));

    let permission_grant_repo =
        Arc::new(agentdash_infrastructure::PostgresPermissionGrantRepository::new(pool));

    let repos = RepositorySet {
        project_repo: project_repo.clone(),
        canvas_repo: canvas_repo.clone(),
        canvas_runtime_state_repo: canvas_runtime_state_repo.clone(),
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
        agent_run_runtime_binding_repo: agent_run_runtime_binding_repo.clone(),
        agent_run_runtime_provisioner: Arc::new(runtime_provisioner_handle),
        agent_run_mailbox_repo: agent_run_mailbox_repo.clone(),
        agent_frame_construction,
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
        auth_session_service,
        extension_package_artifact_storage: Arc::new(
            FilesystemExtensionPackageArtifactStorage::default(),
        ),
        lifecycle_surface_projection,
    })
}

fn builtin_seed_provider_input() -> Result<BuiltinLibrarySeedProviderInput> {
    Ok(BuiltinLibrarySeedProviderInput {
        workflow_templates: agentdash_application_workflow::list_builtin_workflow_templates()
            .map_err(|error| anyhow::anyhow!("{error}"))?,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_application::context::{AuditFilter, ContextAuditBus, InMemoryContextAuditBus};
    use agentdash_application::frame_construction::{
        AgentRunProjectOwnerFrameConstructionAdapter, AgentRunProjectOwnerFrameConstructionDeps,
    };
    use agentdash_application::platform_config::PlatformConfig;
    use agentdash_application::repository_set::RepositorySet;
    use agentdash_application_agentrun::agent_run::frame::AgentFrameSurfaceExt;
    use agentdash_application_ports::agent_frame_materialization::SharedAgentRunFrameConstructionHandle;
    use agentdash_application_ports::agent_run_runtime::SharedAgentRunRuntimeProvisionerHandle;
    use agentdash_domain::agent::ProjectAgent;
    use agentdash_domain::backend::ProjectBackendAccess;
    use agentdash_domain::project::Project;
    use agentdash_domain::story::Story;
    use agentdash_domain::workflow::{
        AgentLaunchIntent, AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource,
        RunPolicy, RuntimePolicy, SubjectRef,
    };
    use agentdash_domain::workspace::{
        Workspace, WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind,
        WorkspaceResolutionPolicy, WorkspaceStatus, identity_payload_from_detected_facts,
    };
    use agentdash_infrastructure::postgres_runtime::PostgresRuntime;
    use agentdash_relay::CapabilitiesPayload;
    use agentdash_spi::AgentConfig;
    use chrono::Utc;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use super::build_repositories;
    use crate::relay::registry::{BackendRegistry, ConnectedBackend};

    async fn migrated_runtime(name: &str) -> PostgresRuntime {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/agent-runtime-frame-tests")
            .join(format!("{name}-{}", Uuid::new_v4().simple()));
        let runtime = PostgresRuntime::resolve_embedded_at_data_root(name, 8, root)
            .await
            .expect("embedded PostgreSQL");
        agentdash_infrastructure::migration::run_postgres_migrations(&runtime.pool)
            .await
            .expect("migrations");
        runtime
    }

    async fn register_backend(registry: &Arc<BackendRegistry>, backend_id: &str) {
        let (sender, _receiver) = mpsc::unbounded_channel();
        registry
            .try_register(ConnectedBackend {
                backend_id: backend_id.to_string(),
                name: "ARD-004 local backend".to_string(),
                version: "test".to_string(),
                capabilities: CapabilitiesPayload::default(),
                sender,
                connected_at: Utc::now(),
            })
            .await
            .expect("register backend");
    }

    async fn create_project_agent_fixture(
        repos: &RepositorySet,
        backend_id: Option<&str>,
    ) -> (Project, ProjectAgent) {
        let mut project = Project::new("ARD-004 project".to_string(), String::new());
        repos
            .project_repo
            .create(&project)
            .await
            .expect("create project");

        if let Some(backend_id) = backend_id {
            let root_ref = "D:/Projects/AgentDash";
            let mut workspace = Workspace::new(
                project.id,
                "ARD-004 workspace".to_string(),
                WorkspaceIdentityKind::LocalDir,
                identity_payload_from_detected_facts(
                    WorkspaceIdentityKind::LocalDir,
                    &serde_json::json!({}),
                    root_ref,
                )
                .expect("local workspace identity"),
                WorkspaceResolutionPolicy::PreferDefaultBinding,
            );
            let mut binding = WorkspaceBinding::new(
                workspace.id,
                backend_id.to_string(),
                root_ref.to_string(),
                serde_json::json!({ "source": "ard-004-test" }),
            );
            binding.status = WorkspaceBindingStatus::Ready;
            workspace.status = WorkspaceStatus::Active;
            workspace.set_bindings(vec![binding]);
            repos
                .workspace_repo
                .create(&workspace)
                .await
                .expect("create workspace");
            project.config.default_workspace_id = Some(workspace.id);
            repos
                .project_repo
                .update(&project)
                .await
                .expect("bind default workspace");
            repos
                .project_backend_access_repo
                .create(&ProjectBackendAccess::new(
                    project.id,
                    backend_id.to_string(),
                    Some("ard-004-test".to_string()),
                ))
                .await
                .expect("grant project backend access");
        }

        let mut project_agent = ProjectAgent::new(project.id, "codex", "CODEX");
        project_agent.config = serde_json::json!({
            "executor": "CODEX",
            "provider_id": "openai_codex",
            "model_id": "gpt-5"
        });
        repos
            .project_agent_repo
            .create(&project_agent)
            .await
            .expect("create ProjectAgent");
        (project, project_agent)
    }

    fn launch_intent(
        project: &Project,
        project_agent: &ProjectAgent,
        subject_ref: Option<SubjectRef>,
    ) -> AgentLaunchIntent {
        let mut effective_profile = AgentConfig::new("CODEX");
        effective_profile.provider_id = Some("openai_codex".to_string());
        effective_profile.model_id = Some("gpt-5.1-codex".to_string());
        AgentLaunchIntent {
            project_id: project.id,
            project_agent_id: Some(project_agent.id),
            execution_profile_override: Some(
                serde_json::to_value(effective_profile).expect("execution profile"),
            ),
            source: ExecutionSource::ProjectAgent,
            created_by_user_id: Some("ard-004-test".to_string()),
            subject_ref,
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: None,
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::ProvisionRuntimeThread,
        }
    }

    #[tokio::test]
    async fn lifecycle_launch_persists_canonical_workspace_surface_before_product_delivery() {
        let runtime = migrated_runtime("ard004_launch_surface").await;
        let frame_construction_handle = SharedAgentRunFrameConstructionHandle::default();
        let bootstrap = build_repositories(
            runtime.pool.clone(),
            Vec::new(),
            None,
            SharedAgentRunRuntimeProvisionerHandle::default(),
            frame_construction_handle.clone(),
        )
        .await
        .expect("repository bootstrap");
        let repos = bootstrap.repos;
        let backend_registry = BackendRegistry::new();
        register_backend(&backend_registry, "local-ard004").await;
        let vfs = crate::bootstrap::vfs::build_vfs_kernel(
            repos.clone(),
            backend_registry.clone(),
            Vec::new(),
        );
        let audit_bus = Arc::new(InMemoryContextAuditBus::new(32));
        let frame_construction_bound = frame_construction_handle
            .set(Arc::new(AgentRunProjectOwnerFrameConstructionAdapter::new(
                AgentRunProjectOwnerFrameConstructionDeps {
                    repos: repos.clone(),
                    vfs_service: vfs.vfs_service,
                    availability: backend_registry,
                    platform_config: Arc::new(PlatformConfig { mcp_base_url: None }),
                    lifecycle_surface_projection: bootstrap.lifecycle_surface_projection,
                    audit_bus: audit_bus.clone(),
                },
            )))
            .is_ok();
        assert!(frame_construction_bound, "bind frame construction");

        let (project, project_agent) =
            create_project_agent_fixture(&repos, Some("local-ard004")).await;
        let story = Story::new(
            project.id,
            "ARD-004 subject story".to_string(),
            "subject context must be available during frame construction".to_string(),
        );
        repos.story_repo.create(&story).await.expect("create story");
        let launched = repos
            .project_agent_lifecycle_launch
            .launch_project_agent(&launch_intent(
                &project,
                &project_agent,
                Some(SubjectRef::new("story", story.id)),
            ))
            .await
            .expect("Lifecycle launch");
        let frame = repos
            .agent_frame_repo
            .get_current(launched.runtime_refs.agent_ref)
            .await
            .expect("load current frame")
            .expect("current frame");
        assert_eq!(frame.id, launched.runtime_refs.frame_ref);
        let vfs = frame.typed_vfs().expect("canonical frame VFS");
        let default_mount = vfs.default_mount().expect("workspace default mount");
        assert_eq!(default_mount.id, "main");
        assert_eq!(default_mount.backend_id, "local-ard004");
        assert_eq!(default_mount.provider, "relay_fs");
        assert_eq!(default_mount.root_ref, "D:/Projects/AgentDash");
        assert!(
            default_mount
                .metadata
                .get("workspace_id")
                .and_then(serde_json::Value::as_str)
                .is_some(),
            "default mount must preserve canonical workspace coordinates"
        );
        assert!(
            default_mount
                .metadata
                .get("workspace_binding_id")
                .and_then(serde_json::Value::as_str)
                .is_some(),
            "default mount must preserve canonical workspace binding coordinates"
        );
        assert!(!default_mount.capabilities.is_empty());
        assert!(frame.typed_capability_state().is_some());
        assert!(frame.context_bundle_summary().is_some());
        assert_eq!(
            frame
                .typed_execution_profile()
                .and_then(|profile| profile.model_id),
            Some("gpt-5.1-codex".to_string())
        );
        let story_context_events = audit_bus.query(
            &launched.runtime_refs.run_ref.to_string(),
            &launched.runtime_refs.agent_ref.to_string(),
            &AuditFilter {
                slot: Some("story".to_string()),
                ..AuditFilter::default()
            },
        );
        assert!(
            story_context_events
                .iter()
                .any(|event| event.fragment.content.contains("ARD-004 subject story")),
            "subject_ref must reach owner composition before Lifecycle persists its association"
        );

        let (project_without_workspace, agent_without_workspace) =
            create_project_agent_fixture(&repos, None).await;
        let error = repos
            .project_agent_lifecycle_launch
            .launch_project_agent(&launch_intent(
                &project_without_workspace,
                &agent_without_workspace,
                None,
            ))
            .await
            .expect_err("missing workspace must fail during frame construction");
        assert!(
            error
                .to_string()
                .contains("缺少可用的 workspace default mount"),
            "unexpected error: {error}"
        );

        drop(runtime);
    }
}
