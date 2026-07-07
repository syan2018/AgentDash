use std::path::PathBuf;
use std::sync::Arc;

use crate::agent_run_terminal_control::{
    ApiLifecycleTerminalConvergenceAdapter, ApiWaitProducerTerminalConvergenceAdapter,
};
use agentdash_application::companion::{
    CompanionModelPreflightError, CompanionModelPreflightPort, CompanionModelPreflightRequest,
};
use agentdash_application::platform_config::SharedPlatformConfig;
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::runtime_session_agent_run_bridge::{
    agent_run_session_control, agent_run_session_core, agent_run_session_eventing,
    agent_run_session_launch,
};
use agentdash_application::runtime_tools::{
    CollaborationRuntimeToolProvider, SessionRuntimeToolComposer, SessionToolServices,
    SharedSessionToolServicesHandle, TaskRuntimeToolProvider, VfsRuntimeToolProvider,
    WorkflowRuntimeToolProvider,
};
use agentdash_application::wait_activity::{
    WaitActivityDeps, WaitActivityService, WaitRuntimeToolProvider,
};
use agentdash_application_agentrun::agent_run::AgentRunTerminalRegistry;
use agentdash_application_agentrun::agent_run::AgentRunWorkspaceTitleAdapter;
use agentdash_application_agentrun::agent_run::{
    AgentRunControlEffectDeps, AgentRunControlEffectService,
    AgentRunEffectiveCapabilityView as ApplicationAgentRunEffectiveCapabilityView,
    AgentRunMailboxRuntimeAdapter, AgentRunMailboxRuntimeBoundaryDeps, AgentRunRuntimeSurfaceQuery,
    AgentRunRuntimeSurfaceQueryDeps,
    AgentRunRuntimeSurfaceQueryPort as ApplicationAgentRunRuntimeSurfaceQueryPort,
    AgentRunRuntimeSurfaceUpdateDeps, AgentRunRuntimeSurfaceUpdateService,
    AgentRunTerminalConvergenceDeps, accepted_launch_commit_port,
    agent_run_effective_capability_port, hook_target_runtime_port,
    runtime_session_effective_capability_port,
};
use agentdash_application_hooks::{AppExecutionHookProvider, AppExecutionHookProviderDeps};
use agentdash_application_ports::agent_run_list_invalidation::AgentRunListInvalidationPort;
use agentdash_application_ports::agent_run_surface::{
    AgentRunEffectiveCapabilityView as PortsAgentRunEffectiveCapabilityView,
    AgentRunRuntimeSurfaceQueryPort as PortsAgentRunRuntimeSurfaceQueryPort,
};
use agentdash_application_ports::frame_launch_envelope::AcceptedLaunchHookRuntimeSync;
use agentdash_application_runtime_session::session::{
    EmptyTerminalHookEffectHandlerRegistry, SessionBranchingService, SessionControlService,
    SessionCoreService, SessionEventingService, SessionHookService, SessionLaunchService,
    SessionRuntimeBuilder, SessionRuntimeService, SessionRuntimeTransitionService, SessionStoreSet,
    SessionTitleService, SessionToolResultCache, SessionToolResultCachePut,
};
use agentdash_application_vfs::tools::RuntimeVfsState;
use agentdash_application_vfs::tools::{ShellTerminalRegistration, ShellTerminalRegistry};
use agentdash_application_vfs::{VfsMaterializationService, VfsService};
use agentdash_domain::canvas::Canvas;
use agentdash_domain::llm_provider::{
    LlmProviderCredentialRepository, LlmProviderRepository, LlmSecretCodec,
};
use agentdash_domain::project::ProjectAuthorizationContext;
use agentdash_domain::settings::SettingsRepository;
use agentdash_executor::AgentConnector;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_executor::connectors::pi_agent::pi_agent_provider_registry::{
    build_effective_profile_catalog_from_db, preflight_effective_model_selection,
};
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_workspace_module::workspace_module::{
    SharedWorkspaceModuleAgentRunBridgeHandle, SharedWorkspaceModuleRuntimeGatewayHandle,
    WorkspaceModuleAgentRunBridge, WorkspaceModuleRuntimeToolProvider,
};
use anyhow::Result;
use async_trait::async_trait;

