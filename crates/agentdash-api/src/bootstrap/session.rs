use std::path::PathBuf;
use std::sync::Arc;

use crate::agent_run_mailbox::AgentRunMailboxTerminalCallback;
use agentdash_application::hooks::AppExecutionHookProvider;
use agentdash_application::platform_config::SharedPlatformConfig;
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::runtime_tools::{
    CollaborationRuntimeToolProvider, SessionRuntimeToolComposer, SessionToolServices,
    SharedRuntimeGatewayHandle, SharedSessionToolServicesHandle, TaskRuntimeToolProvider,
    VfsRuntimeToolProvider, WorkflowRuntimeToolProvider, WorkspaceModuleRuntimeToolProvider,
};
use agentdash_application::session::{
    EmptyTerminalHookEffectHandlerRegistry,
    SessionBranchingService, SessionCapabilityService, SessionControlService, SessionCoreService,
    SessionEffectsService, SessionEventingService, SessionHookService, SessionLaunchService,
    SessionPersistence, SessionRuntimeBuilder, SessionRuntimeService, SessionTerminalCallback,
    SessionTitleService,
};
use agentdash_application::vfs::VfsMaterializationService;
use agentdash_application::vfs::VfsService;
use agentdash_domain::llm_provider::{
    LlmProviderCredentialRepository, LlmProviderRepository, LlmSecretCodec,
};
use agentdash_domain::settings::SettingsRepository;
use agentdash_executor::AgentConnector;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_spi::connector::RuntimeToolProvider;
use anyhow::Result;
use async_trait::async_trait;

use crate::relay::registry::BackendRegistry;

pub(crate) struct SessionBootstrapInput {
    pub repos: RepositorySet,
    pub session_persistence: Arc<dyn SessionPersistence>,
    pub backend_registry: Arc<BackendRegistry>,
    pub vfs_service: Arc<VfsService>,
    pub vfs_materialization_service: Arc<VfsMaterializationService>,
    pub shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    pub mcp_tool_discovery: Arc<dyn agentdash_application_ports::mcp_discovery::McpToolDiscovery>,
    pub function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    pub platform_config: SharedPlatformConfig,
    pub integration_connectors: Vec<Arc<dyn AgentConnector>>,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
    pub llm_provider_secret: Arc<dyn LlmSecretCodec>,
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
    pub session_capability: SessionCapabilityService,
    pub session_effects: SessionEffectsService,
    pub session_title: SessionTitleService,
    pub connector: Arc<dyn AgentConnector>,
    pub hook_provider: Arc<AppExecutionHookProvider>,
    pub runtime_gateway_handle: SharedRuntimeGatewayHandle,
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
}

pub(crate) async fn build_session_runtime(
    input: SessionBootstrapInput,
) -> Result<SessionBootstrapOutput> {
    let SessionBootstrapInput {
        repos,
        session_persistence,
        backend_registry,
        vfs_service,
        vfs_materialization_service,
        shell_output_registry,
        mcp_tool_discovery,
        function_runner,
        platform_config,
        integration_connectors,
        extra_skill_dirs,
        skill_discovery_providers,
        llm_provider_secret,
    } = input;

    let mut sub_connectors: Vec<Arc<dyn AgentConnector>> = Vec::new();
    let mut base_system_prompt: Option<String> = None;

    if let Some(result) = build_pi_agent_connector(PiAgentConnectorDeps {
        settings_repo: repos.settings_repo.clone(),
        llm_provider_repo: repos.llm_provider_repo.clone(),
        llm_provider_credential_repo: repos.llm_provider_credential_repo.clone(),
        llm_provider_secret: llm_provider_secret.clone(),
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
    let runtime_gateway_handle = SharedRuntimeGatewayHandle::default();
    let runtime_tool_provider =
        build_session_runtime_tool_composer(SessionRuntimeToolComposerDeps {
            repos: repos.clone(),
            vfs_service: vfs_service.clone(),
            vfs_materialization_service,
            shell_output_registry,
            session_services_handle: session_services_handle.clone(),
            runtime_gateway_handle: runtime_gateway_handle.clone(),
            backend_registry: backend_registry.clone(),
            function_runner: function_runner.clone(),
            platform_config: platform_config.clone(),
        });
    let hook_provider = Arc::new(AppExecutionHookProvider::new(
        agentdash_application::hooks::AppExecutionHookProviderRepos {
            project_repo: repos.project_repo.clone(),
            story_repo: repos.story_repo.clone(),
            agent_procedure_repo: repos.agent_procedure_repo.clone(),
            agent_frame_repo: repos.agent_frame_repo.clone(),
            lifecycle_agent_repo: repos.lifecycle_agent_repo.clone(),
            lifecycle_run_repo: repos.lifecycle_run_repo.clone(),
            execution_anchor_repo: repos.execution_anchor_repo.clone(),
            lifecycle_subject_association_repo: repos.lifecycle_subject_association_repo.clone(),
            inline_file_repo: repos.inline_file_repo.clone(),
        },
        |preset_scripts| {
            Arc::new(agentdash_infrastructure::RhaiHookScriptEvaluator::new(
                preset_scripts,
            ))
        },
    ));

    let mut session_runtime_builder = SessionRuntimeBuilder::new_with_hooks_and_persistence(
        connector.clone(),
        Some(hook_provider.clone()),
        session_persistence,
    )
    .with_vfs_service(vfs_service.clone())
    .with_extra_skill_dirs(extra_skill_dirs.clone())
    .with_skill_discovery_providers(skill_discovery_providers.clone())
    .with_runtime_tool_provider(runtime_tool_provider)
    .with_mcp_tool_discovery(mcp_tool_discovery)
    .with_backend_execution_placement(relay_transport, repos.backend_execution_lease_repo.clone())
    .with_agent_frame_repo(repos.agent_frame_repo.clone())
    .with_execution_anchor_repo(repos.execution_anchor_repo.clone())
    .with_lifecycle_agent_repo(repos.lifecycle_agent_repo.clone())
    .with_agent_run_mailbox_boundary(
        repos.lifecycle_run_repo.clone(),
        repos.agent_run_command_receipt_repo.clone(),
        repos.agent_run_mailbox_repo.clone(),
    )
    .map_err(anyhow::Error::msg)?
    .with_lifecycle_gate_repo(repos.lifecycle_gate_repo.clone())
    .with_settings_repository(repos.settings_repo.clone());
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
    let session_capability = session_runtime_builder.capability_service();
    let session_effects = session_runtime_builder.effects_service();
    let session_title = session_runtime_builder.title_service();

    let orchestrator = Arc::new(
        agentdash_application::lifecycle::LifecycleOrchestrator::new_with_platform_config(
            repos.clone(),
            platform_config.clone(),
        )
        .with_function_runner(function_runner),
    );
    let mailbox_terminal_callback = Arc::new(AgentRunMailboxTerminalCallback::new(
        repos.clone(),
        session_core.clone(),
        session_control.clone(),
        session_eventing.clone(),
        session_launch.clone(),
    ));
    session_runtime_builder
        .set_terminal_callback(Arc::new(CompositeSessionTerminalCallback {
            callbacks: vec![orchestrator, mailbox_terminal_callback],
        }))
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
            capability: session_capability.clone(),
        })
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
        session_capability,
        session_effects,
        session_title,
        connector,
        hook_provider,
        runtime_gateway_handle,
        extra_skill_dirs,
        skill_discovery_providers,
    })
}

