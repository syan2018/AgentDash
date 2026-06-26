use agentdash_diagnostics::{diag, Subsystem};
use std::{collections::HashMap, io, sync::Arc};

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::ConnectorError;

use super::eventing::SessionEventingService;
use super::hub_support::{
    TurnTerminalKind, build_turn_terminal_envelope, parse_turn_terminal_event_from_envelope,
};
use super::persistence::SessionStoreSet;
use super::turn_supervisor::TurnSupervisor;

#[derive(Clone)]
pub struct SessionRuntimeService {
    stores: SessionStoreSet,
    turn_supervisor: TurnSupervisor,
    eventing: SessionEventingService,
    connector: Arc<dyn agentdash_spi::AgentConnector>,
}

impl SessionRuntimeService {
    pub(super) fn new(
        stores: SessionStoreSet,
        turn_supervisor: TurnSupervisor,
        eventing: SessionEventingService,
        connector: Arc<dyn agentdash_spi::AgentConnector>,
    ) -> Self {
        Self {
            stores,
            turn_supervisor,
            eventing,
            connector,
        }
    }

    pub async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        let cancel_snapshot = self.turn_supervisor.request_cancel(session_id).await;

        if cancel_snapshot.running {
            match self.connector.cancel(session_id).await {
                Ok(()) => {}
                Err(err) => {
                    diag!(Warn, Subsystem::AgentRun,
        
                        session_id = %session_id,
                        error = %err,
                        "connector.cancel 失败，继续通过 turn processor 兜底终止"
                    );
                }
            }
            if let Some(ptx) = cancel_snapshot.processor_tx {
                if ptx
                    .send(super::turn_supervisor::TurnSupervisor::interrupted_event(
                        "执行已取消",
                    ))
                    .is_err()
                {
                    diag!(Warn, Subsystem::AgentRun,
        
                        session_id = %session_id,
                        "向 turn processor 发送 Terminal 失败（通道可能已关闭）"
                    );
                }
            } else {
                diag!(Warn, Subsystem::AgentRun,
        
                    session_id = %session_id,
                    "running=true 但 processor_tx 缺失，无法向 turn processor 发送终止信号"
                );
            }
            return Ok(());
        }

        let history = self
            .stores
            .events
            .list_all_events(session_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let mut latest_turn_id = cancel_snapshot.current_turn_id;
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

        let interrupted = build_turn_terminal_envelope(
            session_id,
            &self.connector_source(None),
            &turn_id,
            TurnTerminalKind::Interrupted,
            Some("检测到未收尾的旧执行，已手动标记为 interrupted".to_string()),
        );
        let _ = cancel_snapshot.tx;
        let _ = self
            .eventing
            .persist_notification(session_id, interrupted)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        Ok(())
    }

    pub async fn recover_interrupted_sessions(&self) -> io::Result<()> {
        let sessions = self.stores.meta.list_sessions().await?;
        for meta in sessions {
            if meta.last_delivery_status != super::types::ExecutionStatus::Running {
                continue;
            }
            diag!(Warn, Subsystem::AgentRun,
        
                session_id = %meta.id,
                "启动恢复：session 上次未正常结束，标记为 interrupted"
            );
            let turn_id = meta
                .last_turn_id
                .clone()
                .unwrap_or_else(|| format!("t_recovery_{}", chrono::Utc::now().timestamp_millis()));
            let notification = build_turn_terminal_envelope(
                &meta.id,
                &SourceInfo {
                    connector_id: "agentdash-server".to_string(),
                    connector_type: "system".to_string(),
                    executor_id: None,
                },
                &turn_id,
                TurnTerminalKind::Interrupted,
                Some("检测到进程重启，已将上次未完成执行标记为 interrupted".to_string()),
            );
            let _ = self
                .eventing
                .persist_notification(&meta.id, notification)
                .await?;
        }
        Ok(())
    }

    pub async fn find_stalled_sessions(&self, stall_timeout_ms: u64) -> Vec<String> {
        self.turn_supervisor
            .find_stalled_sessions(stall_timeout_ms)
            .await
    }

    fn connector_source(&self, executor_id: Option<String>) -> SourceInfo {
        let connector_type = match self.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        SourceInfo {
            connector_id: self.connector.connector_id().to_string(),
            connector_type: connector_type.to_string(),
            executor_id,
        }
    }
}
