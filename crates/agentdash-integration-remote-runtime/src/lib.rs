use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_wire::*;
use agentdash_integration_api::*;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoteRuntimeTransportError {
    #[error("remote runtime placement is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("remote runtime returned a protocol violation: {reason}")]
    Protocol { reason: String, critical: bool },
}

#[async_trait]
pub trait RuntimeWirePlacement: Send + Sync {
    async fn send(&self, frame: RuntimeWireEnvelope) -> Result<(), RemoteRuntimeTransportError>;

    async fn receive(&self) -> Result<RuntimeWireEnvelope, RemoteRuntimeTransportError>;
}

#[async_trait]
pub trait RuntimeWirePlacementResolver: Send + Sync {
    async fn resolve(
        &self,
        host_id: &str,
        transport_id: &AgentRuntimePlacementId,
    ) -> Result<Arc<dyn RuntimeWirePlacement>, RemoteRuntimeTransportError>;
}

pub struct RemoteRuntimeDriverFactory {
    key: AgentRuntimeFactoryKey,
    placements: Arc<dyn RuntimeWirePlacementResolver>,
}

impl RemoteRuntimeDriverFactory {
    pub fn new(
        key: AgentRuntimeFactoryKey,
        placements: Arc<dyn RuntimeWirePlacementResolver>,
    ) -> Self {
        Self { key, placements }
    }
}

#[async_trait]
impl AgentRuntimeDriverFactory for RemoteRuntimeDriverFactory {
    fn factory_key(&self) -> &AgentRuntimeFactoryKey {
        &self.key
    }

    async fn create(
        &self,
        instance: ActivatedAgentServiceInstance,
        _host: RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, DriverFactoryError> {
        let AgentRuntimePlacement::Remote {
            host_id,
            transport_id,
        } = &instance.placement
        else {
            return Err(DriverFactoryError::InvalidConfiguration {
                reason: "Runtime Wire remote driver requires remote placement".to_string(),
            });
        };
        if !instance
            .definition
            .supported_protocol_revisions
            .contains(&RUNTIME_WIRE_PROTOCOL_REVISION)
        {
            return Err(DriverFactoryError::InvalidConfiguration {
                reason: "service definition does not support the owned Runtime Wire revision"
                    .to_string(),
            });
        }
        let placement = self
            .placements
            .resolve(host_id, transport_id)
            .await
            .map_err(factory_transport_error)?;
        let driver = Arc::new(RemoteRuntimeDriver {
            instance_id: instance.instance_id,
            generation: instance.generation,
            placement,
            next_frame_id: AtomicU64::new(1),
            pending: tokio::sync::Mutex::new(HashMap::new()),
            active_bindings: tokio::sync::Mutex::new(HashMap::new()),
            connection_lost: AtomicBool::new(false),
        });
        driver.clone().start_receive_pump();
        Ok(driver)
    }
}

pub fn remote_runtime_contribution(
    definition: AgentServiceDefinition,
    placements: Arc<dyn RuntimeWirePlacementResolver>,
) -> AgentRuntimeDriverContribution {
    let factory_key = definition.factory_key.clone();
    AgentRuntimeDriverContribution {
        definition,
        factory: Arc::new(RemoteRuntimeDriverFactory::new(factory_key, placements)),
    }
}

struct RemoteRuntimeDriver {
    instance_id: RuntimeServiceInstanceId,
    generation: RuntimeDriverGeneration,
    placement: Arc<dyn RuntimeWirePlacement>,
    next_frame_id: AtomicU64,
    pending: tokio::sync::Mutex<HashMap<u64, tokio::sync::oneshot::Sender<RuntimeWireResponse>>>,
    active_bindings: tokio::sync::Mutex<HashMap<RuntimeBindingId, ActiveRemoteBinding>>,
    connection_lost: AtomicBool,
}

#[derive(Clone)]
struct ActiveRemoteBinding {
    binding_id: RuntimeBindingId,
    generation: RuntimeDriverGeneration,
    source_thread_id: DriverThreadId,
    source_turn_id: Option<DriverTurnId>,
    sink: Arc<dyn DriverEventSink>,
}

#[async_trait]
impl AgentRuntimeDriver for RemoteRuntimeDriver {
    async fn describe(
        &self,
        request: DriverDescribeRequest,
    ) -> Result<RuntimeDescriptor, DriverError> {
        let response = self
            .request(RuntimeWireRequest::DriverDescribe(request))
            .await?;
        match response {
            RuntimeWireResponse::DriverDescribe(DriverDescribeResult::Ok(value)) => Ok(*value),
            RuntimeWireResponse::DriverDescribe(DriverDescribeResult::Error(error)) => Err(error),
            _ => Err(protocol_error("driver describe response mismatch")),
        }
    }