fn lifecycle_platform_config(
    platform_config: &SharedPlatformConfig,
) -> agentdash_application_lifecycle::SharedPlatformConfig {
    Arc::new(agentdash_application_lifecycle::PlatformConfig {
        mcp_base_url: platform_config.mcp_base_url.clone(),
    })
}

use crate::relay::registry::BackendRegistry;

#[derive(Clone)]
struct ApplicationWorkspaceModuleAgentRunBridge {
    inner: SharedSessionToolServicesHandle,
}

#[derive(Clone)]
struct RuntimeShellTerminalRegistry {
    terminal_registry: Arc<AgentRunTerminalRegistry>,
}

impl RuntimeShellTerminalRegistry {
    fn new(terminal_registry: Arc<AgentRunTerminalRegistry>) -> Self {
        Self { terminal_registry }
    }
}

impl ShellTerminalRegistry for RuntimeShellTerminalRegistry {
    fn register_shell_terminal(&self, registration: ShellTerminalRegistration) {
        // Resolve AgentRun scope from session binding or from explicit fields
        let agent_run_key = registration
            .run_id
            .as_deref()
            .zip(registration.agent_id.as_deref())
            .map(|(r, a)| (r.to_string(), a.to_string()))
            .or_else(|| {
                self.terminal_registry
                    .resolve_agent_run_for_session(&registration.session_id)
                    .map(|k| (k.run_id, k.agent_id))
            });

        if let Some((run_id, agent_id)) = agent_run_key {
            self.terminal_registry.register_terminal_with_metadata(
                &run_id,
                &agent_id,
                &registration.terminal_id,
                &registration.backend_id,
                None,
                Some(&registration.mount_id),
                Some(&registration.cwd),
                Some(&registration.capability),
            );
        }
    }

    fn resolve_shell_terminal(&self, terminal_id: &str) -> Option<ShellTerminalRegistration> {
        let state = self.terminal_registry.get_terminal(terminal_id)?;
        Some(ShellTerminalRegistration {
            session_id: String::new(), // no longer primary key; kept for structural compat
            terminal_id: state.terminal_id,
            mount_id: state.mount_id?,
            backend_id: state.backend_id,
            cwd: state.cwd.unwrap_or_default(),
            capability: state.capability.unwrap_or_else(|| "state_only".to_string()),
            run_id: Some(state.run_id),
            agent_id: Some(state.agent_id),
        })
    }

    fn remove_shell_terminal(&self, terminal_id: &str) {
        self.terminal_registry.remove_terminal(terminal_id);
    }
}

#[async_trait]
impl WorkspaceModuleAgentRunBridge for ApplicationWorkspaceModuleAgentRunBridge {
    async fn effective_capability_view_for_agent_run_delivery(
        &self,
        delivery_runtime_session_id: &str,
    ) -> Result<PortsAgentRunEffectiveCapabilityView, String> {
        let services = self
            .inner
            .get()
            .await
            .ok_or_else(|| "AgentRun bridge adapter services 尚未完成初始化".to_string())?;
        services
            .runtime_surface_update
            .effective_capability_view_for_delivery_runtime(delivery_runtime_session_id)
            .await
            .map(convert_effective_capability_view)
    }

    async fn apply_canvas_runtime_surface_update_to_agent_run(
        &self,
        delivery_runtime_session_id: &str,
        canvas: &Canvas,
        current_user: Option<&ProjectAuthorizationContext>,
        request: agentdash_application_ports::agent_frame_materialization::RuntimeSurfaceUpdateRequest,
    ) -> Result<RuntimeVfsState, String> {
        let services = self
            .inner
            .get()
            .await
            .ok_or_else(|| "AgentRun bridge adapter services 尚未完成初始化".to_string())?;
        services
            .runtime_surface_update
            .apply_canvas_runtime_surface_update(
                delivery_runtime_session_id,
                canvas,
                current_user,
                request,
            )
            .await
    }

