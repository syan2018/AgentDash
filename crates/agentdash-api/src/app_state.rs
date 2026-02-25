use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;
use tokio::sync::broadcast;

use agentdash_coordinator::CoordinatorManager;
use agentdash_state::StateStore;
use agentdash_state::events::StreamEvent;

use crate::executor::ExecutorHub;
use crate::executor::connectors::vibe_kanban::VibeKanbanExecutorsConnector;

/// 全局应用状态
///
/// 通过 Axum 的 State extractor 注入到各路由处理函数中。
pub struct AppState {
    pub store: StateStore,
    pub coordinator: CoordinatorManager,
    /// 广播通道，用于将 StateChange 实时推送给所有连接的客户端
    pub event_tx: broadcast::Sender<StreamEvent>,
    pub executor_hub: ExecutorHub,
}

impl AppState {
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        let store = StateStore::new(pool.clone());
        store.initialize().await?;

        let coordinator = CoordinatorManager::new(pool);
        coordinator.initialize().await?;

        let (event_tx, _) = broadcast::channel(256);

        let workspace_root = std::env::current_dir()?;
        let connector = Arc::new(VibeKanbanExecutorsConnector::new(workspace_root.clone()));
        let executor_hub = ExecutorHub::new(workspace_root, connector);

        Ok(Self {
            store,
            coordinator,
            event_tx,
            executor_hub,
        })
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<StreamEvent> {
        self.event_tx.subscribe()
    }
}
