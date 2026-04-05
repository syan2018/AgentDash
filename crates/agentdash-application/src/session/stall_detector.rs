//! Session Stall 检测器
//!
//! 后台定时扫描所有 running session，检测无活动超时。
//! 超时的 session 会被自动取消（标记为 interrupted）。
//! 这是平台级安全网，不依赖 Agent 判断。

use std::time::Duration;

use super::hub::SessionHub;

/// 系统默认 stall 超时：5 分钟
pub const DEFAULT_STALL_TIMEOUT_MS: u64 = 300_000;

/// 扫描间隔：每 30 秒检查一次
const SCAN_INTERVAL: Duration = Duration::from_secs(30);

/// 启动 stall 检测后台任务。
///
/// `stall_timeout_ms` 为 0 时不启动检测。
/// 返回 `JoinHandle` 供调用方在需要时取消。
pub fn spawn_stall_detector(
    session_hub: SessionHub,
    stall_timeout_ms: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if stall_timeout_ms == 0 {
            tracing::info!("Stall 检测已禁用 (stall_timeout_ms = 0)");
            return;
        }

        tracing::info!(
            stall_timeout_ms,
            scan_interval_secs = SCAN_INTERVAL.as_secs(),
            "Stall 检测器已启动"
        );

        let mut interval = tokio::time::interval(SCAN_INTERVAL);
        loop {
            interval.tick().await;

            let stalled = session_hub.find_stalled_sessions(stall_timeout_ms).await;
            if stalled.is_empty() {
                continue;
            }

            tracing::warn!(
                count = stalled.len(),
                session_ids = ?stalled,
                "检测到 stalled session，正在取消"
            );

            for session_id in stalled {
                if let Err(err) = session_hub.cancel(&session_id).await {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %err,
                        "取消 stalled session 失败"
                    );
                } else {
                    tracing::info!(
                        session_id = %session_id,
                        "已取消 stalled session"
                    );
                }
            }
        }
    })
}
