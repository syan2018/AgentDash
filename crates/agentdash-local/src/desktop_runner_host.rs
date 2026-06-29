//! 桌面端内嵌 runner host。
//!
//! Tauri 负责桌面生命周期与命令桥接；runner 的启动、复用、停止与日志能力收束在这里。

use crate::runtime::{
    LocalLogEvent, LocalRuntimeConfig, LocalRuntimeManager, LocalRuntimeSnapshot,
    LocalRuntimeState, StopReason,
};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct DesktopRunnerHost {
    runtime: LocalRuntimeManager,
    ensure_lock: Arc<Mutex<()>>,
}

impl Default for DesktopRunnerHost {
    fn default() -> Self {
        Self {
            runtime: LocalRuntimeManager::new(),
            ensure_lock: Arc::new(Mutex::new(())),
        }
    }
}

impl DesktopRunnerHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn ensure_started(
        &self,
        config: LocalRuntimeConfig,
    ) -> anyhow::Result<LocalRuntimeSnapshot> {
        self.ensure_started_with(|| async move { Ok(config) }).await
    }

    pub async fn ensure_started_with<F, Fut>(
        &self,
        build_config: F,
    ) -> anyhow::Result<LocalRuntimeSnapshot>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<LocalRuntimeConfig>>,
    {
        let _guard = self.ensure_lock.lock().await;
        if let Some(snapshot) = self.snapshot().await {
            match snapshot.state {
                LocalRuntimeState::Starting | LocalRuntimeState::Running => {
                    self.record_log("info", "runtime", "runtime 已在启动或运行，复用现有状态")
                        .await;
                    return Ok(snapshot);
                }
                LocalRuntimeState::Stopping => {
                    self.record_log(
                        "info",
                        "runtime",
                        "runtime 正在停止，等待旧实例退出后重试启动",
                    )
                    .await;
                    let _ = self.runtime.stop(StopReason::Shutdown).await;
                }
                LocalRuntimeState::Stopped | LocalRuntimeState::Error => {
                    self.record_log("info", "runtime", "清理已停止或失败的 runtime 后重新启动")
                        .await;
                    let _ = self.runtime.stop(StopReason::Shutdown).await;
                }
            }
        }

        let config = build_config().await?;
        let handle = self.runtime.start(config).await?;
        Ok(handle.status_rx.borrow().clone())
    }

    pub async fn stop(&self, reason: StopReason) -> anyhow::Result<()> {
        self.runtime.stop(reason).await
    }

    pub async fn restart(&self) -> anyhow::Result<LocalRuntimeSnapshot> {
        self.runtime.restart().await
    }

    pub async fn snapshot(&self) -> Option<LocalRuntimeSnapshot> {
        self.runtime.snapshot().await
    }

    pub async fn record_log(
        &self,
        level: impl Into<String>,
        target: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.runtime.record_log(level, target, message).await;
    }

    pub async fn logs_tail(&self, limit: usize) -> Vec<LocalLogEvent> {
        self.runtime.logs_tail(limit).await
    }

    pub async fn logs_clear(&self) {
        self.runtime.logs_clear().await;
    }
}
