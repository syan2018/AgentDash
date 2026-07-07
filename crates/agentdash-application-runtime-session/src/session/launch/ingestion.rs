use agentdash_spi::ConnectorError;

use super::commit::CommittedTurn;
use super::deps::StreamIngestionDeps;
use crate::session::hub_support::{TurnTerminalKind, parse_turn_terminal_event_from_envelope};
use agentdash_diagnostics::{Subsystem, diag};

pub(in crate::session) struct AttachedTurn {
    pub turn_id: String,
}

pub(in crate::session) struct StreamIngestionAttacher {
    deps: StreamIngestionDeps,
}

impl StreamIngestionAttacher {
    pub(super) fn new(deps: StreamIngestionDeps) -> Self {
        Self { deps }
    }

    pub async fn attach(&self, committed: CommittedTurn) -> AttachedTurn {
        let mut accepted = committed.accepted;
        let prepared = accepted.prepared;
        let session_id = prepared.session_id;
        let turn_id = prepared.turn_id;

        let processor = crate::session::turn_processor::SessionTurnProcessor::spawn(
            crate::session::turn_processor::SessionTurnProcessorDeps {
                turn_supervisor: self.deps.turn_supervisor.clone(),
                eventing: self.deps.eventing.clone(),
                terminal_boundary: self.deps.terminal_boundary.clone(),
            },
            crate::session::turn_processor::SessionTurnProcessorConfig {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                source: prepared.source.clone(),
                hook_runtime: prepared.hook_runtime,
                post_turn_handler: prepared.post_turn_handler,
            },
        );

        let processor_tx = processor.tx();

        self.deps
            .turn_supervisor
            .register_processor_tx(&session_id, processor_tx.clone())
            .await;

        let stream_adapter = spawn_stream_adapter(
            self.deps.turn_supervisor.clone(),
            session_id.clone(),
            turn_id.clone(),
            &mut accepted.stream,
            processor_tx,
        );
        self.deps
            .turn_supervisor
            .register_stream_adapter_handle(&session_id, stream_adapter.abort_handle())
            .await;

        AttachedTurn { turn_id }
    }
}

fn spawn_stream_adapter(
    turn_supervisor: crate::session::turn_supervisor::TurnSupervisor,
    session_id: String,
    turn_id: String,
    stream: &mut agentdash_spi::ExecutionStream,
    processor_tx: tokio::sync::mpsc::UnboundedSender<crate::session::turn_processor::TurnEvent>,
) -> tokio::task::JoinHandle<()> {
    use futures::StreamExt;
    let mut stream = std::mem::replace(stream, Box::pin(futures::stream::empty()));
    tokio::spawn(async move {
        while let Some(next) = stream.next().await {
            match next {
                Ok(notification) => {
                    if let Some((terminal_turn_id, kind, message, diagnostic)) =
                        parse_turn_terminal_event_from_envelope(&notification)
                        && terminal_turn_id == turn_id
                    {
                        let _ = processor_tx.send(
                            crate::session::turn_processor::TurnEvent::Terminal {
                                kind,
                                message,
                                diagnostic,
                            },
                        );
                        return;
                    }
                    let _ =
                        processor_tx.send(crate::session::turn_processor::TurnEvent::Notification(
                            Box::new(notification),
                        ));
                }
                Err(e) => {
                    diag!(
                        Error,
                        Subsystem::SessionLaunch,
                        "执行流错误 session_id={}: {}",
                        session_id,
                        e
                    );
                    let (kind, message) =
                        resolve_stream_terminal(&turn_supervisor, &session_id, &turn_id, Some(e))
                            .await;
                    let _ =
                        processor_tx.send(crate::session::turn_processor::TurnEvent::Terminal {
                            kind,
                            message,
                            diagnostic: None,
                        });
                    return;
                }
            }
        }
        let (kind, message) =
            resolve_stream_terminal(&turn_supervisor, &session_id, &turn_id, None).await;
        let _ = processor_tx.send(crate::session::turn_processor::TurnEvent::Terminal {
            kind,
            message,
            diagnostic: None,
        });
    })
}

async fn resolve_stream_terminal(
    turn_supervisor: &crate::session::turn_supervisor::TurnSupervisor,
    session_id: &str,
    turn_id: &str,
    error: Option<ConnectorError>,
) -> (TurnTerminalKind, Option<String>) {
    if let Some(terminal) = turn_supervisor
        .cancel_interrupted_terminal(session_id, turn_id)
        .await
    {
        terminal
    } else if let Some(e) = error {
        (TurnTerminalKind::Failed, Some(e.to_string()))
    } else {
        (TurnTerminalKind::Completed, None)
    }
}
