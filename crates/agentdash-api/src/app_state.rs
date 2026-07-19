use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;
use tokio::sync::broadcast;

use agentdash_agent_runtime_contract::ManagedAgentRuntimeGateway;
use agentdash_agent_runtime_host::{
    CompleteAgentHookHandler, CompleteAgentToolHandler, CompleteAgentVerificationMethod,
    CompleteAgentVerificationRecord, ResolvedCompleteAgentHookCallback,
    ResolvedCompleteAgentToolCallback,
};
use agentdash_agent_service_api::{
    AgentHookDecision, AgentHostCallbackError, AgentHostCallbackErrorCode, AgentHostCallbacks,
    AgentPayloadDigest, AgentServiceInstanceId, AgentToolResult,
};
use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::context::{
    InMemoryContextAuditBus, SharedContextAuditBus, VfsDiscoveryRegistry,
};
use agentdash_application::platform_config::{PlatformConfig, SharedPlatformConfig};
pub use agentdash_application::repository_set::RepositorySet;
use agentdash_application_agentrun::agent_run::{
    AgentRunProductProjectionQueryPort, AgentRunTerminalSourceReconcilePort,
};
use agentdash_application_extension_gateway::{ExtensionGateway, ExtensionRuntimeChannelInvoker};
use agentdash_application_vfs::{MountProviderRegistry, VfsMutationDispatcher, VfsService};
use agentdash_contracts::project::ProjectEventStreamEnvelope;
use agentdash_diagnostics::DiagnosticBuffer;
use agentdash_domain::llm_provider::LlmSecretCodec;
use agentdash_infrastructure::agent_runtime_composition::AgentRunProductProjectionComposition;
use agentdash_infrastructure::{
    CompleteAgentComposition, PinnedCompleteAgentVerificationCatalog,
    PostgresAgentRunProductRuntimeBindingRepository, PostgresAgentRunTerminalProjectionStore,
    PostgresWorkspaceModulePresentationStore,
};
use agentdash_integration_api::{
    AgentDashIntegration, AuthMode, MarketplaceSourceProvider, MemoryDiscoveryProvider,
    SkillDiscoveryProvider,
};
use agentdash_platform_spi::extension_package::ExtensionPackageArtifactStorage;

use crate::integrations::{builtin_integrations, collect_integration_registration};
use crate::project_projection_notification::ProjectProjectionNotificationPublisher;
use crate::relay::{
    RelayAgentRunTerminalProjectionProducer, RelayAgentRunTerminalSourceReconcile,
    registry::BackendRegistry,
};

const BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY: usize = 256;
const PROJECT_CONTROL_PLANE_EVENT_CHANNEL_CAPACITY: usize = 256;
const PLATFORM_MCP_BASE_URL_ENV: &str = "AGENTDASH_MCP_BASE_URL";
const COMPLETE_AGENT_LEASE_DURATION_MS: u64 = 30_000;

fn configured_platform_mcp_base_url() -> Option<String> {
    resolve_platform_mcp_base_url(std::env::var(PLATFORM_MCP_BASE_URL_ENV).ok())
}

