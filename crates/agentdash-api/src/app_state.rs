use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;
use tokio::sync::broadcast;

use agentdash_agent_runtime::{PlatformToolBroker, RuntimeToolExecutor};
use agentdash_agent_runtime_host::{
    CompleteAgentHostError, CompleteAgentLiveCatalog, CompleteAgentLiveCatalogError,
    CompleteAgentVerificationMethod, CompleteAgentVerificationRecord, RuntimePlatformToolHandler,
};
use agentdash_agent_service_api::{
    AgentHostCallbacks, AgentPayloadDigest, AgentServiceErrorCode, AgentServiceInstanceId,
};
use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::companion::RuntimeThreadToolServices;
use agentdash_application::companion::{
    ApplicationCompanionContinuationEffects, ApplicationCompanionRuntimeToolService,
    ApplicationWorkflowScriptPreflightAdapter, CompanionRuntimeToolServiceDeps,
};
use agentdash_application::context::{
    InMemoryContextAuditBus, SharedContextAuditBus, VfsDiscoveryRegistry,
};
use agentdash_application::frame_construction::{
    AgentRunProjectOwnerFrameConstructionAdapter, AgentRunProjectOwnerFrameConstructionDeps,
};
use agentdash_application::hook_workflow_projection::ProductHookWorkflowProjection;
use agentdash_application::platform_config::{PlatformConfig, SharedPlatformConfig};
use agentdash_application::product_runtime_surface::{
    ProductAgentRunAppliedResourceSurfaceCompiler, ProductAgentRunFactsResolver,
};
use agentdash_application::product_runtime_surface_update::ProductAgentRunRuntimeSurfaceUpdateService;
use agentdash_application::project_agent_run_start::{
    ProjectAgentRunStartDeps, ProjectAgentRunStartService,
};
pub use agentdash_application::repository_set::RepositorySet;
use agentdash_application::routine::RoutineExecutor;
use agentdash_application::runtime_tools::workspace_module_product::{
    ApplicationWorkspaceModuleRuntimeToolService, WorkspaceModuleRuntimeToolDeps,
    workspace_module_runtime_tool_schema,
};
use agentdash_application::scheduling::CronSchedulerHandle;
use agentdash_application::task::tools::ApplicationRuntimeTaskToolService;
use agentdash_application::vfs_surface_resolver::{VfsSurfaceResolver, VfsSurfaceResolverDeps};
use agentdash_application::wait_activity::WaitActivityService;
use agentdash_application::workflow_agent_call_product::ApplicationWorkflowAgentCallProductAdapter;
use agentdash_application_agentrun::agent_run::{
    AgentRunAppliedResourceSurfaceQueryPort, AgentRunProductCommandFacade,
    AgentRunProductInputDeliveryPort, AgentRunProductInputDeliveryService,
    AgentRunProductLaunchService, AgentRunProductProjectionQueryPort, AgentRunProductProtocolPorts,
    AgentRunTerminalSourceReconcilePort, CompanionContinuationEffectPort,
    CompanionContinuationSagaRepository, ProcessAgentRunForkSagaRepository,
    ProcessCompanionFreshSagaRepository, ProductAgentRunForkGraphAdapter,
    ProductAgentRunForkRuntimeAdapter, ProductAgentRunRuntimeSnapshotAdapter,
    ProductCompanionFreshRuntimeAdapter, build_workflow_agent_call_dispatch,
};
use agentdash_application_extension_gateway::{
    ExtensionGateway, ExtensionRuntimeBackendServiceInvoker, ExtensionRuntimeChannelInvoker,
};
use agentdash_application_hooks::{AppExecutionHookProvider, AppExecutionHookProviderDeps};
use agentdash_application_lifecycle::run_view_builder::{
    LifecycleRunViewQueryDeps, LifecycleRunViewQueryPort, LifecycleRunViewQueryService,
};
use agentdash_application_lifecycle::{
    AgentRunLifecycleAppliedResourceSurfaceCompiler, AgentRunLifecycleSurfaceProjector,
    LifecycleOrchestrator, LifecycleOrchestratorDeps,
    LifecycleWorkflowAgentNodeMaterializationAdapter,
    LifecycleWorkflowAgentNodeMaterializationDeps,
};
use agentdash_application_ports::agent_frame_materialization::{
    AgentRunFrameConstructionPort, AgentRunRuntimeSurfaceUpdatePort,
};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolService,
};
use agentdash_application_vfs::{
    AppliedVfsRuntimeToolService, MountProviderRegistry, VfsMutationDispatcher, VfsService,
};
use agentdash_application_workflow::OrchestrationExecutorLauncher;
use agentdash_contracts::project::ProjectEventStreamEnvelope;
use agentdash_diagnostics::{DiagnosticBuffer, DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_domain::llm_provider::LlmSecretCodec;
use agentdash_infrastructure::AgentRunProductProjectionComposition;
use agentdash_infrastructure::{
    CompleteAgentComposition, CompleteAgentCompositionError,
    CompleteAgentProductRuntimeProvisioner, CompleteAgentServiceSelectionCatalog,
    DeferredProductRuntimeToolService, PinnedCompleteAgentVerificationCatalog,
    PostgresAgentRunForkGraphStore, PostgresAgentRunProductRuntimeBindingRepository,
    PostgresAgentRunTerminalProjectionStore, PostgresCompanionContinuationSagaRepository,
    PostgresWorkflowExecutorEffectRepository, PostgresWorkflowRecoveryRepository,
    PostgresWorkspaceModulePresentationStore, ProcessShellTerminalRegistry,
    ProductCompleteAgentHookHandler, ProductRuntimeToolAuthorizer,
    ProductionCompleteAgentServiceSelector, WorkspaceModulePresentRuntimeTool,
    final_runtime_tool_catalog, product_runtime_tool_catalog,
};
use agentdash_integration_api::{
    AgentDashIntegration, AuthMode, CompleteAgentContributionError, MarketplaceSourceProvider,
    MemoryDiscoveryProvider, SkillDiscoveryProvider,
};
use agentdash_platform_spi::extension_package::ExtensionPackageArtifactStorage;
use agentdash_workspace_module::workspace_module::presentation_protocol::{
    WorkspaceModulePresentationCommandPort, WorkspaceModulePresentationCommandService,
};

use crate::integrations::{builtin_integrations, collect_integration_registration};
use crate::relay::{
    PinnedRuntimeWireDeploymentCatalog, RelayAgentRunTerminalProjectionProducer,
    RelayAgentRunTerminalSourceReconcile, RuntimeWireCompleteAgentAdmission,
    registry::BackendRegistry, runtime_wire::CloudRuntimeWirePlacementRegistry,
};

const BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY: usize = 256;
const PROJECT_CONTROL_PLANE_EVENT_CHANNEL_CAPACITY: usize = 256;
const PLATFORM_MCP_BASE_URL_ENV: &str = "AGENTDASH_MCP_BASE_URL";
const RUNTIME_WIRE_TRUST_CATALOG_ENV: &str = "AGENTDASH_RUNTIME_WIRE_TRUST_CATALOG";

fn configured_platform_mcp_base_url() -> Option<String> {
    resolve_platform_mcp_base_url(std::env::var(PLATFORM_MCP_BASE_URL_ENV).ok())
}

fn resolve_platform_mcp_base_url(raw_value: Option<String>) -> Option<String> {
    raw_value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn is_optional_complete_agent_materialization_failure(
    error: &CompleteAgentCompositionError,
) -> bool {
    match error {
        CompleteAgentCompositionError::Contribution(CompleteAgentContributionError::Factory(_)) => {
            true
        }
        CompleteAgentCompositionError::Contribution(CompleteAgentContributionError::Service(
            service_error,
        ))
        | CompleteAgentCompositionError::Host(CompleteAgentHostError::LiveCatalog(
            CompleteAgentLiveCatalogError::Service(service_error),
        )) => service_error.code == AgentServiceErrorCode::Unavailable,
        _ => false,
    }
}

fn builtin_complete_agent_verifier() -> Result<PinnedCompleteAgentVerificationCatalog> {
    let descriptor = agentdash_integration_codex::codex_complete_agent_descriptor();
    let revision = agentdash_integration_codex::CODEX_APP_SERVER_PROTOCOL_REVISION;
    Ok(PinnedCompleteAgentVerificationCatalog::new_with_templates(
        [CompleteAgentVerificationRecord {
            service_instance_id: AgentServiceInstanceId::new(
                agentdash_integration_codex::CODEX_COMPLETE_AGENT_INSTANCE_ID,
            )?,
            expected_publisher_integration: "builtin.codex_runtime".to_owned(),
            expected_service_version: revision.to_string(),
            expected_build_digest: AgentPayloadDigest::new(format!("codex-app-server:{revision}"))?,
            expected_profile_digest: descriptor.profile_digest,
            expected_conformance_suite_revision:
                agentdash_integration_codex::CODEX_COMPLETE_AGENT_CONFORMANCE_SUITE.to_owned(),
            method: CompleteAgentVerificationMethod::PinnedBuiltin,
            verifier_identity: "agentdash-api.builtin-catalog".to_owned(),
            verifier_revision: "complete-agent-v1".to_owned(),
            evidence_digest: AgentPayloadDigest::new(format!(
                "pinned-builtin:codex-app-server:{revision}:{}",
                agentdash_integration_codex::CODEX_COMPLETE_AGENT_CONFORMANCE_SUITE
            ))?,
        }],
        [
            agentdash_infrastructure::dash_complete_agent_verification_template()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
        ],
    )?)
}

fn configured_runtime_wire_trust_catalog() -> Result<PinnedRuntimeWireDeploymentCatalog> {
    match std::env::var(RUNTIME_WIRE_TRUST_CATALOG_ENV) {
        Ok(raw) if !raw.trim().is_empty() => {
            PinnedRuntimeWireDeploymentCatalog::from_json(&raw).map_err(anyhow::Error::new)
        }
        _ => Ok(PinnedRuntimeWireDeploymentCatalog::empty()),
    }
}

/// Application services that own live process handles or composed protocol gateways.
pub struct ServiceSet {
    pub complete_agent: Arc<CompleteAgentComposition>,
    pub complete_agent_verifier: Arc<PinnedCompleteAgentVerificationCatalog>,
    pub complete_agent_selections: Arc<CompleteAgentServiceSelectionCatalog>,
    pub complete_agent_callbacks: Arc<dyn AgentHostCallbacks>,
    pub agent_run_product_projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    pub agent_run_product_projection_composition: Arc<AgentRunProductProjectionComposition>,
    pub agent_run_product_resource_surfaces: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
    pub agent_run_product_runtime_bindings: Arc<PostgresAgentRunProductRuntimeBindingRepository>,
    pub agent_run_product_commands: Arc<AgentRunProductCommandFacade>,
    pub agent_run_product_launch: Arc<AgentRunProductLaunchService>,
    pub agent_run_product_protocol: Arc<AgentRunProductProtocolPorts>,
    pub companion_continuations: Arc<dyn CompanionContinuationSagaRepository>,
    pub companion_continuation_effects: Arc<dyn CompanionContinuationEffectPort>,
    pub agent_run_product_input_delivery: Arc<dyn AgentRunProductInputDeliveryPort>,
    pub project_agent_run_start: Arc<ProjectAgentRunStartService>,
    pub agent_run_frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    pub hook_provider: Arc<AppExecutionHookProvider>,
    pub cron_scheduler: CronSchedulerHandle,
    pub routine_executor: Arc<RoutineExecutor>,
    pub runtime_tool_broker: Arc<PlatformToolBroker>,
    pub shell_terminal_registry: Arc<ProcessShellTerminalRegistry>,
    pub lifecycle_run_views: Arc<dyn LifecycleRunViewQueryPort>,
    pub lifecycle_orchestrator: Arc<LifecycleOrchestrator>,
    pub workspace_module_presentations: Arc<PostgresWorkspaceModulePresentationStore>,
    pub terminal_projections: Arc<PostgresAgentRunTerminalProjectionStore>,
    pub terminal_source_reconcile: Arc<dyn AgentRunTerminalSourceReconcilePort>,
    pub terminal_projection_producer: Arc<RelayAgentRunTerminalProjectionProducer>,
    pub vfs_service: Arc<VfsService>,
    pub vfs_surface_resolver: Arc<VfsSurfaceResolver>,
    pub vfs_mutation_dispatcher: Arc<VfsMutationDispatcher>,
    pub extra_skill_dirs: Vec<std::path::PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    pub memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
    pub marketplace_source_providers: Vec<Arc<dyn MarketplaceSourceProvider>>,
    pub backend_registry: Arc<BackendRegistry>,
    pub runtime_wire_placements: Arc<CloudRuntimeWirePlacementRegistry>,
    pub runtime_wire_complete_agents: Arc<RuntimeWireCompleteAgentAdmission>,
    pub backend_runtime_events: broadcast::Sender<String>,
    pub project_control_plane_events: broadcast::Sender<ProjectEventStreamEnvelope>,
    pub shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    pub vfs_registry: VfsDiscoveryRegistry,
    pub mount_provider_registry: Arc<MountProviderRegistry>,
    pub auth_session_service: Arc<AuthSessionService>,
    pub audit_bus: SharedContextAuditBus,
    pub extension_gateway: Arc<ExtensionGateway>,
    pub extension_runtime_channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
    pub extension_package_artifact_storage: Arc<dyn ExtensionPackageArtifactStorage>,
    pub orchestration_executor_launcher: OrchestrationExecutorLauncher,
    pub workflow_recovery: Arc<PostgresWorkflowRecoveryRepository>,
}

pub struct AppConfig {
    pub platform_config: SharedPlatformConfig,
    pub auth_mode: AuthMode,
}

pub struct SecretSet {
    pub llm_provider_secret: Arc<dyn LlmSecretCodec>,
}

pub struct AppState {
    pub repos: RepositorySet,
    pub services: ServiceSet,
    pub config: AppConfig,
    pub secrets: SecretSet,
    pub auth_provider: Option<Arc<dyn agentdash_integration_api::AuthProvider>>,
    pub identity_directory_provider:
        Option<Arc<dyn agentdash_integration_api::IdentityDirectoryProvider>>,
    pub diagnostics: DiagnosticBuffer,
}

impl AppState {
    pub async fn new(pool: PgPool) -> Result<Arc<Self>> {
        Self::new_with_integrations(pool, builtin_integrations(), DiagnosticBuffer::new(0)).await
    }

    pub async fn new_with_integrations(
        pool: PgPool,
        integrations: Vec<Box<dyn AgentDashIntegration>>,
        diagnostics: DiagnosticBuffer,
    ) -> Result<Arc<Self>> {
        let mut integration_registration = collect_integration_registration(integrations)
            .map_err(|error| anyhow::anyhow!("Host Integration 注册失败: {error}"))?;

        let (project_control_plane_events, _project_control_plane_rx) =
            broadcast::channel(PROJECT_CONTROL_PLANE_EVENT_CHANNEL_CAPACITY);
        let repository_bootstrap = crate::bootstrap::repositories::build_repositories(
            pool.clone(),
            integration_registration.library_asset_seeds,
        )
        .await?;
        let repos = repository_bootstrap.repos;
        let auth_session_service = repository_bootstrap.auth_session_service;
        let extension_package_artifact_storage =
            repository_bootstrap.extension_package_artifact_storage;
        let audit_bus: SharedContextAuditBus = Arc::new(InMemoryContextAuditBus::new(2000));
        let llm_provider_secret: Arc<dyn LlmSecretCodec> = Arc::new(
            agentdash_infrastructure::LlmProviderSecretCipher::from_env_or_create_default()?,
        );
        let platform_config: SharedPlatformConfig = Arc::new(PlatformConfig {
            mcp_base_url: configured_platform_mcp_base_url(),
        });

        let relay_bootstrap =
            crate::bootstrap::relay::build_relay_runtime(BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY);
        let backend_registry = relay_bootstrap.backend_registry;
        let backend_runtime_events = relay_bootstrap.backend_runtime_events;
        let mcp_probe_relay = relay_bootstrap.mcp_probe_relay;
        let setup_action_transport = relay_bootstrap.setup_action_transport;
        let shell_output_registry = relay_bootstrap.shell_output_registry;
        let runtime_wire_placements = relay_bootstrap.runtime_wire_placements;

        let vfs_bootstrap = crate::bootstrap::vfs::build_vfs_kernel(
            repos.clone(),
            backend_registry.clone(),
            integration_registration.mount_providers,
        );
        let mount_provider_registry = vfs_bootstrap.mount_provider_registry;
        let vfs_service = vfs_bootstrap.vfs_service;
        let vfs_mutation_dispatcher = vfs_bootstrap.vfs_mutation_dispatcher;
        let vfs_materialization_service = vfs_bootstrap.vfs_materialization_service;
        let lifecycle_history_query = vfs_bootstrap.lifecycle_history_query;

        let runtime_product_bindings = Arc::new(
            PostgresAgentRunProductRuntimeBindingRepository::new(pool.clone()),
        );
        let product_facts = Arc::new(ProductAgentRunFactsResolver::new(
            repos.clone(),
            runtime_product_bindings.clone(),
        ));
        let product_resource_surfaces: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort> =
            Arc::new(AgentRunLifecycleAppliedResourceSurfaceCompiler::new(
                Arc::new(ProductAgentRunAppliedResourceSurfaceCompiler::new(
                    product_facts.as_ref().clone(),
                )),
                product_facts,
            ));
        let vfs_surface_resolver = Arc::new(VfsSurfaceResolver::new(VfsSurfaceResolverDeps {
            repos: repos.clone(),
            vfs_service: vfs_service.clone(),
            applied_resource_surfaces: product_resource_surfaces.clone(),
        }));
        let workspace_module_presentations =
            Arc::new(PostgresWorkspaceModulePresentationStore::new(pool.clone()));
        let workspace_module_presentation_commands: Arc<
            dyn WorkspaceModulePresentationCommandPort,
        > = Arc::new(WorkspaceModulePresentationCommandService::new(
            workspace_module_presentations.clone(),
            workspace_module_presentations.clone(),
        ));
        let shell_terminal_registry = Arc::new(ProcessShellTerminalRegistry::default());
        let wait_activity_service = Arc::new(WaitActivityService::from_repositories(
            repos.lifecycle_agent_repo.clone(),
            repos.agent_frame_repo.clone(),
            runtime_product_bindings.clone(),
            repos.lifecycle_gate_repo.clone(),
            shell_terminal_registry.activity_registry(),
        ));
        let lifecycle_runtime_tool = Arc::new(DeferredProductRuntimeToolService::new(
            ProductRuntimeToolKind::CompleteLifecycleNode,
            agentdash_application_lifecycle::tools::complete_lifecycle_node_parameters_schema(),
        ));
        let companion_request_runtime_tool = Arc::new(DeferredProductRuntimeToolService::new(
            ProductRuntimeToolKind::CompanionRequest,
            agentdash_application::companion::tools::companion_request_parameters_schema(),
        ));
        let companion_respond_runtime_tool = Arc::new(DeferredProductRuntimeToolService::new(
            ProductRuntimeToolKind::CompanionRespond,
            agentdash_application::companion::tools::companion_respond_parameters_schema(),
        ));
        let workspace_module_list_runtime_tool = Arc::new(DeferredProductRuntimeToolService::new(
            ProductRuntimeToolKind::WorkspaceModuleList,
            workspace_module_runtime_tool_schema(ProductRuntimeToolKind::WorkspaceModuleList),
        ));
        let workspace_module_describe_runtime_tool =
            Arc::new(DeferredProductRuntimeToolService::new(
                ProductRuntimeToolKind::WorkspaceModuleDescribe,
                workspace_module_runtime_tool_schema(
                    ProductRuntimeToolKind::WorkspaceModuleDescribe,
                ),
            ));
        let workspace_module_invoke_runtime_tool =
            Arc::new(DeferredProductRuntimeToolService::new(
                ProductRuntimeToolKind::WorkspaceModuleInvoke,
                workspace_module_runtime_tool_schema(ProductRuntimeToolKind::WorkspaceModuleInvoke),
            ));
        let workspace_module_operate_runtime_tool =
            Arc::new(DeferredProductRuntimeToolService::new(
                ProductRuntimeToolKind::WorkspaceModuleOperate,
                workspace_module_runtime_tool_schema(
                    ProductRuntimeToolKind::WorkspaceModuleOperate,
                ),
            ));
        let applied_vfs_tools = Arc::new(
            AppliedVfsRuntimeToolService::new(vfs_service.clone(), shell_terminal_registry.clone())
                .with_materialization(Some(vfs_materialization_service))
                .with_shell_output_registry(Some(shell_output_registry.clone())),
        );
        let runtime_task_tools = Arc::new(ApplicationRuntimeTaskToolService::new(repos.clone()));
        let workspace_module_present_tool: Arc<dyn RuntimeToolExecutor> =
            Arc::new(WorkspaceModulePresentRuntimeTool::new(
                runtime_product_bindings.clone(),
                workspace_module_presentation_commands,
            ));
        let mut runtime_tool_catalog: Vec<Arc<dyn RuntimeToolExecutor>> =
            final_runtime_tool_catalog(
                applied_vfs_tools,
                runtime_task_tools,
                workspace_module_present_tool,
            );
        runtime_tool_catalog.extend(product_runtime_tool_catalog(vec![
            wait_activity_service.clone() as Arc<dyn ProductRuntimeToolService>,
            lifecycle_runtime_tool.clone() as Arc<dyn ProductRuntimeToolService>,
            companion_request_runtime_tool.clone() as Arc<dyn ProductRuntimeToolService>,
            companion_respond_runtime_tool.clone() as Arc<dyn ProductRuntimeToolService>,
            workspace_module_list_runtime_tool.clone() as Arc<dyn ProductRuntimeToolService>,
            workspace_module_describe_runtime_tool.clone() as Arc<dyn ProductRuntimeToolService>,
            workspace_module_operate_runtime_tool.clone() as Arc<dyn ProductRuntimeToolService>,
            workspace_module_invoke_runtime_tool.clone() as Arc<dyn ProductRuntimeToolService>,
        ]));
        let runtime_tool_authorizer = Arc::new(ProductRuntimeToolAuthorizer::new(
            runtime_product_bindings.clone(),
            product_resource_surfaces.clone(),
        ));
        let runtime_tool_broker = Arc::new(
            PlatformToolBroker::new(runtime_tool_catalog, runtime_tool_authorizer)
                .map_err(anyhow::Error::msg)?,
        );
        let runtime_tool_handler =
            Arc::new(RuntimePlatformToolHandler::new(runtime_tool_broker.clone()));
        let hook_provider = Arc::new(AppExecutionHookProvider::new(
            AppExecutionHookProviderDeps {
                workflow_projection: Arc::new(ProductHookWorkflowProjection::new(
                    repos.clone(),
                    runtime_product_bindings.clone(),
                )),
                script_evaluator: Arc::new(agentdash_infrastructure::RhaiHookScriptEvaluator::new(
                    &AppExecutionHookProvider::builtin_preset_scripts(),
                )),
            },
        ));
        let runtime_hook_handler = Arc::new(ProductCompleteAgentHookHandler::new(
            runtime_product_bindings.clone(),
            hook_provider.clone(),
        ));

        let host_incarnation_id = format!("agentdash-api-host-{}", uuid::Uuid::new_v4());
        let complete_agent_verifier = Arc::new(builtin_complete_agent_verifier()?);
        let complete_agent_selections = Arc::new(CompleteAgentServiceSelectionCatalog::default());
        let complete_agent = Arc::new(CompleteAgentComposition::build(
            runtime_tool_handler,
            runtime_hook_handler,
            complete_agent_verifier.clone(),
            host_incarnation_id.clone(),
        )?);
        for contribution in integration_registration
            .complete_agent_registrations
            .drain(..)
        {
            let instance_id = contribution.facts().instance_id().clone();
            let _selection = match complete_agent.register_contribution(contribution).await {
                Ok(selection) => selection,
                Err(error) if is_optional_complete_agent_materialization_failure(&error) => {
                    complete_agent
                        .live_catalog
                        .mark_unavailable(instance_id.clone(), error.to_string())
                        .await;
                    let context = DiagnosticErrorContext::new(
                        "app_state.complete_agent_registration",
                        "materialize_optional_service",
                    );
                    diag_error!(
                        Warn,
                        Subsystem::AgentRun,
                        context = &context,
                        error = &error,
                        service_instance_id = %instance_id,
                        "可选 Complete Agent 服务不可用，跳过本次注册"
                    );
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
        }
        let complete_agent_selector = Arc::new(ProductionCompleteAgentServiceSelector::new(
            complete_agent.clone(),
            complete_agent_selections.clone(),
            AgentServiceInstanceId::new(
                agentdash_integration_codex::CODEX_COMPLETE_AGENT_INSTANCE_ID,
            )?,
            Arc::new(
                agentdash_infrastructure::persistence::postgres::PostgresDashCompleteAgentStore::new(
                    pool.clone(),
                ),
            ),
            repos.llm_provider_repo.clone(),
            repos.llm_provider_credential_repo.clone(),
            llm_provider_secret.clone(),
        ));
        let dynamic_runtime_tools = Arc::new(
            agentdash_infrastructure::mcp::ProductionRuntimeMcpToolCatalog::new(Some(
                mcp_probe_relay.clone(),
            )),
        );
        let product_runtime_provisioner = Arc::new(CompleteAgentProductRuntimeProvisioner::new(
            complete_agent.host.clone(),
            complete_agent_selector,
            runtime_tool_broker.clone(),
            dynamic_runtime_tools,
            repos.agent_frame_repo.clone(),
        ));
        let runtime_wire_complete_agents = RuntimeWireCompleteAgentAdmission::new(
            runtime_wire_placements.clone(),
            complete_agent.clone(),
            complete_agent_verifier.clone(),
            complete_agent_selections.clone(),
            Arc::new(configured_runtime_wire_trust_catalog()?),
        );
        let product_runtime_surface_updates: Arc<dyn AgentRunRuntimeSurfaceUpdatePort> =
            Arc::new(ProductAgentRunRuntimeSurfaceUpdateService::new(
                repos.clone(),
                runtime_product_bindings.clone(),
                product_runtime_provisioner.clone(),
            ));

        let product = Arc::new(
            AgentRunProductProjectionComposition::build(
                pool.clone(),
                complete_agent.live_catalog.clone(),
                product_runtime_provisioner.clone(),
                runtime_product_bindings.clone(),
                workspace_module_presentations,
            )
            .map_err(anyhow::Error::msg)?,
        );
        let product_commands = product.commands.clone();
        let product_input_delivery: Arc<dyn AgentRunProductInputDeliveryPort> = Arc::new(
            AgentRunProductInputDeliveryService::new(product_commands.clone()),
        );
        let product_launch = Arc::new(AgentRunProductLaunchService::new(
            product_runtime_provisioner.clone(),
            runtime_product_bindings.clone(),
            product_commands.clone(),
        ));
        let lifecycle_surface_projection =
            Arc::new(AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(
                repos.skill_asset_repo.clone(),
            ));
        let frame_construction = Arc::new(AgentRunProjectOwnerFrameConstructionAdapter::new(
            AgentRunProjectOwnerFrameConstructionDeps {
                repos: repos.clone(),
                vfs_service: vfs_service.clone(),
                availability: backend_registry.clone(),
                platform_config: platform_config.clone(),
                lifecycle_surface_projection,
                audit_bus: audit_bus.clone(),
                hook_plan_compiler: hook_provider.clone(),
                product_runtime_bindings: product.runtime_bindings.clone(),
            },
        ));
        let project_agent_run_start =
            Arc::new(ProjectAgentRunStartService::new(ProjectAgentRunStartDeps {
                project_agents: repos.project_agent_repo.clone(),
                lifecycle_runs: repos.lifecycle_run_repo.clone(),
                workflow_graphs: repos.workflow_graph_repo.clone(),
                lifecycle_agents: repos.lifecycle_agent_repo.clone(),
                frames: repos.agent_frame_repo.clone(),
                subject_associations: repos.lifecycle_subject_association_repo.clone(),
                lifecycle_gates: repos.lifecycle_gate_repo.clone(),
                agent_lineage: repos.agent_lineage_repo.clone(),
                frame_construction: frame_construction.clone(),
                product_launch: product_launch.clone(),
                product_input: product_input_delivery.clone(),
            }));
        let agent_run_product_protocol = Arc::new(AgentRunProductProtocolPorts::new(
            Arc::new(ProcessAgentRunForkSagaRepository::new(Arc::new(
                PostgresAgentRunForkGraphStore::new(pool.clone()),
            ))),
            Arc::new(ProductAgentRunForkRuntimeAdapter::with_product_launch(
                product_launch.clone(),
            )),
            Arc::new(ProductAgentRunForkGraphAdapter::new(
                repos.lifecycle_run_repo.clone(),
                repos.lifecycle_agent_repo.clone(),
                repos.agent_frame_repo.clone(),
                frame_construction.clone(),
                runtime_product_bindings.clone(),
            )),
            Arc::new(ProcessCompanionFreshSagaRepository::default()),
            Arc::new(ProductCompanionFreshRuntimeAdapter::with_product_launch(
                product_launch.clone(),
            )),
            product_launch.clone(),
            Arc::new(ProductAgentRunRuntimeSnapshotAdapter::new(
                runtime_product_bindings.clone(),
                product.agents.clone(),
            )),
        ));
        let companion_continuations: Arc<dyn CompanionContinuationSagaRepository> = Arc::new(
            PostgresCompanionContinuationSagaRepository::new(pool.clone()),
        );
        let companion_continuation_effects: Arc<dyn CompanionContinuationEffectPort> =
            Arc::new(ApplicationCompanionContinuationEffects::new(
                repos.clone(),
                agent_run_product_protocol.clone(),
                product_input_delivery.clone(),
                frame_construction.clone(),
                hook_provider.clone(),
            ));
        let routine_executor = Arc::new(RoutineExecutor::new(
            repos.clone(),
            backend_registry.clone(),
            product_input_delivery.clone(),
            product_launch.clone(),
            frame_construction.clone(),
        ));
        let cron_scheduler = CronSchedulerHandle::new();
        agentdash_application::scheduling::spawn_cron_scheduler(
            repos.clone(),
            routine_executor.clone(),
            &cron_scheduler,
        )
        .await;
        lifecycle_history_query
            .bind_product_projection(product.gateway.clone())
            .map_err(|error| anyhow::anyhow!(error))?;
        let terminal_source_reconcile: Arc<dyn AgentRunTerminalSourceReconcilePort> =
            Arc::new(RelayAgentRunTerminalSourceReconcile::new(
                backend_registry.clone(),
                product.terminals.clone(),
            ));
        let terminal_projection_producer = Arc::new(RelayAgentRunTerminalProjectionProducer::new(
            product.terminals.clone(),
            terminal_source_reconcile.clone(),
        ));
        let lifecycle_run_views: Arc<dyn LifecycleRunViewQueryPort> = Arc::new(
            LifecycleRunViewQueryService::new(LifecycleRunViewQueryDeps {
                lifecycle_runs: repos.lifecycle_run_repo.clone(),
                lifecycle_agents: repos.lifecycle_agent_repo.clone(),
                subject_associations: repos.lifecycle_subject_association_repo.clone(),
                product_projection: product.gateway.clone(),
            }),
        );

        let extension_gateway = crate::bootstrap::extension_gateway::build_extension_gateway(
            mcp_probe_relay,
            repos.clone(),
            backend_registry.clone(),
            setup_action_transport,
            repos.project_extension_installation_repo.clone(),
            backend_registry.clone(),
        );
        let extension_runtime_channel_invoker = Arc::new(ExtensionRuntimeChannelInvoker::new(
            repos.project_extension_installation_repo.clone(),
            backend_registry.clone(),
        ));
        let extension_runtime_backend_service_invoker =
            Arc::new(ExtensionRuntimeBackendServiceInvoker::new(
                repos.project_extension_installation_repo.clone(),
                backend_registry.clone(),
            ));
        let workspace_module_runtime_tool_deps = WorkspaceModuleRuntimeToolDeps {
            repos: repos.clone(),
            runtime_bindings: runtime_product_bindings.clone(),
            applied_surfaces: product_resource_surfaces.clone(),
            frames: repos.agent_frame_repo.clone(),
            installations: repos.project_extension_installation_repo.clone(),
            canvases: repos.canvas_repo.clone(),
            canvas_runtime_state: repos.canvas_runtime_state_repo.clone(),
            extension_gateway: extension_gateway.clone(),
            channel_invoker: extension_runtime_channel_invoker.clone(),
            backend_service_invoker: extension_runtime_backend_service_invoker,
            runtime_surface_updates: product_runtime_surface_updates,
        };
        workspace_module_list_runtime_tool
            .install(Arc::new(ApplicationWorkspaceModuleRuntimeToolService::new(
                ProductRuntimeToolKind::WorkspaceModuleList,
                workspace_module_runtime_tool_deps.clone(),
            )))
            .map_err(anyhow::Error::msg)?;
        workspace_module_describe_runtime_tool
            .install(Arc::new(ApplicationWorkspaceModuleRuntimeToolService::new(
                ProductRuntimeToolKind::WorkspaceModuleDescribe,
                workspace_module_runtime_tool_deps.clone(),
            )))
            .map_err(anyhow::Error::msg)?;
        workspace_module_invoke_runtime_tool
            .install(Arc::new(ApplicationWorkspaceModuleRuntimeToolService::new(
                ProductRuntimeToolKind::WorkspaceModuleInvoke,
                workspace_module_runtime_tool_deps.clone(),
            )))
            .map_err(anyhow::Error::msg)?;
        workspace_module_operate_runtime_tool
            .install(Arc::new(ApplicationWorkspaceModuleRuntimeToolService::new(
                ProductRuntimeToolKind::WorkspaceModuleOperate,
                workspace_module_runtime_tool_deps,
            )))
            .map_err(anyhow::Error::msg)?;
        let workflow_effects =
            Arc::new(PostgresWorkflowExecutorEffectRepository::new(pool.clone()));
        let function_runner: Arc<dyn agentdash_platform_spi::FunctionRunner> = Arc::new(
            agentdash_infrastructure::DefaultFunctionRunner::new(pool.clone()),
        );
        let workflow_script_preflight = Arc::new(ApplicationWorkflowScriptPreflightAdapter::new(
            Arc::new(agentdash_infrastructure::RhaiWorkflowScriptEvaluator::new()),
        ));
        let companion_runtime_tool_deps = CompanionRuntimeToolServiceDeps {
            repos: repos.clone(),
            runtime_bindings: runtime_product_bindings.clone(),
            runtime_thread_services: RuntimeThreadToolServices {
                product_input_delivery: product_input_delivery.clone(),
                product_runtime_bindings: runtime_product_bindings.clone(),
                product_launch: product_launch.clone(),
                product_protocols: agent_run_product_protocol.clone(),
                companion_continuations: companion_continuations.clone(),
                companion_continuation_effects: companion_continuation_effects.clone(),
                frame_construction: frame_construction.clone(),
            },
            wait_service: wait_activity_service.as_ref().clone(),
            hook_provider: hook_provider.clone(),
            model_preflight: None,
            workflow_script_preflight: Some(workflow_script_preflight),
        };
        companion_request_runtime_tool
            .install(Arc::new(ApplicationCompanionRuntimeToolService::new(
                ProductRuntimeToolKind::CompanionRequest,
                companion_runtime_tool_deps.clone(),
            )))
            .map_err(anyhow::Error::msg)?;
        companion_respond_runtime_tool
            .install(Arc::new(ApplicationCompanionRuntimeToolService::new(
                ProductRuntimeToolKind::CompanionRespond,
                companion_runtime_tool_deps,
            )))
            .map_err(anyhow::Error::msg)?;
        let workflow_recovery = Arc::new(PostgresWorkflowRecoveryRepository::new(pool));
        let workflow_materialization =
            Arc::new(LifecycleWorkflowAgentNodeMaterializationAdapter::new(
                LifecycleWorkflowAgentNodeMaterializationDeps {
                    run_repo: repos.lifecycle_run_repo.clone(),
                    workflow_graph_repo: repos.workflow_graph_repo.clone(),
                    agent_repo: repos.lifecycle_agent_repo.clone(),
                    frame_repo: repos.agent_frame_repo.clone(),
                    association_repo: repos.lifecycle_subject_association_repo.clone(),
                    gate_repo: repos.lifecycle_gate_repo.clone(),
                    lineage_repo: repos.agent_lineage_repo.clone(),
                    frame_construction: frame_construction.clone(),
                    workflow_agent_frame_materialization: frame_construction.clone(),
                },
            ));
        let workflow_product = Arc::new(ApplicationWorkflowAgentCallProductAdapter::new(
            workflow_materialization,
            repos.lifecycle_agent_repo.clone(),
            repos.agent_frame_repo.clone(),
            repos.project_agent_repo.clone(),
        ));
        let workflow_agent_call_dispatch = build_workflow_agent_call_dispatch(
            workflow_product,
            product_launch.clone(),
            product_input_delivery.clone(),
        );
        let orchestration_executor_launcher = OrchestrationExecutorLauncher::new_durable(
            repos.to_workflow_repository_set(),
            workflow_effects,
            function_runner,
        )
        .with_agent_call_dispatch(workflow_agent_call_dispatch);
        let lifecycle_orchestrator =
            Arc::new(LifecycleOrchestrator::new(LifecycleOrchestratorDeps {
                run_repo: repos.lifecycle_run_repo.clone(),
                agent_repo: repos.lifecycle_agent_repo.clone(),
                frame_repo: repos.agent_frame_repo.clone(),
                binding_repo: product.runtime_bindings.clone(),
                inline_file_repo: repos.inline_file_repo.clone(),
                orchestration_launcher: orchestration_executor_launcher.clone(),
            }));
        lifecycle_runtime_tool
            .install(lifecycle_orchestrator.clone())
            .map_err(anyhow::Error::msg)?;
        let auth_mode = crate::bootstrap::auth::validate_auth_provider_registered(
            crate::bootstrap::auth::resolve_configured_auth_mode()?,
            integration_registration.auth_provider.is_some(),
        )?;
        let vfs_registry = crate::bootstrap::vfs::build_vfs_discovery_registry(
            integration_registration.vfs_providers,
        );

        let mut state = Arc::new(Self {
            repos,
            services: ServiceSet {
                complete_agent_callbacks: complete_agent.host_callbacks(),
                complete_agent,
                complete_agent_verifier,
                complete_agent_selections,
                agent_run_product_projection: product.gateway.clone(),
                agent_run_product_projection_composition: product.clone(),
                agent_run_product_resource_surfaces: product_resource_surfaces,
                agent_run_product_runtime_bindings: product.runtime_bindings.clone(),
                agent_run_product_commands: product_commands,
                agent_run_product_launch: product_launch,
                agent_run_product_protocol,
                companion_continuations,
                companion_continuation_effects,
                agent_run_product_input_delivery: product_input_delivery,
                project_agent_run_start,
                agent_run_frame_construction: frame_construction,
                hook_provider,
                cron_scheduler,
                routine_executor,
                runtime_tool_broker,
                shell_terminal_registry,
                lifecycle_run_views,
                lifecycle_orchestrator,
                workspace_module_presentations: product.workspace_presentations.clone(),
                terminal_projections: product.terminals.clone(),
                terminal_source_reconcile,
                terminal_projection_producer,
                vfs_service,
                vfs_surface_resolver,
                vfs_mutation_dispatcher,
                extra_skill_dirs: integration_registration.extra_skill_dirs,
                skill_discovery_providers: integration_registration.skill_discovery_providers,
                memory_discovery_providers: integration_registration.memory_discovery_providers,
                marketplace_source_providers: integration_registration.marketplace_source_providers,
                backend_registry,
                runtime_wire_placements,
                runtime_wire_complete_agents,
                backend_runtime_events,
                project_control_plane_events,
                shell_output_registry,
                vfs_registry,
                mount_provider_registry,
                auth_session_service,
                audit_bus,
                extension_gateway,
                extension_runtime_channel_invoker,
                extension_package_artifact_storage,
                orchestration_executor_launcher,
                workflow_recovery,
            },
            config: AppConfig {
                platform_config,
                auth_mode,
            },
            secrets: SecretSet {
                llm_provider_secret,
            },
            auth_provider: integration_registration.auth_provider,
            identity_directory_provider: integration_registration.identity_directory_provider,
            diagnostics,
        });
        crate::bootstrap::background_workers::start_post_app_state_workers(&mut state).await;
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_service_api::{AgentServiceError, AgentServiceErrorCode};
    use agentdash_infrastructure::CompleteAgentCompositionError;
    use agentdash_integration_api::{
        CompleteAgentContributionError, CompleteAgentServiceFactoryError,
    };

    use super::{
        CompleteAgentHostError, CompleteAgentLiveCatalogError,
        is_optional_complete_agent_materialization_failure, resolve_platform_mcp_base_url,
    };

    #[test]
    fn platform_mcp_base_url_missing_env_keeps_platform_mcp_disabled() {
        assert_eq!(resolve_platform_mcp_base_url(None), None);
    }

    #[test]
    fn platform_mcp_base_url_blank_env_keeps_platform_mcp_disabled() {
        assert_eq!(resolve_platform_mcp_base_url(Some("   ".to_string())), None);
    }

    #[test]
    fn platform_mcp_base_url_uses_explicit_env_value() {
        assert_eq!(
            resolve_platform_mcp_base_url(Some("  http://127.0.0.1:3001/  ".to_string())),
            Some("http://127.0.0.1:3001/".to_string())
        );
    }

    #[test]
    fn optional_complete_agent_materialization_failure_is_isolated() {
        let factory_error =
            CompleteAgentCompositionError::Contribution(CompleteAgentContributionError::Factory(
                CompleteAgentServiceFactoryError::Unavailable {
                    reason: "fixture unavailable".to_owned(),
                    retryable: true,
                },
            ));
        let service_error = CompleteAgentCompositionError::Contribution(
            CompleteAgentContributionError::Service(AgentServiceError::new(
                AgentServiceErrorCode::Unavailable,
                "fixture describe unavailable",
                true,
            )),
        );
        let catalog_service_error =
            CompleteAgentCompositionError::Host(CompleteAgentHostError::LiveCatalog(
                CompleteAgentLiveCatalogError::Service(AgentServiceError::new(
                    AgentServiceErrorCode::Unavailable,
                    "fixture live describe unavailable",
                    true,
                )),
            ));

        assert!(is_optional_complete_agent_materialization_failure(
            &factory_error
        ));
        assert!(is_optional_complete_agent_materialization_failure(
            &service_error
        ));
        assert!(is_optional_complete_agent_materialization_failure(
            &catalog_service_error
        ));
    }

    #[test]
    fn complete_agent_contract_failure_remains_fatal() {
        let error = CompleteAgentCompositionError::Contribution(
            CompleteAgentContributionError::DescriptorMismatch {
                expected: "expected".to_owned(),
                actual: "actual".to_owned(),
            },
        );

        assert!(!is_optional_complete_agent_materialization_failure(&error));
    }
}
