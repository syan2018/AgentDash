use std::{collections::HashMap, sync::Arc};

use agentdash_agent_runtime_contract::AgentRuntimeDriver;
use agentdash_integration_remote_runtime::{RuntimeWireDriverEndpoint, RuntimeWirePlacement};
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
}

struct ActiveRuntimeWireStream {
    transport: RuntimeRelayStream,
    endpoint: Arc<RuntimeWireDriverEndpoint>,
}

pub struct RuntimeWireCommandHandler {
    resolver: Arc<dyn RuntimeDriverEndpointResolver>,
    descriptor: RuntimeRelayTransportDescriptor,
    streams: Mutex<HashMap<RuntimeRelayStreamId, Arc<Mutex<ActiveRuntimeWireStream>>>>,
    outbound: tokio::sync::mpsc::UnboundedSender<RelayMessage>,
}

impl RuntimeWireCommandHandler {
    pub fn new(
        resolver: Arc<dyn RuntimeDriverEndpointResolver>,
        descriptor: RuntimeRelayTransportDescriptor,
        outbound: tokio::sync::mpsc::UnboundedSender<RelayMessage>,
    ) -> Self {
        Self {
            resolver,
            descriptor,
            streams: Mutex::new(HashMap::new()),
            outbound,
        }
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
                        if self
                            .outbound
                            .send(RelayMessage::RuntimeWireFrame {
                                id: frame_id,
                                payload: Box::new(frame),
                            })
                            .is_err()
                        {
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
        let stream = Arc::new(Mutex::new(ActiveRuntimeWireStream {
            transport,
            endpoint: Arc::new(RuntimeWireDriverEndpoint::new(driver)),
        }));
        self.streams
            .lock()
            .await
            .insert(stream_id.clone(), stream.clone());
        self.start_outbound_pump(stream_id, stream);
        RelayMessage::RuntimeWireOpenAck { id, payload: ack }
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
                    Ok(envelope) => envelope,
                    Err(_) => break,
                };
                let frame = match stream.lock().await.transport.enqueue(envelope) {
                    Ok(frame) => frame,
                    Err(_) => break,
                };
                let id = format!("runtime-wire:{}:{}", stream_id.0, frame.sequence);
                if outbound
                    .send(RelayMessage::RuntimeWireFrame {
                        id,
                        payload: Box::new(frame),
                    })
                    .is_err()
                {
                    break;
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
            let endpoint = { stream.lock().await.endpoint.clone() };
            if let Err(error) = endpoint.send(*envelope).await {
                return vec![RelayMessage::Error {
                    id,
                    error: RelayError::runtime_error(error.to_string()),
                }];
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
        async fn resolve(
            &self,
            provenance: &RuntimeRelayProvenance,
        ) -> Result<Arc<dyn AgentRuntimeDriver>, String> {
            if provenance.service_definition_id != "service-definition-local"
                || provenance.service_instance_id != id("service-local")
                || provenance.binding_id != id("binding-local")
                || provenance.binding_generation != RuntimeDriverGeneration(7)
                || provenance.profile_digest != id("binding-profile")
                || provenance.transport_id != "desktop-local"
            {
                return Err("unknown Runtime Driver provenance".into());
            }
            Ok(Arc::new(FakeDriver))
        }
    }

    fn provenance() -> RuntimeRelayProvenance {
        RuntimeRelayProvenance {
            service_definition_id: "service-definition-local".to_string(),
            service_instance_id: id("service-local"),
            binding_id: id("binding-local"),
            binding_generation: RuntimeDriverGeneration(7),
            profile_digest: id("binding-profile"),
            transport_id: "desktop-local".into(),
        }
    }

    fn handler() -> (
        RuntimeWireCommandHandler,
        tokio::sync::mpsc::UnboundedReceiver<RelayMessage>,
    ) {
        let (outbound, receiver) = tokio::sync::mpsc::unbounded_channel();
        (
            RuntimeWireCommandHandler::new(
                Arc::new(FakeResolver),
                RuntimeRelayTransportDescriptor {
                    supported_protocol_revisions: vec![RUNTIME_WIRE_PROTOCOL_REVISION],
                    profile: profile(),
                    profile_digest: id("transport-profile"),
                    max_in_flight_frames: 8,
                },
                outbound,
            ),
            receiver,
        )
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
                frame: RuntimeWireFrame::Request(RuntimeWireRequest::DriverDescribe(
                    DriverDescribeRequest {
                        service_instance_id: id("service-local"),
                    },
                )),
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
    async fn open_rejects_an_unknown_driver_generation_without_creating_a_stream() {
        let (handler, _outbound) = handler();
        let mut invalid = provenance();
        invalid.binding_generation = RuntimeDriverGeneration(8);
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
}
