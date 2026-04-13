use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;

use crate::bootstrap::task_state_reconcile::HubSessionStateReader;
use crate::mount_providers::RelayFsMountProvider;
use crate::plugins::{
    builtin_plugins, collect_plugin_registration, validate_connector_executor_ids,
};
use crate::relay::registry::BackendRegistry;
use agentdash_application::address_space::RelayAddressSpaceService;
use agentdash_application::address_space::tools::provider::{
    RelayRuntimeToolProvider, SharedSessionHubHandle,
};
use agentdash_application::address_space::{MountProviderRegistry, MountProviderRegistryBuilder};
use agentdash_application::auth::session_service::AuthSessionService;
use agentdash_application::context::ContextContributorRegistry;
use agentdash_application::context::{
    AddressSpaceDiscoveryRegistry, builtin_address_space_registry,
};
use agentdash_application::hooks::AppExecutionHookProvider;
pub use agentdash_application::repository_set::RepositorySet;
use agentdash_application::scheduling::CronSchedulerHandle;
use agentdash_application::session::SessionHub;
use agentdash_application::task::service::TaskLifecycleService;
use agentdash_application::task_lock::TaskLockMap;
use agentdash_application::task_restart_tracker::RestartTracker;
use agentdash_domain::llm_provider::LlmProviderRepository;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::settings::SettingsRepository;
use agentdash_domain::story::StateChangeRepository;
use agentdash_domain::task::{TaskAggregateCommandRepository, TaskRepository};
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
};
use agentdash_executor::AgentConnector;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_infrastructure::{
    PostgresAgentRepository, PostgresAuthSessionRepository, PostgresBackendRepository,
    PostgresCanvasRepository, PostgresLlmProviderRepository, PostgresProjectRepository,
    PostgresSessionBindingRepository, PostgresSessionRepository, PostgresSettingsRepository,
    PostgresStateChangeRepository, PostgresStoryRepository, PostgresTaskRepository,
    PostgresUserDirectoryRepository, PostgresWorkflowRepository, PostgresWorkspaceRepository,
};
use agentdash_plugin_api::AgentDashPlugin;
use agentdash_plugin_api::AuthMode;

/// 应用服务集合 — 执行引擎、连接器与各类注册表
pub struct ServiceSet {
    pub session_hub: SessionHub,
    /// 当前活跃的连接器实例（供 discovery 端点查询能力/类型）
    pub connector: Arc<dyn AgentConnector>,
    /// 统一 Address Space 访问服务 — 供 declared sources、runtime tools、workspace browse 共享
    pub address_space_service: Arc<RelayAddressSpaceService>,
    /// WebSocket 中继后端注册表 — 跟踪在线的本机后端
    pub backend_registry: Arc<BackendRegistry>,
    /// 上下文贡献者注册表 — 持有常驻贡献者（Core/Binding/DeclaredSources/Instruction 等）
    pub contributor_registry: Arc<ContextContributorRegistry>,
    /// 寻址空间注册表 — 持有可用的资源引用能力提供者
    pub address_space_registry: AddressSpaceDiscoveryRegistry,
    /// Mount 级 I/O 提供者注册表（`inline_fs` / `relay_fs` 等）
    pub mount_provider_registry: Arc<MountProviderRegistry>,
    /// Task 生命周期服务 — Application 层直接编排 task start/continue/cancel
    pub task_lifecycle_service: Arc<TaskLifecycleService>,
    /// Hook 提供者 — 供 API 层验证脚本等管理接口使用
    pub hook_provider: Arc<AppExecutionHookProvider>,
    /// 统一认证会话服务（application 层）
    pub auth_session_service: Arc<AuthSessionService>,
    /// 运行时对账服务 — Story/Task 状态变更时联动 session 取消
    pub runtime_reconciler: Arc<agentdash_application::reconcile::runtime::RuntimeReconciler>,
    /// Cron 调度器句柄 — 配置变更时调用 `notify_config_changed()` 触发热重载
    pub cron_scheduler: CronSchedulerHandle,
}

/// Task 执行运行时状态 — 并发锁与重试控制
///
/// 与 `TaskLifecycleService` 共享同一套实例（通过 `Arc`）。
pub struct TaskRuntime {
    /// Per-Task 异步操作锁，确保同一 Task 的生命周期操作串行执行
    pub lock_map: Arc<TaskLockMap>,
    /// Per-Task 重启追踪器，控制失败后的自动重试策略
    pub restart_tracker: Arc<RestartTracker>,
}

