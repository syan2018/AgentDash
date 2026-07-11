use std::{collections::HashMap, sync::Arc};

use agentdash_agent_runtime_contract::AgentRuntimeDriver;
use agentdash_agent_runtime_host::IntegrationDriverHost;
use agentdash_agent_runtime_wire::{RuntimeWireEnvelope, RuntimeWireFrame, RuntimeWireResponse};
use agentdash_integration_api::AgentRuntimePlacementId;
use agentdash_integration_remote_runtime::{
    RuntimeWireDriverEndpoint, RuntimeWireHostPortRouter, RuntimeWirePlacement,
};
use agentdash_relay::{
    RelayError, RelayMessage, RuntimeRelayOpen, RuntimeRelayProvenance, RuntimeRelayStream,
    RuntimeRelayStreamId, RuntimeRelayTransportDescriptor, RuntimeRelayTransportError,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

#[async_trait]
pub trait RuntimeDriverEndpointResolver: Send + Sync {
    async fn resolve(
        &self,
        provenance: &RuntimeRelayProvenance,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, String>;

    async fn advertised_offers(
        &self,
    ) -> Result<Vec<agentdash_relay::RuntimeOfferAdvertisement>, String>;
}

pub struct HostRuntimeDriverEndpointResolver {
    host: Arc<IntegrationDriverHost>,
    host_id: String,
    transport_id: AgentRuntimePlacementId,
}

impl HostRuntimeDriverEndpointResolver {
    pub fn new(
        host: Arc<IntegrationDriverHost>,
        host_id: impl Into<String>,
        transport_id: AgentRuntimePlacementId,
    ) -> Self {
        Self {
            host,
            host_id: host_id.into(),
            transport_id,
        }
    }
}

#[async_trait]
impl RuntimeDriverEndpointResolver for HostRuntimeDriverEndpointResolver {
    async fn resolve(
        &self,
        provenance: &RuntimeRelayProvenance,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, String> {
        if provenance.host_id != self.host_id || provenance.transport_id != self.transport_id {
            return Err("Runtime Wire placement targets another local Host transport".to_string());
        }
        self.host
            .driver_endpoint(
                &provenance.service_instance_id,
                provenance.driver_generation,
            )
            .await
            .map_err(|error| error.to_string())
    }

    async fn advertised_offers(
        &self,
    ) -> Result<Vec<agentdash_relay::RuntimeOfferAdvertisement>, String> {
        let mut offers = self
            .host
            .offers()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|offer| offer.available)
            .map(|offer| agentdash_relay::RuntimeOfferAdvertisement {
                definition_id: offer.provenance.definition_id,
                publisher_integration: offer.provenance.publisher_integration,
                service_version: offer.provenance.service_version,
                build_digest: offer.provenance.build_digest.to_string(),
                service_instance_id: offer.service_instance_id,
                instance_revision: offer.instance_revision,
                driver_generation: offer.generation,
                protocol_revision: offer.protocol_revision,
                effective_profile: offer.effective_profile,
                profile_digest: offer.profile_digest,
                conformance_suite_revision: offer.conformance.suite_revision,
                conformance_driver_build_digest: offer.conformance.driver_build_digest,
                conformance_verified_profile_digest: offer.conformance.verified_profile_digest,
                conformance_verified_at: offer.conformance.verified_at,
                transport_id: self.transport_id.clone(),
            })
            .collect::<Vec<_>>();
        offers.sort_by(|left, right| {
            left.definition_id
                .cmp(&right.definition_id)
                .then_with(|| left.service_instance_id.cmp(&right.service_instance_id))
                .then_with(|| left.driver_generation.cmp(&right.driver_generation))
        });
        Ok(offers)
    }
}

struct ActiveRuntimeWireStream {
    transport: RuntimeRelayStream,
    endpoint: Arc<RuntimeWireDriverEndpoint>,
    dispatch: tokio::sync::mpsc::UnboundedSender<(String, RuntimeWireEnvelope)>,
}

pub struct RuntimeWireCommandHandler {
    resolver: Arc<dyn RuntimeDriverEndpointResolver>,
    descriptor: RuntimeRelayTransportDescriptor,
    host_port_router: Arc<RuntimeWireHostPortRouter>,
    streams: Mutex<HashMap<RuntimeRelayStreamId, Arc<Mutex<ActiveRuntimeWireStream>>>>,
    outbound: Arc<std::sync::RwLock<Option<tokio::sync::mpsc::UnboundedSender<RelayMessage>>>>,
}

impl RuntimeWireCommandHandler {
    pub fn new(
        resolver: Arc<dyn RuntimeDriverEndpointResolver>,
        descriptor: RuntimeRelayTransportDescriptor,
    ) -> Self {
        Self::new_with_host_port_router(
            resolver,
            descriptor,
            Arc::new(RuntimeWireHostPortRouter::default()),
        )
    }

    pub fn new_with_host_port_router(
        resolver: Arc<dyn RuntimeDriverEndpointResolver>,
        descriptor: RuntimeRelayTransportDescriptor,
        host_port_router: Arc<RuntimeWireHostPortRouter>,
    ) -> Self {
        Self {
            resolver,
            descriptor,
            host_port_router,
            streams: Mutex::new(HashMap::new()),
            outbound: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub fn attach_outbound(&self, outbound: tokio::sync::mpsc::UnboundedSender<RelayMessage>) {
        *self
            .outbound
            .write()
            .unwrap_or_else(|error| error.into_inner()) = Some(outbound);
    }

    pub fn detach_outbound(&self) {
        *self
            .outbound
            .write()
            .unwrap_or_else(|error| error.into_inner()) = None;
    }

    pub async fn advertised_offers(
        &self,
    ) -> Result<Vec<agentdash_relay::RuntimeOfferAdvertisement>, String> {
        self.resolver.advertised_offers().await
    }

    pub async fn open(&self, id: String, open: RuntimeRelayOpen) -> RelayMessage {
        if let Some(stream) = self.streams.lock().await.get(&open.stream_id).cloned() {
            let resumed = stream
                .lock()
                .await
                .transport
                .resume(&open, &self.descriptor);
            return match resumed {
                Ok((ack, replay)) => {
                    for frame in replay {
                        let frame_id =
                            format!("runtime-wire:{}:{}", frame.stream_id.0, frame.sequence);
                        if !send_outbound(
                            &self.outbound,
                            RelayMessage::RuntimeWireFrame {
                                id: frame_id,
                                payload: Box::new(frame),
                            },
                        ) {
                            return RelayMessage::Error {
                                id,
                                error: RelayError::runtime_error(
                                    "Runtime Wire outbound channel is closed",
                                ),
                            };
                        }
                    }
                    RelayMessage::RuntimeWireOpenAck { id, payload: ack }
                }
                Err(error) => transport_error(id, error),
            };
        }
        let driver = match self.resolver.resolve(&open.provenance).await {
            Ok(driver) => driver,
            Err(reason) => {
                return RelayMessage::Error {
                    id,
                    error: RelayError::runtime_error(reason),
                };
            }
        };
        let (transport, ack) = match RuntimeRelayStream::negotiate(open, &self.descriptor) {
            Ok(value) => value,
            Err(error) => return transport_error(id, error),
        };
        let stream_id = ack.stream_id.clone();
        let endpoint = Arc::new(RuntimeWireDriverEndpoint::new_with_host_port_router(
            driver,
            self.host_port_router.clone(),
        ));
        let (dispatch, dispatch_rx) = tokio::sync::mpsc::unbounded_channel();
        self.start_inbound_dispatch(endpoint.clone(), dispatch_rx);
        let stream = Arc::new(Mutex::new(ActiveRuntimeWireStream {
            transport,
            endpoint,
            dispatch,
        }));
        self.streams
            .lock()
            .await
            .insert(stream_id.clone(), stream.clone());
        self.start_outbound_pump(stream_id, stream);
        RelayMessage::RuntimeWireOpenAck { id, payload: ack }
    }

    fn start_inbound_dispatch(
        &self,
        endpoint: Arc<RuntimeWireDriverEndpoint>,
        mut dispatch: tokio::sync::mpsc::UnboundedReceiver<(String, RuntimeWireEnvelope)>,
    ) {
        let outbound = self.outbound.clone();
        tokio::spawn(async move {
            while let Some((id, envelope)) = dispatch.recv().await {
                if let Err(error) = endpoint.send(envelope).await {
                    send_outbound(
                        &outbound,
                        RelayMessage::Error {
                            id,
                            error: RelayError::runtime_error(error.to_string()),
                        },
                    );
                }
            }
        });
    }

    fn start_outbound_pump(
        &self,
        stream_id: RuntimeRelayStreamId,
        stream: Arc<Mutex<ActiveRuntimeWireStream>>,
    ) {
        let outbound = self.outbound.clone();
        tokio::spawn(async move {
            loop {
                let endpoint = { stream.lock().await.endpoint.clone() };
                let envelope = match endpoint.receive().await {
                    Ok(
                        agentdash_integration_remote_runtime::RuntimeWirePlacementEvent::Frame(
                            envelope,
                        ),
                    ) => *envelope,
                    Ok(
                        agentdash_integration_remote_runtime::RuntimeWirePlacementEvent::Disconnected {
                            ..
                        }
                        | agentdash_integration_remote_runtime::RuntimeWirePlacementEvent::Reconnected,
                    ) => continue,
                    Err(_) => break,
                };
                let frame = match stream.lock().await.transport.enqueue(envelope) {
                    Ok(frame) => frame,
                    Err(_) => break,
                };
                let id = format!("runtime-wire:{}:{}", stream_id.0, frame.sequence);
                if !send_outbound(
                    &outbound,
                    RelayMessage::RuntimeWireFrame {
                        id,
                        payload: Box::new(frame),
                    },
                ) {
                    continue;
                }
            }
        });
    }

    pub async fn frame(
        &self,
        id: String,
        frame: agentdash_relay::RuntimeRelayFrame,
    ) -> Vec<RelayMessage> {
        let stream = self.streams.lock().await.get(&frame.stream_id).cloned();
        let Some(stream) = stream else {
            return vec![RelayMessage::Error {
                id,
                error: RelayError::not_found("Runtime Wire stream is not open"),
            }];
        };
        let received = match stream.lock().await.transport.receive(frame) {
            Ok(received) => received,
            Err(error) => return vec![transport_error(id, error)],
        };
        let mut messages = Vec::new();
        if let agentdash_relay::RuntimeRelayReceive::Accepted(envelope) = received {
            let host_port_response = matches!(
                envelope.frame,
                RuntimeWireFrame::Response {
                    response: RuntimeWireResponse::HostPort(_),
                    ..
                }
            );
            if host_port_response {
                let endpoint = { stream.lock().await.endpoint.clone() };
                if let Err(error) = endpoint.send(*envelope).await {
                    return vec![RelayMessage::Error {
                        id,
                        error: RelayError::runtime_error(error.to_string()),
                    }];
                }
            } else {
                let dispatch = { stream.lock().await.dispatch.clone() };
                if dispatch.send((id.clone(), *envelope)).is_err() {
                    return vec![RelayMessage::Error {
                        id,
                        error: RelayError::runtime_error(
                            "Runtime Wire inbound dispatcher is closed",
                        ),
                    }];
                }
            }
        }
        messages.push(RelayMessage::RuntimeWireAck {
            id,
            payload: stream.lock().await.transport.inbound_ack(),
        });
        messages
    }

    pub async fn acknowledge(
        &self,
        id: String,
        ack: agentdash_relay::RuntimeRelayAck,
    ) -> Option<RelayMessage> {
        let stream = self.streams.lock().await.get(&ack.stream_id).cloned();
        let Some(stream) = stream else {
            return Some(RelayMessage::Error {
                id,
                error: RelayError::not_found("Runtime Wire stream is not open"),
            });
        };
        stream
            .lock()
            .await
            .transport
            .acknowledge(ack)
            .err()
            .map(|error| transport_error(id, error))
    }
}

fn send_outbound(
    outbound: &Arc<std::sync::RwLock<Option<tokio::sync::mpsc::UnboundedSender<RelayMessage>>>>,
    message: RelayMessage,
) -> bool {
    let sender = outbound
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let Some(sender) = sender else {
        return false;
    };
    sender.send(message).is_ok()
}

fn transport_error(id: String, error: RuntimeRelayTransportError) -> RelayMessage {
    RelayMessage::Error {
        id,
        error: RelayError::invalid_message(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, str::FromStr};

    use agentdash_agent_runtime_contract::*;
    use agentdash_agent_runtime_wire::*;
    use agentdash_integration_api::AgentServiceDefinitionId;
    use agentdash_relay::{RuntimeRelayFrame, RuntimeRelayProvenance};

    use super::*;

    fn id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid id")
    }

    fn profile() -> RuntimeProfile {
        RuntimeProfile {
            reference_class: ReferenceRuntimeClass::Conversation,
            input: InputProfile {
                modalities: [InputModality::Text].into(),
            },
            instruction: InstructionProfile {
                channels: BTreeSet::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
            },
            tools: ToolProfile {
                channels: BTreeSet::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
                cancellation: true,
            },
            workspace: WorkspaceProfile {
                capabilities: BTreeSet::new(),
                mechanism: DeliveryMechanism::HostAdaptedBoundary,
            },
            interactions: InteractionProfile {
                kinds: BTreeSet::new(),
                durable_correlation: true,
            },
            lifecycle: [LifecycleCapability::ThreadStart].into(),
            hooks: HookProfile {
                points: Vec::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
            },
            context: ContextProfile {
                capabilities: BTreeSet::new(),
                fidelity: ContextFidelity::EventProjected,
                activation_idempotent: false,
            },
            telemetry_config: BTreeSet::new(),
        }
    }

    struct FakeDriver;

    #[async_trait]
    impl AgentRuntimeDriver for FakeDriver {
        async fn describe(
            &self,
            request: DriverDescribeRequest,
        ) -> Result<RuntimeDescriptor, DriverError> {
            Ok(RuntimeDescriptor {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                service_instance_id: request.service_instance_id,
                profile: profile(),
                profile_digest: id("driver-profile"),
            })
        }

        async fn bind(&self, _: DriverBindRequest) -> Result<DriverBinding, DriverError> {
            Err(DriverError::Unsupported {
                reason: "test".into(),
            })
        }

        async fn dispatch(
            &self,
            _: DriverCommandEnvelope,
            _: Arc<dyn DriverEventSink>,
        ) -> Result<DriverDispatchReceipt, DriverError> {
            Err(DriverError::Unsupported {
                reason: "test".into(),
            })
        }

        async fn inspect(&self, _: DriverInspectionQuery) -> Result<DriverInspection, DriverError> {
            Err(DriverError::Unsupported {
                reason: "test".into(),
            })
        }
    }

    struct FakeResolver;

    #[async_trait]
    impl RuntimeDriverEndpointResolver for FakeResolver {
        async fn advertised_offers(
            &self,
        ) -> Result<Vec<agentdash_relay::RuntimeOfferAdvertisement>, String> {
            Ok(Vec::new())
        }

        async fn resolve(
            &self,
            provenance: &RuntimeRelayProvenance,
        ) -> Result<Arc<dyn AgentRuntimeDriver>, String> {
            if provenance.service_definition_id
                != AgentServiceDefinitionId::new("service-definition-local").expect("definition id")
                || provenance.service_instance_id != id("service-local")
                || provenance.driver_generation != RuntimeDriverGeneration(7)
                || provenance.host_id != "local-host"
                || provenance.transport_id
                    != AgentRuntimePlacementId::new("desktop-local").expect("transport id")
            {
                return Err("unknown Runtime Driver provenance".into());
            }
            Ok(Arc::new(FakeDriver))
        }
    }

    struct StaticResolver(Arc<dyn AgentRuntimeDriver>);

    #[async_trait]
    impl RuntimeDriverEndpointResolver for StaticResolver {
        async fn advertised_offers(
            &self,
        ) -> Result<Vec<agentdash_relay::RuntimeOfferAdvertisement>, String> {
            Ok(Vec::new())
        }

        async fn resolve(
            &self,
            _: &RuntimeRelayProvenance,
        ) -> Result<Arc<dyn AgentRuntimeDriver>, String> {
            Ok(self.0.clone())
        }
    }

    struct GatedDriver {
        entered: Arc<tokio::sync::Semaphore>,
        release: Arc<tokio::sync::Semaphore>,
    }

    #[async_trait]
    impl AgentRuntimeDriver for GatedDriver {
        async fn describe(
            &self,
            request: DriverDescribeRequest,
        ) -> Result<RuntimeDescriptor, DriverError> {
            self.entered.add_permits(1);
            self.release
                .acquire()
                .await
                .expect("test release semaphore remains open")
                .forget();
            Ok(RuntimeDescriptor {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                service_instance_id: request.service_instance_id,
                profile: profile(),
                profile_digest: id("gated-driver-profile"),
            })
        }

        async fn bind(&self, _: DriverBindRequest) -> Result<DriverBinding, DriverError> {
            unreachable!("gated driver only serves describe")
        }

        async fn dispatch(
            &self,
            _: DriverCommandEnvelope,
            _: Arc<dyn DriverEventSink>,
        ) -> Result<DriverDispatchReceipt, DriverError> {
            unreachable!("gated driver only serves describe")
        }

        async fn inspect(&self, _: DriverInspectionQuery) -> Result<DriverInspection, DriverError> {
            unreachable!("gated driver only serves describe")
        }
    }

    fn provenance() -> RuntimeRelayProvenance {
        RuntimeRelayProvenance {
            service_definition_id: AgentServiceDefinitionId::new("service-definition-local")
                .expect("definition id"),
            service_instance_id: id("service-local"),
            driver_generation: RuntimeDriverGeneration(7),
            host_id: "local-host".to_string(),
            transport_id: AgentRuntimePlacementId::new("desktop-local").expect("transport id"),
        }
    }

    fn handler() -> (
        RuntimeWireCommandHandler,
        tokio::sync::mpsc::UnboundedReceiver<RelayMessage>,
    ) {
        let (outbound, receiver) = tokio::sync::mpsc::unbounded_channel();
        let handler = RuntimeWireCommandHandler::new(
            Arc::new(FakeResolver),
            RuntimeRelayTransportDescriptor {
                supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
                profile: profile(),
                profile_digest: id("transport-profile"),
                max_in_flight_frames: 8,
            },
        );
        handler.attach_outbound(outbound);
        (handler, receiver)
    }

    #[tokio::test]
    async fn open_frame_duplicate_and_ack_form_one_ordered_driver_exchange() {
        let (handler, mut outbound) = handler();
        let stream_id = RuntimeRelayStreamId("stream-1".into());
        let open = RuntimeRelayOpen {
            stream_id: stream_id.clone(),
            provenance: provenance(),
            supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
            resume_after_sequence: 0,
            max_in_flight_frames: 4,
        };
        assert!(matches!(
            handler.open("open".into(), open).await,
            RelayMessage::RuntimeWireOpenAck { .. }
        ));

        let input = RuntimeRelayFrame {
            stream_id: stream_id.clone(),
            sequence: 1,
            provenance: provenance(),
            envelope: RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: RuntimeWireFrameId(41),
                critical: true,
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::DriverDescribe(
                    DriverDescribeRequest {
                        service_instance_id: id("service-local"),
                    },
                ))),
            },
        };
        let output = handler.frame("request".into(), input.clone()).await;
        assert!(matches!(
            output.as_slice(),
            [RelayMessage::RuntimeWireAck { .. }]
        ));
        let response = tokio::time::timeout(std::time::Duration::from_secs(1), outbound.recv())
            .await
            .expect("Runtime Wire response timeout")
            .expect("Runtime Wire outbound closed");
        let RelayMessage::RuntimeWireFrame { payload, .. } = &response else {
            panic!("expected response frame")
        };
        let RuntimeWireFrame::Response {
            request_frame_id, ..
        } = &payload.envelope.frame
        else {
            panic!("expected correlated response")
        };
        assert_eq!(*request_frame_id, RuntimeWireFrameId(41));

        let duplicate = handler.frame("duplicate".into(), input).await;
        assert!(matches!(
            duplicate.as_slice(),
            [RelayMessage::RuntimeWireAck { .. }]
        ));
        assert!(
            handler
                .acknowledge(
                    "ack".into(),
                    agentdash_relay::RuntimeRelayAck {
                        stream_id,
                        through_sequence: payload.sequence,
                    }
                )
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn frame_ack_does_not_wait_for_driver_dispatch_completion() {
        let entered = Arc::new(tokio::sync::Semaphore::new(0));
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        let driver: Arc<dyn AgentRuntimeDriver> = Arc::new(GatedDriver {
            entered: entered.clone(),
            release: release.clone(),
        });
        let (outbound_tx, mut outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        let handler = RuntimeWireCommandHandler::new(
            Arc::new(StaticResolver(driver)),
            RuntimeRelayTransportDescriptor {
                supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
                profile: profile(),
                profile_digest: id("transport-profile"),
                max_in_flight_frames: 8,
            },
        );
        handler.attach_outbound(outbound_tx);
        let stream_id = RuntimeRelayStreamId("stream-non-blocking-dispatch".into());
        assert!(matches!(
            handler
                .open(
                    "open".into(),
                    RuntimeRelayOpen {
                        stream_id: stream_id.clone(),
                        provenance: provenance(),
                        supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
                        resume_after_sequence: 0,
                        max_in_flight_frames: 4,
                    },
                )
                .await,
            RelayMessage::RuntimeWireOpenAck { .. }
        ));

        let ack = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            handler.frame(
                "request".into(),
                RuntimeRelayFrame {
                    stream_id: stream_id.clone(),
                    sequence: 1,
                    provenance: provenance(),
                    envelope: RuntimeWireEnvelope {
                        protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                        frame_id: RuntimeWireFrameId(61),
                        critical: true,
                        frame: RuntimeWireFrame::Request(Box::new(
                            RuntimeWireRequest::DriverDescribe(DriverDescribeRequest {
                                service_instance_id: id("service-local"),
                            }),
                        )),
                    },
                },
            ),
        )
        .await
        .expect("frame must acknowledge while driver dispatch remains blocked");
        assert!(matches!(
            ack.as_slice(),
            [RelayMessage::RuntimeWireAck { .. }]
        ));
        entered
            .acquire()
            .await
            .expect("driver dispatch starts")
            .forget();
        assert!(outbound_rx.try_recv().is_err());

        let second_ack = handler
            .frame(
                "second-request".into(),
                RuntimeRelayFrame {
                    stream_id,
                    sequence: 2,
                    provenance: provenance(),
                    envelope: RuntimeWireEnvelope {
                        protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                        frame_id: RuntimeWireFrameId(62),
                        critical: true,
                        frame: RuntimeWireFrame::Request(Box::new(
                            RuntimeWireRequest::DriverDescribe(DriverDescribeRequest {
                                service_instance_id: id("service-local"),
                            }),
                        )),
                    },
                },
            )
            .await;
        assert!(matches!(
            second_ack.as_slice(),
            [RelayMessage::RuntimeWireAck { .. }]
        ));
        assert!(
            entered.try_acquire().is_err(),
            "same-stream Driver requests must remain serialized"
        );

        release.add_permits(1);
        let response = tokio::time::timeout(std::time::Duration::from_secs(1), outbound_rx.recv())
            .await
            .expect("driver response timeout")
            .expect("Runtime Wire outbound closed");
        assert!(matches!(response, RelayMessage::RuntimeWireFrame { .. }));
        entered
            .acquire()
            .await
            .expect("second driver dispatch starts after first completes")
            .forget();
        release.add_permits(1);
        tokio::time::timeout(std::time::Duration::from_secs(1), outbound_rx.recv())
            .await
            .expect("second driver response timeout")
            .expect("Runtime Wire outbound closed");
    }

    #[tokio::test]
    async fn open_rejects_an_unknown_driver_generation_without_creating_a_stream() {
        let (handler, _outbound) = handler();
        let mut invalid = provenance();
        invalid.driver_generation = RuntimeDriverGeneration(8);
        let result = handler
            .open(
                "open".into(),
                RuntimeRelayOpen {
                    stream_id: RuntimeRelayStreamId("stream-invalid".into()),
                    provenance: invalid,
                    supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
                    resume_after_sequence: 0,
                    max_in_flight_frames: 4,
                },
            )
            .await;
        assert!(matches!(result, RelayMessage::Error { .. }));
    }

    #[tokio::test]
    async fn reconnect_replaces_outbound_and_replays_unacknowledged_frames() {
        let (handler, mut first_outbound) = handler();
        let stream_id = RuntimeRelayStreamId("stream-reconnect".into());
        let open = RuntimeRelayOpen {
            stream_id: stream_id.clone(),
            provenance: provenance(),
            supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
            resume_after_sequence: 0,
            max_in_flight_frames: 4,
        };
        assert!(matches!(
            handler.open("open".into(), open.clone()).await,
            RelayMessage::RuntimeWireOpenAck { .. }
        ));
        let request = RuntimeRelayFrame {
            stream_id,
            sequence: 1,
            provenance: provenance(),
            envelope: RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: RuntimeWireFrameId(51),
                critical: true,
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::DriverDescribe(
                    DriverDescribeRequest {
                        service_instance_id: id("service-local"),
                    },
                ))),
            },
        };
        handler.frame("request".into(), request).await;
        let first = tokio::time::timeout(std::time::Duration::from_secs(1), first_outbound.recv())
            .await
            .expect("first outbound timeout")
            .expect("first outbound closed");
        let RelayMessage::RuntimeWireFrame { payload: first, .. } = first else {
            panic!("expected first Runtime Wire frame")
        };

        handler.detach_outbound();
        let (second_tx, mut second_outbound) = tokio::sync::mpsc::unbounded_channel();
        handler.attach_outbound(second_tx);
        assert!(matches!(
            handler.open("reopen".into(), open).await,
            RelayMessage::RuntimeWireOpenAck { .. }
        ));
        let replay =
            tokio::time::timeout(std::time::Duration::from_secs(1), second_outbound.recv())
                .await
                .expect("replay timeout")
                .expect("second outbound closed");
        let RelayMessage::RuntimeWireFrame {
            payload: replay, ..
        } = replay
        else {
            panic!("expected replayed Runtime Wire frame")
        };
        assert_eq!(replay.sequence, first.sequence);
        assert_eq!(replay.envelope, first.envelope);
    }
}
