use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use agentdash_agent_runtime_wire::{RUNTIME_WIRE_PROTOCOL_REVISION, RuntimeWireEnvelope};
use agentdash_integration_remote_runtime::{
    RemoteRuntimeTransportError, RuntimeWirePlacement, RuntimeWirePlacementEvent,
    RuntimeWirePlacementRequest, RuntimeWirePlacementResolver,
};
use agentdash_relay::{
    RelayMessage, RuntimeRelayAck, RuntimeRelayFrame, RuntimeRelayOpen, RuntimeRelayOpenAck,
    RuntimeRelayProvenance, RuntimeRelayReceive, RuntimeRelayStream, RuntimeRelayStreamId,
};
use async_trait::async_trait;
use tokio::sync::{Mutex, Notify, mpsc, oneshot};

use super::registry::{BackendCommandError, BackendRegistry};

pub struct CloudRuntimeWirePlacementResolver {
    registry: Arc<BackendRegistry>,
    max_in_flight_frames: usize,
}

impl CloudRuntimeWirePlacementResolver {
    pub fn new(registry: Arc<BackendRegistry>, max_in_flight_frames: usize) -> Self {
        Self {
            registry,
            max_in_flight_frames,
        }
    }
}

#[async_trait]
impl RuntimeWirePlacementResolver for CloudRuntimeWirePlacementResolver {
    async fn resolve(
        &self,
        request: RuntimeWirePlacementRequest,
    ) -> Result<Arc<dyn RuntimeWirePlacement>, RemoteRuntimeTransportError> {
        self.registry
            .resolve_runtime_wire_placement(request, self.max_in_flight_frames)
            .await
            .map(|placement| placement as Arc<dyn RuntimeWirePlacement>)
    }
}

pub(crate) struct CloudRuntimeWirePlacement {
    request: RuntimeWirePlacementRequest,
    provenance: RuntimeRelayProvenance,
    stream_id: RuntimeRelayStreamId,
    max_in_flight_frames: usize,
    state: Mutex<Option<RuntimeRelayStream>>,
    inbound_tx: mpsc::UnboundedSender<RuntimeWirePlacementEvent>,
    inbound_rx: Mutex<mpsc::UnboundedReceiver<RuntimeWirePlacementEvent>>,
    open_waiter: Mutex<Option<oneshot::Sender<Result<(), RemoteRuntimeTransportError>>>>,
    open_failure: Mutex<Option<RemoteRuntimeTransportError>>,
    ready: Notify,
    disconnect_acknowledged: Notify,
    /// A disconnected placement is a closed connection epoch. It remains alive long enough for
    /// its driver to observe/ack loss, but can never be reopened or used for a later binding.
    retired: AtomicBool,
    registry: Arc<BackendRegistry>,
}