/// 应用级配置
pub struct AppConfig {
    /// MCP 服务基础 URL（用于向 Agent 注入 MCP 端点信息）
    pub mcp_base_url: Option<String>,
    /// 当前宿主配置的认证模式
    pub auth_mode: AuthMode,
}

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
/// 按职责分为 4 个子集：repos / services / task_runtime / config。
pub struct AppState {
    pub repos: RepositorySet,
    pub services: ServiceSet,
    pub task_runtime: TaskRuntime,
    pub config: AppConfig,
    /// 认证/授权提供者（由插件注入，None 表示无认证）
    pub auth_provider: Option<Arc<dyn agentdash_plugin_api::AuthProvider>>,
}

impl AppState {
    pub async fn new(pool: PgPool) -> Result<Arc<Self>> {
        Self::new_with_plugins(pool, builtin_plugins()).await
    }

    /// 携带插件列表构建 AppState
    ///
    /// 返回 `Arc<Self>` 以支持内部 `DeferredTurnDispatcher` 的延迟绑定。
    pub async fn new_with_plugins(
        pool: PgPool,
        plugins: Vec<Box<dyn AgentDashPlugin>>,
    ) -> Result<Arc<Self>> {
        let plugin_registration = collect_plugin_registration(plugins)
            .map_err(|err| anyhow::anyhow!("插件注册失败: {err}"))?;

        // 按依赖顺序初始化：projects → workspaces → stories → tasks
        let project_repo = Arc::new(PostgresProjectRepository::new(pool.clone()));

        let canvas_repo = Arc::new(PostgresCanvasRepository::new(pool.clone()));

        let workspace_repo = Arc::new(PostgresWorkspaceRepository::new(pool.clone()));
        workspace_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("workspaces 表初始化失败: {e}"))?;

        let story_repo = Arc::new(PostgresStoryRepository::new(pool.clone()));
        let state_change_repo = Arc::new(PostgresStateChangeRepository::new(pool.clone()));

        let task_repo = Arc::new(PostgresTaskRepository::new(pool.clone()));
        let session_binding_repo = Arc::new(PostgresSessionBindingRepository::new(pool.clone()));
        let session_repo = Arc::new(PostgresSessionRepository::new(pool.clone()));

        let backend_repo = Arc::new(PostgresBackendRepository::new(pool.clone()));

        let user_directory_repo = Arc::new(PostgresUserDirectoryRepository::new(pool.clone()));

        let settings_repo = Arc::new(PostgresSettingsRepository::new(pool.clone()));

        let agent_repo = Arc::new(PostgresAgentRepository::new(pool.clone()));

