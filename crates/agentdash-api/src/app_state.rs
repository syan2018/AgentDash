use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::integrations::{builtin_integrations, collect_integration_registration};
use crate::relay::registry::BackendRegistry;
use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::context::{
    InMemoryContextAuditBus, SharedContextAuditBus, VfsDiscoveryRegistry,
};
use agentdash_application::platform_config::{PlatformConfig, SharedPlatformConfig};
pub use agentdash_application::repository_set::RepositorySet;
use agentdash_application::routine::RoutineExecutor;
use agentdash_application::runtime_session_agent_run_bridge::{
    agent_run_session_core, agent_run_session_eventing,
};
use agentdash_application::scheduling::CronSchedulerHandle;
use agentdash_application::vfs_surface_resolver::{VfsSurfaceResolver, VfsSurfaceResolverDeps};
use agentdash_application_agentrun::agent_run::runtime_surface::{
    AgentRunResourceSurfaceQuery, AgentRunResourceSurfaceQueryDeps,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunPresentationReadModelQuery, AgentRunPresentationReadModelQueryDeps,
    AgentRunRuntimeSurfaceQuery, AgentRunRuntimeSurfaceQueryDeps, AgentRunRuntimeSurfaceQueryPort,
    AgentRunRuntimeSurfaceUpdateService,
};
use agentdash_application_hooks::AppExecutionHookProvider;
use agentdash_application_lifecycle::AgentRunLifecycleSurfaceProjector;
use agentdash_application_lifecycle::run_view_builder::LifecycleReadModelQueryAdapter;
use agentdash_application_ports::agent_run_surface::AgentRunResourceSurfaceQueryPort;
use agentdash_application_ports::lifecycle_read_model::LifecycleReadModelQueryPort;
use agentdash_application_runtime_gateway::{
    CurrentSurfaceRuntimeMcpAccess, ExtensionRuntimeChannelInvoker, RuntimeGateway,
};
use agentdash_application_runtime_session::session::{
    SessionBranchingService, SessionControlService, SessionCoreService, SessionEffectsService,
    SessionEventingService, SessionHookService, SessionLaunchService, SessionRuntimeService,
    SessionRuntimeTransitionService, SessionTitleService,
};
use agentdash_application_vfs::MountProviderRegistry;
use agentdash_application_vfs::{VfsMutationDispatcher, VfsService};
use agentdash_diagnostics::DiagnosticBuffer;
use agentdash_domain::llm_provider::LlmSecretCodec;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::story::{StateChangeRepository, StoryRepository};
use agentdash_executor::AgentConnector;
use agentdash_integration_api::AgentDashIntegration;
use agentdash_integration_api::AuthMode;
use agentdash_integration_api::MarketplaceSourceProvider;
use agentdash_integration_api::MemoryDiscoveryProvider;
use agentdash_integration_api::SkillDiscoveryProvider;
use agentdash_spi::extension_package::ExtensionPackageArtifactStorage;

const BACKEND_RUNTIME_EVENT_CHANNEL_CAPACITY: usize = 256;
const PLATFORM_MCP_BASE_URL_ENV: &str = "AGENTDASH_MCP_BASE_URL";

fn configured_platform_mcp_base_url() -> Option<String> {
    resolve_platform_mcp_base_url(std::env::var(PLATFORM_MCP_BASE_URL_ENV).ok())
}