    async fn inject_agent_run_notification(
        &self,
        delivery_runtime_session_id: &str,
        notification: agentdash_agent_protocol::BackboneEnvelope,
    ) -> Result<(), String> {
        let services = self
            .inner
            .get()
            .await
            .ok_or_else(|| "AgentRun bridge adapter services 尚未完成初始化".to_string())?;
        services
            .eventing
            .inject_notification(delivery_runtime_session_id, notification)
            .await
            .map_err(|error| error.to_string())
    }
}

fn convert_effective_capability_view(
    view: ApplicationAgentRunEffectiveCapabilityView,
) -> PortsAgentRunEffectiveCapabilityView {
    view.into()
}

pub(crate) struct SessionBootstrapInput {
    pub repos: RepositorySet,
    pub session_stores: SessionStoreSet,
    pub tool_result_cache: Arc<SessionToolResultCache>,
    pub backend_registry: Arc<BackendRegistry>,
    pub vfs_service: Arc<VfsService>,
    pub vfs_materialization_service: Arc<VfsMaterializationService>,
    pub shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    pub terminal_registry: Arc<AgentRunTerminalRegistry>,
    pub mcp_tool_discovery: Arc<dyn agentdash_application_ports::mcp_discovery::McpToolDiscovery>,
    pub function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    pub platform_config: SharedPlatformConfig,
    pub integration_connectors: Vec<Arc<dyn AgentConnector>>,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
    pub memory_discovery_providers: Vec<Arc<dyn agentdash_spi::MemoryDiscoveryProvider>>,
    pub llm_provider_secret: Arc<dyn LlmSecretCodec>,
    pub agent_run_list_invalidation: Option<Arc<dyn AgentRunListInvalidationPort>>,
}

pub(crate) struct SessionBootstrapOutput {
    pub session_runtime_builder: SessionRuntimeBuilder,
    pub session_core: SessionCoreService,
    pub session_branching: SessionBranchingService,
    pub session_eventing: SessionEventingService,
    pub session_runtime: SessionRuntimeService,
    pub session_control: SessionControlService,
    pub session_launch: SessionLaunchService,
    pub session_hooks: SessionHookService,
    pub session_runtime_transition: SessionRuntimeTransitionService,
    pub runtime_surface_update: AgentRunRuntimeSurfaceUpdateService,
    pub agent_run_control_effects: AgentRunControlEffectService,
    pub session_title: SessionTitleService,
    pub connector: Arc<dyn AgentConnector>,
    pub hook_provider: Arc<AppExecutionHookProvider>,
    pub workspace_module_runtime_gateway_handle: SharedWorkspaceModuleRuntimeGatewayHandle,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
    pub memory_discovery_providers: Vec<Arc<dyn agentdash_spi::MemoryDiscoveryProvider>>,
}

