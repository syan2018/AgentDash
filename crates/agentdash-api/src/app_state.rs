use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;

use crate::bootstrap::task_state_reconcile::reconcile_task_states_on_boot;
use agentdash_domain::backend::{BackendConfig, BackendRepository, BackendType};
use agentdash_domain::project::ProjectRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::workspace::WorkspaceRepository;
use agentdash_executor::connectors::vibe_kanban::VibeKanbanExecutorsConnector;
use agentdash_executor::{AgentConnector, ExecutorHub};
use agentdash_infrastructure::{
    SqliteBackendRepository, SqliteProjectRepository, SqliteSessionBindingRepository,
    SqliteStoryRepository, SqliteTaskRepository, SqliteWorkspaceRepository,
};

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
pub struct AppState {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub sqlite_task_repo: Arc<SqliteTaskRepository>,
    pub session_binding_repo: Arc<dyn SessionBindingRepository>,
    pub backend_repo: Arc<dyn BackendRepository>,
    pub executor_hub: ExecutorHub,
    /// 当前活跃的连接器实例（供 discovery 端点查询能力/类型）
    pub connector: Arc<dyn AgentConnector>,
    /// MCP 服务基础 URL（用于向 Agent 注入 MCP 端点信息）
    pub mcp_base_url: Option<String>,
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

        let backend_repo = Arc::new(SqliteBackendRepository::new(pool));
        backend_repo
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        ensure_default_backend(&backend_repo).await?;

        let workspace_root = std::env::current_dir()?;
        let connector: Arc<dyn AgentConnector> =
            Arc::new(VibeKanbanExecutorsConnector::new(workspace_root.clone()));
        let executor_hub = ExecutorHub::new(workspace_root, connector.clone());
        let project_repo_port: Arc<dyn ProjectRepository> = project_repo.clone();
        let story_repo_port: Arc<dyn StoryRepository> = story_repo.clone();
        let task_repo_port: Arc<dyn TaskRepository> = task_repo.clone();
        reconcile_task_states_on_boot(
            &project_repo_port,
            &story_repo_port,
            &task_repo_port,
            &executor_hub,
        )
        .await?;

        let mcp_base_url = std::env::var("AGENTDASH_MCP_BASE_URL").ok();

        Ok(Self {
            project_repo,
            workspace_repo,
            story_repo,
            task_repo: task_repo.clone(),
            sqlite_task_repo: task_repo,
            session_binding_repo,
            backend_repo,
            executor_hub,
            connector,
            mcp_base_url,
        })
    }
}

async fn ensure_default_backend(backend_repo: &Arc<SqliteBackendRepository>) -> Result<()> {
    let backends = backend_repo
        .list_backends()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if !backends.is_empty() {
        return Ok(());
    }

    let local = BackendConfig {
        id: "local-default".to_string(),
        name: "本地后端".to_string(),
        endpoint: "http://127.0.0.1:3001".to_string(),
        auth_token: None,
        enabled: true,
        backend_type: BackendType::Local,
    };
    backend_repo
        .add_backend(&local)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}