    async fn bind(&self, request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
        if request.service_instance_id != self.instance_id {
            return Err(DriverError::Rejected {
                reason: "binding targets another service instance".to_string(),
            });
        }
        let response = self
            .request(RuntimeWireRequest::DriverBind(request))
            .await?;
        match response {
            RuntimeWireResponse::DriverBind(DriverBindResult::Ok(value)) => Ok(*value),
            RuntimeWireResponse::DriverBind(DriverBindResult::Error(error)) => Err(error),
            _ => Err(protocol_error("driver bind response mismatch")),
        }
    }

    async fn dispatch(
        &self,
        command: DriverCommandEnvelope,
        sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, DriverError> {
        if command.generation != self.generation {
            return Err(DriverError::StaleGeneration);
        }
        let binding_id = command.binding_id.clone();
        self.active_bindings.lock().await.insert(
            binding_id.clone(),
            ActiveRemoteBinding {
                binding_id: command.binding_id.clone(),
                generation: command.generation,
                source_thread_id: command.source_thread_id.clone(),
                source_turn_id: None,
                sink,
            },
        );
        let response = self
            .request(RuntimeWireRequest::DriverDispatch(command))
            .await;
        match response {
            Ok(RuntimeWireResponse::DriverDispatch(DriverDispatchResult::Ok(value))) => Ok(*value),
            Ok(RuntimeWireResponse::DriverDispatch(DriverDispatchResult::Error(error))) => {
                Err(error)
            }
            Ok(_) => Err(protocol_error("driver dispatch response mismatch")),
            Err(error) => Err(error),
        }
    }

    async fn inspect(&self, query: DriverInspectionQuery) -> Result<DriverInspection, DriverError> {
        let response = self
            .request(RuntimeWireRequest::DriverInspect(query))
            .await?;
        match response {
            RuntimeWireResponse::DriverInspect(DriverInspectResult::Ok(value)) => Ok(*value),
            RuntimeWireResponse::DriverInspect(DriverInspectResult::Error(error)) => Err(error),
            _ => Err(protocol_error("driver inspect response mismatch")),
        }
    }
}

impl RemoteRuntimeDriver {
    fn start_receive_pump(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                match self.placement.receive().await {
                    Ok(envelope) => self.handle_inbound(envelope).await,
                    Err(error) => {
                        self.handle_disconnect(error.to_string()).await;
                        break;
                    }
                }
            }
        });
    }

    async fn handle_inbound(&self, envelope: RuntimeWireEnvelope) {
        match envelope.frame {
            RuntimeWireFrame::Response {
                request_frame_id,
                response,
            } => {
                if let Some(pending) = self.pending.lock().await.remove(&request_frame_id.0) {
                    let _ = pending.send(response);
                }
            }
            RuntimeWireFrame::Notification(RuntimeWireNotification::DriverEvent(event)) => {
                if event.generation != self.generation {
                    return;
                }
                let sink = {
                    self.active_bindings
                        .lock()
                        .await
                        .get(&event.binding_id)
                        .map(|binding| binding.sink.clone())
                };
                if let Some(sink) = sink {
                    let _ = sink.emit(event).await;
                }
            }
            RuntimeWireFrame::Ack(_) | RuntimeWireFrame::Notification(_) => {}
            RuntimeWireFrame::Request(_) => {
                self.handle_disconnect(
                    "remote placement sent a request to a Driver client".to_string(),
                )
                .await
            }
        }
    }

    async fn handle_disconnect(&self, reason: String) {
        if self.connection_lost.swap(true, Ordering::AcqRel) {
            return;
        }
        self.pending.lock().await.clear();
        let bindings = std::mem::take(&mut *self.active_bindings.lock().await);
        for (_, binding) in bindings {
            let _ = binding
                .sink
                .emit(DriverEventEnvelope {
                    binding_id: binding.binding_id.clone(),
                    generation: binding.generation,
                    source_thread_id: binding.source_thread_id,
                    source_turn_id: binding.source_turn_id,
                    source_item_id: None,
                    event: RuntimeEvent::BindingLost {
                        binding_id: binding.binding_id,
                        reason: reason.clone(),
                    },
                })
                .await;
        }
    }

    async fn request(
        &self,
        request: RuntimeWireRequest,
    ) -> Result<RuntimeWireResponse, DriverError> {
        if self.connection_lost.load(Ordering::Acquire) {
            return Err(DriverError::Lost {
                reason: "remote Runtime Wire placement is disconnected".to_string(),
                retryable: true,
            });
        }
        let frame_id = RuntimeWireFrameId(self.next_frame_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().await.insert(frame_id.0, tx);
        if let Err(error) = self
            .placement
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id,
                critical: true,
                frame: RuntimeWireFrame::Request(request),
            })
            .await
        {
            self.pending.lock().await.remove(&frame_id.0);
            return Err(driver_transport_error(error));
        }
        rx.await.map_err(|_| DriverError::Lost {
            reason: "remote transport closed before correlated response".to_string(),
            retryable: true,
        })
    }
}