pub(crate) async fn build_session_runtime(
    input: SessionBootstrapInput,
) -> Result<SessionBootstrapOutput> {
    let SessionBootstrapInput {
        repos,
        session_stores,
        tool_result_cache,
        backend_registry,
        vfs_service,
        vfs_materialization_service,
        shell_output_registry,
        terminal_registry,
        mcp_tool_discovery,
        function_runner,
        platform_config,
        integration_connectors,
        extra_skill_dirs,
        skill_discovery_providers,
        memory_discovery_providers,
        llm_provider_secret,
        agent_run_list_invalidation,
    } = input;

    let mut sub_connectors: Vec<Arc<dyn AgentConnector>> = Vec::new();
    let mut base_system_prompt: Option<String> = None;

    if let Some(result) = build_pi_agent_connector(PiAgentConnectorDeps {
        settings_repo: repos.settings_repo.clone(),
        llm_provider_repo: repos.llm_provider_repo.clone(),
        llm_provider_credential_repo: repos.llm_provider_credential_repo.clone(),
        llm_provider_secret: llm_provider_secret.clone(),
        tool_result_cache: tool_result_cache.clone(),
    })
    .await
    {
        base_system_prompt = Some(result.connector.base_system_prompt().to_string());
        sub_connectors.push(Arc::new(result.connector));
    }

    let relay_transport: Arc<
        dyn agentdash_application_ports::backend_transport::RelayPromptTransport,
    > = backend_registry.clone();
    sub_connectors.push(Arc::new(
        agentdash_application::relay_connector::RelayAgentConnector::new(
            relay_transport.clone(),
            repos.backend_execution_lease_repo.clone(),
        ),
    ));

    sub_connectors.extend(integration_connectors);
    crate::integrations::validate_connector_executor_ids(&sub_connectors)
        .map_err(|err| anyhow::anyhow!("连接器注册失败: {err}"))?;

    let connector: Arc<dyn AgentConnector> = Arc::new(CompositeConnector::new(sub_connectors));
    let session_services_handle = SharedSessionToolServicesHandle::default();
    let workspace_module_agent_run_bridge_handle =
        SharedWorkspaceModuleAgentRunBridgeHandle::default();
    let workspace_module_runtime_gateway_handle =
        SharedWorkspaceModuleRuntimeGatewayHandle::default();
    let runtime_tool_provider =
        build_session_runtime_tool_composer(SessionRuntimeToolComposerDeps {
            repos: repos.clone(),
            vfs_service: vfs_service.clone(),
            vfs_materialization_service,
            shell_output_registry,
            terminal_registry,
            session_services_handle: session_services_handle.clone(),
            workspace_module_agent_run_bridge_handle: workspace_module_agent_run_bridge_handle
                .clone(),
            workspace_module_runtime_gateway_handle: workspace_module_runtime_gateway_handle
                .clone(),
            backend_registry: backend_registry.clone(),
            function_runner: function_runner.clone(),
            platform_config: platform_config.clone(),
            llm_provider_secret: llm_provider_secret.clone(),
        });
    let hook_preset_scripts = AppExecutionHookProvider::builtin_preset_scripts();
    let hook_provider = Arc::new(AppExecutionHookProvider::new(
        AppExecutionHookProviderDeps {
            workflow_projection: repos.hook_workflow_projection_port(),
            script_evaluator: Arc::new(agentdash_infrastructure::RhaiHookScriptEvaluator::new(
                &hook_preset_scripts,
            )),
        },
    ));
    let runtime_surface_query_impl = Arc::new(AgentRunRuntimeSurfaceQuery::new(
        AgentRunRuntimeSurfaceQueryDeps {
            anchor_repo: repos.execution_anchor_repo.clone(),
            run_repo: repos.lifecycle_run_repo.clone(),
            agent_repo: repos.lifecycle_agent_repo.clone(),
            frame_repo: repos.agent_frame_repo.clone(),
            delivery_binding_repo: repos.agent_run_delivery_binding_repo.clone(),
            permission_grant_repo: repos.permission_grant_repo.clone(),
        },
    ));
    let session_runtime_surface_query: Arc<dyn PortsAgentRunRuntimeSurfaceQueryPort> =
        runtime_surface_query_impl.clone();
    let runtime_surface_update_query: Arc<dyn ApplicationAgentRunRuntimeSurfaceQueryPort> =
        runtime_surface_query_impl.clone();
    let effective_capability_port = runtime_session_effective_capability_port(
        repos.execution_anchor_repo.clone(),
        repos.agent_frame_repo.clone(),
        repos.permission_grant_repo.clone(),
    );
    let agent_run_capability_port = agent_run_effective_capability_port(
        repos.execution_anchor_repo.clone(),
        repos.agent_frame_repo.clone(),
        repos.permission_grant_repo.clone(),
    );

    let control_effect_store = session_stores.control_effects.clone();
    let mut session_runtime_builder = SessionRuntimeBuilder::new_with_hooks_and_stores(
        connector.clone(),
        Some(hook_provider.clone()),
        session_stores,
    )
    .with_vfs_service(vfs_service.clone())
    .with_extra_skill_dirs(extra_skill_dirs.clone())
    .with_skill_discovery_providers(skill_discovery_providers.clone())
    .with_runtime_tool_provider(runtime_tool_provider)
    .with_mcp_tool_discovery(mcp_tool_discovery)
    .with_backend_execution_placement(relay_transport, repos.backend_execution_lease_repo.clone())
    .with_agent_frame_repo(repos.agent_frame_repo.clone())
    .with_execution_anchor_repo(repos.execution_anchor_repo.clone())
    .with_runtime_surface_query(session_runtime_surface_query)
    .with_agent_run_effective_capability_port(agent_run_capability_port)
    .with_lifecycle_agent_repo(repos.lifecycle_agent_repo.clone())
    .with_permission_grant_repo(repos.permission_grant_repo.clone())
    .with_effective_capability_port(effective_capability_port)
    .with_hook_target_port(hook_target_runtime_port())
    .with_lifecycle_gate_repo(repos.lifecycle_gate_repo.clone())
    .with_settings_repository(repos.settings_repo.clone())
    .with_workspace_title_port(Arc::new(
        AgentRunWorkspaceTitleAdapter::new(
            repos.execution_anchor_repo.clone(),
            repos.lifecycle_agent_repo.clone(),
        )
        .with_agent_run_list_invalidation(agent_run_list_invalidation.clone()),
    ));
    if let Some(base_sp) = base_system_prompt {
        session_runtime_builder = session_runtime_builder.with_system_prompt_config(base_sp);
    }

    let session_core = session_runtime_builder.core_service();
    let session_branching = session_runtime_builder.branching_service();
    let session_eventing = session_runtime_builder.eventing_service();
    let session_runtime = session_runtime_builder.runtime_service();
    let session_control = session_runtime_builder.control_service();
    let session_launch = session_runtime_builder.launch_service();
    let session_hooks = session_runtime_builder.hook_service();
    let session_runtime_transition = session_runtime_builder.runtime_transition_service();
    let accepted_launch_hook_sync: Arc<dyn AcceptedLaunchHookRuntimeSync> =
        Arc::new(session_hooks.clone());
    session_runtime_builder
        .set_accepted_launch_commit_port(accepted_launch_commit_port(
            Some(repos.agent_frame_repo.clone()),
            Some(repos.execution_anchor_repo.clone()),
            Some(repos.agent_run_delivery_binding_repo.clone()),
            Some(repos.lifecycle_agent_repo.clone()),
            Some(
                agentdash_application_lifecycle::accepted_turn_lifecycle_advance_port(
                    repos.execution_anchor_repo.clone(),
                    repos.lifecycle_run_repo.clone(),
                ),
            ),
            Some(accepted_launch_hook_sync),
            agent_run_list_invalidation.clone(),
        ))
        .await;
    let mailbox_runtime_adapter =
        AgentRunMailboxRuntimeAdapter::new(AgentRunMailboxRuntimeBoundaryDeps {
            lifecycle_run_repo: repos.lifecycle_run_repo.clone(),
            lifecycle_agent_repo: repos.lifecycle_agent_repo.clone(),
            project_agent_repo: repos.project_agent_repo.clone(),
            agent_frame_repo: repos.agent_frame_repo.clone(),
            execution_anchor_repo: repos.execution_anchor_repo.clone(),
            delivery_binding_repo: repos.agent_run_delivery_binding_repo.clone(),
            project_backend_access_repo: repos.project_backend_access_repo.clone(),
            command_receipt_repo: repos.agent_run_command_receipt_repo.clone(),
            mailbox_repo: repos.agent_run_mailbox_repo.clone(),
            session_core: agent_run_session_core(session_core.clone()),
            session_control: agent_run_session_control(session_control.clone()),
            session_eventing: agent_run_session_eventing(session_eventing.clone()),
            session_launch: Arc::new(agent_run_session_launch(session_launch.clone())),
        });
    session_runtime_builder
        .set_mailbox_runtime_port(Arc::new(mailbox_runtime_adapter.clone()))
        .await;
    let runtime_surface_update =
        AgentRunRuntimeSurfaceUpdateService::new(AgentRunRuntimeSurfaceUpdateDeps {
            surface_query: runtime_surface_update_query,
            frame_repo: repos.agent_frame_repo.clone(),
            vfs_service: Some(vfs_service.clone()),
            active_adopter: session_runtime_builder.runtime_surface_adoption_port(),
            extra_skill_dirs: extra_skill_dirs.clone(),
            skill_discovery_providers: skill_discovery_providers.clone(),
        });
    let session_title = session_runtime_builder.title_service();

    let orchestrator = Arc::new(
        agentdash_application_lifecycle::LifecycleOrchestrator::new_with_platform_config(
            repos.lifecycle_orchestrator_deps(),
            lifecycle_platform_config(&platform_config),
        )
        .with_function_runner(function_runner),
    );

    let wait_producer_terminal_port = Arc::new(ApiWaitProducerTerminalConvergenceAdapter::new(
        repos.clone(),
        agent_run_session_core(session_core.clone()),
        agent_run_session_control(session_control.clone()),
        agent_run_session_eventing(session_eventing.clone()),
        agent_run_session_launch(session_launch.clone()),
    ));
    let lifecycle_terminal_port = Arc::new(ApiLifecycleTerminalConvergenceAdapter::new(
        orchestrator.clone(),
    ));
    let agent_run_control_effects = AgentRunControlEffectService::new(AgentRunControlEffectDeps {
        control_effect_store,
        terminal_convergence_deps: AgentRunTerminalConvergenceDeps {
            lifecycle_run_repo: repos.lifecycle_run_repo.clone(),
            lifecycle_agent_repo: repos.lifecycle_agent_repo.clone(),
            project_agent_repo: repos.project_agent_repo.clone(),
            agent_frame_repo: repos.agent_frame_repo.clone(),
            execution_anchor_repo: repos.execution_anchor_repo.clone(),
            delivery_binding_repo: repos.agent_run_delivery_binding_repo.clone(),
            project_backend_access_repo: repos.project_backend_access_repo.clone(),
            command_receipt_repo: repos.agent_run_command_receipt_repo.clone(),
            mailbox_repo: repos.agent_run_mailbox_repo.clone(),
            agent_run_list_invalidation: agent_run_list_invalidation.clone(),
        },
        session_core: agent_run_session_core(session_core.clone()),
        session_control: agent_run_session_control(session_control.clone()),
        session_eventing: agent_run_session_eventing(session_eventing.clone()),
        session_launch: agent_run_session_launch(session_launch.clone()),
        mailbox_runtime: mailbox_runtime_adapter,
        terminal_hook_trigger_port: session_runtime_builder.agent_run_terminal_hook_trigger_port(),
        wait_producer_terminal_port,
        lifecycle_terminal_port,
        hook_effect_handler_registry: Arc::new(tokio::sync::RwLock::new(None)),
    });
    agent_run_control_effects
        .set_hook_effect_handler_registry(Arc::new(EmptyTerminalHookEffectHandlerRegistry))
        .await;
    session_runtime_builder
        .set_agent_run_control_effect_port(Arc::new(agent_run_control_effects.clone()))
        .await;
    session_runtime_builder
        .set_hook_effect_handler_registry(Arc::new(EmptyTerminalHookEffectHandlerRegistry))
        .await;

    session_services_handle
        .set(SessionToolServices {
            core: session_core.clone(),
            eventing: session_eventing.clone(),
            control: session_control.clone(),
            launch: session_launch.clone(),
            hooks: session_hooks.clone(),
            runtime_transition: session_runtime_transition.clone(),
            runtime_surface_update: runtime_surface_update.clone(),
        })
        .await;
    workspace_module_agent_run_bridge_handle
        .set(Arc::new(ApplicationWorkspaceModuleAgentRunBridge {
            inner: session_services_handle.clone(),
        }))
        .await;

    Ok(SessionBootstrapOutput {
        session_runtime_builder,
        session_core,
        session_branching,
        session_eventing,
        session_runtime,
        session_control,
        session_launch,
        session_hooks,
        session_runtime_transition,
        runtime_surface_update,
        agent_run_control_effects,
        session_title,
        connector,
        hook_provider,
        workspace_module_runtime_gateway_handle,
        extra_skill_dirs,
        skill_discovery_providers,
        memory_discovery_providers,
    })
}