impl CloudRuntimeWirePlacement {
    pub(crate) fn new(
        request: RuntimeWirePlacementRequest,
        max_in_flight_frames: usize,
        registry: Arc<BackendRegistry>,
    ) -> Arc<Self> {
        let stream_id = RuntimeRelayStreamId(format!(
            "runtime-wire:{}:{}:{}:{}:{}",
            request.host_id,
            request.transport_id,
            request.service_instance_id,
            request.generation.0,
            request.host_incarnation_id
        ));
        let provenance = RuntimeRelayProvenance {
            service_definition_id: request.definition_id.clone(),
            service_instance_id: request.service_instance_id.clone(),
            driver_generation: request.generation,
            host_incarnation_id: request.host_incarnation_id.clone(),
            host_id: request.host_id.clone(),
            transport_id: request.transport_id.clone(),
        };
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            request,
            provenance,
            stream_id,
            max_in_flight_frames,
            state: Mutex::new(None),
            inbound_tx,
            inbound_rx: Mutex::new(inbound_rx),
            open_waiter: Mutex::new(None),
            open_failure: Mutex::new(None),
            ready: Notify::new(),
            disconnect_acknowledged: Notify::new(),
            retired: AtomicBool::new(false),
            registry,
        })
    }

    pub(crate) fn request(&self) -> &RuntimeWirePlacementRequest {
        &self.request
    }

    pub(crate) fn stream_id(&self) -> &RuntimeRelayStreamId {
        &self.stream_id
    }

    pub(crate) fn is_retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }

    fn ensure_active(&self) -> Result<(), RemoteRuntimeTransportError> {
        if self.is_retired() {
            Err(unavailable(
                "Runtime Wire placement connection epoch is retired",
                false,
            ))
        } else {
            Ok(())
        }
    }

    fn open_request(&self, resume_after_sequence: u64) -> RuntimeRelayOpen {
        RuntimeRelayOpen {
            stream_id: self.stream_id.clone(),
            provenance: self.provenance.clone(),
            supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
            resume_after_sequence,
            max_in_flight_frames: self.max_in_flight_frames,
        }
    }

    pub(crate) async fn open(&self) -> Result<(), RemoteRuntimeTransportError> {
        self.ensure_active()?;
        let open = self.open_request(0);
        let (tx, rx) = oneshot::channel();
        *self.open_waiter.lock().await = Some(tx);
        if let Err(error) = self
            .registry
            .send_runtime_wire_message(
                &self.request.host_id,
                RelayMessage::RuntimeWireOpen {
                    id: format!("runtime-wire-open:{}", self.stream_id.0),
                    payload: open,
                },
            )
            .await
        {
            let error = backend_error(error);
            *self.open_failure.lock().await = Some(error.clone());
            self.ready.notify_waiters();
            return Err(error);
        }
        tokio::time::timeout(std::time::Duration::from_secs(10), rx)
            .await
            .map_err(|_| unavailable("Runtime Wire open timed out", true))?
            .map_err(|_| unavailable("Runtime Wire open waiter was dropped", true))?
    }

    pub(crate) async fn wait_until_open(&self) -> Result<(), RemoteRuntimeTransportError> {
        self.ensure_active()?;
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let notified = self.ready.notified();
                tokio::pin!(notified);
                self.ensure_active()?;
                if self.state.lock().await.is_some() {
                    return Ok(());
                }
                if let Some(error) = self.open_failure.lock().await.clone() {
                    return Err(error);
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| unavailable("Runtime Wire concurrent open timed out", true))?
    }

    pub(crate) async fn disconnect(&self) {
        if self.retired.swap(true, Ordering::AcqRel) {
            return;
        }
        let disconnected = if let Some(state) = self.state.lock().await.as_mut() {
            let disconnected = state.disconnect().is_some();
            if disconnected {
                state.abandon_unacknowledged();
            }
            disconnected
        } else {
            false
        };
        if disconnected {
            let acknowledged = self.disconnect_acknowledged.notified();
            let _ = self
                .inbound_tx
                .send(RuntimeWirePlacementEvent::Disconnected {
                    reason: "Runtime Wire backend connection was lost".to_string(),
                });
            let _ = tokio::time::timeout(std::time::Duration::from_secs(10), acknowledged).await;
        } else {
            let failure = unavailable(
                "Runtime Wire placement disconnected before open completed",
                true,
            );
            *self.open_failure.lock().await = Some(failure.clone());
            if let Some(waiter) = self.open_waiter.lock().await.take() {
                let _ = waiter.send(Err(failure));
            }
            self.ready.notify_waiters();
        }
    }

    pub(crate) async fn accept_open(
        &self,
        ack: RuntimeRelayOpenAck,
    ) -> Result<Vec<RuntimeRelayFrame>, RemoteRuntimeTransportError> {
        self.ensure_active()?;
        let mut state = self.state.lock().await;
        let replay = if let Some(existing) = state.as_mut() {
            existing
                .acknowledge(RuntimeRelayAck {
                    stream_id: self.stream_id.clone(),
                    through_sequence: ack.accepted_after_sequence,
                })
                .map_err(transport_error)?;
            existing
                .reconnect(&self.provenance)
                .map_err(transport_error)?
        } else {
            *state = Some(
                RuntimeRelayStream::connect(self.open_request(0), &ack).map_err(transport_error)?,
            );
            Vec::new()
        };
        if let Some(waiter) = self.open_waiter.lock().await.take() {
            let _ = waiter.send(Ok(()));
        } else {
            let _ = self.inbound_tx.send(RuntimeWirePlacementEvent::Reconnected);
        }
        self.ready.notify_waiters();
        Ok(replay)
    }

    pub(crate) async fn accept_frame(
        &self,
        frame: RuntimeRelayFrame,
    ) -> Result<RuntimeRelayAck, RemoteRuntimeTransportError> {
        self.ensure_active()?;
        let mut state = self.state.lock().await;
        let state = state
            .as_mut()
            .ok_or_else(|| unavailable("Runtime Wire stream is not negotiated", true))?;
        if let RuntimeRelayReceive::Accepted(envelope) =
            state.receive(frame).map_err(transport_error)?
        {
            self.inbound_tx
                .send(RuntimeWirePlacementEvent::Frame(envelope))
                .map_err(|_| unavailable("Runtime Wire receiver is closed", false))?;
        }
        Ok(state.inbound_ack())
    }

    pub(crate) async fn accept_ack(
        &self,
        ack: RuntimeRelayAck,
    ) -> Result<(), RemoteRuntimeTransportError> {
        self.ensure_active()?;
        self.state
            .lock()
            .await
            .as_mut()
            .ok_or_else(|| unavailable("Runtime Wire stream is not negotiated", true))?
            .acknowledge(ack)
            .map_err(transport_error)
    }

    pub(crate) async fn reject_open(&self, reason: String) {
        let failure = unavailable(reason, false);
        *self.open_failure.lock().await = Some(failure.clone());
        if let Some(waiter) = self.open_waiter.lock().await.take() {
            let _ = waiter.send(Err(failure));
        }
        self.ready.notify_waiters();
    }
}

#[async_trait]
impl RuntimeWirePlacement for CloudRuntimeWirePlacement {
    async fn send(&self, envelope: RuntimeWireEnvelope) -> Result<(), RemoteRuntimeTransportError> {
        self.ensure_active()?;
        let frame = self
            .state
            .lock()
            .await
            .as_mut()
            .ok_or_else(|| unavailable("Runtime Wire stream is not negotiated", true))?
            .enqueue(envelope)
            .map_err(transport_error)?;
        self.registry
            .send_runtime_wire_message(
                &self.request.host_id,
                RelayMessage::RuntimeWireFrame {
                    id: format!(
                        "runtime-wire-frame:{}:{}",
                        frame.stream_id.0, frame.sequence
                    ),
                    payload: Box::new(frame),
                },
            )
            .await
            .map_err(backend_error)
    }

    async fn receive(&self) -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError> {
        self.inbound_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| unavailable("Runtime Wire receive channel is closed", false))
    }

    async fn acknowledge_disconnect(&self) {
        self.disconnect_acknowledged.notify_one();
    }
}

fn backend_error(error: BackendCommandError) -> RemoteRuntimeTransportError {
    unavailable(error.to_string(), true)
}

fn transport_error(
    error: agentdash_relay::RuntimeRelayTransportError,
) -> RemoteRuntimeTransportError {
    RemoteRuntimeTransportError::Protocol {
        reason: error.to_string(),
        critical: true,
    }
}

fn unavailable(reason: impl Into<String>, retryable: bool) -> RemoteRuntimeTransportError {
    RemoteRuntimeTransportError::Unavailable {
        reason: reason.into(),
        retryable,
    }
}
