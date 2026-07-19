use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use agentdash_agent_runtime_wire::{
    RUNTIME_WIRE_PROTOCOL_REVISION, RuntimeWirePlacementAck, RuntimeWirePlacementCloseCode,
    RuntimeWirePlacementClosed, RuntimeWirePlacementFrame, RuntimeWirePlacementLossCode,
    RuntimeWirePlacementLost, RuntimeWirePlacementOpen, RuntimeWirePlacementOpenAck,
    RuntimeWirePlacementOpenRejected, RuntimeWirePlacementProtocolError,
    RuntimeWirePlacementSequence, RuntimeWirePlacementStreamId,
    RuntimeWireServiceOfferAdvertisement,
};
use agentdash_integration_remote_runtime::{
    RuntimeWireAgentServiceEndpoint, RuntimeWirePlacement, RuntimeWirePlacementEvent,
};
use agentdash_relay::RelayMessage;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock, mpsc};

const LOCAL_RUNTIME_WIRE_MAX_IN_FLIGHT: usize = 64;

pub struct LocalRuntimeWireEndpoint {
    pub advertisement: RuntimeWireServiceOfferAdvertisement,
    pub endpoint: Arc<RuntimeWireAgentServiceEndpoint>,
}

#[derive(Default)]
pub struct LocalRuntimeWireEndpointCatalog {
    endpoints: RwLock<BTreeMap<String, Arc<LocalRuntimeWireEndpoint>>>,
}

impl LocalRuntimeWireEndpointCatalog {
    pub fn empty() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn register(
        &self,
        endpoint: LocalRuntimeWireEndpoint,
    ) -> Result<(), LocalRuntimeWireError> {
        endpoint.advertisement.validate_shape()?;
        let target = endpoint.endpoint.target();
        if target.service_instance_id != endpoint.advertisement.service_instance_id
            || target.binding_generation != endpoint.advertisement.binding_generation
        {
            return Err(LocalRuntimeWireError::EndpointMismatch);
        }
        let mut endpoints = self.endpoints.write().await;
        if endpoints
            .insert(
                endpoint.advertisement.endpoint_id.clone(),
                Arc::new(endpoint),
            )
            .is_some()
        {
            return Err(LocalRuntimeWireError::DuplicateEndpoint);
        }
        Ok(())
    }

    pub async fn advertisements(&self) -> Vec<RuntimeWireServiceOfferAdvertisement> {
        self.endpoints
            .read()
            .await
            .values()
            .map(|endpoint| endpoint.advertisement.clone())
            .collect()
    }

    async fn resolve(&self, endpoint_id: &str) -> Option<Arc<LocalRuntimeWireEndpoint>> {
        self.endpoints.read().await.get(endpoint_id).cloned()
    }
}

#[derive(Debug, Error)]
pub enum LocalRuntimeWireError {
    #[error("duplicate Local Runtime Wire endpoint")]
    DuplicateEndpoint,
    #[error("Local Runtime Wire endpoint advertisement does not match its concrete service")]
    EndpointMismatch,
    #[error("Local Runtime Wire endpoint is unavailable")]
    EndpointUnavailable,
    #[error("Local Runtime Wire placement provenance is stale")]
    StaleProvenance,
    #[error("Local Runtime Wire placement stream does not exist")]
    StreamMissing,
    #[error("Local Runtime Wire queue overflowed")]
    QueueOverflow,
    #[error(transparent)]
    Protocol(#[from] RuntimeWirePlacementProtocolError),
}

struct LocalRuntimeWireStream {
    open: RuntimeWirePlacementOpen,
    endpoint: Arc<RuntimeWireAgentServiceEndpoint>,
    next_outbound_sequence: Mutex<u64>,
    last_inbound_sequence: Mutex<u64>,
    outbound_unacked: Mutex<BTreeMap<RuntimeWirePlacementSequence, RelayMessage>>,
    closed: AtomicBool,
}

pub struct LocalRuntimeWireRouter {
    backend_id: String,
    catalog: Arc<LocalRuntimeWireEndpointCatalog>,
    control_tx: mpsc::Sender<RelayMessage>,
    critical_tx: mpsc::Sender<RelayMessage>,
    streams: Mutex<BTreeMap<RuntimeWirePlacementStreamId, Arc<LocalRuntimeWireStream>>>,
}

impl LocalRuntimeWireRouter {
    pub fn new(
        backend_id: String,
        catalog: Arc<LocalRuntimeWireEndpointCatalog>,
        control_tx: mpsc::Sender<RelayMessage>,
        critical_tx: mpsc::Sender<RelayMessage>,
    ) -> Arc<Self> {
        Arc::new(Self {
            backend_id,
            catalog,
            control_tx,
            critical_tx,
            streams: Mutex::new(BTreeMap::new()),
        })
    }

