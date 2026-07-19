use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Weak},
    time::Duration,
};

use agentdash_agent_runtime_wire::{
    RUNTIME_WIRE_PROTOCOL_REVISION, RuntimeWireAuthenticatedTransport, RuntimeWirePlacementAck,
    RuntimeWirePlacementClosed, RuntimeWirePlacementFrame, RuntimeWirePlacementLossCode,
    RuntimeWirePlacementLost, RuntimeWirePlacementOpen, RuntimeWirePlacementOpenAck,
    RuntimeWirePlacementOpenRejected, RuntimeWirePlacementProtocolError,
    RuntimeWirePlacementProvenance, RuntimeWirePlacementSequence, RuntimeWirePlacementStreamId,
    RuntimeWireServiceOfferAdvertisement,
};
use agentdash_integration_remote_runtime::{
    RemoteRuntimeTransportError, RuntimeWirePlacement, RuntimeWirePlacementEvent,
};
use agentdash_relay::RelayMessage;
use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{Mutex, Notify, mpsc, oneshot};

pub const RUNTIME_WIRE_CONTROL_QUEUE_CAPACITY: usize = 32;
pub const RUNTIME_WIRE_CRITICAL_QUEUE_CAPACITY: usize = 128;
pub const RUNTIME_WIRE_STREAM_QUEUE_CAPACITY: usize = 128;
pub const RUNTIME_WIRE_DEFAULT_MAX_IN_FLIGHT: u32 = 64;
const RUNTIME_WIRE_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const RUNTIME_WIRE_DISCONNECT_ACK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeWireAdvertisementAdmission {
    Accepted,
    Replayed,
}