fn protocol_error(reason: &str) -> DriverError {
    DriverError::ProtocolViolation {
        reason: reason.to_string(),
        critical: true,
    }
}

fn driver_transport_error(error: RemoteRuntimeTransportError) -> DriverError {
    match error {
        RemoteRuntimeTransportError::Unavailable { reason, retryable } => {
            DriverError::Unavailable { reason, retryable }
        }
        RemoteRuntimeTransportError::Protocol { reason, critical } => {
            DriverError::ProtocolViolation { reason, critical }
        }
    }
}

fn factory_transport_error(error: RemoteRuntimeTransportError) -> DriverFactoryError {
    match error {
        RemoteRuntimeTransportError::Unavailable { reason, retryable } => {
            DriverFactoryError::Unavailable { reason, retryable }
        }
        RemoteRuntimeTransportError::Protocol { reason, .. } => DriverFactoryError::Unavailable {
            reason,
            retryable: false,
        },
    }
}

/// Local-side terminator for a concrete Native/Codex/enterprise driver. It executes only owned
/// Driver Wire requests and never constructs a second Managed Runtime.
pub struct RuntimeWireDriverEndpoint {
    driver: Arc<dyn AgentRuntimeDriver>,
    next_frame_id: Arc<AtomicU64>,
    outbound_tx: tokio::sync::mpsc::UnboundedSender<RuntimeWireEnvelope>,
    outbound_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<RuntimeWireEnvelope>>,
}

impl RuntimeWireDriverEndpoint {
    pub fn new(driver: Arc<dyn AgentRuntimeDriver>) -> Self {
        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            driver,
            next_frame_id: Arc::new(AtomicU64::new(1)),
            outbound_tx,
            outbound_rx: tokio::sync::Mutex::new(outbound_rx),
        }
    }

    fn response(
        &self,
        request_frame_id: RuntimeWireFrameId,
        response: RuntimeWireResponse,
    ) -> RuntimeWireEnvelope {
        RuntimeWireEnvelope {
            protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
            frame_id: RuntimeWireFrameId(self.next_frame_id.fetch_add(1, Ordering::Relaxed)),
            critical: true,
            frame: RuntimeWireFrame::Response {
                request_frame_id,
                response,
            },
        }
    }
}

