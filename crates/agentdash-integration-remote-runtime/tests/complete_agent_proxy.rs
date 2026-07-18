use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use agentdash_agent_runtime_wire::{
    RUNTIME_WIRE_PROTOCOL_REVISION, RuntimeWireAgentBindingTarget,
    RuntimeWireAgentChangeNotification, RuntimeWireAgentHostCallbackRequest,
    RuntimeWireAgentHostCallbackResponse, RuntimeWireAgentServiceRequest,
    RuntimeWireAgentServiceResponse, RuntimeWireEnvelope, RuntimeWireFrame, RuntimeWireFrameId,
    RuntimeWireNotification, RuntimeWireRequest, RuntimeWireResponse,
};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentChange, AgentChangePayload,
    AgentChangesQuery, AgentCommand, AgentCommandEnvelope, AgentCommandId, AgentCommandMeta,
    AgentEffectIdentity, AgentHostCallbackError, AgentHostCallbackMeta, AgentHostCallbacks,
    AgentIdempotencyKey, AgentInput, AgentInputContent, AgentLifecycleStatus, AgentReadQuery,
    AgentReceiptState, AgentServiceErrorCode, AgentServiceInstanceId, AgentSourceCoordinate,
    AgentSourceCursor, AgentToolInvocation, AgentToolName, AgentToolResult, AgentTurnId,
    CompleteAgentService,
};
use agentdash_integration_remote_runtime::{
    RemoteCompleteAgentService, RemoteRuntimeTransportError, RuntimeWirePlacement,
    RuntimeWirePlacementEvent,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::{Mutex, mpsc};

struct LoopbackPlacement {
    sent: Mutex<Vec<RuntimeWireEnvelope>>,
    events_tx: mpsc::UnboundedSender<RuntimeWirePlacementEvent>,
    events_rx: Mutex<mpsc::UnboundedReceiver<RuntimeWirePlacementEvent>>,
    next_remote_frame_id: AtomicU64,
    execute_requests: AtomicU64,
}

impl LoopbackPlacement {
    fn new() -> Arc<Self> {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            sent: Mutex::new(Vec::new()),
            events_tx,
            events_rx: Mutex::new(events_rx),
            next_remote_frame_id: AtomicU64::new(1),
            execute_requests: AtomicU64::new(0),
        })
    }

    fn remote_envelope(&self, critical: bool, frame: RuntimeWireFrame) -> RuntimeWireEnvelope {
        RuntimeWireEnvelope {
            protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
            frame_id: RuntimeWireFrameId(self.next_remote_frame_id.fetch_add(1, Ordering::Relaxed)),
            critical,
            frame,
        }
    }

    fn inject(&self, critical: bool, frame: RuntimeWireFrame) {
        self.events_tx
            .send(RuntimeWirePlacementEvent::Frame(Box::new(
                self.remote_envelope(critical, frame),
            )))
            .expect("proxy receiver");
    }

    fn inject_exact(&self, envelope: RuntimeWireEnvelope) {
        self.events_tx
            .send(RuntimeWirePlacementEvent::Frame(Box::new(envelope)))
            .expect("proxy receiver");
    }

    fn disconnect(&self, reason: &str) {
        self.events_tx
            .send(RuntimeWirePlacementEvent::Disconnected {
                reason: reason.to_owned(),
            })
            .expect("proxy receiver");
    }
}

#[async_trait]
impl RuntimeWirePlacement for LoopbackPlacement {
    async fn send(&self, envelope: RuntimeWireEnvelope) -> Result<(), RemoteRuntimeTransportError> {
        self.sent.lock().await.push(envelope.clone());
        let RuntimeWireFrame::Request(request) = envelope.frame else {
            return Ok(());
        };
        let RuntimeWireRequest::AgentService(request) = *request else {
            return Ok(());
        };
        if let RuntimeWireAgentServiceRequest::Execute { target, command } = *request {
            self.execute_requests.fetch_add(1, Ordering::Relaxed);
            assert_eq!(target.binding_generation, AgentBindingGeneration(9));
            assert_eq!(
                command.meta.binding_generation,
                AgentBindingGeneration(9),
                "proxy must rewrite only at the Runtime Wire boundary"
            );
            let response = RuntimeWireAgentServiceResponse::Execute(Ok(Box::new(
                agentdash_agent_service_api::AgentCommandReceipt {
                    command_id: command.meta.command_id,
                    effect_id: command.meta.effect_id,
                    source: command.source,
                    state: AgentReceiptState::Accepted,
                    snapshot_revision: None,
                    initial_context: None,
                },
            )));
            self.inject(
                true,
                RuntimeWireFrame::Response {
                    request_frame_id: envelope.frame_id,
                    response: RuntimeWireResponse::AgentService(response),
                },
            );
        }
        Ok(())
    }