#[derive(Debug, Error)]
pub enum CloudRuntimeWireError {
    #[error("runtime wire backend is not connected: {backend_id}")]
    BackendDisconnected { backend_id: String },
    #[error("runtime wire transport provenance is stale")]
    StaleTransport,
    #[error("runtime wire endpoint is not advertised: {endpoint_id}")]
    EndpointMissing { endpoint_id: String },
    #[error("runtime wire advertisement is stale")]
    AdvertisementExpired,
    #[error("runtime wire connection epoch has been retired")]
    RetiredEpoch,
    #[error("runtime wire stream does not exist: {stream_id:?}")]
    StreamMissing {
        stream_id: RuntimeWirePlacementStreamId,
    },
    #[error("runtime wire queue overflowed")]
    QueueOverflow,
    #[error("runtime wire Complete Agent admission failed: {reason}")]
    CompleteAgentAdmission { reason: String },
    #[error("runtime wire placement open timed out")]
    OpenTimeout,
    #[error("runtime wire placement open was rejected: {reason}")]
    OpenRejected { reason: String },
    #[error(transparent)]
    Protocol(#[from] RuntimeWirePlacementProtocolError),
}

pub struct RuntimeWireConnectionQueues {
    pub transport: RuntimeWireAuthenticatedTransport,
    pub control_rx: mpsc::Receiver<RelayMessage>,
    pub critical_rx: mpsc::Receiver<RelayMessage>,
}

#[derive(Clone)]
struct RuntimeWireConnection {
    transport: RuntimeWireAuthenticatedTransport,
    control_tx: mpsc::Sender<RelayMessage>,
    critical_tx: mpsc::Sender<RelayMessage>,
}

struct RuntimeWireStream {
    provenance: RuntimeWirePlacementProvenance,
    max_in_flight: usize,
    next_outbound_sequence: u64,
    last_inbound_sequence: u64,
    outbound_unacked: BTreeMap<RuntimeWirePlacementSequence, RelayMessage>,
    inbound_tx: mpsc::Sender<RuntimeWirePlacementEvent>,
    open_tx: Option<
        oneshot::Sender<Result<RuntimeWirePlacementOpenAck, RuntimeWirePlacementOpenRejected>>,
    >,
    disconnect_ack: Arc<Notify>,
    disconnected: bool,
}

#[derive(Default)]
struct RuntimeWireRegistryState {
    connections: BTreeMap<String, RuntimeWireConnection>,
    advertisements: BTreeMap<(String, String), RuntimeWireServiceOfferAdvertisement>,
    streams: BTreeMap<RuntimeWirePlacementStreamId, RuntimeWireStream>,
    retired_epochs: BTreeSet<String>,
    next_stream_id: u64,
}

pub struct CloudRuntimeWirePlacementRegistry {
    state: Mutex<RuntimeWireRegistryState>,
}

impl CloudRuntimeWirePlacementRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(RuntimeWireRegistryState {
                next_stream_id: 1,
                ..RuntimeWireRegistryState::default()
            }),
        })
    }

    pub async fn register_connection(
        &self,
        backend_id: &str,
        transport_id: String,
    ) -> Result<RuntimeWireConnectionQueues, CloudRuntimeWireError> {
        let transport = RuntimeWireAuthenticatedTransport {
            backend_id: backend_id.to_owned(),
            transport_id,
        };
        let (control_tx, control_rx) = mpsc::channel(RUNTIME_WIRE_CONTROL_QUEUE_CAPACITY);
        let (critical_tx, critical_rx) = mpsc::channel(RUNTIME_WIRE_CRITICAL_QUEUE_CAPACITY);
        let mut state = self.state.lock().await;
        if state.connections.contains_key(backend_id) {
            return Err(CloudRuntimeWireError::StaleTransport);
        }
        state.connections.insert(
            backend_id.to_owned(),
            RuntimeWireConnection {
                transport: transport.clone(),
                control_tx,
                critical_tx,
            },
        );
        Ok(RuntimeWireConnectionQueues {
            transport,
            control_rx,
            critical_rx,
        })
    }

    pub async fn advertise(
        &self,
        transport: &RuntimeWireAuthenticatedTransport,
        advertisement: RuntimeWireServiceOfferAdvertisement,
        now_unix_ms: i64,
    ) -> Result<RuntimeWireAdvertisementAdmission, CloudRuntimeWireError> {
        advertisement.validate_shape()?;
        if !advertisement.is_fresh_at(now_unix_ms) {
            return Err(CloudRuntimeWireError::AdvertisementExpired);
        }
        let mut state = self.state.lock().await;
        ensure_connection(&state, transport)?;
        if state
            .retired_epochs
            .contains(&advertisement_epoch_key(&advertisement))
        {
            return Err(CloudRuntimeWireError::RetiredEpoch);
        }
        let key = (
            transport.backend_id.clone(),
            advertisement.endpoint_id.clone(),
        );
        if let Some(current) = state.advertisements.get(&key) {
            if advertisement.revision < current.revision {
                return Err(
                    RuntimeWirePlacementProtocolError::AdvertisementRevisionRegression {
                        current: current.revision,
                        received: advertisement.revision,
                    }
                    .into(),
                );
            }
            if advertisement.revision == current.revision {
                if advertisement.digest == current.digest {
                    return Ok(RuntimeWireAdvertisementAdmission::Replayed);
                }
                return Err(
                    RuntimeWirePlacementProtocolError::AdvertisementRevisionConflict {
                        revision: advertisement.revision,
                    }
                    .into(),
                );
            }
        }
        state.advertisements.insert(key, advertisement);
        Ok(RuntimeWireAdvertisementAdmission::Accepted)
    }

    pub async fn withdraw(
        &self,
        transport: &RuntimeWireAuthenticatedTransport,
        endpoint_id: &str,
        revision: agentdash_agent_runtime_wire::RuntimeWireAdvertisementRevision,
    ) -> Result<(), CloudRuntimeWireError> {
        let mut state = self.state.lock().await;
        ensure_connection(&state, transport)?;
        let key = (transport.backend_id.clone(), endpoint_id.to_owned());
        if let Some(current) = state.advertisements.get(&key) {
            if revision < current.revision {
                return Err(
                    RuntimeWirePlacementProtocolError::AdvertisementRevisionRegression {
                        current: current.revision,
                        received: revision,
                    }
                    .into(),
                );
            }
        }
        state.advertisements.remove(&key);
        Ok(())
    }

    pub async fn advertisement(
        &self,
        backend_id: &str,
        endpoint_id: &str,
    ) -> Option<RuntimeWireServiceOfferAdvertisement> {
        self.state
            .lock()
            .await
            .advertisements
            .get(&(backend_id.to_owned(), endpoint_id.to_owned()))
            .cloned()
    }

    pub async fn send_control(
        &self,
        transport: &RuntimeWireAuthenticatedTransport,
        message: RelayMessage,
    ) -> Result<(), CloudRuntimeWireError> {
        let sender = {
            let state = self.state.lock().await;
            ensure_connection(&state, transport)?.control_tx.clone()
        };
        sender
            .try_send(message)
            .map_err(|_| CloudRuntimeWireError::QueueOverflow)
    }

    pub async fn open(
        self: &Arc<Self>,
        provenance: RuntimeWirePlacementProvenance,
        max_in_flight: u32,
        now_unix_ms: i64,
    ) -> Result<Arc<dyn RuntimeWirePlacement>, CloudRuntimeWireError> {
        let (open_rx, inbound_rx, disconnect_ack, control_tx, open_message, stream_id) = {
            let mut state = self.state.lock().await;
            let connection = ensure_connection(&state, &provenance.transport)?.clone();
            let advertisement = state
                .advertisements
                .get(&(
                    provenance.transport.backend_id.clone(),
                    provenance.endpoint_id.clone(),
                ))
                .ok_or_else(|| CloudRuntimeWireError::EndpointMissing {
                    endpoint_id: provenance.endpoint_id.clone(),
                })?;
            if !advertisement.is_fresh_at(now_unix_ms) {
                return Err(CloudRuntimeWireError::AdvertisementExpired);
            }
            if !provenance.matches_advertisement(advertisement) {
                return Err(CloudRuntimeWireError::StaleTransport);
            }
            if state
                .retired_epochs
                .contains(&provenance_epoch_key(&provenance))
            {
                return Err(CloudRuntimeWireError::RetiredEpoch);
            }
            let stream_id = RuntimeWirePlacementStreamId(state.next_stream_id);
            state.next_stream_id = state
                .next_stream_id
                .checked_add(1)
                .ok_or(RuntimeWirePlacementProtocolError::StreamIdExhausted)?;
            let (inbound_tx, inbound_rx) = mpsc::channel(RUNTIME_WIRE_STREAM_QUEUE_CAPACITY);
            let (open_tx, open_rx) = oneshot::channel();
            let disconnect_ack = Arc::new(Notify::new());
            let negotiated = max_in_flight.min(RUNTIME_WIRE_DEFAULT_MAX_IN_FLIGHT).max(1);
            let open = RuntimeWirePlacementOpen {
                stream_id,
                provenance: provenance.clone(),
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                max_in_flight: negotiated,
            };
            let message = RelayMessage::RuntimeWirePlacementOpen {
                id: RelayMessage::new_id("runtime-wire-open"),
                payload: Box::new(open),
            };
            state.streams.insert(
                stream_id,
                RuntimeWireStream {
                    provenance: provenance.clone(),
                    max_in_flight: negotiated as usize,
                    next_outbound_sequence: 1,
                    last_inbound_sequence: 0,
                    outbound_unacked: BTreeMap::new(),
                    inbound_tx,
                    open_tx: Some(open_tx),
                    disconnect_ack: disconnect_ack.clone(),
                    disconnected: false,
                },
            );
            (
                open_rx,
                inbound_rx,
                disconnect_ack,
                connection.control_tx,
                message,
                stream_id,
            )
        };
        control_tx
            .try_send(open_message)
            .map_err(|_| CloudRuntimeWireError::QueueOverflow)?;
        match tokio::time::timeout(RUNTIME_WIRE_OPEN_TIMEOUT, open_rx).await {
            Ok(Ok(Ok(ack))) if ack.provenance == provenance => {}
            Ok(Ok(Ok(_))) => return Err(CloudRuntimeWireError::StaleTransport),
            Ok(Ok(Err(rejected))) => {
                return Err(CloudRuntimeWireError::OpenRejected {
                    reason: rejected.reason,
                });
            }
            Ok(Err(_)) | Err(_) => {
                self.retire_stream(stream_id, RuntimeWirePlacementLossCode::EndpointUnavailable)
                    .await;
                return Err(CloudRuntimeWireError::OpenTimeout);
            }
        }
        Ok(Arc::new(CloudRuntimeWirePlacement {
            registry: Arc::downgrade(self),
            stream_id,
            provenance,
            inbound_rx: Mutex::new(inbound_rx),
            disconnect_ack,
        }))
    }

    pub async fn route_backend_message(
        &self,
        transport: &RuntimeWireAuthenticatedTransport,
        message: &RelayMessage,
    ) -> Result<bool, CloudRuntimeWireError> {
        match message {
            RelayMessage::RuntimeWirePlacementOpenAck { payload, .. } => {
                let mut state = self.state.lock().await;
                ensure_connection(&state, transport)?;
                let stream = stream_mut(&mut state, payload.stream_id)?;
                ensure_stream_provenance(stream, &payload.provenance)?;
                if let Some(tx) = stream.open_tx.take() {
                    let _ = tx.send(Ok((**payload).clone()));
                }
                Ok(true)
            }
            RelayMessage::RuntimeWirePlacementOpenRejected { payload, .. } => {
                let mut state = self.state.lock().await;
                ensure_connection(&state, transport)?;
                let stream = stream_mut(&mut state, payload.stream_id)?;
                ensure_stream_provenance(stream, &payload.provenance)?;
                if let Some(tx) = stream.open_tx.take() {
                    let _ = tx.send(Err((**payload).clone()));
                }
                Ok(true)
            }
            RelayMessage::RuntimeWirePlacementFrame { payload, .. } => {
                self.route_inbound_frame(transport, payload).await?;
                Ok(true)
            }
            RelayMessage::RuntimeWirePlacementAck { payload, .. } => {
                let mut state = self.state.lock().await;
                ensure_connection(&state, transport)?;
                let stream = stream_mut(&mut state, payload.stream_id)?;
                ensure_stream_provenance(stream, &payload.provenance)?;
                stream
                    .outbound_unacked
                    .retain(|sequence, _| *sequence > payload.through_sequence);
                Ok(true)
            }
            RelayMessage::RuntimeWirePlacementClosed { payload, .. } => {
                self.route_disconnect(
                    transport,
                    payload.stream_id,
                    &payload.provenance,
                    payload.reason.clone(),
                )
                .await?;
                Ok(true)
            }
            RelayMessage::RuntimeWirePlacementLost { payload, .. } => {
                self.route_disconnect(
                    transport,
                    payload.stream_id,
                    &payload.provenance,
                    payload.reason.clone(),
                )
                .await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub async fn disconnect_backend(
        &self,
        transport: &RuntimeWireAuthenticatedTransport,
        reason: &str,
    ) {
        let waits = {
            let mut state = self.state.lock().await;
            if ensure_connection(&state, transport).is_err() {
                return;
            }
            let ids = state
                .streams
                .iter()
                .filter_map(|(id, stream)| {
                    (stream.provenance.transport == *transport).then_some(*id)
                })
                .collect::<Vec<_>>();
            let mut waits = Vec::new();
            for id in ids {
                if let Some(stream) = state.streams.get_mut(&id) {
                    let epoch = provenance_epoch_key(&stream.provenance);
                    stream.disconnected = true;
                    let _ = stream
                        .inbound_tx
                        .try_send(RuntimeWirePlacementEvent::Disconnected {
                            reason: reason.to_owned(),
                        });
                    let ack = stream.disconnect_ack.clone();
                    state.retired_epochs.insert(epoch);
                    waits.push(ack);
                }
            }
            waits
        };
        for wait in waits {
            let _ =
                tokio::time::timeout(RUNTIME_WIRE_DISCONNECT_ACK_TIMEOUT, wait.notified()).await;
        }
        let mut state = self.state.lock().await;
        state
            .advertisements
            .retain(|(backend_id, _), _| backend_id != &transport.backend_id);
        state
            .streams
            .retain(|_, stream| stream.provenance.transport != *transport);
        if state
            .connections
            .get(&transport.backend_id)
            .is_some_and(|connection| connection.transport == *transport)
        {
            state.connections.remove(&transport.backend_id);
        }
    }

    async fn send_frame(
        &self,
        stream_id: RuntimeWirePlacementStreamId,
        provenance: &RuntimeWirePlacementProvenance,
        envelope: agentdash_agent_runtime_wire::RuntimeWireEnvelope,
    ) -> Result<(), CloudRuntimeWireError> {
        let (sender, message, sequence) = {
            let mut state = self.state.lock().await;
            let connection = ensure_connection(&state, &provenance.transport)?.clone();
            let stream = stream_mut(&mut state, stream_id)?;
            ensure_stream_provenance(stream, provenance)?;
            if stream.disconnected || stream.outbound_unacked.len() >= stream.max_in_flight {
                return Err(CloudRuntimeWireError::QueueOverflow);
            }
            let sequence = RuntimeWirePlacementSequence(stream.next_outbound_sequence);
            stream.next_outbound_sequence = stream
                .next_outbound_sequence
                .checked_add(1)
                .ok_or(RuntimeWirePlacementProtocolError::SequenceExhausted)?;
            let message = RelayMessage::RuntimeWirePlacementFrame {
                id: RelayMessage::new_id("runtime-wire-frame"),
                payload: Box::new(RuntimeWirePlacementFrame {
                    stream_id,
                    provenance: provenance.clone(),
                    sequence,
                    envelope,
                }),
            };
            stream.outbound_unacked.insert(sequence, message.clone());
            (connection.critical_tx, message, sequence)
        };
        if sender.try_send(message).is_err() {
            self.retire_stream(stream_id, RuntimeWirePlacementLossCode::QueueOverflow)
                .await;
            return Err(CloudRuntimeWireError::QueueOverflow);
        }
        let _ = sequence;
        Ok(())
    }

    async fn route_inbound_frame(
        &self,
        transport: &RuntimeWireAuthenticatedTransport,
        frame: &RuntimeWirePlacementFrame,
    ) -> Result<(), CloudRuntimeWireError> {
        let (inbound, control, ack) = {
            let mut state = self.state.lock().await;
            let connection = ensure_connection(&state, transport)?.clone();
            let stream = stream_mut(&mut state, frame.stream_id)?;
            ensure_stream_provenance(stream, &frame.provenance)?;
            let expected = stream
                .last_inbound_sequence
                .checked_add(1)
                .ok_or(RuntimeWirePlacementProtocolError::SequenceExhausted)?;
            if frame.sequence.0 > expected {
                return Err(RuntimeWirePlacementProtocolError::SequenceGap {
                    expected: RuntimeWirePlacementSequence(expected),
                    received: frame.sequence,
                }
                .into());
            }
            if frame.sequence.0 == expected {
                stream.last_inbound_sequence = expected;
            }
            let ack = RelayMessage::RuntimeWirePlacementAck {
                id: RelayMessage::new_id("runtime-wire-ack"),
                payload: Box::new(RuntimeWirePlacementAck {
                    stream_id: frame.stream_id,
                    provenance: frame.provenance.clone(),
                    through_sequence: RuntimeWirePlacementSequence(stream.last_inbound_sequence),
                }),
            };
            (
                (frame.sequence.0 == expected).then(|| stream.inbound_tx.clone()),
                connection.control_tx,
                ack,
            )
        };
        if let Some(inbound) = inbound {
            inbound
                .try_send(RuntimeWirePlacementEvent::Frame(Box::new(
                    frame.envelope.clone(),
                )))
                .map_err(|_| CloudRuntimeWireError::QueueOverflow)?;
        }
        control
            .try_send(ack)
            .map_err(|_| CloudRuntimeWireError::QueueOverflow)?;
        Ok(())
    }

    async fn route_disconnect(
        &self,
        transport: &RuntimeWireAuthenticatedTransport,
        stream_id: RuntimeWirePlacementStreamId,
        provenance: &RuntimeWirePlacementProvenance,
        reason: String,
    ) -> Result<(), CloudRuntimeWireError> {
        let inbound = {
            let mut state = self.state.lock().await;
            ensure_connection(&state, transport)?;
            let inbound = {
                let stream = stream_mut(&mut state, stream_id)?;
                ensure_stream_provenance(stream, provenance)?;
                if stream.disconnected {
                    return Ok(());
                }
                stream.disconnected = true;
                stream.inbound_tx.clone()
            };
            state
                .retired_epochs
                .insert(provenance_epoch_key(provenance));
            inbound
        };
        inbound
            .try_send(RuntimeWirePlacementEvent::Disconnected { reason })
            .map_err(|_| CloudRuntimeWireError::QueueOverflow)
    }

    async fn retire_stream(
        &self,
        stream_id: RuntimeWirePlacementStreamId,
        code: RuntimeWirePlacementLossCode,
    ) {
        let outbound = {
            let mut state = self.state.lock().await;
            let (provenance, epoch) = {
                let Some(stream) = state.streams.get_mut(&stream_id) else {
                    return;
                };
                if stream.disconnected {
                    return;
                }
                stream.disconnected = true;
                (
                    stream.provenance.clone(),
                    provenance_epoch_key(&stream.provenance),
                )
            };
            state.retired_epochs.insert(epoch);
            let Some(connection) = state.connections.get(&provenance.transport.backend_id) else {
                return;
            };
            (
                connection.control_tx.clone(),
                RelayMessage::RuntimeWirePlacementLost {
                    id: RelayMessage::new_id("runtime-wire-lost"),
                    payload: Box::new(RuntimeWirePlacementLost {
                        stream_id,
                        provenance,
                        code,
                        reason: "runtime wire placement retired".to_owned(),
                    }),
                },
            )
        };
        let _ = outbound.0.try_send(outbound.1);
    }

    async fn close_stream(
        &self,
        stream_id: RuntimeWirePlacementStreamId,
        provenance: &RuntimeWirePlacementProvenance,
        reason: &str,
    ) {
        let outbound = {
            let mut state = self.state.lock().await;
            let Some(stream) = state.streams.get(&stream_id) else {
                return;
            };
            if stream.provenance != *provenance {
                return;
            }
            let stream = state
                .streams
                .remove(&stream_id)
                .expect("checked Runtime Wire stream");
            state
                .retired_epochs
                .insert(provenance_epoch_key(provenance));
            let _ = stream
                .inbound_tx
                .try_send(RuntimeWirePlacementEvent::Disconnected {
                    reason: reason.to_owned(),
                });
            state.connections.get(&provenance.transport.backend_id).map(
                |connection| {
                    (
                        connection.control_tx.clone(),
                        RelayMessage::RuntimeWirePlacementClosed {
                            id: RelayMessage::new_id("runtime-wire-close"),
                            payload: Box::new(RuntimeWirePlacementClosed {
                                stream_id,
                                provenance: provenance.clone(),
                                code: agentdash_agent_runtime_wire::RuntimeWirePlacementCloseCode::Rejected,
                                reason: reason.to_owned(),
                            }),
                        },
                    )
                },
            )
        };
        if let Some((sender, message)) = outbound {
            let _ = sender.try_send(message);
        }
    }
}

struct CloudRuntimeWirePlacement {
    registry: Weak<CloudRuntimeWirePlacementRegistry>,
    stream_id: RuntimeWirePlacementStreamId,
    provenance: RuntimeWirePlacementProvenance,
    inbound_rx: Mutex<mpsc::Receiver<RuntimeWirePlacementEvent>>,
    disconnect_ack: Arc<Notify>,
}

#[async_trait]
impl RuntimeWirePlacement for CloudRuntimeWirePlacement {
    async fn send(
        &self,
        frame: agentdash_agent_runtime_wire::RuntimeWireEnvelope,
    ) -> Result<(), RemoteRuntimeTransportError> {
        let registry = self
            .registry
            .upgrade()
            .ok_or_else(|| unavailable("Cloud Runtime Wire registry was dropped"))?;
        registry
            .send_frame(self.stream_id, &self.provenance, frame)
            .await
            .map_err(map_transport)
    }

    async fn receive(&self) -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError> {
        self.inbound_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| unavailable("Runtime Wire placement stream closed"))
    }

    async fn acknowledge_disconnect(&self) {
        self.disconnect_ack.notify_waiters();
    }

    async fn close(&self, reason: &str) {
        if let Some(registry) = self.registry.upgrade() {
            registry
                .close_stream(self.stream_id, &self.provenance, reason)
                .await;
        }
    }
}

fn ensure_connection<'a>(
    state: &'a RuntimeWireRegistryState,
    transport: &RuntimeWireAuthenticatedTransport,
) -> Result<&'a RuntimeWireConnection, CloudRuntimeWireError> {
    let connection = state
        .connections
        .get(&transport.backend_id)
        .ok_or_else(|| CloudRuntimeWireError::BackendDisconnected {
            backend_id: transport.backend_id.clone(),
        })?;
    if connection.transport != *transport {
        return Err(CloudRuntimeWireError::StaleTransport);
    }
    Ok(connection)
}

