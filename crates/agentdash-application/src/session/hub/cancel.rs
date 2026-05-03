//! Hub 的 cancel 路径。
//!
//! 取消语义分两种：
//! - running=true：通知 connector + 通过 turn_processor 补发 Interrupted Terminal
//! - running=false：若历史里最新 turn 没有 terminal 事件则补一条 interrupted
//!   （处理进程重启后被动 session 的兜底）
//!
//! `event_bridge.rs` 已在 PR 6b 删除，这里不再与它交互。

use std::collections::HashMap;

use agentdash_protocol::SourceInfo;
use agentdash_spi::ConnectorError;
use tokio::sync::broadcast;

use super::super::hub_support::{
    TurnTerminalKind, build_session_runtime, build_turn_terminal_envelope,
    parse_turn_terminal_event_from_envelope,
};
use super::SessionHub;

impl SessionHub {
    pub async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        let (running, current_turn_id, tx, processor_tx) = {
            let mut sessions = self.sessions.lock().await;
            let runtime = sessions.entry(session_id.to_string()).or_insert_with(|| {
                let (tx, _rx) = broadcast::channel(1024);
                build_session_runtime(tx)
            });
            if let Some(turn) = runtime.turn_state.active_turn_mut() {
                turn.cancel_requested = true;
            }
            let (turn_id, processor_tx) = runtime
                .turn_state
                .active_turn()
                .map(|turn| (Some(turn.turn_id.clone()), turn.processor_tx.clone()))
                .unwrap_or((None, None));
            (runtime.is_running(), turn_id, runtime.tx.clone(), processor_tx)
        };

        if running {
            match self.connector.cancel(session_id).await {
                Ok(()) => {}
                Err(err) => {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %err,
                        "connector.cancel 失败，继续通过 turn processor 兜底终止"
                    );
                }
            }
            if let Some(ptx) = processor_tx {
                if ptx
                    .send(super::super::turn_processor::TurnEvent::Terminal {
                        kind: TurnTerminalKind::Interrupted,
                        message: Some("执行已取消".to_string()),
                    })
                    .is_err()
                {
                    tracing::warn!(
                        session_id = %session_id,
                        "向 turn processor 发送 Terminal 失败（通道可能已关闭）"
                    );
                }
            } else {
                tracing::warn!(
                    session_id = %session_id,
                    "running=true 但 processor_tx 缺失，无法向 turn processor 发送终止信号"
                );
            }
            return Ok(());
        }

        let history = self
            .persistence
            .list_all_events(session_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let mut latest_turn_id = current_turn_id;
        let mut terminal_by_turn: HashMap<String, (TurnTerminalKind, Option<String>)> =
            HashMap::new();
        for event in history {
            if let Some(turn_id) = event.notification.trace.turn_id.as_deref() {
                let turn_id = turn_id.trim();
                if !turn_id.is_empty() {
                    latest_turn_id = Some(turn_id.to_string());
                }
            }
            if let Some((turn_id, terminal_kind, message)) =
                parse_turn_terminal_event_from_envelope(&event.notification)
            {
                terminal_by_turn.insert(turn_id, (terminal_kind, message));
            }
        }

        let Some(turn_id) = latest_turn_id else {
            return Ok(());
        };
        if terminal_by_turn.contains_key(&turn_id) {
            return Ok(());
        }

        let source = SourceInfo {
            connector_id: self.connector.connector_id().to_string(),
            connector_type: "local_executor".to_string(),
            executor_id: None,
        };
        let interrupted = build_turn_terminal_envelope(
            session_id,
            &source,
            &turn_id,
            TurnTerminalKind::Interrupted,
            Some("检测到未收尾的旧执行，已手动标记为 interrupted".to_string()),
        );
        let _ = tx;
        let _ = self
            .persist_notification(session_id, interrupted)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        Ok(())
    }
}