    pub async fn advertise_catalog(&self) -> Result<(), LocalRuntimeWireError> {
        for advertisement in self.catalog.advertisements().await {
            self.control_tx
                .try_send(RelayMessage::RuntimeWireOfferAdvertise {
                    id: RelayMessage::new_id("runtime-wire-offer"),
                    payload: Box::new(advertisement),
                })
                .map_err(|_| LocalRuntimeWireError::QueueOverflow)?;
        }
        Ok(())
    }

    pub async fn handle(self: &Arc<Self>, message: &RelayMessage) -> bool {
        let result = match message {
            RelayMessage::RuntimeWirePlacementOpen { payload, .. } => {
                self.open((**payload).clone()).await
            }
            RelayMessage::RuntimeWirePlacementFrame { payload, .. } => {
                self.frame(payload).await
            }
            RelayMessage::RuntimeWirePlacementAck { payload, .. } => self.ack(payload).await,
            RelayMessage::RuntimeWirePlacementClosed { payload, .. } => {
                self.close(
                    payload.stream_id,
                    &payload.provenance,
                    payload.reason.clone(),
                )
                .await
            }
            RelayMessage::RuntimeWirePlacementLost { payload, .. } => {
                self.close(
                    payload.stream_id,
                    &payload.provenance,
                    payload.reason.clone(),
                )
                .await
            }
            RelayMessage::RuntimeWireOfferAdvertise { .. }
            | RelayMessage::RuntimeWireOfferWithdraw { .. }
            | RelayMessage::RuntimeWirePlacementOpenAck { .. }
            | RelayMessage::RuntimeWirePlacementOpenRejected { .. } => {
                Err(LocalRuntimeWireError::StaleProvenance)
            }
            _ => return false,
        };
        if let Err(error) = result {
            let lost = self.loss_for_message(message, error.to_string()).await;
            if let Some(lost) = lost {
                let _ = self.control_tx.try_send(lost);
            }
        }
        true
    }

    async fn open(
        self: &Arc<Self>,
        open: RuntimeWirePlacementOpen,
    ) -> Result<(), LocalRuntimeWireError> {
        if open.protocol_revision != RUNTIME_WIRE_PROTOCOL_REVISION
            || open.provenance.transport.backend_id != self.backend_id
            || open.max_in_flight == 0
            || open.max_in_flight as usize > LOCAL_RUNTIME_WIRE_MAX_IN_FLIGHT
        {
            return self.reject_open(open, "unsupported placement negotiation").await;
        }
        let endpoint = self
            .catalog
            .resolve(&open.provenance.endpoint_id)
            .await
            .ok_or(LocalRuntimeWireError::EndpointUnavailable)?;
        if !open
            .provenance
            .matches_advertisement(&endpoint.advertisement)
        {
            return self.reject_open(open, "stale placement provenance").await;
        }
        let stream = Arc::new(LocalRuntimeWireStream {
            open: open.clone(),
            endpoint: endpoint.endpoint.clone(),
            next_outbound_sequence: Mutex::new(1),
            last_inbound_sequence: Mutex::new(0),
            outbound_unacked: Mutex::new(BTreeMap::new()),
            closed: AtomicBool::new(false),
        });
        let mut streams = self.streams.lock().await;
        if streams.contains_key(&open.stream_id) {
            return self.reject_open(open, "stream identity already exists").await;
        }
        streams.insert(open.stream_id, stream.clone());
        drop(streams);
        stream.endpoint.reconnect_outbound().await;
        self.control_tx
            .try_send(RelayMessage::RuntimeWirePlacementOpenAck {
                id: RelayMessage::new_id("runtime-wire-open-ack"),
                payload: Box::new(RuntimeWirePlacementOpenAck {
                    stream_id: open.stream_id,
                    provenance: open.provenance,
                    protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                    max_in_flight: open.max_in_flight,
                }),
            })
            .map_err(|_| LocalRuntimeWireError::QueueOverflow)?;
        let router = self.clone();
        tokio::spawn(async move {
            router.pump_endpoint(stream).await;
        });
        Ok(())
    }