fn stream_mut(
    state: &mut RuntimeWireRegistryState,
    stream_id: RuntimeWirePlacementStreamId,
) -> Result<&mut RuntimeWireStream, CloudRuntimeWireError> {
    state
        .streams
        .get_mut(&stream_id)
        .ok_or(CloudRuntimeWireError::StreamMissing { stream_id })
}

fn ensure_stream_provenance(
    stream: &RuntimeWireStream,
    provenance: &RuntimeWirePlacementProvenance,
) -> Result<(), CloudRuntimeWireError> {
    if stream.provenance != *provenance {
        return Err(CloudRuntimeWireError::StaleTransport);
    }
    Ok(())
}

fn advertisement_epoch_key(advertisement: &RuntimeWireServiceOfferAdvertisement) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        advertisement.host_incarnation_id,
        advertisement.service_instance_id,
        advertisement.binding_generation.0
    )
}

fn provenance_epoch_key(provenance: &RuntimeWirePlacementProvenance) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        provenance.host_incarnation_id,
        provenance.service_instance_id,
        provenance.binding_generation.0
    )
}

fn unavailable(reason: impl Into<String>) -> RemoteRuntimeTransportError {
    RemoteRuntimeTransportError::Unavailable {
        reason: reason.into(),
        retryable: false,
    }
}

