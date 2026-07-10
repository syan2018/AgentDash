use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::integrations::{builtin_integrations, collect_integration_registration};
use crate::project_projection_notification::ProjectProjectionNotificationPublisher;
use crate::relay::registry::BackendRegistry;
use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::context::{
    InMemoryContextAuditBus, SharedContextAuditBus, VfsDiscoveryRegistry,
};
use agentdash_application::platform_config::{PlatformConfig, SharedPlatformConfig};
pub use agentdash_application::repository_set::RepositorySet;
use agentdash_application::routine::RoutineExecutor;
use agentdash_application::scheduling::CronSchedulerHandle;
use agentdash_application::vfs_surface_resolver::{VfsSurfaceResolver, VfsSurfaceResolverDeps};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductDeliveryPort, AgentRunRuntime, BusinessFrameSurfaceQuery,
    BusinessFrameSurfaceQueryDeps, BusinessResourceSurfaceQuery, BusinessResourceSurfaceQueryDeps,
    ManagedAgentRunRuntime, RuntimeAgentRunMailbox,
};
use agentdash_application_hooks::AppExecutionHookProvider;
use agentdash_application_lifecycle::AgentRunLifecycleSurfaceProjector;
use agentdash_application_lifecycle::run_view_builder::LifecycleReadModelQueryAdapter;
use agentdash_application_ports::agent_run_surface::{
    AgentRunEffectiveCapabilityPort, AgentRunResourceSurfaceQueryPort,
    AgentRunRuntimeSurfaceQueryPort,
};
use agentdash_application_ports::lifecycle_read_model::LifecycleReadModelQueryPort;
use agentdash_application_runtime_gateway::{
    CurrentSurfaceRuntimeMcpAccess, ExtensionRuntimeProtocolInvoker, RuntimeGateway,
};
use agentdash_application_vfs::MountProviderRegistry;
use agentdash_application_vfs::{VfsMutationDispatcher, VfsService};
use agentdash_contracts::project::ProjectEventStreamEnvelope;
use agentdash_diagnostics::DiagnosticBuffer;
use agentdash_domain::llm_provider::LlmSecretCodec;
use agentdash_integration_api::AgentDashIntegration;
use agentdash_integration_api::AuthMode;
use agentdash_integration_api::MarketplaceSourceProvider;
use agentdash_integration_api::MemoryDiscoveryProvider;
use agentdash_integration_api::SkillDiscoveryProvider;
use agentdash_spi::extension_package::ExtensionPackageArtifactStorage;

const BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY: usize = 256;
const PROJECT_CONTROL_PLANE_EVENT_CHANNEL_CAPACITY: usize = 256;
const PLATFORM_MCP_BASE_URL_ENV: &str = "AGENTDASH_MCP_BASE_URL";

fn configured_platform_mcp_base_url() -> Option<String> {
    resolve_platform_mcp_base_url(std::env::var(PLATFORM_MCP_BASE_URL_ENV).ok())
}

