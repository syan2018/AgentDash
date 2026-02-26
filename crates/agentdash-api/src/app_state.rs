use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;

use agentdash_coordinator::CoordinatorManager;
use agentdash_state::StateStore;

use crate::executor::{AgentConnector, ExecutorHub};
use crate::executor::connectors::vibe_kanban::VibeKanbanExecutorsConnector;

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
pub struct AppState {
    pub store: StateStore,
    pub coordinator: CoordinatorManager,
    pub executor_hub: ExecutorHub,
    /// 当前活跃的连接器实例（供 discovery 端点查询能力/类型）
    pub connector: Arc<dyn AgentConnector>,
}

impl AppState {
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        let store = StateStore::new(pool.clone());
        store.initialize().await?;

        let coordinator = CoordinatorManager::new(pool);
        coordinator.initialize().await?;

        let workspace_root = std::env::current_dir()?;
        let connector: Arc<dyn AgentConnector> =
            Arc::new(VibeKanbanExecutorsConnector::new(workspace_root.clone()));
        let executor_hub = ExecutorHub::new(workspace_root, connector.clone());

        Ok(Self {
            store,
            coordinator,
            executor_hub,
            connector,
        })
    }
}
