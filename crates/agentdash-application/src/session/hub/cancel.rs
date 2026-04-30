//! Hub 的 cancel 路径。
//!
//! 取消语义分两种：
//! - running=true：通知 connector + 通过 turn_processor 补发 Interrupted Terminal
//! - running=false：若历史里最新 turn 没有 terminal 事件则补一条 interrupted
//!   （处理进程重启后被动 session 的兜底）
//!
//! `event_bridge.rs` 已在 PR 6b 删除，这里不再与它交互。

use std::collections::HashMap;

use agent_client_protocol::SessionUpdate;
use agentdash_acp_meta::AgentDashSourceV1;
use agentdash_spi::ConnectorError;
use tokio::sync::broadcast;

use super::super::hub_support::{
    build_session_runtime, build_turn_terminal_notification, parse_turn_id,
    parse_turn_terminal_event, TurnTerminalKind,
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
            if runtime.running {
                runtime.cancel_requested = true;
            }
            (
                runtime.running,
                runtime.current_turn_id.clone(),
                runtime.tx.clone(),
                runtime.processor_tx.clone(),
            )
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
            match &event.notification.update {
                SessionUpdate::UserMessageChunk(chunk) => {
                    if let Some(turn_id) = parse_turn_id(chunk.meta.as_ref()) {
                        latest_turn_id = Some(turn_id);
                    }
                }
                SessionUpdate::SessionInfoUpdate(info) => {
                    if let Some((turn_id, terminal_kind, message)) =
                        parse_turn_terminal_event(info.meta.as_ref())
                    {
                        terminal_by_turn.insert(turn_id, (terminal_kind, message));
                    }
                }
                _ => {}
            }
        }

        let Some(turn_id) = latest_turn_id else {
            return Ok(());
        };
        if terminal_by_turn.contains_key(&turn_id) {
            return Ok(());
        }

        let source = AgentDashSourceV1::new(self.connector.connector_id(), "local_executor");
        let interrupted = build_turn_terminal_notification(
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
