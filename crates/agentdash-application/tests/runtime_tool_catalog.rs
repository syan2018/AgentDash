use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEvent, ItemCompletedNotification, ItemStartedNotification, ItemUpdatedNotification,
};
use agentdash_agent_runtime::{
    ContributionMeta, ContributionRequirement, SurfaceSourceRef, ToolContribution,
};
use agentdash_agent_runtime_contract::{
    ConfigurationBoundary, ToolChannel, ToolPresentationEmitter, ToolProtocolProjection,
};
use agentdash_agent_runtime_test_support::session_parity::{
    NormalizedPresentationEvent, PresentationDurability, compare_ordered_presentation_events,
};
use agentdash_application::repository_set::{
    LifecycleProjectAgentLaunchAdapter, LifecycleProjectAgentLaunchDeps, RepositorySet,
};
use agentdash_application::runtime_tools::{
    CollaborationRuntimeToolProvider, SessionRuntimeToolComposer, SharedSessionToolServicesHandle,
    TaskRuntimeToolProvider, VfsRuntimeToolProvider,
};
use agentdash_application::wait_activity::{WaitActivityService, WaitRuntimeToolProvider};
use agentdash_application_agentrun::agent_run::AgentRunTerminalRegistry;
use agentdash_application_agentrun::agent_run::frame::{
    AgentRunLaunchAnchorFrameConstructionAdapter, AgentRunWorkflowNodeFrameMaterializationAdapter,
};
use agentdash_application_lifecycle::AgentRunLifecycleSurfaceProjector;
use agentdash_application_lifecycle::lifecycle::tools::{
    SharedSessionToolServicesHandle as LifecycleSessionToolServicesHandle,
    WorkflowRuntimeToolProvider,
};
use agentdash_application_ports::agent_frame_hook_plan::SharedAgentFrameHookPlanCompiler;
use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
use agentdash_application_ports::agent_run_runtime::SharedAgentRunRuntimeProvisionerHandle;
use agentdash_application_ports::lifecycle_surface_projection::LifecycleSurfaceProjectionPort;
use agentdash_application_vfs::{MountProviderRegistryBuilder, VfsService};
use agentdash_domain::workflow::{ApiRequestExecutorSpec, BashExecExecutorSpec};
use agentdash_infrastructure::*;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{
    AgentConfig, ApiRequestOutcome, BashExecOutcome, CapabilityState, ExecutionContext,
    ExecutionSessionFrame, ExecutionTurnFrame, FunctionRunner, RuntimeVfsAccessPolicy,
    ToolCapability, ToolCluster, Vfs, WorkspaceModuleDimension,
};
use agentdash_workspace_module::workspace_module::{
    SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModulePresentationAppendHandle,
    SharedWorkspaceModuleRuntimeGatewayHandle, WorkspaceModuleRuntimeToolProvider,
};
use async_trait::async_trait;
use base64::Engine;
use flate2::read::GzDecoder;
use serde::Deserialize;
use sqlx::postgres::PgPoolOptions;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct RejectingFunctionRunner;

#[async_trait]
impl FunctionRunner for RejectingFunctionRunner {
    async fn run_api_request(
        &self,
        _spec: &ApiRequestExecutorSpec,
        _context: &serde_json::Value,
    ) -> Result<ApiRequestOutcome, String> {
        Err("fixture runner has no external transport".to_string())
    }

    async fn run_bash(
        &self,
        _spec: &BashExecExecutorSpec,
        _context: &serde_json::Value,
    ) -> Result<BashExecOutcome, String> {
        Err("fixture runner has no external process".to_string())
    }
}