fn map_transport(error: CloudRuntimeWireError) -> RemoteRuntimeTransportError {
    match error {
        CloudRuntimeWireError::Protocol(error) => RemoteRuntimeTransportError::Protocol {
            reason: format!("{error:?}"),
            critical: true,
        },
        other => unavailable(other.to_string()),
    }
}

#[allow(dead_code)]
fn _closed_is_availability_only(_closed: RuntimeWirePlacementClosed) {}

#[cfg(test)]
mod tests {
    use agentdash_agent_runtime_wire::RuntimeWireAdvertisementRevision;
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentPayloadDigest, AgentProfileDigest, AgentServiceInstanceId,
    };

    use super::*;

    #[tokio::test]
    async fn explicit_close_retires_opened_stream_and_traces_peer_close() {
        let registry = CloudRuntimeWirePlacementRegistry::new();
        let mut queues = registry
            .register_connection("backend-a", "transport-a".to_owned())
            .await
            .unwrap();
        let provenance = RuntimeWirePlacementProvenance {
            transport: queues.transport.clone(),
            endpoint_id: "codex".to_owned(),
            host_incarnation_id: "host-a".to_owned(),
            service_instance_id: AgentServiceInstanceId::new("remote-codex").unwrap(),
            binding_generation: AgentBindingGeneration(4),
            advertisement_revision: RuntimeWireAdvertisementRevision(7),
            advertisement_digest: AgentPayloadDigest::new("sha256:advertisement").unwrap(),
            profile_digest: AgentProfileDigest::new("sha256:profile").unwrap(),
        };
        let stream_id = RuntimeWirePlacementStreamId(41);
        let (inbound_tx, inbound_rx) = mpsc::channel(1);
        let disconnect_ack = Arc::new(Notify::new());
        registry.state.lock().await.streams.insert(
            stream_id,
            RuntimeWireStream {
                provenance: provenance.clone(),
                max_in_flight: 1,
                next_outbound_sequence: 1,
                last_inbound_sequence: 0,
                outbound_unacked: BTreeMap::new(),
                inbound_tx,
                open_tx: None,
                disconnect_ack: disconnect_ack.clone(),
                disconnected: false,
            },
        );
        let placement = CloudRuntimeWirePlacement {
            registry: Arc::downgrade(&registry),
            stream_id,
            provenance: provenance.clone(),
            inbound_rx: Mutex::new(inbound_rx),
            disconnect_ack,
        };

        placement.close("superseded").await;

        assert!(matches!(
            placement.receive().await.unwrap(),
            RuntimeWirePlacementEvent::Disconnected { reason } if reason == "superseded"
        ));
        assert!(matches!(
            queues.control_rx.recv().await.unwrap(),
            RelayMessage::RuntimeWirePlacementClosed { payload, .. }
                if payload.stream_id == stream_id
                    && payload.provenance == provenance
                    && payload.reason == "superseded"
        ));
        let state = registry.state.lock().await;
        assert!(!state.streams.contains_key(&stream_id));
        assert!(
            state
                .retired_epochs
                .contains(&provenance_epoch_key(&provenance))
        );
    }
}
