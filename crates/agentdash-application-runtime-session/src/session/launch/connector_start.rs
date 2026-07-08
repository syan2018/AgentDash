use agentdash_diagnostics::{Subsystem, diag};
use agentdash_spi::{ConnectorError, ExecutionStream};

use super::deps::ConnectorStartDeps;
use super::preparation::PreparedTurn;
use crate::session::hub_support::TurnTerminalKind;
use crate::session::turn_processor::{
    SessionTurnProcessorDeps, TurnTerminalDispatch, process_turn_terminal,
};

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

        diag!(Debug, Subsystem::SessionLaunch,

            session_id = %prepared.session_id,
            turn_id = %prepared.turn_id,
            hook_facets = prepared.runtime_delegate_composition.hook_facets,
            mailbox_turn_boundary = prepared.runtime_delegate_composition.mailbox_turn_boundary,
            admission_tool_policy = prepared.runtime_delegate_composition.admission_tool_policy,
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
                diag!(Debug, Subsystem::SessionLaunch,

                    session_id = %prepared.session_id,
                    turn_id = %prepared.turn_id,
                    "connector starter accepted connector stream"
                );
                stream
            }
            Err(error) => {
                process_turn_terminal(
                    &SessionTurnProcessorDeps {
                        turn_supervisor: self.deps.turn_supervisor.clone(),
                        eventing: self.deps.eventing.clone(),
                        terminal_boundary: self.deps.terminal_boundary.clone(),
                    },
                    TurnTerminalDispatch {
                        session_id: prepared.session_id.clone(),
                        turn_id: prepared.turn_id.clone(),
                        source: prepared.source.clone(),
                        terminal_kind: TurnTerminalKind::Failed,
                        terminal_message: Some(error.to_string()),
                        terminal_diagnostic: None,
                        effect_mode: agentdash_application_ports::agent_run_control_effect::AgentRunTerminalControlEffectMode::ImmediateAll,
                        hook_runtime: prepared.hook_runtime.clone(),
                        post_turn_handler: prepared.post_turn_handler.clone(),
                    },
                )
                .await;
                return Err(error);
            }
        };

        Ok(ConnectorAcceptedTurn { prepared, stream })
    }
}