fn lazy_repository_set() -> RepositorySet {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/fixture")
        .expect("lazy postgres pool");
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
    let runtime_binding_repo =
        Arc::new(PostgresAgentRuntimeCompositionRepository::new(pool.clone()));
    let mailbox_repo = Arc::new(PostgresAgentRunMailboxRepository::new(pool.clone()));
    let inline_file_repo = Arc::new(PostgresInlineFileRepository::new(pool.clone()));
    let hook_compiler = Arc::new(SharedAgentFrameHookPlanCompiler::default());
    let frame_construction: Arc<dyn AgentRunFrameConstructionPort> =
        Arc::new(AgentRunLaunchAnchorFrameConstructionAdapter::new(
            agent_frame_repo.clone(),
            hook_compiler.clone(),
        ));
    let lifecycle_surface: Arc<dyn LifecycleSurfaceProjectionPort> = Arc::new(
        AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(skill_asset_repo.clone()),
    );
    let workflow_materialization = Arc::new(AgentRunWorkflowNodeFrameMaterializationAdapter::new(
        agent_frame_repo.clone(),
        lifecycle_surface,
        hook_compiler,
    ));
    let project_launch = Arc::new(LifecycleProjectAgentLaunchAdapter::new(
        LifecycleProjectAgentLaunchDeps {
            run_repo: workflow_repo.clone(),
            workflow_graph_repo: workflow_repo.clone(),
            agent_repo: lifecycle_agent_repo.clone(),
            frame_repo: agent_frame_repo.clone(),
            association_repo: lifecycle_subject_association_repo.clone(),
            gate_repo: lifecycle_gate_repo.clone(),
            lineage_repo: agent_lineage_repo.clone(),
            frame_construction: frame_construction.clone(),
        },
    ));
    RepositorySet {
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
        agent_run_runtime_binding_repo: runtime_binding_repo,
        agent_run_runtime_provisioner: Arc::new(SharedAgentRunRuntimeProvisionerHandle::default()),
        workflow_agent_run_delivery:
            agentdash_application_ports::workflow_agent_run_delivery::SharedWorkflowAgentRunDeliveryHandle::default(),
        agent_run_mailbox_repo: mailbox_repo,
        agent_frame_construction: frame_construction,
        workflow_agent_frame_materialization: workflow_materialization,
        project_agent_lifecycle_launch: project_launch,
        routine_repo: Arc::new(PostgresRoutineRepository::new(pool.clone())),
        routine_execution_repo: Arc::new(PostgresRoutineExecutionRepository::new(pool.clone())),
        inline_file_repo,
        permission_grant_repo: Arc::new(PostgresPermissionGrantRepository::new(pool)),
        project_projection_notifications: None,
    }
}

fn execution_context(project_id: Uuid) -> ExecutionContext {
    let vfs = Vfs {
        source_project_id: Some(project_id.to_string()),
        ..Default::default()
    };
    let clusters = [
        ToolCluster::Read,
        ToolCluster::Write,
        ToolCluster::Execute,
        ToolCluster::Workflow,
        ToolCluster::Collaboration,
        ToolCluster::Task,
        ToolCluster::WorkspaceModule,
    ];
    let mut capability_state = CapabilityState::from_clusters(clusters);
    capability_state.workspace_module = WorkspaceModuleDimension::all();
    for key in [
        "file_read",
        "file_write",
        "shell_execute",
        "workflow",
        "collaboration",
        "task",
        "workspace_module",
    ] {
        capability_state
            .tool
            .capabilities
            .insert(ToolCapability::new(key));
    }
    ExecutionContext {
        session: ExecutionSessionFrame {
            turn_id: "turn-production-catalog".to_string(),
            working_directory: std::path::PathBuf::from("."),
            environment_variables: HashMap::new(),
            executor_config: AgentConfig::default(),
            mcp_servers: Vec::new(),
            vfs: Some(vfs.clone()),
            vfs_access_policy: Some(RuntimeVfsAccessPolicy::whole_mounts_from_vfs(&vfs)),
            backend_execution: None,
            runtime_backend_anchor: None,
            identity: None,
        },
        turn: ExecutionTurnFrame {
            capability_state,
            ..Default::default()
        },
    }
}