struct SessionRuntimeToolComposerDeps {
    repos: RepositorySet,
    vfs_service: Arc<VfsService>,
    vfs_materialization_service: Arc<VfsMaterializationService>,
    shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    terminal_registry: Arc<AgentRunTerminalRegistry>,
    session_services_handle: SharedSessionToolServicesHandle,
    workspace_module_agent_run_bridge_handle: SharedWorkspaceModuleAgentRunBridgeHandle,
    workspace_module_runtime_gateway_handle: SharedWorkspaceModuleRuntimeGatewayHandle,
    backend_registry: Arc<BackendRegistry>,
    function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    platform_config: SharedPlatformConfig,
    llm_provider_secret: Arc<dyn LlmSecretCodec>,
}

#[derive(Clone)]
struct EffectiveProviderCompanionModelPreflight {
    repos: RepositorySet,
    llm_provider_secret: Arc<dyn LlmSecretCodec>,
}

impl EffectiveProviderCompanionModelPreflight {
    fn new(repos: RepositorySet, llm_provider_secret: Arc<dyn LlmSecretCodec>) -> Self {
        Self {
            repos,
            llm_provider_secret,
        }
    }
}

#[async_trait]
impl CompanionModelPreflightPort for EffectiveProviderCompanionModelPreflight {
    async fn preflight_companion_model(
        &self,
        request: CompanionModelPreflightRequest,
    ) -> Result<(), CompanionModelPreflightError> {
        let provider_id =
            normalize_model_selector_value(request.executor_config.provider_id.as_deref());
        let model_id = normalize_model_selector_value(request.executor_config.model_id.as_deref());
        let catalog = build_effective_profile_catalog_from_db(
            self.repos.llm_provider_repo.as_ref(),
            Some(self.repos.llm_provider_credential_repo.as_ref()),
            self.llm_provider_secret.as_ref(),
            request.identity.as_ref(),
        )
        .await;

        preflight_effective_model_selection(
            &catalog.providers,
            provider_id.as_deref(),
            model_id.as_deref(),
        )
        .map_err(|reason| {
            CompanionModelPreflightError::new(format_companion_model_preflight_error(
                &request,
                provider_id.as_deref(),
                model_id.as_deref(),
                &reason,
            ))
        })
    }
}

