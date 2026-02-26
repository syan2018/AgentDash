use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;

use agentdash_coordinator::CoordinatorManager;
use agentdash_state::StateStore;

use crate::executor::ExecutorHub;
use crate::executor::connectors::vibe_kanban::VibeKanbanExecutorsConnector;

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
pub struct AppState {
    pub store: StateStore,
    pub coordinator: CoordinatorManager,
    pub executor_hub: ExecutorHub,
}

impl AppState {
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        let store = StateStore::new(pool.clone());
        store.initialize().await?;

        let coordinator = CoordinatorManager::new(pool);
        coordinator.initialize().await?;

        let workspace_root = std::env::current_dir()?;
        let connector = Arc::new(VibeKanbanExecutorsConnector::new(workspace_root.clone()));
        let executor_hub = ExecutorHub::new(workspace_root, connector);

        Ok(Self {
            store,
            coordinator,
            executor_hub,
        })
    }
}