#[async_trait]
impl RuntimeWirePlacement for RuntimeWireDriverEndpoint {
    async fn send(&self, request: RuntimeWireEnvelope) -> Result<(), RemoteRuntimeTransportError> {
        if request.protocol_revision != RUNTIME_WIRE_PROTOCOL_REVISION {
            return Err(RemoteRuntimeTransportError::Protocol {
                reason: "unsupported Runtime Wire revision".to_string(),
                critical: true,
            });
        }
        let RuntimeWireFrame::Request(method) = request.frame else {
            return Err(RemoteRuntimeTransportError::Protocol {
                reason: "local endpoint accepts request frames only".to_string(),
                critical: true,
            });
        };
        let response = match method {
            RuntimeWireRequest::DriverDescribe(value) => {
                RuntimeWireResponse::DriverDescribe(match self.driver.describe(value).await {
                    Ok(value) => DriverDescribeResult::Ok(Box::new(value)),
                    Err(error) => DriverDescribeResult::Error(error),
                })
            }
            RuntimeWireRequest::DriverBind(value) => {
                RuntimeWireResponse::DriverBind(match self.driver.bind(value).await {
                    Ok(value) => DriverBindResult::Ok(Box::new(value)),
                    Err(error) => DriverBindResult::Error(error),
                })
            }
            RuntimeWireRequest::DriverDispatch(value) => {
                let sink = Arc::new(ForwardingSink {
                    outbound: self.outbound_tx.clone(),
                    next_frame_id: self.next_frame_id.clone(),
                });
                let result = self.driver.dispatch(value, sink).await;
                RuntimeWireResponse::DriverDispatch(match result {
                    Ok(value) => DriverDispatchResult::Ok(Box::new(value)),
                    Err(error) => DriverDispatchResult::Error(error),
                })
            }
            RuntimeWireRequest::DriverInspect(value) => {
                RuntimeWireResponse::DriverInspect(match self.driver.inspect(value).await {
                    Ok(value) => DriverInspectResult::Ok(Box::new(value)),
                    Err(error) => DriverInspectResult::Error(error),
                })
            }
            _ => {
                return Err(RemoteRuntimeTransportError::Protocol {
                    reason: "local driver endpoint cannot own Managed Runtime requests".to_string(),
                    critical: true,
                });
            }
        };
        self.outbound_tx
            .send(self.response(request.frame_id, response))
            .map_err(|_| RemoteRuntimeTransportError::Unavailable {
                reason: "local Runtime Wire receiver is closed".to_string(),
                retryable: true,
            })?;
        Ok(())
    }

    async fn receive(&self) -> Result<RuntimeWireEnvelope, RemoteRuntimeTransportError> {
        self.outbound_rx.lock().await.recv().await.ok_or_else(|| {
            RemoteRuntimeTransportError::Unavailable {
                reason: "local Runtime Wire endpoint closed".to_string(),
                retryable: true,
            }
        })
    }
}

struct ForwardingSink {
    outbound: tokio::sync::mpsc::UnboundedSender<RuntimeWireEnvelope>,
    next_frame_id: Arc<AtomicU64>,
}