fn normalize_model_selector_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn format_companion_model_preflight_error(
    request: &CompanionModelPreflightRequest,
    provider_id: Option<&str>,
    model_id: Option<&str>,
    reason: &str,
) -> String {
    format!(
        "SubAgent dispatch model preflight 失败：{reason}。companion_label=`{}`, agent_key=`{}`, project_id={}, project_agent_id={}, parent_run_id={}, parent_agent_id={}, provider_id={}, model_id={}",
        request.companion_label,
        request.selected_agent_key,
        request.project_id,
        request.selected_project_agent_id,
        request.parent_run_id,
        request.parent_agent_id,
        provider_id.unwrap_or("(missing)"),
        model_id.unwrap_or("(missing)")
    )
}

fn build_session_runtime_tool_composer(
    deps: SessionRuntimeToolComposerDeps,
) -> Arc<dyn RuntimeToolProvider> {
    let inline_persister: Arc<
        dyn agentdash_application_vfs::inline_persistence::InlineContentPersister,
    > = Arc::new(
        agentdash_application_vfs::inline_persistence::DbInlineContentPersister::new(
            deps.repos.inline_file_repo.clone(),
        ),
    );

    let terminal_registry = deps.terminal_registry.clone();
    let wait_service = WaitActivityService::new(WaitActivityDeps {
        repositories: deps.repos.wait_activity_repositories(),
        terminal_registry: terminal_registry.clone(),
    });
    let vfs_provider = VfsRuntimeToolProvider::new(deps.vfs_service, Some(inline_persister))
        .with_materialization_service(deps.vfs_materialization_service)
        .with_shell_output_registry(deps.shell_output_registry)
        .with_terminal_registry(Arc::new(RuntimeShellTerminalRegistry::new(
            deps.terminal_registry,
        )));
    let workflow_provider = WorkflowRuntimeToolProvider::new(
        deps.repos.lifecycle_orchestrator_deps(),
        agentdash_application_lifecycle::lifecycle::tools::SharedSessionToolServicesHandle,
        lifecycle_platform_config(&deps.platform_config),
        deps.function_runner,
    );
    let collaboration_provider = CollaborationRuntimeToolProvider::new(
        deps.repos.clone(),
        deps.session_services_handle.clone(),
    )
    .with_wait_service(wait_service.clone())
    .with_model_preflight(Arc::new(EffectiveProviderCompanionModelPreflight::new(
        deps.repos.clone(),
        deps.llm_provider_secret.clone(),
    )));
    let task_provider = TaskRuntimeToolProvider::new(deps.repos.clone());
    let wait_provider = WaitRuntimeToolProvider::from_service(wait_service);
    let workspace_module_provider = WorkspaceModuleRuntimeToolProvider::new(
        deps.repos.project_extension_installation_repo.clone(),
        deps.repos.project_repo.clone(),
        deps.repos.canvas_repo.clone(),
        deps.repos.canvas_runtime_state_repo.clone(),
        deps.repos.execution_anchor_repo.clone(),
        deps.workspace_module_agent_run_bridge_handle,
        deps.workspace_module_runtime_gateway_handle,
    )
    .with_extension_channel_transport(deps.backend_registry.clone())
    .with_extension_backend_service_transport(deps.backend_registry);

    Arc::new(SessionRuntimeToolComposer::new(vec![
        Arc::new(vfs_provider) as Arc<dyn RuntimeToolProvider>,
        Arc::new(workflow_provider) as Arc<dyn RuntimeToolProvider>,
        Arc::new(collaboration_provider) as Arc<dyn RuntimeToolProvider>,
        Arc::new(task_provider) as Arc<dyn RuntimeToolProvider>,
        Arc::new(wait_provider) as Arc<dyn RuntimeToolProvider>,
        Arc::new(workspace_module_provider) as Arc<dyn RuntimeToolProvider>,
    ]))
}

