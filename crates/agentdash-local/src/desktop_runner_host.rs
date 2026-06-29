//! 桌面端内嵌 runner host。
//!
//! Tauri 负责桌面生命周期与命令桥接；runner 的启动、复用、停止与日志能力收束在这里。

use crate::runner_redaction::redact_secret;
use crate::runtime::{
    LocalLogEvent, LocalRuntimeConfig, LocalRuntimeManager, LocalRuntimeSnapshot,
    LocalRuntimeState, LocalRuntimeStatus, StopReason,
};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct DesktopRunnerHost {
    runtime: LocalRuntimeManager,
    ensure_lock: Arc<Mutex<()>>,
    supervisor_status: Arc<Mutex<Option<LocalRuntimeSnapshot>>>,
}

impl Default for DesktopRunnerHost {
    fn default() -> Self {
        Self {
            runtime: LocalRuntimeManager::new(),
            ensure_lock: Arc::new(Mutex::new(())),
            supervisor_status: Arc::new(Mutex::new(None)),
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
                LocalRuntimeState::Claiming
                | LocalRuntimeState::Starting
                | LocalRuntimeState::Running
                | LocalRuntimeState::Retrying => {
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
                LocalRuntimeState::Idle
                | LocalRuntimeState::Disabled
                | LocalRuntimeState::WaitingForAuth
                | LocalRuntimeState::WaitingForApi => {}
            }
        }

        self.record_supervisor_state(
            LocalRuntimeState::Claiming,
            "正在领取桌面本机 runtime 凭据",
            None,
            None,
            None,
        )
        .await;

        let config = match build_config().await {
            Ok(config) => config,
            Err(error) => {
                self.record_supervisor_state(
                    LocalRuntimeState::Error,
                    "桌面本机 runtime 启动前置步骤失败",
                    Some(error.to_string()),
                    None,
                    None,
                )
                .await;
                return Err(error);
            }
        };
        let handle = self.runtime.start(config).await?;
        let snapshot = handle.status_rx.borrow().clone();
        self.store_supervisor_snapshot(Some(snapshot.clone())).await;
        Ok(snapshot)
    }

    pub async fn stop(&self, reason: StopReason) -> anyhow::Result<()> {
        self.record_supervisor_state(
            LocalRuntimeState::Stopping,
            "正在停止桌面本机 runtime",
            None,
            None,
            None,
        )
        .await;
        match self.runtime.stop(reason).await {
            Ok(()) => {
                self.record_supervisor_state(
                    LocalRuntimeState::Stopped,
                    "桌面本机 runtime 已停止",
                    None,
                    None,
                    None,
                )
                .await;
                Ok(())
            }
            Err(error) => {
                self.record_supervisor_state(
                    LocalRuntimeState::Error,
                    "停止桌面本机 runtime 失败",
                    Some(error.to_string()),
                    None,
                    None,
                )
                .await;
                Err(error)
            }
        }
    }

    pub async fn restart(&self) -> anyhow::Result<LocalRuntimeSnapshot> {
        self.runtime.restart().await
    }

    pub async fn snapshot(&self) -> Option<LocalRuntimeSnapshot> {
        if let Some(snapshot) = self.runtime.snapshot().await {
            self.store_supervisor_snapshot(Some(snapshot.clone())).await;
            return Some(snapshot);
        }
        self.supervisor_status.lock().await.clone()
    }

    pub async fn mark_idle(&self, message: impl Into<String>) -> LocalRuntimeSnapshot {
        self.record_supervisor_state(LocalRuntimeState::Idle, message, None, None, None)
            .await
    }

    pub async fn mark_disabled(&self, message: impl Into<String>) -> LocalRuntimeSnapshot {
        self.record_supervisor_state(LocalRuntimeState::Disabled, message, None, None, None)
            .await
    }

    pub async fn mark_waiting_for_auth(&self, message: impl Into<String>) -> LocalRuntimeSnapshot {
        self.record_supervisor_state(LocalRuntimeState::WaitingForAuth, message, None, None, None)
            .await
    }

    pub async fn mark_waiting_for_api(
        &self,
        message: impl Into<String>,
        last_error: Option<String>,
        retry_count: Option<u32>,
        next_retry_at: Option<String>,
    ) -> LocalRuntimeSnapshot {
        self.record_supervisor_state(
            LocalRuntimeState::WaitingForApi,
            message,
            last_error,
            retry_count,
            next_retry_at,
        )
        .await
    }