#[async_trait]
impl DriverEventSink for ForwardingSink {
    async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
        self.outbound
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: RuntimeWireFrameId(self.next_frame_id.fetch_add(1, Ordering::Relaxed)),
                critical: true,
                frame: RuntimeWireFrame::Notification(RuntimeWireNotification::DriverEvent(event)),
            })
            .map_err(|_| DriverError::Lost {
                reason: "Runtime Wire notification receiver is closed".to_string(),
                retryable: true,
            })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, str::FromStr};

    use super::*;

    fn id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid id")
    }

    fn profile() -> RuntimeProfile {
        RuntimeProfile {
            reference_class: ReferenceRuntimeClass::Turn,
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
                cancellation: false,
            },
            workspace: WorkspaceProfile {
                capabilities: BTreeSet::new(),
                mechanism: DeliveryMechanism::Observed,
            },
            interactions: InteractionProfile {
                kinds: BTreeSet::new(),
                durable_correlation: false,
            },
            lifecycle: [
                LifecycleCapability::ThreadStart,
                LifecycleCapability::TurnStart,
            ]
            .into(),
            hooks: HookProfile {
                points: Vec::new(),
                configuration_boundary: ConfigurationBoundary::Binding,
            },
            context: ContextProfile {
                capabilities: BTreeSet::new(),
                fidelity: ContextFidelity::Opaque,
                activation_idempotent: false,
            },
            telemetry_config: BTreeSet::new(),
        }
    }

    struct FakeDriver;

    struct AsyncEventDriver;

    #[derive(Default)]
    struct RecordingSink {
        events: tokio::sync::Mutex<Vec<DriverEventEnvelope>>,
    }

    #[async_trait]
    impl DriverEventSink for RecordingSink {
        async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
            self.events.lock().await.push(event);
            Ok(())
        }
    }

    struct ClosedPlacement;

    #[async_trait]
    impl RuntimeWirePlacement for ClosedPlacement {
        async fn send(&self, _: RuntimeWireEnvelope) -> Result<(), RemoteRuntimeTransportError> {
            Ok(())
        }

        async fn receive(&self) -> Result<RuntimeWireEnvelope, RemoteRuntimeTransportError> {
            Err(RemoteRuntimeTransportError::Unavailable {
                reason: "closed".to_string(),
                retryable: true,
            })
        }
    }

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
                profile_digest: id("remote-profile"),
            })
        }

        async fn bind(&self, _request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
            Err(DriverError::Unsupported {
                reason: "test".to_string(),
            })
        }

        async fn dispatch(
            &self,
            _command: DriverCommandEnvelope,
            _sink: Arc<dyn DriverEventSink>,
        ) -> Result<DriverDispatchReceipt, DriverError> {
            Err(DriverError::Unsupported {
                reason: "test".to_string(),
            })
        }

        async fn inspect(
            &self,
            _query: DriverInspectionQuery,
        ) -> Result<DriverInspection, DriverError> {
            Err(DriverError::Unsupported {
                reason: "test".to_string(),
            })
        }
    }

    #[async_trait]
    impl AgentRuntimeDriver for AsyncEventDriver {
        async fn describe(
            &self,
            _request: DriverDescribeRequest,
        ) -> Result<RuntimeDescriptor, DriverError> {
            Err(DriverError::Unsupported {
                reason: "test".into(),
            })
        }

        async fn bind(&self, _request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
            Err(DriverError::Unsupported {
                reason: "test".into(),
            })
        }

        async fn dispatch(
            &self,
            command: DriverCommandEnvelope,
            sink: Arc<dyn DriverEventSink>,
        ) -> Result<DriverDispatchReceipt, DriverError> {
            let request_id = command.request_id.clone();
            tokio::spawn(async move {
                tokio::task::yield_now().await;
                sink.emit(DriverEventEnvelope {
                    binding_id: command.binding_id.clone(),
                    generation: command.generation,
                    source_thread_id: command.source_thread_id,
                    source_turn_id: None,
                    source_item_id: None,
                    event: RuntimeEvent::BindingEstablished {
                        binding_id: command.binding_id,
                    },
                })
                .await
                .expect("forward delayed event");
            });
            Ok(DriverDispatchReceipt {
                request_id,
                duplicate: false,
                applied_tool_set: None,
            })
        }

        async fn inspect(
            &self,
            _query: DriverInspectionQuery,
        ) -> Result<DriverInspection, DriverError> {
            Err(DriverError::Unsupported {
                reason: "test".into(),
            })
        }
    }

    #[tokio::test]
    async fn local_endpoint_preserves_request_correlation_and_owned_descriptor() {
        let endpoint = RuntimeWireDriverEndpoint::new(Arc::new(FakeDriver));
        endpoint
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: RuntimeWireFrameId(41),
                critical: true,
                frame: RuntimeWireFrame::Request(RuntimeWireRequest::DriverDescribe(
                    DriverDescribeRequest {
                        service_instance_id: id("service-loopback"),
                    },
                )),
            })
            .await
            .expect("send loopback describe");
        let frame = endpoint.receive().await.expect("loopback describe");
        let RuntimeWireFrame::Response {
            request_frame_id,
            response,
        } = &frame.frame
        else {
            panic!("expected response")
        };
        assert_eq!(*request_frame_id, RuntimeWireFrameId(41));
        let RuntimeWireResponse::DriverDescribe(DriverDescribeResult::Ok(descriptor)) = response
        else {
            panic!("expected descriptor")
        };
        assert_eq!(descriptor.service_instance_id, id("service-loopback"));
    }

    #[tokio::test]
    async fn local_driver_endpoint_rejects_managed_runtime_requests() {
        let endpoint = RuntimeWireDriverEndpoint::new(Arc::new(FakeDriver));
        let error = endpoint
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: RuntimeWireFrameId(9),
                critical: true,
                frame: RuntimeWireFrame::Request(RuntimeWireRequest::Snapshot(
                    RuntimeSnapshotQuery::Thread {
                        thread_id: id("thread"),
                        at_revision: None,
                    },
                )),
            })
            .await
            .expect_err("local placement must not host a second Managed Runtime");
        assert!(matches!(
            error,
            RemoteRuntimeTransportError::Protocol { critical: true, .. }
        ));
    }

    #[tokio::test]
    async fn remote_driver_reports_lost_when_transport_closes_before_correlation() {
        let driver = Arc::new(RemoteRuntimeDriver {
            instance_id: id("service-remote"),
            generation: RuntimeDriverGeneration(3),
            placement: Arc::new(ClosedPlacement),
            next_frame_id: AtomicU64::new(1),
            pending: tokio::sync::Mutex::new(HashMap::new()),
            active_bindings: tokio::sync::Mutex::new(HashMap::new()),
            connection_lost: AtomicBool::new(false),
        });
        driver.clone().start_receive_pump();
        let error = driver
            .describe(DriverDescribeRequest {
                service_instance_id: id("service-remote"),
            })
            .await
            .expect_err("EOF cannot be interpreted as a completed response");
        assert!(matches!(
            error,
            DriverError::Lost {
                retryable: true,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn dispatch_keeps_forwarding_events_after_the_receipt() {
        let placement: Arc<dyn RuntimeWirePlacement> =
            Arc::new(RuntimeWireDriverEndpoint::new(Arc::new(AsyncEventDriver)));
        let driver = Arc::new(RemoteRuntimeDriver {
            instance_id: id("service-remote"),
            generation: RuntimeDriverGeneration(3),
            placement,
            next_frame_id: AtomicU64::new(1),
            pending: tokio::sync::Mutex::new(HashMap::new()),
            active_bindings: tokio::sync::Mutex::new(HashMap::new()),
            connection_lost: AtomicBool::new(false),
        });
        driver.clone().start_receive_pump();
        let sink = Arc::new(RecordingSink::default());
        let receipt = driver
            .dispatch(
                DriverCommandEnvelope {
                    request_id: id("request-1"),
                    binding_id: id("binding-1"),
                    generation: RuntimeDriverGeneration(3),
                    source_thread_id: id("source-thread-1"),
                    command: RuntimeCommand::ThreadResume {
                        thread_id: id("runtime-thread-1"),
                    },
                },
                sink.clone(),
            )
            .await
            .expect("dispatch receipt");
        assert_eq!(receipt.applied_tool_set, None);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if !sink.events.lock().await.is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("delayed Driver event must remain connected after receipt");
    }
}