struct PiAgentConnectorDeps {
    settings_repo: Arc<dyn SettingsRepository>,
    llm_provider_repo: Arc<dyn LlmProviderRepository>,
    llm_provider_credential_repo: Arc<dyn LlmProviderCredentialRepository>,
    llm_provider_secret: Arc<dyn LlmSecretCodec>,
    tool_result_cache: Arc<SessionToolResultCache>,
}

struct PiAgentConnectorBuildResult {
    connector: agentdash_executor::connectors::pi_agent::PiAgentConnector,
}

async fn build_pi_agent_connector(
    deps: PiAgentConnectorDeps,
) -> Option<PiAgentConnectorBuildResult> {
    let mut connector = agentdash_executor::connectors::pi_agent::build_pi_agent_connector(
        deps.settings_repo.as_ref(),
        deps.llm_provider_repo.as_ref(),
        deps.llm_provider_credential_repo.as_ref(),
        deps.llm_provider_secret.as_ref(),
    )
    .await?;
    connector.set_settings_repository(deps.settings_repo);
    connector.set_llm_provider_repository(deps.llm_provider_repo);
    connector.set_llm_provider_credential_repository(deps.llm_provider_credential_repo);
    connector.set_llm_secret_codec(deps.llm_provider_secret);
    let cache = deps.tool_result_cache;
    connector.set_tool_result_cache_writer(Some(Arc::new(move |write| {
        let expected_lifecycle_path = write.lifecycle_path.clone();
        let metadata = cache.put_text_entry(SessionToolResultCachePut {
            session_id: write.session_id,
            item_id: write.item_id,
            lifecycle_path: write.lifecycle_path,
            turn_alias: write.turn_alias,
            body_alias: write.body_alias,
            body_kind: write.body_kind,
            raw_turn_id: write.raw_turn_id,
            raw_tool_call_id: write.raw_tool_call_id,
            tool_name: write.tool_name,
            text: write.text,
            original_bytes: write.original_bytes,
        });
        debug_assert_eq!(metadata.lifecycle_path, expected_lifecycle_path);
    })));
    Some(PiAgentConnectorBuildResult { connector })
}
