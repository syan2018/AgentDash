use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::bootstrap::task_state_reconcile::reconcile_task_states_on_boot;
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
use agentdash_application::context::ContextContributorRegistry;
use agentdash_application::hooks::AppExecutionHookProvider;
pub use agentdash_application::repository_set::RepositorySet;
use agentdash_application::session::SessionHub;
use agentdash_application::task::service::TaskLifecycleService;
use agentdash_application::task_lock::TaskLockMap;
use agentdash_application::task_restart_tracker::RestartTracker;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::settings::SettingsRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
};
use agentdash_executor::AgentConnector;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_infrastructure::{
    SqliteAgentRepository, SqliteBackendRepository, SqliteProjectRepository,
    SqliteSessionBindingRepository, SqliteSettingsRepository, SqliteStoryRepository,
    SqliteTaskRepository, SqliteUserDirectoryRepository, SqliteWorkflowRepository,
    SqliteWorkspaceRepository,
};
use agentdash_injection::AddressSpaceDiscoveryRegistry;
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
    /// 远程会话映射：session_id → backend_id（路由到远程后端的会话）
    pub remote_sessions: Arc<RwLock<HashMap<String, String>>>,
    /// 认证/授权提供者（由插件注入，None 表示无认证）
    pub auth_provider: Option<Arc<dyn agentdash_plugin_api::AuthProvider>>,
}

impl AppState {
    pub async fn new(pool: SqlitePool) -> Result<Arc<Self>> {
        Self::new_with_plugins(pool, builtin_plugins()).await
    }

    /// 携带插件列表构建 AppState
    ///
    /// 返回 `Arc<Self>` 以支持内部 `DeferredTurnDispatcher` 的延迟绑定。
    pub async fn new_with_plugins(
        pool: SqlitePool,
        plugins: Vec<Box<dyn AgentDashPlugin>>,
    ) -> Result<Arc<Self>> {
        let plugin_registration = collect_plugin_registration(plugins)
            .map_err(|err| anyhow::anyhow!("插件注册失败: {err}"))?;

        // 按依赖顺序初始化：projects → workspaces → stories → tasks
        let project_repo = Arc::new(SqliteProjectRepository::new(pool.clone()));
        project_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(pool.clone()));
        workspace_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let story_repo = Arc::new(SqliteStoryRepository::new(pool.clone()));
        story_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let task_repo = Arc::new(SqliteTaskRepository::new(pool.clone()));
        task_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let session_binding_repo = Arc::new(SqliteSessionBindingRepository::new(pool.clone()));
        session_binding_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let backend_repo = Arc::new(SqliteBackendRepository::new(pool.clone()));
        backend_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let user_directory_repo = Arc::new(SqliteUserDirectoryRepository::new(pool.clone()));
        user_directory_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let settings_repo = Arc::new(SqliteSettingsRepository::new(pool.clone()));
        settings_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let agent_repo = Arc::new(SqliteAgentRepository::new(pool.clone()));
        agent_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let workflow_repo = Arc::new(SqliteWorkflowRepository::new(pool));
        workflow_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let workspace_root = std::env::current_dir()?;
        let backend_registry = BackendRegistry::new();

        let mount_provider_registry = Arc::new(
            MountProviderRegistryBuilder::new()
                .with_builtins(workflow_repo.clone())
                .register(Arc::new(RelayFsMountProvider::new(
                    backend_registry.clone(),
                )))
                .build(),
        );

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