#[tokio::test]
async fn final_catalog_uses_six_production_providers_and_executes_representative_tools() {
    let repos = lazy_repository_set();
    let wait_service =
        WaitActivityService::new(agentdash_application::wait_activity::WaitActivityDeps {
            repositories: repos.wait_activity_repositories(),
            terminal_registry: Arc::new(AgentRunTerminalRegistry::default()),
        });
    let workspace_bridge = SharedWorkspaceModuleAgentRunBridgeHandle::default();
    let workspace_gateway = SharedWorkspaceModuleRuntimeGatewayHandle::default();
    let providers: [Arc<dyn RuntimeToolProvider>; 6] = [
        Arc::new(VfsRuntimeToolProvider::new(
            Arc::new(VfsService::new(Arc::new(
                MountProviderRegistryBuilder::new().build(),
            ))),
            None,
        )),
        Arc::new(WorkflowRuntimeToolProvider::new(
            repos.lifecycle_orchestrator_deps(),
            LifecycleSessionToolServicesHandle,
            Arc::new(RejectingFunctionRunner),
        )),
        Arc::new(
            CollaborationRuntimeToolProvider::new(
                repos.clone(),
                SharedSessionToolServicesHandle::default(),
            )
            .with_wait_service(wait_service.clone()),
        ),
        Arc::new(TaskRuntimeToolProvider::new(repos.clone())),
        Arc::new(WaitRuntimeToolProvider::from_service(wait_service)),
        Arc::new(
            WorkspaceModuleRuntimeToolProvider::new(
                repos.project_extension_installation_repo.clone(),
                repos.project_repo.clone(),
                repos.canvas_repo.clone(),
                repos.canvas_runtime_state_repo.clone(),
                repos.agent_run_runtime_binding_repo.clone(),
                workspace_bridge,
                workspace_gateway,
            )
            .with_presentation_append_handle(
                SharedWorkspaceModulePresentationAppendHandle::default(),
            ),
        ),
    ];
    let tools = SessionRuntimeToolComposer::from_final_catalog_providers(providers)
        .build_tools(&execution_context(Uuid::new_v4()))
        .await
        .expect("production catalog");
    let names = tools
        .iter()
        .map(|tool| tool.name())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        tools.len(),
        names.len(),
        "production catalog names must be unique"
    );
    assert_eq!(tools.len(), 17, "production catalog contribution count");
    let scenarios = load_main_tool_scenarios();
    assert_eq!(scenarios.len(), tools.len());
    for tool in &tools {
        let contribution = contribution_from_production_tool(tool.as_ref());
        assert_eq!(
            contribution.presentation_emitter,
            ToolPresentationEmitter::ToolBroker
        );
        let fixture = scenarios
            .iter()
            .find(|scenario| scenario.fixture_id == contribution.parity_fixture_id)
            .unwrap_or_else(|| panic!("missing real main fixture for {}", tool.name()));
        assert_eq!(fixture.tool_name, tool.name());
        compare_tool_projection(&contribution, fixture);
    }
    for expected in [
        "fs_read",
        "complete_lifecycle_node",
        "companion_request",
        "task_read",
        "wait",
        "workspace_module_list",
    ] {
        assert!(
            names.contains(expected),
            "missing production tool {expected}: {names:?}"
        );
        let tool = tools.iter().find(|tool| tool.name() == expected).unwrap();
        assert!(tool.protocol_projector().is_some());
        assert!(tool.protocol_fixture_id().is_some());
        let _ = tool
            .execute(
                "production-fixture-call",
                serde_json::json!({}),
                CancellationToken::new(),
                None,
            )
            .await;
    }
}

#[derive(Debug, Deserialize)]
struct ToolProjectionScenario {
    fixture_id: String,
    tool_name: String,
    arguments: serde_json::Value,
    protected_events: Vec<serde_json::Value>,
}

fn load_main_tool_scenarios() -> Vec<ToolProjectionScenario> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../agentdash-agent-runtime-test-support/fixtures/session-parity/main/tool-contributions.json"
    )))
    .unwrap();
    assert_eq!(fixture["oracle_commit"], "957fa9d60");
    assert_eq!(fixture["encoding"], "gzip+base64");
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(fixture["protected_scenarios"].as_str().unwrap())
        .unwrap();
    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut json = Vec::new();
    decoder.read_to_end(&mut json).unwrap();
    serde_json::from_slice(&json).unwrap()
}

fn contribution_from_production_tool(tool: &dyn agentdash_spi::AgentTool) -> ToolContribution {
    let projection = match tool.protocol_projector().expect("owner projector") {
        agentdash_agent_types::ToolProtocolProjector::Command => ToolProtocolProjection::Command,
        agentdash_agent_types::ToolProtocolProjector::FileChange => {
            ToolProtocolProjection::FileChange
        }
        agentdash_agent_types::ToolProtocolProjector::FsRead => ToolProtocolProjection::FsRead,
        agentdash_agent_types::ToolProtocolProjector::FsGrep => ToolProtocolProjection::FsGrep,
        agentdash_agent_types::ToolProtocolProjector::FsGlob => ToolProtocolProjection::FsGlob,
        agentdash_agent_types::ToolProtocolProjector::Mcp { server_key } => {
            ToolProtocolProjection::Mcp { server_key }
        }
        agentdash_agent_types::ToolProtocolProjector::Dynamic { namespace } => {
            ToolProtocolProjection::Dynamic { namespace }
        }
    };
    ToolContribution {
        meta: ContributionMeta {
            key: format!("tool:test:{}", tool.name()),
            source: SurfaceSourceRef {
                layer: "production_catalog_test".to_string(),
                key: tool.name().to_string(),
            },
            priority: 0,
            requirement: ContributionRequirement::Required,
        },
        runtime_name: tool.name().to_string(),
        description: tool.description().to_string(),
        parameters_schema: tool.parameters_schema(),
        capability_key: "fixture".to_string(),
        tool_path: format!("fixture::{}", tool.name()),
        allowed_channels: [ToolChannel::DirectCallback].into(),
        configuration_boundary: ConfigurationBoundary::Binding,
        protocol_projection: projection,
        presentation_emitter: ToolPresentationEmitter::ToolBroker,
        parity_fixture_id: tool.protocol_fixture_id().expect("owner fixture id"),
    }
}

