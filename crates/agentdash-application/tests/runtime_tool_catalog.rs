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
use agentdash_application::companion::ApplicationWorkflowScriptPreflightAdapter;
use agentdash_application::repository_set::{
    LifecycleProjectAgentLaunchAdapter, LifecycleProjectAgentLaunchDeps, RepositorySet,
};
use agentdash_application::runtime_tools::{
    CollaborationRuntimeToolProvider, SessionRuntimeToolComposer, SharedSessionToolServicesHandle,
    TaskRuntimeToolProvider, VfsRuntimeToolProvider,
};
use agentdash_application::wait_activity::{WaitActivityService, WaitRuntimeToolProvider};
use agentdash_application_agentrun::agent_run::frame::{
    AgentRunLaunchAnchorFrameConstructionAdapter, AgentRunWorkflowNodeFrameMaterializationAdapter,
};
use agentdash_application_agentrun::agent_run::{AgentFrameHookRuntime, AgentRunTerminalRegistry};
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
use agentdash_domain::workflow::{
    ActivationRule, ApiRequestExecutorSpec, BashExecExecutorSpec, LifecycleRun,
    OrchestrationInstance, OrchestrationPlanSnapshot, OrchestrationSourceRef, OrchestrationStatus,
    PlanNode, PlanNodeKind, RuntimeNodeState, RuntimeNodeStatus,
};
use agentdash_infrastructure::*;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{
    AgentConfig, AgentFrameHookSnapshot, ApiRequestOutcome, BashExecOutcome, CapabilityState,
    ExecutionContext, ExecutionSessionFrame, ExecutionTurnFrame, FunctionRunner,
    NoopExecutionHookProvider, RuntimeVfsAccessPolicy, ToolCapability, ToolCluster, Vfs,
    WorkflowScriptEvaluator, WorkspaceModuleDimension,
};
use agentdash_workspace_module::workspace_module::{
    SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModulePresentationAppendHandle,
    SharedWorkspaceModuleRuntimeGatewayHandle, WorkspaceModuleRuntimeToolProvider,
};
use async_trait::async_trait;
use base64::Engine;
use chrono::Utc;
use flate2::read::GzDecoder;
use serde::Deserialize;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct RejectingFunctionRunner;

struct ProductionCatalogWorkflowScriptEvaluator;

impl WorkflowScriptEvaluator for ProductionCatalogWorkflowScriptEvaluator {
    fn validate_workflow_script(&self, _script: &str) -> Result<(), Vec<String>> {
        Ok(())
    }

    fn eval_workflow_script(
        &self,
        _script: &str,
        _ctx: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "kind": "workflow",
            "body": [{
                "kind": "agent",
                "name": "catalog_probe",
                "procedure": "catalog.probe"
            }]
        }))
    }
}

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
    repository_set(pool)
}

fn repository_set(pool: sqlx::PgPool) -> RepositorySet {
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
        agent_run_fork_graph_store: Arc::new(PostgresAgentRunForkGraphStore::new(pool.clone())),
        agent_run_delete_store: Arc::new(PostgresAgentRunDeleteStore::new(pool.clone())),
        agent_run_runtime_binding_repo: runtime_binding_repo,
        agent_run_runtime_provisioner: Arc::new(SharedAgentRunRuntimeProvisionerHandle::default()),
        workflow_agent_run_delivery:
            agentdash_application_ports::workflow_agent_run_delivery::SharedWorkflowAgentRunDeliveryHandle::default(),
        agent_run_mailbox_repo: mailbox_repo,
        agent_run_command_receipt_repo: Arc::new(PostgresAgentRunCommandReceiptRepository::new(
            pool.clone(),
        )),
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
    execution_context_for_owner(project_id, Uuid::nil(), Uuid::nil(), Uuid::nil())
}