    async fn reject_open(
        &self,
        open: RuntimeWirePlacementOpen,
        reason: &str,
    ) -> Result<(), LocalRuntimeWireError> {
        self.control_tx
            .try_send(RelayMessage::RuntimeWirePlacementOpenRejected {
                id: RelayMessage::new_id("runtime-wire-open-rejected"),
                payload: Box::new(RuntimeWirePlacementOpenRejected {
                    stream_id: open.stream_id,
                    provenance: open.provenance,
                    code: RuntimeWirePlacementCloseCode::Rejected,
                    reason: reason.to_owned(),
                }),
            })
            .map_err(|_| LocalRuntimeWireError::QueueOverflow)?;
        Ok(())
    }

    async fn frame(
        &self,
        frame: &RuntimeWirePlacementFrame,
    ) -> Result<(), LocalRuntimeWireError> {
        let stream = self.stream(frame.stream_id, &frame.provenance).await?;
        let mut last = stream.last_inbound_sequence.lock().await;
        let expected = last
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
            stream
                .endpoint
                .send(frame.envelope.clone())
                .await
                .map_err(|_| LocalRuntimeWireError::EndpointUnavailable)?;
            *last = expected;
        }
        self.control_tx
            .try_send(RelayMessage::RuntimeWirePlacementAck {
                id: RelayMessage::new_id("runtime-wire-ack"),
                payload: Box::new(RuntimeWirePlacementAck {
                    stream_id: frame.stream_id,
                    provenance: frame.provenance.clone(),
                    through_sequence: RuntimeWirePlacementSequence(*last),
                }),
            })
            .map_err(|_| LocalRuntimeWireError::QueueOverflow)
    }

    async fn ack(&self, ack: &RuntimeWirePlacementAck) -> Result<(), LocalRuntimeWireError> {
        let stream = self.stream(ack.stream_id, &ack.provenance).await?;
        stream
            .outbound_unacked
            .lock()
            .await
            .retain(|sequence, _| *sequence > ack.through_sequence);
        Ok(())
    }

    async fn close(
        &self,
        stream_id: RuntimeWirePlacementStreamId,
        provenance: &agentdash_agent_runtime_wire::RuntimeWirePlacementProvenance,
        _reason: String,
    ) -> Result<(), LocalRuntimeWireError> {
        let stream = self.stream(stream_id, provenance).await?;
        if !stream.closed.swap(true, Ordering::AcqRel) {
            stream.endpoint.disconnect_outbound().await;
        }
        self.streams.lock().await.remove(&stream_id);
        Ok(())
    }

    async fn stream(
        &self,
        stream_id: RuntimeWirePlacementStreamId,
        provenance: &agentdash_agent_runtime_wire::RuntimeWirePlacementProvenance,
    ) -> Result<Arc<LocalRuntimeWireStream>, LocalRuntimeWireError> {
        let stream = self
            .streams
            .lock()
            .await
            .get(&stream_id)
            .cloned()
            .ok_or(LocalRuntimeWireError::StreamMissing)?;
        if stream.open.provenance != *provenance || stream.closed.load(Ordering::Acquire) {
            return Err(LocalRuntimeWireError::StaleProvenance);
        }
        Ok(stream)
    }

