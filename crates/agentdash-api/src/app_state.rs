use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;

use agentdash_domain::project::ProjectRepository;
use agentdash_domain::workspace::WorkspaceRepository;
use agentdash_domain::story::StoryRepository;
use agentdash_domain::task::TaskRepository;
use agentdash_domain::backend::BackendRepository;
use agentdash_infrastructure::{
    SqliteProjectRepository, SqliteWorkspaceRepository,
    SqliteStoryRepository, SqliteTaskRepository, SqliteBackendRepository,
};
use agentdash_executor::{AgentConnector, ExecutorHub};
use agentdash_executor::connectors::vibe_kanban::VibeKanbanExecutorsConnector;

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
pub struct AppState {
    pub project_repo: Arc<dyn ProjectRepository>,
    pub workspace_repo: Arc<dyn WorkspaceRepository>,
    pub story_repo: Arc<dyn StoryRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub backend_repo: Arc<dyn BackendRepository>,
    pub executor_hub: ExecutorHub,
    /// 当前活跃的连接器实例（供 discovery 端点查询能力/类型）
    pub connector: Arc<dyn AgentConnector>,
}

impl AppState {
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        // 按依赖顺序初始化：projects → workspaces → stories → tasks
        let project_repo = Arc::new(SqliteProjectRepository::new(pool.clone()));
        project_repo.initialize().await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(pool.clone()));
        workspace_repo.initialize().await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let story_repo = Arc::new(SqliteStoryRepository::new(pool.clone()));
        story_repo.initialize().await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let task_repo = Arc::new(SqliteTaskRepository::new(pool.clone()));
        task_repo.initialize().await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let backend_repo = Arc::new(SqliteBackendRepository::new(pool));
        backend_repo.initialize().await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let workspace_root = std::env::current_dir()?;
        let connector: Arc<dyn AgentConnector> =
            Arc::new(VibeKanbanExecutorsConnector::new(workspace_root.clone()));
        let executor_hub = ExecutorHub::new(workspace_root, connector.clone());

        Ok(Self {
            project_repo,
            workspace_repo,
            story_repo,
            task_repo,
            backend_repo,
            executor_hub,
            connector,
        })
    }
}