    async fn receive(&self) -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError> {
        self.events_rx.lock().await.recv().await.ok_or_else(|| {
            RemoteRuntimeTransportError::Unavailable {
                reason: "test placement closed".to_owned(),
                retryable: true,
            }
        })
    }
}

#[derive(Default)]
struct RecordingCallbacks {
    generations: Mutex<Vec<AgentBindingGeneration>>,
}

#[async_trait]
impl AgentHostCallbacks for RecordingCallbacks {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.generations
            .lock()
            .await
            .push(call.meta.binding_generation);
        Ok(AgentToolResult::Completed {
            output: json!({"ok": true}),
        })
    }

    async fn invoke_hook(
        &self,
        _: agentdash_agent_service_api::AgentHookInvocation,
    ) -> Result<agentdash_agent_service_api::AgentHookDecision, AgentHostCallbackError> {
        unreachable!("tool callback test")
    }
}

fn target() -> RuntimeWireAgentBindingTarget {
    RuntimeWireAgentBindingTarget {
        service_instance_id: AgentServiceInstanceId::new("remote-service").expect("service"),
        binding_generation: AgentBindingGeneration(9),
    }
}

fn meta(id: &str, generation: u64) -> AgentCommandMeta {
    AgentCommandMeta {
        command_id: AgentCommandId::new(format!("command-{id}")).expect("command"),
        effect_id: AgentEffectIdentity::new(format!("effect-{id}")).expect("effect"),
        idempotency_key: AgentIdempotencyKey::new(format!("idem-{id}")).expect("idempotency"),
        binding_generation: AgentBindingGeneration(generation),
        expected_snapshot_revision: None,
    }
}

fn execute(id: &str, generation: u64) -> AgentCommandEnvelope {
    AgentCommandEnvelope {
        meta: meta(id, generation),
        source: AgentSourceCoordinate::new("thread-1").expect("source"),
        command: AgentCommand::SubmitInput {
            input: AgentInput {
                content: vec![AgentInputContent::Text {
                    text: "hello".to_owned(),
                }],
            },
        },
    }
}