fn compare_tool_projection(contribution: &ToolContribution, fixture: &ToolProjectionScenario) {
    assert_eq!(fixture.protected_events.len(), 3);
    let item_id = fixture.protected_events[0]["payload"]["item"]["id"]
        .as_str()
        .unwrap();
    let content_items_value = if matches!(
        contribution.protocol_projection,
        ToolProtocolProjection::Command
    ) {
        serde_json::json!([{"type":"inputText","text":format!("{} fixture result", fixture.tool_name)}])
    } else {
        fixture.protected_events[1]["payload"]["item"]["contentItems"].clone()
    };
    let content_items: Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem> =
        if content_items_value.is_null() {
            Vec::new()
        } else {
            serde_json::from_value(content_items_value.clone()).unwrap()
        };
    let started = contribution
        .project_started(item_id, fixture.arguments.clone())
        .unwrap();
    let started_event = BackboneEvent::ItemStarted(ItemStartedNotification {
        item: started.item().clone(),
        thread_id: "session-fixture".to_string(),
        turn_id: "turn-fixture".to_string(),
        started_at_ms: 1_720_000_000_000,
    });
    let update_event = if matches!(
        contribution.protocol_projection,
        ToolProtocolProjection::Command
    ) {
        BackboneEvent::CommandOutputDelta(
            agentdash_agent_protocol::codex_app_server_protocol::CommandExecutionOutputDeltaNotification {
                thread_id: "session-fixture".to_string(),
                turn_id: "turn-fixture".to_string(),
                item_id: item_id.to_string(),
                delta: format!("{} fixture result", fixture.tool_name),
            },
        )
    } else {
        let updated = contribution
            .project_updated(item_id, fixture.arguments.clone(), content_items.clone())
            .unwrap();
        BackboneEvent::ItemUpdated(ItemUpdatedNotification {
            item: updated.item().clone(),
            thread_id: "session-fixture".to_string(),
            turn_id: "turn-fixture".to_string(),
            updated_at_ms: 1_720_000_000_001,
        })
    };
    let output = if matches!(
        contribution.protocol_projection,
        ToolProtocolProjection::Command
    ) {
        serde_json::json!({"aggregated_output":format!("{} fixture result", fixture.tool_name)})
    } else if matches!(
        contribution.protocol_projection,
        ToolProtocolProjection::FileChange
    ) {
        serde_json::json!({})
    } else {
        serde_json::json!({"content_items":content_items_value})
    };
    let completed = contribution
        .project_completed(item_id, fixture.arguments.clone(), &output, false)
        .unwrap();
    let completed_event = BackboneEvent::ItemCompleted(ItemCompletedNotification {
        item: completed.item().clone(),
        thread_id: "session-fixture".to_string(),
        turn_id: "turn-fixture".to_string(),
        completed_at_ms: 1_720_000_000_002,
    });
    let current = [
        NormalizedPresentationEvent {
            durability: PresentationDurability::Durable,
            event: serde_json::to_value(started_event).unwrap(),
        },
        NormalizedPresentationEvent {
            durability: PresentationDurability::Ephemeral,
            event: serde_json::to_value(update_event).unwrap(),
        },
        NormalizedPresentationEvent {
            durability: PresentationDurability::Durable,
            event: serde_json::to_value(completed_event).unwrap(),
        },
    ];
    let main = fixture
        .protected_events
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, event)| NormalizedPresentationEvent {
            durability: if index == 1 {
                PresentationDurability::Ephemeral
            } else {
                PresentationDurability::Durable
            },
            event,
        })
        .collect::<Vec<_>>();
    compare_ordered_presentation_events(&main, &current).unwrap_or_else(|error| {
        panic!(
            "{} main protected-body mismatch: {error:?}",
            fixture.tool_name
        )
    });
}