fn resolve_platform_mcp_base_url(raw_value: Option<String>) -> Option<String> {
    raw_value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

struct RejectUndeclaredRuntimeCredentials;

#[async_trait::async_trait]
impl agentdash_integration_api::AgentRuntimeCredentialBroker
    for RejectUndeclaredRuntimeCredentials
{
    async fn resolve(
        &self,
        slot: &agentdash_integration_api::AgentRuntimeCredentialSlot,
        _reference: &agentdash_integration_api::AgentRuntimeCredentialRef,
        _purpose: &str,
    ) -> Result<
        agentdash_integration_api::CredentialLease,
        agentdash_integration_api::CredentialResolveError,
    > {
        Err(
            agentdash_integration_api::CredentialResolveError::Unavailable {
                slot: slot.clone(),
                reason: "no Host Integration registered a credential resolver for this slot"
                    .to_string(),
            },
        )
    }
}

/// 应用服务集合 — 执行引擎、连接器与各类注册表
pub struct ServiceSet {
    pub agent_run_runtime: Arc<dyn AgentRunRuntime>,
    pub agent_run_product_delivery: Arc<dyn AgentRunProductDeliveryPort>,
    pub agent_runtime_host: Arc<agentdash_agent_runtime_host::IntegrationDriverHost>,
    pub agent_runtime_inventory: Arc<crate::relay::CloudRemoteRuntimeInventory>,
    pub runtime_surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    pub lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort>,
    pub resource_surface_query: Arc<dyn AgentRunResourceSurfaceQueryPort>,
    pub vfs_surface_resolver: VfsSurfaceResolver,
    /// 统一 VFS 访问服务 — 供 declared sources、runtime tools、workspace browse 共享
    pub vfs_service: Arc<VfsService>,
    /// VFS 写入分发器 — 统一 surface/tool mutation 与 inline_fs storage 坐标解析。
    pub vfs_mutation_dispatcher: Arc<VfsMutationDispatcher>,
    /// Host Integration 额外 skill 目录 — frame construction 阶段统一 discovery 后进入 session capabilities。
    pub extra_skill_dirs: Vec<std::path::PathBuf>,
    /// Host Integration 动态 skill discovery providers — frame construction 阶段统一聚合。
    pub skill_discovery_providers: Vec<Arc<dyn SkillDiscoveryProvider>>,
    /// Host Integration 动态 memory discovery providers — 启动期统一聚合，供 frame construction 消费。
    pub memory_discovery_providers: Vec<Arc<dyn MemoryDiscoveryProvider>>,
    /// Host Integration Marketplace Source providers — 后续 external marketplace API 统一从这里读取来源。
    pub marketplace_source_providers: Vec<Arc<dyn MarketplaceSourceProvider>>,
    /// WebSocket 中继后端注册表 — 跟踪在线的本机后端
    pub backend_registry: Arc<BackendRegistry>,
    /// Backend runtime 在线/离线/能力变化事件 — 供全局事件流驱动前端刷新
    pub backend_runtime_events: broadcast::Sender<String>,
    /// Project scoped control-plane projection invalidation — 供 AgentRun list 等投影刷新
    pub project_control_plane_events: broadcast::Sender<ProjectEventStreamEnvelope>,
    /// 串行 Shell 流式输出路由 — ShellExecTool 注册，ws_handler 投递
    pub shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    /// AgentRun scope 终端运行时状态注册表
    pub terminal_registry: Arc<agentdash_application_agentrun::agent_run::AgentRunTerminalRegistry>,
    /// 寻址空间注册表 — 持有可用的资源引用能力提供者
    pub vfs_registry: VfsDiscoveryRegistry,
    /// Mount 级 I/O 提供者注册表（`inline_fs` / `relay_fs` 等）
    pub mount_provider_registry: Arc<MountProviderRegistry>,
    /// Hook 提供者 — 供 API 层验证脚本等管理接口使用
    pub hook_provider: Arc<AppExecutionHookProvider>,
    /// 统一认证会话服务（application 层）
    pub auth_session_service: Arc<AuthSessionService>,
    /// Cron 调度器句柄 — 配置变更时调用 `notify_config_changed()` 触发热重载
    pub cron_scheduler: CronSchedulerHandle,
    /// Routine 执行器 — 统一处理定时/Webhook/Host Integration 触发
    pub routine_executor: Option<Arc<RoutineExecutor>>,
    /// Session 上下文审计总线 — Bundle / Fragment 产出与消费的可观测轨迹
    pub audit_bus: SharedContextAuditBus,
    /// 统一运行时能力网关 — Session/Setup runtime action 的共享入口
    pub runtime_gateway: Arc<RuntimeGateway>,
    pub extension_runtime_protocol_invoker: Arc<ExtensionRuntimeProtocolInvoker>,
    /// Extension package archive object 存储端口 — API 只通过 application use case 消费。
    pub extension_package_artifact_storage: Arc<dyn ExtensionPackageArtifactStorage>,
    /// Workflow function/local-effect executor port — orchestration scheduler 共享。
    pub function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
}

/// 应用级配置
pub struct AppConfig {
    /// 进程级平台配置（MCP base URL 等不变量，`Arc` 共享避免逐层透传）
    pub platform_config: SharedPlatformConfig,
    /// 当前宿主配置的认证模式
    pub auth_mode: AuthMode,
}

pub struct SecretSet {
    pub llm_provider_secret: Arc<dyn LlmSecretCodec>,
}

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
/// 按职责分为 3 个子集：repos / services / config。
pub struct AppState {
    pub repos: RepositorySet,
    pub services: ServiceSet,
    pub config: AppConfig,
    pub secrets: SecretSet,
    /// 认证/授权提供者（由 Host Integration 注入，None 表示无认证）
    pub auth_provider: Option<Arc<dyn agentdash_integration_api::AuthProvider>>,
    /// 身份目录提供者（由 Host Integration 注入，None 表示仅使用本地 projection）
    pub identity_directory_provider:
        Option<Arc<dyn agentdash_integration_api::IdentityDirectoryProvider>>,
    /// 编译期受信 Agent service definition/factory 注册表。
    pub runtime_definition_registry:
        Arc<agentdash_agent_runtime_host::AgentServiceDefinitionRegistry>,
    /// 统一诊断环形缓冲句柄 — 供 `GET /api/diagnostics` 查询"近期"诊断。
    ///
    /// 仅 `agentdash-api` main 把它接进 tracing 订阅器（[`DiagnosticLayer`]）；
    /// 其它宿主（tauri/local）传入一个未接订阅器的空缓冲，查询端点返回空集，
    /// 行为与原先一致。
    pub diagnostics: DiagnosticBuffer,
}

impl AppState {
    pub async fn new(pool: PgPool) -> Result<Arc<Self>> {
        Self::new_with_integrations(pool, builtin_integrations(), DiagnosticBuffer::new(0)).await
    }

    /// 携带 Host Integration 列表构建 AppState
    ///
    /// `diagnostics` 为统一诊断环形缓冲句柄：`agentdash-api` main 传入已接进
    /// tracing 订阅器的缓冲，其它宿主传入空缓冲即可。
    ///
    /// 返回 `Arc<Self>` 以支持需要 AppState 引用的延迟装配。
    pub async fn new_with_integrations(
        pool: PgPool,
        integrations: Vec<Box<dyn AgentDashIntegration>>,
        diagnostics: DiagnosticBuffer,
    ) -> Result<Arc<Self>> {
        let integration_registration = collect_integration_registration(integrations)
            .map_err(|err| anyhow::anyhow!("Host Integration 注册失败: {err}"))?;
        let runtime_definition_registry = Arc::new(
            agentdash_agent_runtime_host::AgentServiceDefinitionRegistry::collect(
                integration_registration
                    .runtime_driver_contributions
                    .clone(),
            )
            .map_err(|err| anyhow::anyhow!("Agent Runtime Integration 注册失败: {err}"))?,
        );

        let (project_control_plane_events, _project_control_plane_rx) =
            broadcast::channel(PROJECT_CONTROL_PLANE_EVENT_CHANNEL_CAPACITY);
        let project_projection_notifications = Arc::new(
            ProjectProjectionNotificationPublisher::new(project_control_plane_events.clone()),
        );

        let runtime_pool = pool.clone();
        let runtime_provisioner_handle =
            agentdash_application_ports::agent_run_runtime::SharedAgentRunRuntimeProvisionerHandle::default();
        let repository_bootstrap = crate::bootstrap::repositories::build_repositories(
            pool,
            integration_registration.library_asset_seeds.clone(),
            Some(project_projection_notifications.clone()),
            runtime_provisioner_handle.clone(),
        )
        .await?;
        let repos = repository_bootstrap.repos;
        let auth_session_service = repository_bootstrap.auth_session_service;
        let extension_package_artifact_storage =
            repository_bootstrap.extension_package_artifact_storage;
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
        let terminal_registry = relay_bootstrap.terminal_registry;
        let function_runner: Arc<dyn agentdash_spi::FunctionRunner> =
            Arc::new(agentdash_infrastructure::DefaultFunctionRunner::new());

        let vfs_bootstrap = crate::bootstrap::vfs::build_vfs_kernel(
            repos.clone(),
            backend_registry.clone(),
            integration_registration.mount_providers,
        );
        let mount_provider_registry = vfs_bootstrap.mount_provider_registry;
        let vfs_service = vfs_bootstrap.vfs_service;
        let vfs_mutation_dispatcher = vfs_bootstrap.vfs_mutation_dispatcher;
        let vfs_materialization_service = vfs_bootstrap.vfs_materialization_service;
        let mcp_relay_provider = vfs_bootstrap.mcp_relay_provider;
        let mcp_tool_discovery: Arc<
            dyn agentdash_application_ports::mcp_discovery::McpToolDiscovery,
        > = Arc::new(agentdash_executor::mcp::ExecutorMcpToolDiscovery::new(
            Some(mcp_relay_provider.clone()),
        ));
        let extra_skill_dirs = integration_registration.extra_skill_dirs;
        let skill_discovery_providers = integration_registration.skill_discovery_providers;
        let memory_discovery_providers = integration_registration.memory_discovery_providers;
        let hook_preset_scripts = AppExecutionHookProvider::builtin_preset_scripts();
        let hook_provider = Arc::new(AppExecutionHookProvider::new(
            agentdash_application_hooks::AppExecutionHookProviderDeps {
                workflow_projection: repos.hook_workflow_projection_port(),
                script_evaluator: Arc::new(agentdash_infrastructure::RhaiHookScriptEvaluator::new(
                    &hook_preset_scripts,
                )),
            },
        ));
        let inline_persister: Arc<
            dyn agentdash_application_vfs::inline_persistence::InlineContentPersister,
        > = Arc::new(
            agentdash_application_vfs::inline_persistence::DbInlineContentPersister::new(
                repos.inline_file_repo.clone(),
            ),
        );
        let runtime_tool_provider: Arc<dyn agentdash_spi::connector::RuntimeToolProvider> =
            Arc::new(
                agentdash_application::runtime_tools::VfsRuntimeToolProvider::new(
                    vfs_service.clone(),
                    Some(inline_persister),
                )
                .with_materialization_service(vfs_materialization_service)
                .with_shell_output_registry(shell_output_registry.clone()),
            );
        let runtime_surface_query = Arc::new(BusinessFrameSurfaceQuery::new(
            BusinessFrameSurfaceQueryDeps {
                binding_repo: repos.agent_run_runtime_binding_repo.clone(),
                run_repo: repos.lifecycle_run_repo.clone(),
                agent_repo: repos.lifecycle_agent_repo.clone(),
                frame_repo: repos.agent_frame_repo.clone(),
            },
        ));
        let runtime_surface_query_port: Arc<dyn AgentRunRuntimeSurfaceQueryPort> =
            runtime_surface_query.clone();
        let agent_run_effective_capability: Arc<dyn AgentRunEffectiveCapabilityPort> =
            runtime_surface_query.clone();
        let tool_registry = Arc::new(
            crate::bootstrap::agent_runtime_surface::CompiledAgentRunToolRegistry::default(),
        );
        let surface_compiler = Arc::new(
            crate::bootstrap::agent_runtime_surface::AgentFrameNativeSurfaceCompiler::new(
                runtime_surface_query.clone(),
                repos.agent_frame_repo.clone(),
                runtime_tool_provider,
                hook_provider.clone(),
                tool_registry.clone(),
            ),
        );
        let canonical_runtime_repository = Arc::new(
            agentdash_infrastructure::PostgresRuntimeRepository::new(runtime_pool.clone()),
        );
        let canonical_runtime = Arc::new(agentdash_agent_runtime::ManagedAgentRuntime::new(
            canonical_runtime_repository.clone(),
        ));
        let tool_broker_resolver = Arc::new(
            crate::bootstrap::agent_runtime_surface::PostgresAgentRunToolBrokerResolver::new(
                runtime_pool.clone(),
                canonical_runtime_repository,
                tool_registry.clone(),
                agent_run_effective_capability,
            ),
        );
        let tool_callback: Arc<dyn agentdash_integration_api::AgentRuntimeToolCallback> = Arc::new(
            crate::bootstrap::agent_runtime::PlatformAgentRuntimeToolCallback::new(
                tool_broker_resolver,
            ),
        );
        let hook_callback: Arc<dyn agentdash_integration_api::AgentRuntimeHookCallback> = Arc::new(
            crate::bootstrap::agent_runtime_surface::CanonicalAgentRuntimeHookCallback::new(
                canonical_runtime,
                hook_provider.clone(),
                tool_registry,
            ),
        );
        let runtime_composition =
            crate::bootstrap::agent_runtime::build_native_agent_runtime_composition(
                crate::bootstrap::agent_runtime::NativeAgentRuntimeCompositionInput {
                    pool: runtime_pool,
                    provider_repository: repos.llm_provider_repo.clone(),
                    provider_credential_repository: repos.llm_provider_credential_repo.clone(),
                    secret_codec: llm_provider_secret.clone(),
                    surface_compiler,
                    credential_broker: Arc::new(RejectUndeclaredRuntimeCredentials),
                    tool_callback,
                    hook_callback,
                    remote_definitions: runtime_definition_registry.definitions(),
                    remote_trust_manifests: integration_registration.runtime_trust_manifests,
                    remote_placements: Arc::new(
                        crate::relay::CloudRuntimeWirePlacementResolver::new(
                            backend_registry.clone(),
                            64,
                        ),
                    ),
                    node_id: "agentdash-api".to_string(),
                },
            )?;
        runtime_provisioner_handle
            .set(runtime_composition.provisioner.clone())
            .map_err(|_| anyhow::anyhow!("AgentRun runtime provisioner 重复绑定"))?;
        let agent_run_runtime: Arc<dyn AgentRunRuntime> = Arc::new(ManagedAgentRunRuntime::new(
            runtime_composition.gateway.clone(),
            runtime_composition.bindings.clone(),
            runtime_composition.provisioner.clone(),
        ));
        runtime_composition
            .outbox_worker
            .clone()
            .spawn(tokio_util::sync::CancellationToken::new());
        runtime_composition
            .durable_workers
            .clone()
            .spawn(tokio_util::sync::CancellationToken::new());
        let runtime_mailbox_worker = Arc::new(RuntimeAgentRunMailbox::new(
            repos.agent_run_mailbox_repo.clone(),
            agent_run_runtime.clone(),
        ));
        let agent_run_product_delivery: Arc<dyn AgentRunProductDeliveryPort> =
            runtime_mailbox_worker.clone();
        let mailbox_recovery_worker = runtime_mailbox_worker.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                if let Err(error) = mailbox_recovery_worker.recover_pending_once().await {
                    diag!(Error, Subsystem::AgentRun,
                        error = %error,
                        "AgentRun runtime mailbox recovery failed"
                    );
                }
            }
        });
        let agent_runtime_host = runtime_composition.host;
        let agent_runtime_inventory = Arc::new(crate::relay::CloudRemoteRuntimeInventory::new(
            agent_runtime_host.clone(),
            runtime_definition_registry.clone(),
        ));
        let lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort> = Arc::new(
            LifecycleReadModelQueryAdapter::new(repos.lifecycle_read_model_repos()),
        );
        let resource_surface_query = Arc::new(BusinessResourceSurfaceQuery::new(
            BusinessResourceSurfaceQueryDeps {
                binding_repo: repos.agent_run_runtime_binding_repo.clone(),
                surface_query: runtime_surface_query_port.clone(),
                lifecycle_surface_projection: Arc::new(
                    AgentRunLifecycleSurfaceProjector::from_skill_asset_repo(
                        repos.skill_asset_repo.clone(),
                    ),
                ),
            },
        ));
        let resource_surface_query_port: Arc<dyn AgentRunResourceSurfaceQueryPort> =
            resource_surface_query.clone();
        let vfs_surface_resolver = VfsSurfaceResolver::new(VfsSurfaceResolverDeps {
            repos: repos.clone(),
            vfs_service: vfs_service.clone(),
            resource_surface_query: resource_surface_query_port,
        });
        let session_mcp_access = Arc::new(CurrentSurfaceRuntimeMcpAccess::new(
            runtime_surface_query.clone(),
            mcp_tool_discovery,
        ));
        let runtime_gateway = crate::bootstrap::runtime_gateway::build_runtime_gateway(
            mcp_probe_relay,
            repos.clone(),
            backend_registry.clone(),
            setup_action_transport,
            session_mcp_access,
            repos.project_extension_installation_repo.clone(),
            backend_registry.clone(),
        );
        let extension_runtime_protocol_invoker = Arc::new(ExtensionRuntimeProtocolInvoker::new(
            repos.project_extension_installation_repo.clone(),
            backend_registry.clone(),
        ));

        let auth_mode = crate::bootstrap::auth::validate_auth_provider_registered(
            crate::bootstrap::auth::resolve_configured_auth_mode()?,
            integration_registration.auth_provider.is_some(),
        )?;

        let vfs_registry = crate::bootstrap::vfs::build_vfs_discovery_registry(
            integration_registration.vfs_providers,
        );

        let audit_bus: SharedContextAuditBus = Arc::new(InMemoryContextAuditBus::new(2000));
        let routine_executor = Arc::new(RoutineExecutor::new(
            repos.clone(),
            backend_registry.clone(),
            agent_run_product_delivery.clone(),
        ));

        let state = Self {
            repos,
            services: ServiceSet {
                agent_run_runtime,
                agent_run_product_delivery,
                agent_runtime_host,
                agent_runtime_inventory,
                runtime_surface_query: runtime_surface_query_port,
                lifecycle_read_model_query,
                resource_surface_query,
                vfs_surface_resolver,
                vfs_service,
                vfs_mutation_dispatcher,
                extra_skill_dirs,
                skill_discovery_providers,
                memory_discovery_providers,
                marketplace_source_providers: integration_registration.marketplace_source_providers,
                backend_registry,
                backend_runtime_events,
                project_control_plane_events,
                shell_output_registry,
                terminal_registry,
                vfs_registry,
                mount_provider_registry,
                hook_provider,
                auth_session_service,
                cron_scheduler: CronSchedulerHandle::new(),
                routine_executor: Some(routine_executor),
                audit_bus,
                runtime_gateway,
                extension_runtime_protocol_invoker,
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
            runtime_definition_registry,
            diagnostics,
        };

        let mut state = Arc::new(state);

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