    async fn pump_endpoint(self: Arc<Self>, stream: Arc<LocalRuntimeWireStream>) {
        loop {
            match stream.endpoint.receive().await {
                Ok(RuntimeWirePlacementEvent::Frame(envelope)) => {
                    let sequence = {
                        let mut next = stream.next_outbound_sequence.lock().await;
                        let sequence = RuntimeWirePlacementSequence(*next);
                        let Some(incremented) = next.checked_add(1) else {
                            self.emit_lost(
                                &stream,
                                RuntimeWirePlacementLossCode::QueueOverflow,
                                "outbound sequence exhausted",
                            );
                            return;
                        };
                        *next = incremented;
                        sequence
                    };
                    let message = RelayMessage::RuntimeWirePlacementFrame {
                        id: RelayMessage::new_id("runtime-wire-frame"),
                        payload: Box::new(RuntimeWirePlacementFrame {
                            stream_id: stream.open.stream_id,
                            provenance: stream.open.provenance.clone(),
                            sequence,
                            envelope: *envelope,
                        }),
                    };
                    let mut unacked = stream.outbound_unacked.lock().await;
                    if unacked.len() >= stream.open.max_in_flight as usize {
                        drop(unacked);
                        self.emit_lost(
                            &stream,
                            RuntimeWirePlacementLossCode::QueueOverflow,
                            "outbound in-flight window exhausted",
                        );
                        return;
                    }
                    unacked.insert(sequence, message.clone());
                    drop(unacked);
                    if self.critical_tx.try_send(message).is_err() {
                        self.emit_lost(
                            &stream,
                            RuntimeWirePlacementLossCode::QueueOverflow,
                            "critical outbound queue exhausted",
                        );
                        return;
                    }
                }
                Ok(RuntimeWirePlacementEvent::Disconnected { reason }) => {
                    self.emit_lost(
                        &stream,
                        RuntimeWirePlacementLossCode::EndpointUnavailable,
                        &reason,
                    );
                    return;
                }
                Ok(RuntimeWirePlacementEvent::Reconnected) => {}
                Err(error) => {
                    self.emit_lost(
                        &stream,
                        RuntimeWirePlacementLossCode::EndpointUnavailable,
                        &error.to_string(),
                    );
                    return;
                }
            }
        }
    }

    fn emit_lost(
        &self,
        stream: &LocalRuntimeWireStream,
        code: RuntimeWirePlacementLossCode,
        reason: &str,
    ) {
        if stream.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = self
            .control_tx
            .try_send(RelayMessage::RuntimeWirePlacementLost {
                id: RelayMessage::new_id("runtime-wire-lost"),
                payload: Box::new(RuntimeWirePlacementLost {
                    stream_id: stream.open.stream_id,
                    provenance: stream.open.provenance.clone(),
                    code,
                    reason: reason.to_owned(),
                }),
            });
    }

    async fn loss_for_message(
        &self,
        message: &RelayMessage,
        reason: String,
    ) -> Option<RelayMessage> {
        let (stream_id, provenance) = match message {
            RelayMessage::RuntimeWirePlacementFrame { payload, .. } => {
                (payload.stream_id, payload.provenance.clone())
            }
            RelayMessage::RuntimeWirePlacementAck { payload, .. } => {
                (payload.stream_id, payload.provenance.clone())
            }
            RelayMessage::RuntimeWirePlacementClosed { payload, .. } => {
                (payload.stream_id, payload.provenance.clone())
            }
            RelayMessage::RuntimeWirePlacementLost { payload, .. } => {
                (payload.stream_id, payload.provenance.clone())
            }
            _ => return None,
        };
        Some(RelayMessage::RuntimeWirePlacementLost {
            id: RelayMessage::new_id("runtime-wire-lost"),
            payload: Box::new(RuntimeWirePlacementLost {
                stream_id,
                provenance,
                code: RuntimeWirePlacementLossCode::StaleProvenance,
                reason,
            }),
        })
    }
}

#[allow(dead_code)]
fn _closed_is_availability_only(_closed: RuntimeWirePlacementClosed) {}