        if let Some(pi_connector) = build_pi_agent_connector(
            &workspace_root,
            PiAgentConnectorDeps {
                settings_repo: settings_repo.clone(),
                address_space_service: address_space_service.clone(),
                session_binding_repo: session_binding_repo.clone(),
                workflow_definition_repo: workflow_repo.clone(),
                lifecycle_definition_repo: workflow_repo.clone(),
                lifecycle_run_repo: workflow_repo.clone(),
                session_hub_handle: session_hub_handle.clone(),
                inline_persister: Some(inline_persister),
            },
        )
        .await
        {
            sub_connectors.push(Arc::new(pi_connector));
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
            workflow_repo.clone(),
            workflow_repo.clone(),
            workflow_repo.clone(),
        ));
        let session_hub = SessionHub::new_with_hooks(
            workspace_root,
            connector.clone(),
            Some(hook_provider.clone()),
        );
        session_hub_handle.set(session_hub.clone()).await;

        // 启动恢复：将上次进程异常退出时残留的 running 状态修正为 interrupted
        if let Err(e) = session_hub.recover_interrupted_sessions().await {
            tracing::warn!("启动恢复 session 状态失败（非致命）: {e}");
        }

        let restart_tracker = Arc::new(RestartTracker::default());
        let lock_map = Arc::new(TaskLockMap::new());

        let project_repo_port: Arc<dyn ProjectRepository> = project_repo.clone();
        let story_repo_port: Arc<dyn StoryRepository> = story_repo.clone();
        let task_repo_port: Arc<dyn TaskRepository> = task_repo.clone();
        reconcile_task_states_on_boot(
            &project_repo_port,
            &story_repo_port,
            &task_repo_port,
            &session_hub,
            &restart_tracker,
        )
        .await?;

        let mcp_base_url = std::env::var("AGENTDASH_MCP_BASE_URL").ok().or_else(|| {
            let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
            Some(format!("http://127.0.0.1:{port}"))
        });
        let auth_mode = resolve_configured_auth_mode()?;

        if plugin_registration.auth_provider.is_none() {
            anyhow::bail!("认证模式 `{auth_mode}` 未注册 AuthProvider，无法启动服务");
        }

        tracing::info!(auth_mode = %auth_mode, "认证模式已加载");

        let mut address_space_registry = agentdash_injection::builtin_address_space_registry();
        for provider in plugin_registration.address_space_providers {
            address_space_registry.register(provider);
        }

        let contributor_registry = Arc::new(ContextContributorRegistry::with_builtins());
        let remote_sessions = Arc::new(RwLock::new(HashMap::new()));

        let repos = RepositorySet {
            project_repo,
            workspace_repo,
            story_repo,
            task_repo,
            session_binding_repo,
            backend_repo,
            user_directory_repo,
            settings_repo,
            agent_repo: agent_repo.clone(),
            agent_link_repo: agent_repo,
            workflow_definition_repo: workflow_repo.clone(),
            lifecycle_definition_repo: workflow_repo.clone(),
            workflow_assignment_repo: workflow_repo.clone(),
            lifecycle_run_repo: workflow_repo,
        };

        let dispatcher = crate::bootstrap::turn_dispatcher::AppStateTurnDispatcher::new(
            session_hub.clone(),
            backend_registry.clone(),
            repos.clone(),
            restart_tracker.clone(),
            remote_sessions.clone(),
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
            },
            task_runtime: TaskRuntime {
                lock_map,
                restart_tracker,
            },
            config: AppConfig {
                mcp_base_url,
                auth_mode,
            },
            remote_sessions,
            auth_provider: plugin_registration.auth_provider,
        };

        let state = Arc::new(state);
        dispatcher.set_retry_service(state.services.task_lifecycle_service.clone());

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
    address_space_service: Arc<RelayAddressSpaceService>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    session_hub_handle: SharedSessionHubHandle,
    inline_persister: Option<
        Arc<dyn agentdash_application::address_space::inline_persistence::InlineContentPersister>,
    >,
}

async fn build_pi_agent_connector(
    workspace_root: &std::path::Path,
    deps: PiAgentConnectorDeps,
) -> Option<agentdash_executor::connectors::pi_agent::PiAgentConnector> {
    let mut connector = agentdash_executor::connectors::pi_agent::build_pi_agent_connector(
        workspace_root,
        deps.settings_repo.as_ref(),
    )
    .await?;
    connector.set_settings_repository(deps.settings_repo);
    connector.set_runtime_tool_provider(Arc::new(RelayRuntimeToolProvider::new(
        deps.address_space_service,
        deps.session_binding_repo,
        deps.workflow_definition_repo,
        deps.lifecycle_definition_repo,
        deps.lifecycle_run_repo,
        deps.session_hub_handle,
        deps.inline_persister,
    )));
    Some(connector)
}