        let llm_provider_repo = Arc::new(PostgresLlmProviderRepository::new(pool.clone()));
        llm_provider_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("llm_providers 表初始化失败: {e}"))?;

        let auth_session_repo = Arc::new(PostgresAuthSessionRepository::new(pool.clone()));
        let auth_session_service = Arc::new(AuthSessionService::new(auth_session_repo.clone()));

        let workflow_repo = Arc::new(PostgresWorkflowRepository::new(pool));

        let backend_registry = BackendRegistry::new();

        let mut mount_registry_builder = MountProviderRegistryBuilder::new()
            .with_builtins(workflow_repo.clone(), canvas_repo.clone())
            .register(Arc::new(RelayFsMountProvider::new(
                backend_registry.clone(),
            )));

        for provider in plugin_registration.mount_providers {
            tracing::info!("注册插件 MountProvider: {}", provider.provider_id());
            mount_registry_builder = mount_registry_builder.register(provider);
        }

        let mount_provider_registry = Arc::new(mount_registry_builder.build());

        let address_space_service = Arc::new(RelayAddressSpaceService::new(
            mount_provider_registry.clone(),
        ));
        let session_hub_handle = SharedSessionHubHandle::default();

        let inline_persister: Arc<
            dyn agentdash_application::address_space::inline_persistence::InlineContentPersister,
        > = Arc::new(
            agentdash_application::address_space::inline_persistence::DbInlineContentPersister::new(
                project_repo.clone(),
                story_repo.clone(),
            ),
        );

        let mut sub_connectors: Vec<Arc<dyn AgentConnector>> = Vec::new();
        let mut title_bridge: Option<Arc<dyn agentdash_agent::LlmBridge>> = None;

        if let Some(pi_connector) = build_pi_agent_connector(PiAgentConnectorDeps {
            settings_repo: settings_repo.clone(),
            llm_provider_repo: llm_provider_repo.clone(),
            address_space_service: address_space_service.clone(),
            canvas_repo: canvas_repo.clone(),
            session_binding_repo: session_binding_repo.clone(),
            agent_repo: agent_repo.clone(),
            agent_link_repo: agent_repo.clone(),
            workflow_definition_repo: workflow_repo.clone(),
            lifecycle_definition_repo: workflow_repo.clone(),
            lifecycle_run_repo: workflow_repo.clone(),
            session_hub_handle: session_hub_handle.clone(),
            inline_persister: Some(inline_persister),
        })
        .await
        {
            title_bridge = Some(pi_connector.default_bridge());
            sub_connectors.push(Arc::new(pi_connector));
        }
        // relay connector — 将远程后端上报的执行器纳入统一路由
        {
            let relay_transport: Arc<
                dyn agentdash_application::backend_transport::RelayPromptTransport,
            > = backend_registry.clone();
            sub_connectors.push(Arc::new(
                agentdash_application::relay_connector::RelayAgentConnector::new(relay_transport),
            ));
        }

        sub_connectors.extend(plugin_registration.connectors);
        validate_connector_executor_ids(&sub_connectors)
            .map_err(|err| anyhow::anyhow!("连接器注册失败: {err}"))?;

        let connector: Arc<dyn AgentConnector> = Arc::new(CompositeConnector::new(sub_connectors));
        let hook_provider = Arc::new(AppExecutionHookProvider::new(
            project_repo.clone(),
            story_repo.clone(),
            task_repo.clone(),
            session_binding_repo.clone(),
            agent_repo.clone(),
            agent_repo.clone(),
            workflow_repo.clone(),
            workflow_repo.clone(),
            workflow_repo.clone(),
        ));
        let mut session_hub = SessionHub::new_with_hooks_and_persistence(
            None,
            connector.clone(),
            Some(hook_provider.clone()),
            session_repo,
        )
        .with_address_space_service(address_space_service.clone())
        .with_extra_skill_dirs(plugin_registration.extra_skill_dirs);
        if let Some(bridge) = title_bridge {
            session_hub = session_hub.with_title_generator(Arc::new(
                crate::title_generator::LlmTitleGenerator::new(bridge),
            ));
        }
        session_hub_handle.set(session_hub.clone()).await;

        let restart_tracker = Arc::new(RestartTracker::default());
        let lock_map = Arc::new(TaskLockMap::new());

        let project_repo_port: Arc<dyn ProjectRepository> = project_repo.clone();
        let state_change_repo_port: Arc<dyn StateChangeRepository> = state_change_repo.clone();
        let task_repo_port: Arc<dyn TaskRepository> = task_repo.clone();
        let task_command_repo_port: Arc<dyn TaskAggregateCommandRepository> = task_repo.clone();

        // 启动对账管线：Session → Task → Infrastructure（有序不可跳过）
        {
            let deps = agentdash_application::reconcile::boot::BootReconcileDeps {
                session_hub: session_hub.clone(),
                project_repo: project_repo_port.clone(),
                state_change_repo: state_change_repo_port.clone(),
                task_repo: task_repo_port.clone(),
                restart_tracker: restart_tracker.clone(),
                session_state_reader: Arc::new(HubSessionStateReader {
                    hub: session_hub.clone(),
                }),
            };
            let report = agentdash_application::reconcile::boot::run_boot_reconcile(&deps).await;
            if report.has_errors() {
                for phase in &report.phases {
                    for err in &phase.errors {
                        tracing::warn!(phase = phase.phase, error = %err, "启动对账阶段出错");
                    }
                }
            }
        }

        let mcp_base_url = std::env::var("AGENTDASH_MCP_BASE_URL").ok().or_else(|| {
            let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
            Some(format!("http://127.0.0.1:{port}"))
        });
        let auth_mode = resolve_configured_auth_mode()?;

        if plugin_registration.auth_provider.is_none() {
            anyhow::bail!("认证模式 `{auth_mode}` 未注册 AuthProvider，无法启动服务");
        }

        tracing::info!(auth_mode = %auth_mode, "认证模式已加载");

        let mut address_space_registry = builtin_address_space_registry();
        for provider in plugin_registration.address_space_providers {
            address_space_registry.register(provider);
        }

        let contributor_registry = Arc::new(ContextContributorRegistry::with_builtins());

        let repos = RepositorySet {
            project_repo,
            canvas_repo,
            workspace_repo,
            story_repo,
            state_change_repo,
            task_repo,
            task_command_repo: task_command_repo_port,
            session_binding_repo,
            backend_repo,
            auth_session_repo,
            user_directory_repo,
            settings_repo,
            llm_provider_repo,
            agent_repo: agent_repo.clone(),
            agent_link_repo: agent_repo,
            workflow_definition_repo: workflow_repo.clone(),
            lifecycle_definition_repo: workflow_repo.clone(),
            workflow_assignment_repo: workflow_repo.clone(),
            lifecycle_run_repo: workflow_repo,
        };

        let runtime_reconciler = Arc::new(
            agentdash_application::reconcile::runtime::RuntimeReconciler::new(
                session_hub.clone(),
                task_repo_port.clone(),
            ),
        );

        let dispatcher = crate::bootstrap::turn_dispatcher::AppStateTurnDispatcher::new(
            session_hub.clone(),
            backend_registry.clone(),
        );

        let task_lifecycle_service = Arc::new(TaskLifecycleService {
            repos: repos.clone(),
            hub: session_hub.clone(),
            address_space_service: address_space_service.clone(),
            contributor_registry: contributor_registry.clone(),
            mcp_base_url: mcp_base_url.clone(),
            backend_availability: backend_registry.clone(),
            dispatcher: dispatcher.clone(),
            restart_tracker: restart_tracker.clone(),
            lock_map: lock_map.clone(),
        });

        let state = Self {
            repos,
            services: ServiceSet {
                session_hub,
                connector,
                address_space_service,
                backend_registry,
                contributor_registry: contributor_registry.clone(),
                address_space_registry,
                mount_provider_registry,
                task_lifecycle_service,
                hook_provider,
                auth_session_service,
                runtime_reconciler,
                cron_scheduler: CronSchedulerHandle::new(),
            },
            task_runtime: TaskRuntime {
                lock_map,
                restart_tracker,
            },
            config: AppConfig {
                mcp_base_url,
                auth_mode,
            },
            auth_provider: plugin_registration.auth_provider,
        };

        let state = Arc::new(state);
        // 后台 session stall 检测：定期扫描 running session，超时自动取消
        agentdash_application::session::stall_detector::spawn_stall_detector(
            state.services.session_hub.clone(),
            agentdash_application::session::stall_detector::DEFAULT_STALL_TIMEOUT_MS,
        );

        // 后台 cron 调度器：按 Agent 配置的 cron 表达式定期触发 session
        {
            let cron_target = Arc::new(crate::bootstrap::cron_target::AppCronTriggerTarget {
                session_hub: state.services.session_hub.clone(),
            });
            let cron_repos = state.repos.clone();
            let cron_handle = state.services.cron_scheduler.clone();
            tokio::spawn(async move {
                agentdash_application::scheduling::cron_scheduler::spawn_cron_scheduler(
                    cron_repos,
                    cron_target,
                    &cron_handle,
                )
                .await;
            });
        }

        // 后台定时清理过期认证会话，避免 auth_sessions 无限增长。
        {
            let auth_session_service = state.services.auth_session_service.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10 * 60));
                loop {
                    interval.tick().await;
                    match auth_session_service.cleanup_expired_sessions().await {
                        Ok(count) if count > 0 => {
                            tracing::info!(deleted = count, "已清理过期认证会话")
                        }
                        Ok(_) => {}
                        Err(err) => {
                            tracing::warn!(error = %err, "清理过期认证会话失败")
                        }
                    }
                }
            });
        }

        Ok(state)
    }
}