async fn wait_until(mut predicate: impl FnMut() -> bool) {
    for _ in 0..100 {
        if predicate() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("condition was not reached");
}

#[tokio::test]
async fn proxy_rewrites_generation_and_replays_completed_duplicate_without_second_effect() {
    let placement = LoopbackPlacement::new();
    let callbacks = Arc::new(RecordingCallbacks::default());
    let proxy = RemoteCompleteAgentService::new(
        AgentBindingGeneration(3),
        target(),
        placement.clone(),
        callbacks,
    );
    let command = execute("same", 3);

    let first = proxy.execute(command.clone()).await.expect("first execute");
    let replay = proxy.execute(command).await.expect("duplicate replay");

    assert_eq!(first, replay);
    assert_eq!(placement.execute_requests.load(Ordering::Relaxed), 1);
    let sent = placement.sent.lock().await;
    assert!(sent.iter().any(|envelope| matches!(
        &envelope.frame,
        RuntimeWireFrame::Ack(ack) if ack.through_frame_id == RuntimeWireFrameId(1)
    )));
}

#[tokio::test]
async fn stale_local_generation_is_fenced_before_remote_dispatch() {
    let placement = LoopbackPlacement::new();
    let proxy = RemoteCompleteAgentService::new(
        AgentBindingGeneration(3),
        target(),
        placement.clone(),
        Arc::new(RecordingCallbacks::default()),
    );

    let error = proxy
        .execute(execute("stale", 2))
        .await
        .expect_err("stale generation");

    assert_eq!(error.code, AgentServiceErrorCode::StaleBindingGeneration);
    assert_eq!(placement.execute_requests.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn pushed_change_replay_is_idempotent_and_frame_gap_loses_the_binding() {
    let placement = LoopbackPlacement::new();
    let proxy = RemoteCompleteAgentService::new(
        AgentBindingGeneration(3),
        target(),
        placement.clone(),
        Arc::new(RecordingCallbacks::default()),
    );
    let change = RuntimeWireAgentChangeNotification {
        target: target(),
        source: AgentSourceCoordinate::new("thread-1").expect("source"),
        change: AgentChange {
            cursor: AgentSourceCursor::new("cursor-1").expect("cursor"),
            source_revision: None,
            occurred_at_ms: 1,
            payload: AgentChangePayload::LifecycleChanged {
                status: AgentLifecycleStatus::Active,
            },
        },
    };
    let first = placement.remote_envelope(
        true,
        RuntimeWireFrame::Notification(Box::new(RuntimeWireNotification::AgentChange(Box::new(
            change,
        )))),
    );
    placement.inject_exact(first.clone());
    placement.inject_exact(first);
    wait_until(|| {
        placement.sent.try_lock().is_ok_and(|sent| {
            sent.iter()
                .filter(|frame| matches!(frame.frame, RuntimeWireFrame::Ack(_)))
                .count()
                >= 2
        })
    })
    .await;

    let page = proxy
        .changes(AgentChangesQuery {
            source: AgentSourceCoordinate::new("thread-1").expect("source"),
            after: None,
            limit: 16,
        })
        .await
        .expect("buffered changes");
    assert_eq!(page.changes.len(), 1);

    placement.inject_exact(RuntimeWireEnvelope {
        protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
        frame_id: RuntimeWireFrameId(3),
        critical: true,
        frame: RuntimeWireFrame::Ack(agentdash_agent_runtime_wire::RuntimeWireAck {
            through_frame_id: RuntimeWireFrameId(1),
        }),
    });
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        proxy.read(AgentReadQuery {
            source: AgentSourceCoordinate::new("thread-1").expect("source"),
            at_revision: None,
        }),
    )
    .await
    .expect("gap must terminate pending work")
    .expect_err("frame gap loses proxy binding");
    assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
}

#[tokio::test]
async fn reverse_callback_rewrites_source_generation_and_preserves_request_correlation() {
    let placement = LoopbackPlacement::new();
    let callbacks = Arc::new(RecordingCallbacks::default());
    let _proxy = RemoteCompleteAgentService::new(
        AgentBindingGeneration(3),
        target(),
        placement.clone(),
        callbacks.clone(),
    );
    placement.inject(
        true,
        RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::AgentHostCallback(Box::new(
            RuntimeWireAgentHostCallbackRequest::Tool(AgentToolInvocation {
                meta: AgentHostCallbackMeta {
                    route_id: AgentCallbackRouteId::new("route-1").expect("route"),
                    binding_generation: AgentBindingGeneration(9),
                    source: AgentSourceCoordinate::new("thread-1").expect("source"),
                    turn_id: AgentTurnId::new("turn-1").expect("turn"),
                    item_id: None,
                    interaction_id: None,
                    effect_id: AgentEffectIdentity::new("callback-effect").expect("effect"),
                    idempotency_key: AgentIdempotencyKey::new("callback-idem")
                        .expect("idempotency"),
                    deadline_at_ms: u64::MAX,
                },
                tool: AgentToolName::new("search").expect("tool"),
                arguments: json!({"query": "runtime"}),
            }),
        )))),
    );
    wait_until(|| {
        callbacks
            .generations
            .try_lock()
            .is_ok_and(|values| !values.is_empty())
    })
    .await;

    assert_eq!(
        callbacks.generations.lock().await.as_slice(),
        &[AgentBindingGeneration(3)]
    );
    let sent = placement.sent.lock().await;
    assert!(sent.iter().any(|envelope| matches!(
        &envelope.frame,
        RuntimeWireFrame::Response {
            request_frame_id: RuntimeWireFrameId(1),
            response: RuntimeWireResponse::AgentHostCallback(
                RuntimeWireAgentHostCallbackResponse::Tool(Ok(_))
            ),
        }
    )));
}

#[tokio::test]
async fn disconnect_is_unavailable_never_a_fabricated_completion() {
    let placement = LoopbackPlacement::new();
    let proxy = RemoteCompleteAgentService::new(
        AgentBindingGeneration(3),
        target(),
        placement.clone(),
        Arc::new(RecordingCallbacks::default()),
    );
    placement.disconnect("network lost");
    tokio::task::yield_now().await;

    let error = proxy
        .execute(execute("after-disconnect", 3))
        .await
        .expect_err("disconnect");
    assert_eq!(error.code, AgentServiceErrorCode::Unavailable);
    assert!(error.retryable);
}
