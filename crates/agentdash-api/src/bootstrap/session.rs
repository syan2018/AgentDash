use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use agentdash_application::hooks::AppExecutionHookProvider;
use agentdash_application::platform_config::SharedPlatformConfig;
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::session::{
    SessionBranchingService, SessionCapabilityService, SessionControlService, SessionCoreService,
    SessionEffectsService, SessionEventingService, SessionHookService, SessionLaunchService,
    SessionPersistence, SessionRuntimeBuilder, SessionRuntimeService, SessionTitleService,
};
use agentdash_application::vfs::RelayVfsService;
use agentdash_application::vfs::tools::provider::{
    SessionToolServices, SharedSessionToolServicesHandle,
};
use agentdash_domain::llm_provider::{
    LlmProviderCredentialRepository, LlmProviderRepository, LlmSecretCodec,
};
use agentdash_domain::settings::SettingsRepository;
use agentdash_executor::AgentConnector;
use agentdash_executor::connectors::composite::CompositeConnector;

use crate::relay::registry::BackendRegistry;

pub(crate) struct SessionBootstrapInput {
    pub repos: RepositorySet,
    pub session_persistence: Arc<dyn SessionPersistence>,
    pub backend_registry: Arc<BackendRegistry>,
    pub vfs_service: Arc<RelayVfsService>,
    pub session_services_handle: SharedSessionToolServicesHandle,
    pub runtime_tool_provider: Arc<dyn agentdash_spi::connector::RuntimeToolProvider>,
    pub mcp_relay_provider: Arc<dyn agentdash_spi::McpRelayProvider>,
    pub platform_config: SharedPlatformConfig,
    pub plugin_connectors: Vec<Arc<dyn AgentConnector>>,
    pub extra_skill_dirs: Vec<PathBuf>,
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
    pub extra_skill_dirs: Vec<PathBuf>,
}

pub(crate) async fn build_session_runtime(
    input: SessionBootstrapInput,
) -> Result<SessionBootstrapOutput> {
    let SessionBootstrapInput {
        repos,
        session_persistence,
        backend_registry,
        vfs_service,
        session_services_handle,
        runtime_tool_provider,
        mcp_relay_provider,
        platform_config,
        plugin_connectors,
        extra_skill_dirs,
        llm_provider_secret,
    } = input;

    let mut sub_connectors: Vec<Arc<dyn AgentConnector>> = Vec::new();
    let mut prompt_config: Option<(String, Vec<String>)> = None;

    if let Some(result) = build_pi_agent_connector(PiAgentConnectorDeps {
        settings_repo: repos.settings_repo.clone(),
        llm_provider_repo: repos.llm_provider_repo.clone(),
        llm_provider_credential_repo: repos.llm_provider_credential_repo.clone(),
        llm_provider_secret: llm_provider_secret.clone(),
    })
    .await
    {
        prompt_config = Some((
            result.connector.base_system_prompt().to_string(),
            result.connector.user_preferences().to_vec(),
        ));
        sub_connectors.push(Arc::new(result.connector));
    }

    let relay_transport: Arc<dyn agentdash_application::backend_transport::RelayPromptTransport> =
        backend_registry.clone();
    sub_connectors.push(Arc::new(
        agentdash_application::relay_connector::RelayAgentConnector::new(
            relay_transport.clone(),
            repos.backend_execution_lease_repo.clone(),
        ),
    ));

    sub_connectors.extend(plugin_connectors);
    crate::plugins::validate_connector_executor_ids(&sub_connectors)
        .map_err(|err| anyhow::anyhow!("连接器注册失败: {err}"))?;

    let connector: Arc<dyn AgentConnector> = Arc::new(CompositeConnector::new(sub_connectors));
    let hook_provider = Arc::new(AppExecutionHookProvider::new(
        repos.project_repo.clone(),
        repos.story_repo.clone(),
        repos.session_binding_repo.clone(),
        repos.workflow_definition_repo.clone(),
        repos.activity_lifecycle_definition_repo.clone(),
        repos.lifecycle_run_repo.clone(),
        repos.inline_file_repo.clone(),
    ));

    let mut session_runtime_builder = SessionRuntimeBuilder::new_with_hooks_and_persistence(
        connector.clone(),
        Some(hook_provider.clone()),
        session_persistence,
    )
    .with_vfs_service(vfs_service.clone())
    .with_extra_skill_dirs(extra_skill_dirs.clone())
    .with_runtime_tool_provider(runtime_tool_provider)
    .with_mcp_relay_provider(mcp_relay_provider)
    .with_backend_execution_placement(relay_transport, repos.backend_execution_lease_repo.clone());
    if let Some((base_sp, user_prefs)) = prompt_config {
        session_runtime_builder =
            session_runtime_builder.with_system_prompt_config(base_sp, user_prefs);
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

    let orchestrator = Arc::new(agentdash_application::workflow::LifecycleOrchestrator::new(
        session_core.clone(),
        session_launch.clone(),
        session_hooks.clone(),
        session_capability.clone(),
        repos,
        platform_config,
    ));
    session_runtime_builder
        .set_terminal_callback(orchestrator)
        .await;

    session_services_handle
        .set(SessionToolServices {
            core: session_core.clone(),
            eventing: session_eventing.clone(),
            control: session_control.clone(),
            launch: session_launch.clone(),
            hooks: session_hooks.clone(),
            capability: session_capability.clone(),
            companion_wait_registry: session_runtime_builder.companion_wait_registry(),
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
        extra_skill_dirs,
    })
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