fn resolve_platform_mcp_base_url(raw_value: Option<String>) -> Option<String> {
    raw_value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 应用服务集合 — 执行引擎、连接器与各类注册表
pub struct ServiceSet {
    pub session_core: SessionCoreService,
    pub session_branching: SessionBranchingService,
    pub session_eventing: SessionEventingService,
    pub session_runtime: SessionRuntimeService,
    pub session_control: SessionControlService,
    pub session_launch: SessionLaunchService,
    pub session_hooks: SessionHookService,
    pub session_runtime_transition: SessionRuntimeTransitionService,
    pub runtime_surface_update: AgentRunRuntimeSurfaceUpdateService,
    pub runtime_surface_query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    pub lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort>,
    pub presentation_read_model_query: AgentRunPresentationReadModelQuery,
    pub resource_surface_query: AgentRunResourceSurfaceQuery,
    pub vfs_surface_resolver: VfsSurfaceResolver,
    pub session_effects: SessionEffectsService,
    pub session_title: SessionTitleService,
    /// 当前活跃的连接器实例（供 discovery 端点查询能力/类型）
    pub connector: Arc<dyn AgentConnector>,
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
    /// 串行 Shell 流式输出路由 — ShellExecTool 注册，ws_handler 投递
    pub shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    /// 交互式终端运行时状态缓存
    pub terminal_cache:
        Arc<agentdash_application_runtime_session::session::terminal_cache::SessionTerminalCache>,
    /// 寻址空间注册表 — 持有可用的资源引用能力提供者
    pub vfs_registry: VfsDiscoveryRegistry,
    /// Mount 级 I/O 提供者注册表（`inline_fs` / `relay_fs` 等）
    pub mount_provider_registry: Arc<MountProviderRegistry>,
    /// Hook 提供者 — 供 API 层验证脚本等管理接口使用
    pub hook_provider: Arc<AppExecutionHookProvider>,
    /// 统一认证会话服务（application 层）
    pub auth_session_service: Arc<AuthSessionService>,
    /// 业务终态 → session cancel 指令通道 — Story/Task 状态变更时联动 session 取消
    pub terminal_cancel_coordinator:
        Arc<agentdash_application::reconcile::terminal_cancel::TerminalCancelCoordinator>,
    /// Cron 调度器句柄 — 配置变更时调用 `notify_config_changed()` 触发热重载
    pub cron_scheduler: CronSchedulerHandle,
    /// Routine 执行器 — 统一处理定时/Webhook/Host Integration 触发
    pub routine_executor: Option<Arc<RoutineExecutor>>,
    /// Session 上下文审计总线 — Bundle / Fragment 产出与消费的可观测轨迹
    pub audit_bus: SharedContextAuditBus,
    /// 统一运行时能力网关 — Session/Setup runtime action 的共享入口
    pub runtime_gateway: Arc<RuntimeGateway>,
    pub extension_runtime_channel_invoker: Arc<ExtensionRuntimeChannelInvoker>,
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

        let repository_bootstrap = crate::bootstrap::repositories::build_repositories(
            pool,
            integration_registration.library_asset_seeds.clone(),
        )
        .await?;
        let repos = repository_bootstrap.repos;
        let auth_session_service = repository_bootstrap.auth_session_service;
        let session_persistence = repository_bootstrap.session_persistence;
        let tool_result_cache =
            agentdash_application_runtime_session::session::SessionToolResultCache::new();
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
        let terminal_cache = relay_bootstrap.terminal_cache;
        let function_runner: Arc<dyn agentdash_spi::FunctionRunner> =
            Arc::new(agentdash_infrastructure::DefaultFunctionRunner::new());

        let vfs_bootstrap = crate::bootstrap::vfs::build_vfs_kernel(
            repos.clone(),
            session_persistence.clone(),
            tool_result_cache.clone(),
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
        let session_bootstrap = crate::bootstrap::session::build_session_runtime(
            crate::bootstrap::session::SessionBootstrapInput {
                repos: repos.clone(),
                session_persistence: session_persistence.clone(),
                tool_result_cache: tool_result_cache.clone(),
                backend_registry: backend_registry.clone(),
                vfs_service: vfs_service.clone(),
                vfs_materialization_service,
                shell_output_registry: shell_output_registry.clone(),
                terminal_cache: terminal_cache.clone(),
                mcp_tool_discovery: mcp_tool_discovery.clone(),
                function_runner: function_runner.clone(),
                platform_config: platform_config.clone(),
                integration_connectors: integration_registration.connectors,
                extra_skill_dirs: integration_registration.extra_skill_dirs,
                skill_discovery_providers: integration_registration.skill_discovery_providers,
                memory_discovery_providers: integration_registration.memory_discovery_providers,
                llm_provider_secret: llm_provider_secret.clone(),
            },
        )
        .await?;
        let session_runtime_builder = session_bootstrap.session_runtime_builder;
        let session_core = session_bootstrap.session_core;
        let session_branching = session_bootstrap.session_branching;
        let session_eventing = session_bootstrap.session_eventing;
        let session_runtime = session_bootstrap.session_runtime;
        let session_control = session_bootstrap.session_control;
        let session_launch = session_bootstrap.session_launch;
        let session_hooks = session_bootstrap.session_hooks;
        let session_runtime_transition = session_bootstrap.session_runtime_transition;
        let runtime_surface_update = session_bootstrap.runtime_surface_update;
        let session_effects = session_bootstrap.session_effects;
        let session_title = session_bootstrap.session_title;
        let connector = session_bootstrap.connector;
        let hook_provider = session_bootstrap.hook_provider;
        let workspace_module_runtime_gateway_handle =
            session_bootstrap.workspace_module_runtime_gateway_handle;
        let extra_skill_dirs = session_bootstrap.extra_skill_dirs;
        let skill_discovery_providers = session_bootstrap.skill_discovery_providers;
        let memory_discovery_providers = session_bootstrap.memory_discovery_providers;

        let runtime_surface_query = Arc::new(AgentRunRuntimeSurfaceQuery::new(
            AgentRunRuntimeSurfaceQueryDeps {
                anchor_repo: repos.execution_anchor_repo.clone(),
                run_repo: repos.lifecycle_run_repo.clone(),
                agent_repo: repos.lifecycle_agent_repo.clone(),
                frame_repo: repos.agent_frame_repo.clone(),
                permission_grant_repo: repos.permission_grant_repo.clone(),
            },
        ));
        let runtime_surface_query_port: Arc<dyn AgentRunRuntimeSurfaceQueryPort> =
            runtime_surface_query.clone();
        let lifecycle_read_model_query: Arc<dyn LifecycleReadModelQueryPort> = Arc::new(
            LifecycleReadModelQueryAdapter::new(repos.to_lifecycle_repository_set()),
        );
        let resource_surface_query =
            AgentRunResourceSurfaceQuery::new(AgentRunResourceSurfaceQueryDeps {
                anchor_repo: repos.execution_anchor_repo.clone(),
                surface_query: runtime_surface_query_port.clone(),
                lifecycle_surface_projection: Arc::new(AgentRunLifecycleSurfaceProjector::new(
                    &repos.to_lifecycle_repository_set(),
                )),
            });
        let resource_surface_query_port: Arc<dyn AgentRunResourceSurfaceQueryPort> =
            Arc::new(resource_surface_query.clone());
        let vfs_surface_resolver = VfsSurfaceResolver::new(VfsSurfaceResolverDeps {
            repos: repos.clone(),
            vfs_service: vfs_service.clone(),
            resource_surface_query: resource_surface_query_port,
        });
        let presentation_read_model_query =
            AgentRunPresentationReadModelQuery::new(AgentRunPresentationReadModelQueryDeps {
                repos: repos.to_agent_run_repository_set(),
                session_core: agent_run_session_core(session_core.clone()),
                session_eventing: agent_run_session_eventing(session_eventing.clone()),
                surface_query: runtime_surface_query_port.clone(),
                lifecycle_read_model: lifecycle_read_model_query.clone(),
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
        // RuntimeGateway 装配序晚于 session runtime tool composer；此处把 gateway
        // 回填进延迟句柄，供 workspace_module_invoke。
        workspace_module_runtime_gateway_handle
            .set(runtime_gateway.clone())
            .await;
        let extension_runtime_channel_invoker = Arc::new(ExtensionRuntimeChannelInvoker::new(
            repos.project_extension_installation_repo.clone(),
            backend_registry.clone(),
        ));

        let project_repo_port: Arc<dyn ProjectRepository> = repos.project_repo.clone();
        let state_change_repo_port: Arc<dyn StateChangeRepository> =
            repos.state_change_repo.clone();
        let story_repo_port: Arc<dyn StoryRepository> = repos.story_repo.clone();

        // 启动对账管线：Session → Task → Infrastructure（有序不可跳过）
        //
        // M2-c：Task 对账改为从 LifecycleRun/step state 反投影，不再需要 session 状态读取器。
        {
            let deps = agentdash_application::reconcile::boot::BootReconcileDeps {
                session_runtime: session_runtime.clone(),
                project_repo: project_repo_port.clone(),
                state_change_repo: state_change_repo_port.clone(),
                story_repo: story_repo_port.clone(),
                lifecycle_subject_association_repo: repos
                    .lifecycle_subject_association_repo
                    .clone(),
                lifecycle_run_repo: repos.lifecycle_run_repo.clone(),
                lifecycle_agent_repo: repos.lifecycle_agent_repo.clone(),
                execution_anchor_repo: repos.execution_anchor_repo.clone(),
            };
            let report = agentdash_application::reconcile::boot::run_boot_reconcile(&deps).await;
            if report.has_errors() {
                for phase in &report.phases {
                    for err in &phase.errors {
                        diag!(Warn, Subsystem::Api,
        phase = phase.phase, error = %err, "启动对账阶段出错");
                    }
                }
            }
        }

        let auth_mode = crate::bootstrap::auth::validate_auth_provider_registered(
            crate::bootstrap::auth::resolve_configured_auth_mode()?,
            integration_registration.auth_provider.is_some(),
        )?;

        let vfs_registry = crate::bootstrap::vfs::build_vfs_discovery_registry(
            integration_registration.vfs_providers,
        );

        let terminal_cancel_coordinator = Arc::new(
            agentdash_application::reconcile::terminal_cancel::TerminalCancelCoordinator::new(
                session_runtime.clone(),
                story_repo_port.clone(),
                repos.agent_run_command_receipt_repo.clone(),
                repos.lifecycle_run_repo.clone(),
                repos.lifecycle_subject_association_repo.clone(),
                repos.lifecycle_agent_repo.clone(),
                repos.agent_frame_repo.clone(),
                repos.execution_anchor_repo.clone(),
            ),
        );

        let audit_bus: SharedContextAuditBus = Arc::new(InMemoryContextAuditBus::new(2000));
        session_runtime_builder
            .set_context_audit_bus(audit_bus.clone())
            .await;

        let state = Self {
            repos,
            services: ServiceSet {
                session_core,
                session_branching,
                session_eventing,
                session_runtime,
                session_control,
                session_launch,
                session_hooks,
                session_runtime_transition,
                runtime_surface_update,
                runtime_surface_query: runtime_surface_query_port,
                lifecycle_read_model_query,
                presentation_read_model_query,
                resource_surface_query,
                vfs_surface_resolver,
                session_effects,
                session_title,
                connector,
                vfs_service,
                vfs_mutation_dispatcher,
                extra_skill_dirs,
                skill_discovery_providers,
                memory_discovery_providers,
                marketplace_source_providers: integration_registration.marketplace_source_providers,
                backend_registry,
                backend_runtime_events,
                shell_output_registry,
                terminal_cache,
                vfs_registry,
                mount_provider_registry,
                hook_provider,
                auth_session_service,
                terminal_cancel_coordinator,
                cron_scheduler: CronSchedulerHandle::new(),
                routine_executor: None,
                audit_bus,
                runtime_gateway,
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
        };

        let mut state = Arc::new(state);

        // 注入 RuntimeSession launch provider：所有 prompt 先定位 AgentFrame，
        // 再从 frame revision 投影 connector 所需的 runtime surface。
        {
            let provider = Arc::new(
                crate::bootstrap::frame_launch_envelope_provider::AppStateFrameLaunchEnvelopePort::new(
                    state.clone(),
                ),
            );
            session_runtime_builder
                .set_frame_launch_envelope_provider(provider)
                .await;
        }

        session_runtime_builder
            .assert_ready_for_app_state()
            .await
            .map_err(|err| anyhow::anyhow!("AppState session 依赖未 ready: {err}"))?;

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