fn resolve_configured_auth_mode() -> Result<AuthMode> {
    match std::env::var("AGENTDASH_AUTH_MODE") {
        Ok(raw) => raw
            .parse::<AuthMode>()
            .map_err(|err| anyhow::anyhow!("AGENTDASH_AUTH_MODE 配置无效: {err}")),
        Err(std::env::VarError::NotPresent) => Ok(AuthMode::Personal),
        Err(err) => Err(anyhow::anyhow!("读取 AGENTDASH_AUTH_MODE 失败: {err}")),
    }
}

struct PiAgentConnectorDeps {
    settings_repo: Arc<dyn SettingsRepository>,
    llm_provider_repo: Arc<dyn LlmProviderRepository>,
    address_space_service: Arc<RelayAddressSpaceService>,
    canvas_repo: Arc<dyn agentdash_domain::canvas::CanvasRepository>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    agent_repo: Arc<dyn agentdash_domain::agent::AgentRepository>,
    agent_link_repo: Arc<dyn agentdash_domain::agent::ProjectAgentLinkRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    session_hub_handle: SharedSessionHubHandle,
    inline_persister: Option<
        Arc<dyn agentdash_application::address_space::inline_persistence::InlineContentPersister>,
    >,
}

async fn build_pi_agent_connector(
    deps: PiAgentConnectorDeps,
) -> Option<agentdash_executor::connectors::pi_agent::PiAgentConnector> {
    let mut connector = agentdash_executor::connectors::pi_agent::build_pi_agent_connector(
        deps.settings_repo.as_ref(),
        deps.llm_provider_repo.as_ref(),
    )
    .await?;
    connector.set_settings_repository(deps.settings_repo);
    connector.set_llm_provider_repository(deps.llm_provider_repo);
    connector.set_runtime_tool_provider(Arc::new(RelayRuntimeToolProvider::new(
        deps.address_space_service,
        deps.canvas_repo,
        deps.session_binding_repo,
        deps.agent_repo,
        deps.agent_link_repo,
        deps.workflow_definition_repo,
        deps.lifecycle_definition_repo,
        deps.lifecycle_run_repo,
        deps.session_hub_handle,
        deps.inline_persister,
    )));
    Some(connector)
}