    pub async fn mark_error(
        &self,
        message: impl Into<String>,
        last_error: impl Into<String>,
    ) -> LocalRuntimeSnapshot {
        self.record_supervisor_state(
            LocalRuntimeState::Error,
            message,
            Some(last_error.into()),
            None,
            None,
        )
        .await
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

    async fn store_supervisor_snapshot(&self, snapshot: Option<LocalRuntimeSnapshot>) {
        let mut guard = self.supervisor_status.lock().await;
        *guard = snapshot;
    }

    async fn record_supervisor_state(
        &self,
        state: LocalRuntimeState,
        message: impl Into<String>,
        last_error: Option<String>,
        retry_count: Option<u32>,
        next_retry_at: Option<String>,
    ) -> LocalRuntimeSnapshot {
        let previous = self.supervisor_status.lock().await.clone();
        let message = Some(message.into());
        let last_error = last_error.map(|error| redact_secret(&error));
        let now = Some(chrono::Utc::now().to_rfc3339());
        let snapshot = supervisor_snapshot(
            previous.as_ref(),
            state,
            message,
            last_error,
            now,
            retry_count,
            next_retry_at,
        );
        self.store_supervisor_snapshot(Some(snapshot.clone())).await;
        snapshot
    }
}

fn supervisor_snapshot(
    previous: Option<&LocalRuntimeSnapshot>,
    state: LocalRuntimeState,
    message: Option<String>,
    last_error: Option<String>,
    last_attempt_at: Option<String>,
    retry_count: Option<u32>,
    next_retry_at: Option<String>,
) -> LocalRuntimeSnapshot {
    LocalRuntimeStatus {
        state,
        owner: "desktop_embedded_runner".to_string(),
        registration_source: Some("desktop_access_token".to_string()),
        backend_id: previous
            .map(|snapshot| snapshot.backend_id.clone())
            .unwrap_or_default(),
        name: previous
            .map(|snapshot| snapshot.name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Desktop Local Runtime".to_string()),
        workspace_roots: previous
            .map(|snapshot| snapshot.workspace_roots.clone())
            .unwrap_or_default(),
        executor_enabled: previous
            .map(|snapshot| snapshot.executor_enabled)
            .unwrap_or(true),
        mcp_server_count: previous
            .map(|snapshot| snapshot.mcp_server_count)
            .unwrap_or(0),
        message,
        last_error,
        last_attempt_at,
        next_retry_at,
        retry_count,
        relay_connection: previous.and_then(|snapshot| snapshot.relay_connection.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn desktop_runner_host_records_supervisor_states() {
        let host = DesktopRunnerHost::new();

        assert_eq!(
            host.mark_idle("等待 profile").await.state,
            LocalRuntimeState::Idle
        );
        assert_eq!(
            host.mark_disabled("已关闭").await.state,
            LocalRuntimeState::Disabled
        );
        assert_eq!(
            host.mark_waiting_for_auth("等待登录").await.state,
            LocalRuntimeState::WaitingForAuth
        );
        let waiting_for_api = host
            .mark_waiting_for_api(
                "等待 API",
                Some("connect failed".to_string()),
                Some(2),
                Some("2026-06-29T00:00:00Z".to_string()),
            )
            .await;

        assert_eq!(waiting_for_api.state, LocalRuntimeState::WaitingForApi);
        assert_eq!(
            waiting_for_api.last_error.as_deref(),
            Some("connect failed")
        );
        assert_eq!(waiting_for_api.retry_count, Some(2));
        assert_eq!(
            waiting_for_api.registration_source.as_deref(),
            Some("desktop_access_token")
        );
        assert_eq!(waiting_for_api.owner, "desktop_embedded_runner");
    }

    #[tokio::test]
    async fn desktop_runner_host_exposes_claiming_then_error_when_config_build_fails() {
        let host = DesktopRunnerHost::new();
        let host_for_build = host.clone();

        let result = host
            .ensure_started_with(|| async move {
                let snapshot = host_for_build
                    .snapshot()
                    .await
                    .expect("claiming snapshot should be visible while building config");
                assert_eq!(snapshot.state, LocalRuntimeState::Claiming);
                anyhow::bail!("claim failed with token=secret")
            })
            .await;

        assert!(result.is_err());
        let snapshot = host
            .snapshot()
            .await
            .expect("error snapshot should remain visible");
        assert_eq!(snapshot.state, LocalRuntimeState::Error);
        assert_eq!(
            snapshot.last_error.as_deref(),
            Some("claim failed with token=***")
        );
    }
}
