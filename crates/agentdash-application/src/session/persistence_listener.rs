//! SessionTurnProcessor 的持久化副作用（SessionMeta 写入）监听器。
//!
//! PR 7 把 turn_processor 身上"直接写 SessionMeta"的职责外移到本模块：
//! processor 只负责识别事件并调用这里暴露的 fn，真正的 meta get/save
//! 逻辑集中在此。职责分离后：
//! - `turn_processor` 只管 per-turn 事件流 + 终态 hook 评估；
//! - 持久化副作用（执行器 session_id 同步、未来可能的其他持久化动作）
//!   在本模块，便于统一加策略（幂等、节流、审计）。
//!
//! 选型说明：采用"processor 调用 fn"而非"fn 订阅多播 channel"的形态，
//! 因为 SessionPersistence 已经是 Arc<dyn> 抽象，单播调用最小侵入，
//! 没必要引入额外 tokio::spawn / subscriber 机制。

use std::sync::Arc;

use agentdash_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent};

use super::persistence::SessionPersistence;

/// 针对一条 BackboneEnvelope 进行持久化副作用检查：
/// 若发现 `ExecutorSessionBound` 事件，同步写回 `SessionMeta.executor_session_id`。
pub(super) async fn sync_executor_session_id(
    persistence: &Arc<dyn SessionPersistence>,
    session_id: &str,
    turn_id: &str,
    envelope: &BackboneEnvelope,
    last_executor_session_id: &mut Option<String>,
) {
    let executor_session_id = match &envelope.event {
        BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
            executor_session_id,
        }) => executor_session_id.clone(),
        _ => return,
    };
    if last_executor_session_id.as_deref() == Some(executor_session_id.as_str()) {
        return;
    }
    *last_executor_session_id = Some(executor_session_id.clone());

    match persistence.get_session_meta(session_id).await {
        Ok(Some(mut meta)) => {
            if meta.executor_session_id.as_deref() == Some(executor_session_id.as_str()) {
                return;
            }
            meta.executor_session_id = Some(executor_session_id);
            meta.updated_at = chrono::Utc::now().timestamp_millis();
            if let Err(error) = persistence.save_session_meta(&meta).await {
                tracing::warn!(
                    session_id = %session_id,
                    turn_id = %turn_id,
                    error = %error,
                    "持久化 executor_session_id 失败"
                );
            }
        }
        Ok(None) => {
            tracing::warn!(
                session_id = %session_id,
                "同步 executor_session_id 时 session meta 不存在"
            );
        }
        Err(error) => {
            tracing::warn!(
                session_id = %session_id,
                turn_id = %turn_id,
                error = %error,
                "读取 session meta 失败，跳过 executor_session_id 同步"
            );
        }
    }
}