struct SessionRuntimeToolComposerDeps {
    repos: RepositorySet,
    vfs_service: Arc<VfsService>,
    vfs_materialization_service: Arc<VfsMaterializationService>,
    shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    session_services_handle: SharedSessionToolServicesHandle,
    runtime_gateway_handle: SharedRuntimeGatewayHandle,
    backend_registry: Arc<BackendRegistry>,
    function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    platform_config: SharedPlatformConfig,
}

fn build_session_runtime_tool_composer(
    deps: SessionRuntimeToolComposerDeps,
) -> Arc<dyn RuntimeToolProvider> {
    let inline_persister: Arc<
        dyn agentdash_application::vfs::inline_persistence::InlineContentPersister,
    > = Arc::new(
        agentdash_application::vfs::inline_persistence::DbInlineContentPersister::new(
            deps.repos.inline_file_repo.clone(),
        ),
    );

    let vfs_provider = VfsRuntimeToolProvider::new(deps.vfs_service, Some(inline_persister))
        .with_materialization_service(deps.vfs_materialization_service)
        .with_shell_output_registry(deps.shell_output_registry);
    let workflow_provider = WorkflowRuntimeToolProvider::new(
        deps.repos.clone(),
        deps.session_services_handle.clone(),
        deps.platform_config,
        deps.function_runner,
    );
    let collaboration_provider = CollaborationRuntimeToolProvider::new(
        deps.repos.clone(),
        deps.session_services_handle.clone(),
    );
    let task_provider = TaskRuntimeToolProvider::new(deps.repos.clone());
    let workspace_module_provider = WorkspaceModuleRuntimeToolProvider::new(
        deps.repos.project_extension_installation_repo.clone(),
        deps.repos.canvas_repo.clone(),
        deps.session_services_handle,
        deps.runtime_gateway_handle,
    )
    .with_extension_channel_transport(deps.backend_registry);

    Arc::new(SessionRuntimeToolComposer::new(vec![
        Arc::new(vfs_provider) as Arc<dyn RuntimeToolProvider>,
        Arc::new(workflow_provider) as Arc<dyn RuntimeToolProvider>,
        Arc::new(collaboration_provider) as Arc<dyn RuntimeToolProvider>,
        Arc::new(task_provider) as Arc<dyn RuntimeToolProvider>,
        Arc::new(workspace_module_provider) as Arc<dyn RuntimeToolProvider>,
    ]))
}

struct CompositeSessionTerminalCallback {
    callbacks: Vec<Arc<dyn SessionTerminalCallback>>,
}

#[async_trait]
impl SessionTerminalCallback for CompositeSessionTerminalCallback {
    async fn on_session_terminal(&self, session_id: &str, terminal_state: &str) {
        for callback in &self.callbacks {
            callback
                .on_session_terminal(session_id, terminal_state)
                .await;
        }
    }
}

struct PiAgentConnectorDeps {
    settings_repo: Arc<dyn SettingsRepository>,
    llm_provider_repo: Arc<dyn LlmProviderRepository>,
    llm_provider_credential_repo: Arc<dyn LlmProviderCredentialRepository>,
    llm_provider_secret: Arc<dyn LlmSecretCodec>,
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
    Some(PiAgentConnectorBuildResult { connector })
}