fn resolve_platform_mcp_base_url(raw_value: Option<String>) -> Option<String> {
    raw_value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

struct UnsupportedAgentNativeCallbacks;

fn unsupported_callback(kind: &str) -> AgentHostCallbackError {
    AgentHostCallbackError::new(
        AgentHostCallbackErrorCode::Unsupported,
        format!("{kind} callback has no Product handler registered"),
        false,
    )
}

#[async_trait]
impl CompleteAgentToolHandler for UnsupportedAgentNativeCallbacks {
    async fn invoke(
        &self,
        _callback: ResolvedCompleteAgentToolCallback,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        Err(unsupported_callback("tool"))
    }
}

#[async_trait]
impl CompleteAgentHookHandler for UnsupportedAgentNativeCallbacks {
    async fn invoke(
        &self,
        _callback: ResolvedCompleteAgentHookCallback,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        Err(unsupported_callback("hook"))
    }
}

fn builtin_complete_agent_verifier() -> Result<PinnedCompleteAgentVerificationCatalog> {
    let descriptor = agentdash_integration_codex::codex_complete_agent_descriptor();
    let revision = agentdash_integration_codex::CODEX_APP_SERVER_PROTOCOL_REVISION;
    Ok(PinnedCompleteAgentVerificationCatalog::new([
        CompleteAgentVerificationRecord {
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
        },
    ])?)
}

/// Application services that own live process handles or composed protocol gateways.
pub struct ServiceSet {
    pub complete_agent: Arc<CompleteAgentComposition>,
    pub managed_runtime: Arc<dyn ManagedAgentRuntimeGateway>,
    pub complete_agent_callbacks: Arc<dyn AgentHostCallbacks>,
    pub agent_run_product_projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    pub agent_run_product_runtime_bindings: Arc<PostgresAgentRunProductRuntimeBindingRepository>,
    pub workspace_module_presentations: Arc<PostgresWorkspaceModulePresentationStore>,
    pub terminal_projections: Arc<PostgresAgentRunTerminalProjectionStore>,
    pub terminal_source_reconcile: Arc<dyn AgentRunTerminalSourceReconcilePort>,
    pub terminal_projection_producer: Arc<RelayAgentRunTerminalProjectionProducer>,
    pub vfs_service: Arc<VfsService>,
    pub vfs_mutation_dispatcher: Arc<VfsMutationDispatcher>,
    pub extra_skill_dirs: Vec<std::path::PathBuf>,
    pub skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    pub memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
    pub marketplace_source_providers: Vec<Arc<dyn MarketplaceSourceProvider>>,
    pub backend_registry: Arc<BackendRegistry>,
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
    pub function_runner: Arc<dyn agentdash_platform_spi::FunctionRunner>,
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
        let _project_projection_notifications = Arc::new(
            ProjectProjectionNotificationPublisher::new(project_control_plane_events.clone()),
        );

        let repository_bootstrap = crate::bootstrap::repositories::build_repositories(
            pool.clone(),
            integration_registration.library_asset_seeds,
        )
        .await?;
        let repos = repository_bootstrap.repos;
        let auth_session_service = repository_bootstrap.auth_session_service;
        let extension_package_artifact_storage =
            repository_bootstrap.extension_package_artifact_storage;

        let relay_bootstrap =
            crate::bootstrap::relay::build_relay_runtime(BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY);
        let backend_registry = relay_bootstrap.backend_registry;
        let backend_runtime_events = relay_bootstrap.backend_runtime_events;
        let mcp_probe_relay = relay_bootstrap.mcp_probe_relay;
        let setup_action_transport = relay_bootstrap.setup_action_transport;
        let shell_output_registry = relay_bootstrap.shell_output_registry;

        let vfs_bootstrap = crate::bootstrap::vfs::build_vfs_kernel(
            repos.clone(),
            backend_registry.clone(),
            integration_registration.mount_providers,
        );
        let mount_provider_registry = vfs_bootstrap.mount_provider_registry;
        let vfs_service = vfs_bootstrap.vfs_service;
        let vfs_mutation_dispatcher = vfs_bootstrap.vfs_mutation_dispatcher;

        let complete_agent = Arc::new(CompleteAgentComposition::build(
            pool.clone(),
            Arc::new(UnsupportedAgentNativeCallbacks),
            Arc::new(UnsupportedAgentNativeCallbacks),
            Arc::new(builtin_complete_agent_verifier()?),
            format!("agentdash-api-host-{}", uuid::Uuid::new_v4()),
            format!("agentdash-api-runtime-{}", uuid::Uuid::new_v4()),
            COMPLETE_AGENT_LEASE_DURATION_MS,
        )?);
        for contribution in integration_registration
            .complete_agent_registrations
            .drain(..)
        {
            complete_agent.register_contribution(contribution).await?;
        }

        let product =
            AgentRunProductProjectionComposition::build(pool, complete_agent.runtime.clone());
        let terminal_source_reconcile: Arc<dyn AgentRunTerminalSourceReconcilePort> =
            Arc::new(RelayAgentRunTerminalSourceReconcile::new(
                backend_registry.clone(),
                product.terminals.clone(),
            ));
        let terminal_projection_producer = Arc::new(RelayAgentRunTerminalProjectionProducer::new(
            product.terminals.clone(),
            terminal_source_reconcile.clone(),
        ));

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
        let function_runner: Arc<dyn agentdash_platform_spi::FunctionRunner> =
            Arc::new(agentdash_infrastructure::DefaultFunctionRunner::new());
        let audit_bus: SharedContextAuditBus = Arc::new(InMemoryContextAuditBus::new(2000));
        let llm_provider_secret: Arc<dyn LlmSecretCodec> = Arc::new(
            agentdash_infrastructure::LlmProviderSecretCipher::from_env_or_create_default()?,
        );
        let platform_config: SharedPlatformConfig = Arc::new(PlatformConfig {
            mcp_base_url: configured_platform_mcp_base_url(),
        });
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
                managed_runtime: complete_agent.runtime.clone(),
                complete_agent,
                agent_run_product_projection: product.gateway,
                agent_run_product_runtime_bindings: product.runtime_bindings,
                workspace_module_presentations: product.workspace_presentations,
                terminal_projections: product.terminals,
                terminal_source_reconcile,
                terminal_projection_producer,
                vfs_service,
                vfs_mutation_dispatcher,
                extra_skill_dirs: integration_registration.extra_skill_dirs,
                skill_discovery_providers: integration_registration.skill_discovery_providers,
                memory_discovery_providers: integration_registration.memory_discovery_providers,
                marketplace_source_providers: integration_registration.marketplace_source_providers,
                backend_registry,
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
                function_runner,
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
    use super::resolve_platform_mcp_base_url;

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
}