fn execution_context_for_owner(
    project_id: Uuid,
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Uuid,
) -> ExecutionContext {
    let runtime_thread_id = "thread-production-catalog";
    let presentation_thread_id = "presentation-production-catalog";
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
            hook_runtime: Some(Arc::new(AgentFrameHookRuntime::new(
                run_id,
                agent_id,
                frame_id,
                1,
                runtime_thread_id.to_string(),
                Arc::new(NoopExecutionHookProvider),
                AgentFrameHookSnapshot::default(),
            ))),
            platform_tool_execution: Some(agentdash_spi::PlatformToolExecutionContext {
                run_id,
                project_id,
                agent_id,
                frame_id,
                runtime_thread_id: runtime_thread_id.parse().expect("runtime thread"),
                presentation_thread_id: presentation_thread_id
                    .parse()
                    .expect("presentation thread"),
                visible_workspace_module_refs: Vec::new(),
                invocation: Some(agentdash_spi::PlatformToolInvocationCoordinates {
                    runtime_turn_id: "turn-production-catalog".parse().expect("runtime turn"),
                    runtime_item_id: "item-production-catalog".parse().expect("runtime item"),
                    presentation_item_id: "turn-production-catalog:tool-production-catalog"
                        .parse()
                        .expect("presentation item"),
                    source_thread_id: "source-production-catalog".parse().expect("source thread"),
                    source_turn_id: "source-turn-production-catalog"
                        .parse()
                        .expect("source turn"),
                    source_item_id: "source-item-production-catalog"
                        .parse()
                        .expect("source item"),
                    binding_id: "binding-production-catalog"
                        .parse()
                        .expect("runtime binding"),
                    binding_generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration(
                        1,
                    ),
                    tool_set_revision: agentdash_agent_runtime_contract::ToolSetRevision(1),
                }),
                launch_evidence_frame_id: frame_id,
                current_surface_frame_id: frame_id,
                orchestration_id: Some(Uuid::new_v4()),
                node_path: Some("root/catalog".to_string()),
                node_attempt: Some(1),
            }),
            capability_state,
            ..Default::default()
        },
    }
}

