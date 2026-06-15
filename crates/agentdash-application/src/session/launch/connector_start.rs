use agentdash_spi::{ConnectorError, ExecutionStream};

use super::deps::ConnectorStartDeps;
use super::preparation::PreparedTurn;
use crate::session::hub_support::{TurnTerminalKind, build_turn_terminal_envelope};

/// Session turn accepted boundary: connector.prompt 已返回 ExecutionStream。
///
/// 这里不表达 command receipt accepted、mailbox delivery accepted 或 frame/bootstrap
/// accepted；这些边界分别由 AgentRun mailbox/receipt 和 commit stage 负责。
pub(in crate::session) struct ConnectorAcceptedTurn {
    pub prepared: PreparedTurn,
    pub stream: ExecutionStream,
}

pub(in crate::session) struct ConnectorStarter {
    deps: ConnectorStartDeps,
}

impl ConnectorStarter {
    pub(super) fn new(deps: ConnectorStartDeps) -> Self {
        Self { deps }
    }

    pub async fn start(
        &self,
        mut prepared: PreparedTurn,
    ) -> Result<ConnectorAcceptedTurn, ConnectorError> {
        let Some(context) = prepared.connector_context.take() else {
            return Err(ConnectorError::Runtime(
                "PreparedTurn 缺少 connector context，无法启动 connector".to_string(),
            ));
        };

        tracing::debug!(
            session_id = %prepared.session_id,
            turn_id = %prepared.turn_id,
            "connector starter calling connector.prompt"
        );
        let stream = match self
            .deps
            .connector
            .prompt(
                &prepared.session_id,
                prepared.resolved_follow_up_session_id.as_deref(),
                &prepared.resolved_payload.prompt_payload,
                context,
            )
            .await
        {
            Ok(stream) => {
                tracing::debug!(
                    session_id = %prepared.session_id,
                    turn_id = %prepared.turn_id,
                    "connector starter accepted connector stream"
                );
                stream
            }
            Err(error) => {
                self.deps
                    .turn_supervisor
                    .clear_turn_and_hook(&prepared.session_id)
                    .await;
                let failed = build_turn_terminal_envelope(
                    &prepared.session_id,
                    &prepared.source,
                    &prepared.turn_id,
                    TurnTerminalKind::Failed,
                    Some(error.to_string()),
                );
                let _ = self
                    .deps
                    .eventing
                    .persist_notification(&prepared.session_id, failed)
                    .await;
                return Err(error);
            }
        };

        Ok(ConnectorAcceptedTurn { prepared, stream })
    }
}
