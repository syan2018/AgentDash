use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::address_space_access::{
    RelayAddressSpaceService, RelayRuntimeToolProvider, SharedExecutorHubHandle,
};
use crate::bootstrap::task_state_reconcile::reconcile_task_states_on_boot;
use crate::execution_hooks::AppExecutionHookProvider;
use crate::relay::registry::BackendRegistry;
use crate::task_agent_context::ContextContributorRegistry;
use agentdash_application::task_lock::TaskLockMap;
use agentdash_application::task_restart_tracker::RestartTracker;
use agentdash_domain::backend::BackendRepository;
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::settings::SettingsRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workflow::{
    WorkflowAssignmentRepository, WorkflowDefinitionRepository, WorkflowRunRepository,
};
use agentdash_domain::workspace::WorkspaceRepository;
use agentdash_executor::connectors::composite::CompositeConnector;
use agentdash_executor::{AgentConnector, ExecutorHub};
use agentdash_infrastructure::{
    SqliteBackendRepository, SqliteProjectRepository, SqliteSessionBindingRepository,
    SqliteSettingsRepository, SqliteStoryRepository, SqliteTaskRepository,
    SqliteWorkflowRepository, SqliteWorkspaceRepository,
};
use agentdash_injection::AddressSpaceRegistry;

/// 持久化层端口 — 所有 Repository trait 对象的集合
pub struct RepositorySet {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub session_binding_repo: Arc<dyn SessionBindingRepository>,
    pub backend_repo: Arc<dyn BackendRepository>,
    pub settings_repo: Arc<dyn SettingsRepository>,
    pub workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    pub workflow_assignment_repo: Arc<dyn WorkflowAssignmentRepository>,
    pub workflow_run_repo: Arc<dyn WorkflowRunRepository>,
}

/// 应用服务集合 — 执行引擎、连接器与各类注册表
pub struct ServiceSet {
    pub executor_hub: ExecutorHub,
    /// 当前活跃的连接器实例（供 discovery 端点查询能力/类型）
    pub connector: Arc<dyn AgentConnector>,
    /// 统一 Address Space 访问服务 — 供 declared sources、runtime tools、workspace browse 共享
    pub address_space_service: Arc<RelayAddressSpaceService>,
    /// WebSocket 中继后端注册表 — 跟踪在线的本机后端
    pub backend_registry: Arc<BackendRegistry>,
    /// 上下文贡献者注册表 — 持有常驻贡献者（Core/Binding/DeclaredSources/Instruction 等）
    pub contributor_registry: ContextContributorRegistry,
    /// 寻址空间注册表 — 持有可用的资源引用能力提供者
    pub address_space_registry: AddressSpaceRegistry,
}

/// Task 执行运行时状态 — 并发锁与重试控制
pub struct TaskRuntime {
    /// Per-Task 异步操作锁，确保同一 Task 的生命周期操作串行执行
    pub lock_map: TaskLockMap,
    /// Per-Task 重启追踪器，控制失败后的自动重试策略
    pub restart_tracker: RestartTracker,
}

/// 应用级配置
pub struct AppConfig {
    /// MCP 服务基础 URL（用于向 Agent 注入 MCP 端点信息）
    pub mcp_base_url: Option<String>,
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
}

impl AppState {
    pub async fn new(pool: SqlitePool) -> Result<Self> {
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
        story_repo
            .reconcile_task_counts()
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

        let settings_repo = Arc::new(SqliteSettingsRepository::new(pool.clone()));
        settings_repo
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
        let address_space_service =
            Arc::new(RelayAddressSpaceService::new(backend_registry.clone()));
        let executor_hub_handle = SharedExecutorHubHandle::default();

        let mut sub_connectors: Vec<Arc<dyn AgentConnector>> = Vec::new();

        if let Some(pi_connector) = build_pi_agent_connector(
            &workspace_root,
            settings_repo.as_ref(),
            address_space_service.clone(),
            session_binding_repo.clone(),
            workflow_repo.clone(),
            workflow_repo.clone(),
            executor_hub_handle.clone(),
        )
        .await
        {
            sub_connectors.push(Arc::new(pi_connector));
        }

        let connector: Arc<dyn AgentConnector> = Arc::new(CompositeConnector::new(sub_connectors));
        let hook_provider = Arc::new(AppExecutionHookProvider::new(
            project_repo.clone(),
            story_repo.clone(),
            task_repo.clone(),
            session_binding_repo.clone(),
            workflow_repo.clone(),
            workflow_repo.clone(),
        ));
        let executor_hub =
            ExecutorHub::new_with_hooks(workspace_root, connector.clone(), Some(hook_provider));
        executor_hub_handle.set(executor_hub.clone()).await;
        let restart_tracker = RestartTracker::default();

        let project_repo_port: Arc<dyn ProjectRepository> = project_repo.clone();
        let story_repo_port: Arc<dyn StoryRepository> = story_repo.clone();
        let task_repo_port: Arc<dyn TaskRepository> = task_repo.clone();
        reconcile_task_states_on_boot(
            &project_repo_port,
            &story_repo_port,
            &task_repo_port,
            &executor_hub,
            &restart_tracker,
        )
        .await?;

        let mcp_base_url = std::env::var("AGENTDASH_MCP_BASE_URL").ok().or_else(|| {
            let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
            Some(format!("http://127.0.0.1:{port}"))
        });

        Ok(Self {
            repos: RepositorySet {
                project_repo,
                workspace_repo,
                story_repo,
                task_repo,
                session_binding_repo,
                backend_repo,
                settings_repo,
                workflow_definition_repo: workflow_repo.clone(),
                workflow_assignment_repo: workflow_repo.clone(),
                workflow_run_repo: workflow_repo,
            },
            services: ServiceSet {
                executor_hub,
                connector,
                address_space_service,
                backend_registry,
                contributor_registry: ContextContributorRegistry::with_builtins(),
                address_space_registry: agentdash_injection::builtin_address_space_registry(),
            },
            task_runtime: TaskRuntime {
                lock_map: TaskLockMap::new(),
                restart_tracker,
            },
            config: AppConfig { mcp_base_url },
            remote_sessions: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

/// 尝试构建 PiAgentConnector，委托给 executor 层的 factory 后附加 runtime tool provider。
async fn build_pi_agent_connector(
    workspace_root: &std::path::Path,
    settings: &dyn SettingsRepository,
    address_space_service: Arc<RelayAddressSpaceService>,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    workflow_run_repo: Arc<dyn WorkflowRunRepository>,
    executor_hub_handle: SharedExecutorHubHandle,
) -> Option<agentdash_executor::connectors::pi_agent::PiAgentConnector> {
    let mut connector =
        agentdash_executor::connectors::pi_agent::build_pi_agent_connector(workspace_root, settings)
            .await?;
    connector.set_runtime_tool_provider(Arc::new(RelayRuntimeToolProvider::new(
        address_space_service,
        session_binding_repo,
        workflow_definition_repo,
        workflow_run_repo,
        executor_hub_handle,
    )));
    Some(connector)
}