fn attach_running_catalog_orchestration(run: &mut LifecycleRun) -> Uuid {
    let node_id = "catalog_probe".to_string();
    let source_ref = OrchestrationSourceRef::Inline {
        source_digest: "sha256:production-catalog-probe".to_string(),
    };
    let mut orchestration = OrchestrationInstance::new(
        "production_catalog_probe",
        source_ref.clone(),
        OrchestrationPlanSnapshot {
            plan_digest: "sha256:production-catalog-probe-plan".to_string(),
            plan_version: 1,
            source_ref,
            nodes: vec![PlanNode {
                node_id: node_id.clone(),
                node_path: node_id.clone(),
                parent_node_id: None,
                kind: PlanNodeKind::AgentCall,
                label: Some("Production catalog probe".to_string()),
                executor: None,
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec![node_id.clone()],
            activation_rules: vec![ActivationRule::Entry {
                node_id: node_id.clone(),
            }],
            state_exchange_rules: Vec::new(),
            limits: Default::default(),
            metadata: None,
            created_at: Utc::now(),
        },
    );
    orchestration.status = OrchestrationStatus::Running;
    orchestration.node_tree.push(RuntimeNodeState {
        node_id: node_id.clone(),
        node_path: node_id,
        kind: PlanNodeKind::AgentCall,
        status: RuntimeNodeStatus::Claiming,
        attempt: 1,
        inputs: Vec::new(),
        outputs: Vec::new(),
        executor_run_ref: None,
        children: Vec::new(),
        phase_path: Vec::new(),
        started_at: None,
        completed_at: None,
        error: None,
        trace_refs: Vec::new(),
        cache: None,
    });
    let orchestration_id = orchestration.orchestration_id;
    assert!(run.add_orchestration(orchestration));
    orchestration_id
}

async fn production_test_database() -> (
    sqlx::PgPool,
    agentdash_infrastructure::postgres_runtime::PostgresRuntime,
    tokio::sync::OwnedSemaphorePermit,
) {
    static SERIAL: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> = std::sync::OnceLock::new();
    let permit = SERIAL
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
        .clone()
        .acquire_owned()
        .await
        .expect("runtime tool catalog database semaphore");
    let data_root = std::env::temp_dir()
        .join("agentdash-tests")
        .join(format!("runtime-tool-owner-context-{}", std::process::id()));
    let runtime =
        agentdash_infrastructure::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "runtime-tool-owner-context",
            8,
            data_root,
        )
        .await
        .expect("start embedded PostgreSQL");
    let database = format!("runtime_tool_owner_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {database}"))
        .execute(&runtime.pool)
        .await
        .expect("create runtime tool owner database");
    let options: PgConnectOptions = runtime
        .pool
        .connect_options()
        .as_ref()
        .clone()
        .database(&database);
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect_with(options)
        .await
        .expect("connect runtime tool owner database");
    agentdash_infrastructure::migration::run_postgres_migrations(&pool)
        .await
        .expect("migrate runtime tool owner database");
    (pool, runtime, permit)
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
        let arguments = scenarios
            .iter()
            .find(|scenario| scenario.tool_name == expected)
            .expect("representative production tool fixture")
            .arguments
            .clone();
        let execution = tool
            .execute(
                "production-fixture-call",
                arguments,
                CancellationToken::new(),
                None,
            )
            .await;
        match execution {
            Ok(result) => assert!(
                !result.content.is_empty() || result.details.is_some(),
                "production tool `{expected}` returned no observable result"
            ),
            Err(error) => {
                let message = error.to_string();
                assert!(!message.trim().is_empty());
                if matches!(expected, "task_read" | "workspace_module_list") {
                    assert!(
                        !message.contains("缺少 hook runtime")
                            && !message.contains("surface-bootstrap")
                            && !message.contains("runtime surface query missing anchor"),
                        "production invocation context was not wired for `{expected}`: {message}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn all_six_production_providers_execute_with_real_typed_owner_scope() {
    let (pool, _postgres, _serial) = production_test_database().await;
    let repos = repository_set(pool);
    let project_id = Uuid::new_v4();
    let mut run = LifecycleRun::new_plain(project_id);
    let orchestration_id = attach_running_catalog_orchestration(&mut run);
    repos
        .lifecycle_run_repo
        .create(&run)
        .await
        .expect("seed LifecycleRun");
    let agent_id = Uuid::new_v4();
    let frame_id = Uuid::new_v4();
    let mut context = execution_context_for_owner(project_id, run.id, agent_id, frame_id);
    let owner = context
        .turn
        .platform_tool_execution
        .as_mut()
        .expect("typed owner context");
    owner.orchestration_id = Some(orchestration_id);
    owner.node_path = Some("catalog_probe".to_string());
    owner.node_attempt = Some(1);
    assert_ne!(
        owner.runtime_thread_id.as_str(),
        owner.presentation_thread_id.as_str(),
        "the production catalog test must not hide runtime/presentation coordinate conflation"
    );
    let wait_service =
        WaitActivityService::new(agentdash_application::wait_activity::WaitActivityDeps {
            repositories: repos.wait_activity_repositories(),
            terminal_registry: Arc::new(AgentRunTerminalRegistry::default()),
        });
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
            .with_wait_service(wait_service.clone())
            .with_workflow_script_preflight(Arc::new(
                ApplicationWorkflowScriptPreflightAdapter::new(Arc::new(
                    ProductionCatalogWorkflowScriptEvaluator,
                )),
            )),
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
                SharedWorkspaceModuleAgentRunBridgeHandle::default(),
                SharedWorkspaceModuleRuntimeGatewayHandle::default(),
            )
            .with_presentation_append_handle(
                SharedWorkspaceModulePresentationAppendHandle::default(),
            ),
        ),
    ];
    let tools = SessionRuntimeToolComposer::from_final_catalog_providers(providers)
        .build_tools(&context)
        .await
        .expect("typed owner production tools");
    let scenarios = load_main_tool_scenarios();

    let calls = [
        ("mounts_list", serde_json::json!({})),
        (
            "companion_request",
            serde_json::json!({
                "target": "platform",
                "wait": false,
                "payload": {
                    "type": "workflow_script_preflight",
                    "source_text": "workflow catalog probe"
                }
            }),
        ),
        (
            "task_read",
            scenarios
                .iter()
                .find(|scenario| scenario.tool_name == "task_read")
                .expect("main task fixture")
                .arguments
                .clone(),
        ),
        (
            "wait",
            serde_json::json!({
                "activity_refs": [],
                "kinds": [],
                "timeout_ms": 0,
                "max_items": 10,
                "after_cursor": null
            }),
        ),
        ("workspace_module_list", serde_json::json!({})),
        (
            "complete_lifecycle_node",
            serde_json::json!({
                "outcome": "failed",
                "summary": "production provider execution probe"
            }),
        ),
    ];

    for (expected, arguments) in calls {
        let tool = tools
            .iter()
            .find(|tool| tool.name() == expected)
            .unwrap_or_else(|| panic!("missing production tool {expected}"));
        let result = tool
            .execute(
                &format!("typed-owner-{expected}"),
                arguments,
                CancellationToken::new(),
                None,
            )
            .await
            .unwrap_or_else(|error| panic!("{expected} typed owner execution failed: {error}"));

        assert!(!result.is_error, "{expected} returned a business error");
        assert!(
            !result.content.is_empty(),
            "{expected} returned no model result"
        );
        if expected == "mounts_list" {
            assert!(
                result
                    .content
                    .iter()
                    .any(|part| part.extract_text().is_some()),
                "mounts_list returned no textual VFS projection"
            );
            continue;
        }
        let details = result.details.expect("structured tool result");
        match expected {
            "companion_request" => assert_eq!(details["valid"], true),
            "task_read" => {
                assert_eq!(details["scope"]["project_id"], project_id.to_string());
                assert_eq!(details["scope"]["run_id"], run.id.to_string());
                assert_eq!(details["scope"]["agent_id"], agent_id.to_string());
            }
            "wait" => assert_eq!(details["type"], "wait"),
            "workspace_module_list" => assert_eq!(details["module_count"], 0),
            "complete_lifecycle_node" => {
                assert_eq!(details["run_id"], run.id.to_string());
                assert_eq!(details["orchestration_id"], orchestration_id.to_string());
                assert_eq!(details["node_path"], "catalog_probe");
                assert_eq!(details["outcome"], "failed");
            }
            other => panic!("unasserted production provider tool {other}"),
        }
    }

    let persisted_run = repos
        .lifecycle_run_repo
        .get_by_id(run.id)
        .await
        .expect("reload LifecycleRun")
        .expect("persisted LifecycleRun");
    let persisted_node = persisted_run
        .orchestrations
        .iter()
        .find(|value| value.orchestration_id == orchestration_id)
        .and_then(|value| {
            value
                .node_tree
                .iter()
                .find(|node| node.node_path == "catalog_probe" && node.attempt == 1)
        })
        .expect("persisted catalog node");
    assert_eq!(persisted_node.status, RuntimeNodeStatus::Failed);
    assert_eq!(persisted_node.executor_run_ref, None);
}

#[test]
fn fs_glob_tool_broker_projection_keeps_partial_arguments_non_fatal() {
    let contribution = ToolContribution {
        meta: ContributionMeta {
            key: "tool:test:fs_glob_partial".to_string(),
            source: SurfaceSourceRef {
                layer: "production_catalog_test".to_string(),
                key: "fs_glob_partial".to_string(),
            },
            priority: 0,
            requirement: ContributionRequirement::Required,
        },
        runtime_name: "fs_glob".to_string(),
        description: "Glob files".to_string(),
        parameters_schema: serde_json::json!({
            "type": "object",
            "properties": { "pattern": { "type": "string" } },
            "required": ["pattern"]
        }),
        capability_key: "file_read".to_string(),
        tool_path: "vfs::fs_glob".to_string(),
        allowed_channels: [ToolChannel::DirectCallback].into(),
        configuration_boundary: ConfigurationBoundary::Binding,
        protocol_projection: ToolProtocolProjection::FsGlob,
        presentation_emitter: ToolPresentationEmitter::ToolBroker,
        parity_fixture_id: "main_tool_fs_glob_lifecycle".to_string(),
    };

    for arguments in [
        serde_json::json!({}),
        serde_json::json!({ "pattern": null }),
    ] {
        let started = contribution
            .project_started("turn_001:tool_001", arguments.clone())
            .expect("partial fs_glob arguments must remain presentation-safe");
        let started = serde_json::to_value(started.item()).expect("serialize fs_glob start");
        assert_eq!(started["type"], "fsGlob");
        assert_eq!(started["pattern"], "");
        assert_eq!(started["arguments"], arguments);
        assert_eq!(started["status"], "inProgress");

        let failed = contribution
            .project_completed(
                "turn_001:tool_001",
                arguments.clone(),
                &serde_json::json!({
                    "content_items": [{
                        "type": "inputText",
                        "text": "pattern is required"
                    }]
                }),
                true,
            )
            .expect("executor validation failure must still project a terminal fs_glob item");
        let failed = serde_json::to_value(failed.item()).expect("serialize fs_glob terminal");
        assert_eq!(failed["type"], "fsGlob");
        assert_eq!(failed["pattern"], "");
        assert_eq!(failed["arguments"], arguments);
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["success"], false);
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
