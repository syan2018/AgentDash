use agentdash_spi::ConnectorError;

use super::SessionLaunchDeps;
use super::commit::CommittedTurn;
use crate::session::hub_support::TurnTerminalKind;

pub(in crate::session) struct AttachedTurn {
    pub turn_id: String,
}

pub(in crate::session) struct StreamIngestionAttacher {
    deps: SessionLaunchDeps,
}

impl StreamIngestionAttacher {
    pub fn new(deps: SessionLaunchDeps) -> Self {
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
                effects: self.deps.effects.clone(),
            },
            crate::session::turn_processor::SessionTurnProcessorConfig {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                source: prepared.source.clone(),
                hook_session: prepared.hook_session,
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
                    let _ =
                        processor_tx.send(crate::session::turn_processor::TurnEvent::Notification(
                            Box::new(notification),
                        ));
                }
                Err(e) => {
                    tracing::error!("执行流错误 session_id={}: {}", session_id, e);
                    let (kind, message) =
                        resolve_stream_terminal(&turn_supervisor, &session_id, &turn_id, Some(e))
                            .await;
                    let _ =
                        processor_tx.send(crate::session::turn_processor::TurnEvent::Terminal {
                            kind,
                            message,
                        });
                    return;
                }
            }
        }
        let (kind, message) =
            resolve_stream_terminal(&turn_supervisor, &session_id, &turn_id, None).await;
        let _ = processor_tx
            .send(crate::session::turn_processor::TurnEvent::Terminal { kind, message });
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
