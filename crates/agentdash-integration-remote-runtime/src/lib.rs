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
use serde::Deserialize;
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

    async fn receive(&self) -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError>;

    async fn acknowledge_disconnect(&self) {}
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeWirePlacementEvent {
    Frame(Box<RuntimeWireEnvelope>),
    Disconnected { reason: String },
    Reconnected,
}

#[async_trait]
pub trait RuntimeWirePlacementResolver: Send + Sync {
    async fn resolve(
        &self,
        request: RuntimeWirePlacementRequest,
    ) -> Result<Arc<dyn RuntimeWirePlacement>, RemoteRuntimeTransportError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeWirePlacementRequest {
    pub host_id: String,
    pub transport_id: AgentRuntimePlacementId,
    pub definition_id: AgentServiceDefinitionId,
    pub service_instance_id: RuntimeServiceInstanceId,
    pub generation: RuntimeDriverGeneration,
    pub host_incarnation_id: agentdash_agent_runtime_contract::HostIncarnationId,
}

pub struct RemoteRuntimeDriverFactory {
    key: AgentRuntimeFactoryKey,
    placements: Arc<dyn RuntimeWirePlacementResolver>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemoteRuntimeProxyConfig {
    source_service_instance_id: RuntimeServiceInstanceId,
    source_driver_generation: RuntimeDriverGeneration,
    source_host_incarnation_id: agentdash_agent_runtime_contract::HostIncarnationId,
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
        host: RuntimeDriverHostPorts,
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
        let proxy_config: RemoteRuntimeProxyConfig =
            serde_json::from_value(instance.config.clone()).map_err(|error| {
                DriverFactoryError::InvalidConfiguration {
                    reason: format!("invalid Runtime Wire proxy coordinates: {error}"),
                }
            })?;
        let placement = self
            .placements
            .resolve(RuntimeWirePlacementRequest {
                host_id: host_id.clone(),
                transport_id: transport_id.clone(),
                definition_id: instance.definition.provenance.definition_id.clone(),
                service_instance_id: proxy_config.source_service_instance_id.clone(),
                generation: proxy_config.source_driver_generation,
                host_incarnation_id: proxy_config.source_host_incarnation_id,
            })
            .await
            .map_err(factory_transport_error)?;
        let driver = Arc::new(RemoteRuntimeDriver {
            instance_id: instance.instance_id,
            generation: instance.generation,
            source_instance_id: proxy_config.source_service_instance_id,
            source_generation: proxy_config.source_driver_generation,
            placement,
            host,
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
        conversation_projection:
            agentdash_integration_api::DriverConversationProjectionProfile::full_fidelity(1),
        factory: Arc::new(RemoteRuntimeDriverFactory::new(factory_key, placements)),
    }
}

struct RemoteRuntimeDriver {
    instance_id: RuntimeServiceInstanceId,
    generation: RuntimeDriverGeneration,
    source_instance_id: RuntimeServiceInstanceId,
    source_generation: RuntimeDriverGeneration,
    placement: Arc<dyn RuntimeWirePlacement>,
    host: RuntimeDriverHostPorts,
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
        if request.service_instance_id != self.instance_id {
            return Err(DriverError::Rejected {
                reason: "describe targets another service instance".to_string(),
            });
        }
        let response = self
            .request(RuntimeWireRequest::DriverDescribe(DriverDescribeRequest {
                service_instance_id: self.source_instance_id.clone(),
            }))
            .await?;
        match response {
            RuntimeWireResponse::DriverDescribe(RuntimeWireDriverDescribeResult::Ok(value)) => {
                let mut descriptor = *value;
                if descriptor.service_instance_id != self.source_instance_id {
                    return Err(protocol_error(
                        "remote descriptor returned another source service instance",
                    ));
                }
                descriptor.service_instance_id = self.instance_id.clone();
                Ok(descriptor)
            }
            RuntimeWireResponse::DriverDescribe(RuntimeWireDriverDescribeResult::Error(error)) => {
                Err(error)
            }
            _ => Err(protocol_error("driver describe response mismatch")),
        }
    }

    async fn bind(&self, request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
        if request.service_instance_id != self.instance_id {
            return Err(DriverError::Rejected {
                reason: "binding targets another service instance".to_string(),
            });
        }
        let mut source_request = request;
        source_request.service_instance_id = self.source_instance_id.clone();
        let response = self
            .request(RuntimeWireRequest::DriverBind(source_request))
            .await?;
        match response {
            RuntimeWireResponse::DriverBind(RuntimeWireDriverBindResult::Ok(value)) => Ok(*value),
            RuntimeWireResponse::DriverBind(RuntimeWireDriverBindResult::Error(error)) => {
                Err(error)
            }
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
        let mut source_command = command;
        source_command.generation = self.source_generation;
        let response = self
            .request(RuntimeWireRequest::DriverDispatch(source_command))
            .await;
        match response {
            Ok(RuntimeWireResponse::DriverDispatch(RuntimeWireDriverDispatchResult::Ok(value))) => {
                Ok(*value)
            }
            Ok(RuntimeWireResponse::DriverDispatch(RuntimeWireDriverDispatchResult::Error(
                error,
            ))) => Err(error),
            Ok(_) => Err(protocol_error("driver dispatch response mismatch")),
            Err(error) => Err(error),
        }
    }

    async fn inspect(&self, query: DriverInspectionQuery) -> Result<DriverInspection, DriverError> {
        let response = self
            .request(RuntimeWireRequest::DriverInspect(query))
            .await?;
        match response {
            RuntimeWireResponse::DriverInspect(RuntimeWireDriverInspectResult::Ok(value)) => {
                Ok(*value)
            }
            RuntimeWireResponse::DriverInspect(RuntimeWireDriverInspectResult::Error(error)) => {
                Err(error)
            }
            _ => Err(protocol_error("driver inspect response mismatch")),
        }
    }
}

enum RemoteInboundSerial {
    OrderedFrame(RuntimeWireEnvelope),
    Disconnected(String, tokio::sync::oneshot::Sender<()>),
    Reconnected,
}

impl RemoteRuntimeDriver {
    fn start_receive_pump(self: Arc<Self>) {
        let (serial_tx, mut serial_rx) = tokio::sync::mpsc::unbounded_channel();
        let serial_driver = self.clone();
        tokio::spawn(async move {
            while let Some(inbound) = serial_rx.recv().await {
                match inbound {
                    RemoteInboundSerial::OrderedFrame(envelope) => {
                        serial_driver.handle_inbound(envelope).await;
                    }
                    RemoteInboundSerial::Disconnected(reason, acknowledged) => {
                        serial_driver.handle_disconnect(reason).await;
                        let _ = acknowledged.send(());
                    }
                    RemoteInboundSerial::Reconnected => {
                        serial_driver
                            .connection_lost
                            .store(false, Ordering::Release);
                    }
                }
            }
        });
        tokio::spawn(async move {
            loop {
                match self.placement.receive().await {
                    Ok(RuntimeWirePlacementEvent::Frame(envelope)) => {
                        if matches!(
                            &envelope.frame,
                            RuntimeWireFrame::Notification(notification)
                                if matches!(notification.as_ref(), RuntimeWireNotification::DriverEvent(_))
                        ) || matches!(&envelope.frame, RuntimeWireFrame::Response { .. })
                        {
                            if serial_tx
                                .send(RemoteInboundSerial::OrderedFrame(*envelope))
                                .is_err()
                            {
                                break;
                            }
                        } else if matches!(&envelope.frame, RuntimeWireFrame::Request(_)) {
                            let driver = self.clone();
                            tokio::spawn(async move { driver.handle_inbound(*envelope).await });
                        } else {
                            self.handle_inbound(*envelope).await;
                        }
                    }
                    Ok(RuntimeWirePlacementEvent::Disconnected { reason }) => {
                        let (acknowledged_tx, acknowledged_rx) = tokio::sync::oneshot::channel();
                        if serial_tx
                            .send(RemoteInboundSerial::Disconnected(reason, acknowledged_tx))
                            .is_err()
                        {
                            break;
                        }
                        let _ = acknowledged_rx.await;
                        self.placement.acknowledge_disconnect().await;
                    }
                    Ok(RuntimeWirePlacementEvent::Reconnected) => {
                        if serial_tx.send(RemoteInboundSerial::Reconnected).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let (acknowledged_tx, _acknowledged_rx) = tokio::sync::oneshot::channel();
                        let _ = serial_tx.send(RemoteInboundSerial::Disconnected(
                            error.to_string(),
                            acknowledged_tx,
                        ));
                        break;
                    }
                }
            }
        });
    }

    async fn handle_inbound(&self, envelope: RuntimeWireEnvelope) {
        let inbound_frame_id = envelope.frame_id;
        match envelope.frame {
            RuntimeWireFrame::Response {
                request_frame_id,
                response,
            } => {
                if let Some(pending) = self.pending.lock().await.remove(&request_frame_id.0) {
                    let _ = pending.send(response);
                }
            }
            RuntimeWireFrame::Notification(notification) => {
                let RuntimeWireNotification::DriverEvent(event) = *notification else {
                    return;
                };
                if event.generation != self.source_generation {
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
                    let mut canonical_event = event;
                    canonical_event.generation = self.generation;
                    let _ = sink.emit(canonical_event).await;
                }
            }
            RuntimeWireFrame::Ack(_) => {}
            RuntimeWireFrame::Request(request) => {
                let response = self.handle_host_port_request(*request).await;
                let response = match response {
                    Ok(response) => response,
                    Err(error) => {
                        self.handle_disconnect(error.to_string()).await;
                        return;
                    }
                };
                let _ = self
                    .placement
                    .send(RuntimeWireEnvelope {
                        protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                        frame_id: RuntimeWireFrameId(
                            self.next_frame_id.fetch_add(1, Ordering::Relaxed),
                        ),
                        critical: true,
                        frame: RuntimeWireFrame::Response {
                            request_frame_id: inbound_frame_id,
                            response,
                        },
                    })
                    .await;
            }
        }
    }

    async fn handle_host_port_request(
        &self,
        request: RuntimeWireRequest,
    ) -> Result<RuntimeWireResponse, RemoteRuntimeTransportError> {
        let RuntimeWireRequest::HostPort(request) = request else {
            return Err(RemoteRuntimeTransportError::Protocol {
                reason: "Local Runtime Wire may call only typed Host ports".to_string(),
                critical: true,
            });
        };
        let response = match *request {
            RuntimeWireHostPortRequest::SurfaceMaterialize(request) => {
                RuntimeWireHostPortResponse::SurfaceMaterialize(
                    self.host
                        .surfaces
                        .materialize(request)
                        .await
                        .map(Box::new)
                        .map_err(host_port_error),
                )
            }
            RuntimeWireHostPortRequest::ToolSetMaterialize {
                binding_id,
                revision,
                digest,
            } => RuntimeWireHostPortResponse::ToolSetMaterialize(
                self.host
                    .surfaces
                    .materialize_tool_set(binding_id, revision, &digest)
                    .await
                    .map(Box::new)
                    .map_err(host_port_error),
            ),
            RuntimeWireHostPortRequest::ContextCheckpoint(mut request) => {
                if request.generation != self.source_generation {
                    return Ok(RuntimeWireResponse::HostPort(
                        RuntimeWireHostPortResponse::ContextCheckpoint(Err(
                            stale_source_generation_error(),
                        )),
                    ));
                }
                request.generation = self.generation;
                RuntimeWireHostPortResponse::ContextCheckpoint(
                    self.host
                        .context
                        .load_checkpoint(request)
                        .await
                        .map(Box::new)
                        .map_err(host_port_error),
                )
            }
            RuntimeWireHostPortRequest::CompactionActivation(mut request) => {
                if request.generation != self.source_generation {
                    return Ok(RuntimeWireResponse::HostPort(
                        RuntimeWireHostPortResponse::CompactionActivation(Err(
                            stale_source_generation_error(),
                        )),
                    ));
                }
                request.generation = self.generation;
                RuntimeWireHostPortResponse::CompactionActivation(
                    self.host
                        .context
                        .compaction_activation(request)
                        .await
                        .map(Box::new)
                        .map_err(host_port_error),
                )
            }
            RuntimeWireHostPortRequest::ToolInvoke(mut request) => {
                if request.generation != self.source_generation {
                    return Ok(RuntimeWireResponse::HostPort(
                        RuntimeWireHostPortResponse::ToolInvoke(Err(
                            stale_source_generation_error(),
                        )),
                    ));
                }
                request.generation = self.generation;
                RuntimeWireHostPortResponse::ToolInvoke(
                    self.host
                        .tools
                        .invoke(request)
                        .await
                        .map(Box::new)
                        .map_err(host_port_error),
                )
            }
            RuntimeWireHostPortRequest::HookExecute(mut request) => {
                if request.generation != self.source_generation {
                    return Ok(RuntimeWireResponse::HostPort(
                        RuntimeWireHostPortResponse::HookExecute(Err(
                            stale_source_generation_error(),
                        )),
                    ));
                }
                request.generation = self.generation;
                RuntimeWireHostPortResponse::HookExecute(
                    self.host
                        .hooks
                        .execute(request)
                        .await
                        .map(Box::new)
                        .map_err(host_port_error),
                )
            }
        };
        Ok(RuntimeWireResponse::HostPort(response))
    }

    async fn handle_disconnect(&self, reason: String) {
        if self.connection_lost.swap(true, Ordering::AcqRel) {
            return;
        }
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
                    source_request_id: None,
                    source_entry_index: None,
                    facts: vec![RuntimeJournalFact::Internal(RuntimeEvent::BindingLost {
                        binding_id: binding.binding_id,
                        reason: reason.clone(),
                    })],
                })
                .await;
        }
        self.pending.lock().await.clear();
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
                frame: RuntimeWireFrame::Request(Box::new(request)),
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

fn host_port_error(error: impl ToString) -> RuntimeWireHostPortError {
    RuntimeWireHostPortError {
        reason: error.to_string(),
        retryable: true,
        stale: false,
    }
}

fn stale_source_generation_error() -> RuntimeWireHostPortError {
    RuntimeWireHostPortError {
        reason: "Runtime Wire HostPort request carries a stale source generation".to_string(),
        retryable: false,
        stale: true,
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
    host_ports: Option<Arc<RuntimeWireHostPortRouter>>,
    next_frame_id: Arc<AtomicU64>,
    pending: Arc<
        tokio::sync::Mutex<HashMap<u64, tokio::sync::oneshot::Sender<RuntimeWireHostPortResponse>>>,
    >,
    outbound_tx: tokio::sync::mpsc::UnboundedSender<RuntimeWireEnvelope>,
    outbound_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<RuntimeWireEnvelope>>,
}

const MAX_PENDING_HOST_PORT_REQUESTS: usize = 1_024;

impl RuntimeWireDriverEndpoint {
    pub fn new(driver: Arc<dyn AgentRuntimeDriver>) -> Self {
        Self::with_host_port_router(driver, None)
    }

    pub fn new_with_host_port_router(
        driver: Arc<dyn AgentRuntimeDriver>,
        router: Arc<RuntimeWireHostPortRouter>,
    ) -> Self {
        Self::with_host_port_router(driver, Some(router))
    }

    fn with_host_port_router(
        driver: Arc<dyn AgentRuntimeDriver>,
        host_ports: Option<Arc<RuntimeWireHostPortRouter>>,
    ) -> Self {
        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            driver,
            host_ports,
            next_frame_id: Arc::new(AtomicU64::new(1)),
            pending: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
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
        let method = match request.frame {
            RuntimeWireFrame::Response {
                request_frame_id,
                response: RuntimeWireResponse::HostPort(response),
            } => {
                if let Some(pending) = self.pending.lock().await.remove(&request_frame_id.0) {
                    let _ = pending.send(response);
                }
                return Ok(());
            }
            RuntimeWireFrame::Request(method) => method,
            _ => {
                return Err(RemoteRuntimeTransportError::Protocol {
                    reason: "local endpoint accepts Driver requests and HostPort responses only"
                        .to_string(),
                    critical: true,
                });
            }
        };
        let response = match *method {
            RuntimeWireRequest::DriverDescribe(value) => {
                RuntimeWireResponse::DriverDescribe(match self.driver.describe(value).await {
                    Ok(value) => RuntimeWireDriverDescribeResult::Ok(Box::new(value)),
                    Err(error) => RuntimeWireDriverDescribeResult::Error(error),
                })
            }
            RuntimeWireRequest::DriverBind(value) => {
                let binding_id = value.binding_id.clone();
                let client = RuntimeWireHostPortClient {
                    outbound: self.outbound_tx.clone(),
                    pending: self.pending.clone(),
                    next_frame_id: self.next_frame_id.clone(),
                };
                if let Some(router) = &self.host_ports {
                    router.bind(binding_id.clone(), client.clone()).await;
                }
                let result = self.driver.bind(value).await;
                if result.is_err()
                    && let Some(router) = &self.host_ports
                {
                    router.unbind(&binding_id, &client).await;
                }
                RuntimeWireResponse::DriverBind(match result {
                    Ok(value) => RuntimeWireDriverBindResult::Ok(Box::new(value)),
                    Err(error) => RuntimeWireDriverBindResult::Error(error),
                })
            }
            RuntimeWireRequest::DriverDispatch(value) => {
                let sink = Arc::new(ForwardingSink {
                    outbound: self.outbound_tx.clone(),
                    next_frame_id: self.next_frame_id.clone(),
                });
                let result = self.driver.dispatch(value, sink).await;
                RuntimeWireResponse::DriverDispatch(match result {
                    Ok(value) => RuntimeWireDriverDispatchResult::Ok(Box::new(value)),
                    Err(error) => RuntimeWireDriverDispatchResult::Error(error),
                })
            }
            RuntimeWireRequest::DriverInspect(value) => {
                RuntimeWireResponse::DriverInspect(match self.driver.inspect(value).await {
                    Ok(value) => RuntimeWireDriverInspectResult::Ok(Box::new(value)),
                    Err(error) => RuntimeWireDriverInspectResult::Error(error),
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

    async fn receive(&self) -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError> {
        self.outbound_rx
            .lock()
            .await
            .recv()
            .await
            .map(|envelope| RuntimeWirePlacementEvent::Frame(Box::new(envelope)))
            .ok_or_else(|| RemoteRuntimeTransportError::Unavailable {
                reason: "local Runtime Wire endpoint closed".to_string(),
                retryable: true,
            })
    }
}

#[derive(Default)]
pub struct RuntimeWireHostPortRouter {
    bindings: tokio::sync::RwLock<HashMap<RuntimeBindingId, RuntimeWireHostPortClient>>,
}

impl RuntimeWireHostPortRouter {
    pub fn host_ports(
        self: &Arc<Self>,
        credentials: Arc<dyn AgentRuntimeCredentialBroker>,
    ) -> RuntimeDriverHostPorts {
        RuntimeDriverHostPorts {
            credentials,
            surfaces: Arc::new(RuntimeWireSurfaceBroker(self.clone())),
            context: Arc::new(RuntimeWireContextBroker(self.clone())),
            tools: Arc::new(RuntimeWireToolCallback(self.clone())),
            hooks: Arc::new(RuntimeWireHookCallback(self.clone())),
        }
    }

    async fn bind(&self, binding_id: RuntimeBindingId, client: RuntimeWireHostPortClient) {
        self.bindings.write().await.insert(binding_id, client);
    }

    async fn unbind(&self, binding_id: &RuntimeBindingId, client: &RuntimeWireHostPortClient) {
        let mut bindings = self.bindings.write().await;
        if bindings
            .get(binding_id)
            .is_some_and(|active| active.outbound.same_channel(&client.outbound))
        {
            bindings.remove(binding_id);
        }
    }

    async fn client(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<RuntimeWireHostPortClient, RuntimeWireHostPortError> {
        self.bindings
            .read()
            .await
            .get(binding_id)
            .cloned()
            .ok_or_else(|| RuntimeWireHostPortError {
                reason: format!("Runtime Wire binding {binding_id} has no active stream"),
                retryable: true,
                stale: true,
            })
    }
}

#[derive(Clone)]
struct RuntimeWireHostPortClient {
    outbound: tokio::sync::mpsc::UnboundedSender<RuntimeWireEnvelope>,
    pending: Arc<
        tokio::sync::Mutex<HashMap<u64, tokio::sync::oneshot::Sender<RuntimeWireHostPortResponse>>>,
    >,
    next_frame_id: Arc<AtomicU64>,
}

impl RuntimeWireHostPortClient {
    async fn request(
        &self,
        request: RuntimeWireHostPortRequest,
    ) -> Result<RuntimeWireHostPortResponse, RuntimeWireHostPortError> {
        let frame_id = RuntimeWireFrameId(self.next_frame_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            if pending.len() >= MAX_PENDING_HOST_PORT_REQUESTS {
                return Err(RuntimeWireHostPortError {
                    reason: format!(
                        "Runtime Wire HostPort reached its {MAX_PENDING_HOST_PORT_REQUESTS} in-flight request limit"
                    ),
                    retryable: true,
                    stale: false,
                });
            }
            pending.insert(frame_id.0, tx);
        }
        if self
            .outbound
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id,
                critical: true,
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::HostPort(Box::new(
                    request,
                )))),
            })
            .is_err()
        {
            self.pending.lock().await.remove(&frame_id.0);
            return Err(RuntimeWireHostPortError {
                reason: "Runtime Wire HostPort stream is closed".to_string(),
                retryable: true,
                stale: false,
            });
        }
        rx.await.map_err(|_| RuntimeWireHostPortError {
            reason: "Runtime Wire HostPort response correlation was lost".to_string(),
            retryable: true,
            stale: false,
        })
    }
}

struct RuntimeWireSurfaceBroker(Arc<RuntimeWireHostPortRouter>);

#[async_trait]
impl AgentRuntimeSurfaceBroker for RuntimeWireSurfaceBroker {
    async fn materialize(
        &self,
        request: DriverSurfaceRequest,
    ) -> Result<MaterializedDriverSurface, DriverSurfaceError> {
        match self
            .0
            .client(&request.binding_id)
            .await
            .map_err(surface_wire_error)?
            .request(RuntimeWireHostPortRequest::SurfaceMaterialize(request))
            .await
            .map_err(surface_wire_error)?
        {
            RuntimeWireHostPortResponse::SurfaceMaterialize(Ok(value)) => Ok(*value),
            RuntimeWireHostPortResponse::SurfaceMaterialize(Err(error)) => {
                Err(surface_wire_error(error))
            }
            _ => Err(DriverSurfaceError::InvalidMaterialization {
                reason: "Runtime Wire surface response mismatch".to_string(),
            }),
        }
    }

    async fn materialize_tool_set(
        &self,
        binding_id: RuntimeBindingId,
        revision: ToolSetRevision,
        digest: &str,
    ) -> Result<DriverToolSurface, DriverSurfaceError> {
        match self
            .0
            .client(&binding_id)
            .await
            .map_err(surface_wire_error)?
            .request(RuntimeWireHostPortRequest::ToolSetMaterialize {
                binding_id,
                revision,
                digest: digest.to_string(),
            })
            .await
            .map_err(surface_wire_error)?
        {
            RuntimeWireHostPortResponse::ToolSetMaterialize(Ok(value)) => Ok(*value),
            RuntimeWireHostPortResponse::ToolSetMaterialize(Err(error)) => {
                Err(surface_wire_error(error))
            }
            _ => Err(DriverSurfaceError::InvalidMaterialization {
                reason: "Runtime Wire tool-set response mismatch".to_string(),
            }),
        }
    }
}

struct RuntimeWireContextBroker(Arc<RuntimeWireHostPortRouter>);

#[async_trait]
impl AgentRuntimeContextBroker for RuntimeWireContextBroker {
    async fn load_checkpoint(
        &self,
        request: DriverContextCheckpointRequest,
    ) -> Result<DriverContextActivation, DriverContextError> {
        let binding_id = request.binding_id.clone();
        match self
            .0
            .client(&binding_id)
            .await
            .map_err(context_wire_error)?
            .request(RuntimeWireHostPortRequest::ContextCheckpoint(request))
            .await
            .map_err(context_wire_error)?
        {
            RuntimeWireHostPortResponse::ContextCheckpoint(Ok(value)) => Ok(*value),
            RuntimeWireHostPortResponse::ContextCheckpoint(Err(error)) => {
                Err(context_wire_error(error))
            }
            _ => Err(DriverContextError::InvalidMaterialization {
                reason: "Runtime Wire context response mismatch".to_string(),
            }),
        }
    }

    async fn compaction_activation(
        &self,
        request: DriverCompactionActivationRequest,
    ) -> Result<DriverContextActivation, DriverContextError> {
        let binding_id = request.binding_id.clone();
        match self
            .0
            .client(&binding_id)
            .await
            .map_err(context_wire_error)?
            .request(RuntimeWireHostPortRequest::CompactionActivation(request))
            .await
            .map_err(context_wire_error)?
        {
            RuntimeWireHostPortResponse::CompactionActivation(Ok(value)) => Ok(*value),
            RuntimeWireHostPortResponse::CompactionActivation(Err(error)) => {
                Err(context_wire_error(error))
            }
            _ => Err(DriverContextError::InvalidMaterialization {
                reason: "Runtime Wire compaction response mismatch".to_string(),
            }),
        }
    }
}

struct RuntimeWireToolCallback(Arc<RuntimeWireHostPortRouter>);

#[async_trait]
impl AgentRuntimeToolCallback for RuntimeWireToolCallback {
    async fn invoke(
        &self,
        request: DriverToolInvocation,
    ) -> Result<DriverToolOutcome, DriverToolCallbackError> {
        let binding_id = request.binding_id.clone();
        match self
            .0
            .client(&binding_id)
            .await
            .map_err(tool_wire_error)?
            .request(RuntimeWireHostPortRequest::ToolInvoke(request))
            .await
            .map_err(tool_wire_error)?
        {
            RuntimeWireHostPortResponse::ToolInvoke(Ok(value)) => Ok(*value),
            RuntimeWireHostPortResponse::ToolInvoke(Err(error)) => Err(tool_wire_error(error)),
            _ => Err(DriverToolCallbackError::ProtocolViolation {
                reason: "Runtime Wire tool response mismatch".to_string(),
            }),
        }
    }
}

struct RuntimeWireHookCallback(Arc<RuntimeWireHostPortRouter>);

#[async_trait]
impl AgentRuntimeHookCallback for RuntimeWireHookCallback {
    async fn execute(
        &self,
        request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError> {
        let binding_id = request.binding_id.clone();
        match self
            .0
            .client(&binding_id)
            .await
            .map_err(hook_wire_error)?
            .request(RuntimeWireHostPortRequest::HookExecute(request))
            .await
            .map_err(hook_wire_error)?
        {
            RuntimeWireHostPortResponse::HookExecute(Ok(value)) => Ok(*value),
            RuntimeWireHostPortResponse::HookExecute(Err(error)) => Err(hook_wire_error(error)),
            _ => Err(DriverHookCallbackError::ProtocolViolation {
                reason: "Runtime Wire hook response mismatch".to_string(),
            }),
        }
    }
}

fn surface_wire_error(error: RuntimeWireHostPortError) -> DriverSurfaceError {
    if error.stale {
        DriverSurfaceError::Stale
    } else {
        DriverSurfaceError::Unavailable {
            reason: error.reason,
            retryable: error.retryable,
        }
    }
}

fn context_wire_error(error: RuntimeWireHostPortError) -> DriverContextError {
    if error.stale {
        DriverContextError::Stale
    } else {
        DriverContextError::Unavailable {
            reason: error.reason,
            retryable: error.retryable,
        }
    }
}

fn tool_wire_error(error: RuntimeWireHostPortError) -> DriverToolCallbackError {
    if error.stale {
        DriverToolCallbackError::Stale
    } else {
        DriverToolCallbackError::Unavailable {
            reason: error.reason,
            retryable: error.retryable,
        }
    }
}

fn hook_wire_error(error: RuntimeWireHostPortError) -> DriverHookCallbackError {
    if error.stale {
        DriverHookCallbackError::Stale
    } else {
        DriverHookCallbackError::Unavailable {
            reason: error.reason,
            retryable: error.retryable,
        }
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
                frame: RuntimeWireFrame::Notification(Box::new(
                    RuntimeWireNotification::DriverEvent(event),
                )),
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

    use agentdash_agent_runtime_test_support::session_parity::{
        NormalizedPresentationEvent, PresentationDurability as StrictDurability,
        compare_ordered_presentation_events,
    };
    use agentdash_relay::{
        RelayMessage, RuntimeRelayFrame, RuntimeRelayProvenance, RuntimeRelayStreamId,
    };

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
    struct SuccessfulBindDriver;

    struct AsyncEventDriver;

    struct BlockingSink {
        entered: tokio::sync::Semaphore,
        release: tokio::sync::Semaphore,
    }

    #[async_trait]
    impl DriverEventSink for BlockingSink {
        async fn emit(&self, _event: DriverEventEnvelope) -> Result<(), DriverError> {
            self.entered.add_permits(1);
            self.release
                .acquire()
                .await
                .expect("release remains open")
                .forget();
            Ok(())
        }
    }

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

    struct TestCredentialBroker;

    #[async_trait]
    impl AgentRuntimeCredentialBroker for TestCredentialBroker {
        async fn resolve(
            &self,
            slot: &AgentRuntimeCredentialSlot,
            _reference: &AgentRuntimeCredentialRef,
            _purpose: &str,
        ) -> Result<CredentialLease, CredentialResolveError> {
            Err(CredentialResolveError::Unavailable {
                slot: slot.clone(),
                reason: "test credential unavailable".to_string(),
            })
        }
    }

    fn test_host_ports() -> RuntimeDriverHostPorts {
        Arc::new(RuntimeWireHostPortRouter::default()).host_ports(Arc::new(TestCredentialBroker))
    }

    struct EpochPlacement {
        sent: tokio::sync::mpsc::UnboundedSender<RuntimeWireEnvelope>,
        events: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<RuntimeWirePlacementEvent>>,
    }

    #[async_trait]
    impl RuntimeWirePlacement for EpochPlacement {
        async fn send(
            &self,
            envelope: RuntimeWireEnvelope,
        ) -> Result<(), RemoteRuntimeTransportError> {
            self.sent
                .send(envelope)
                .map_err(|_| RemoteRuntimeTransportError::Unavailable {
                    reason: "test outbound closed".to_string(),
                    retryable: true,
                })
        }

        async fn receive(&self) -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError> {
            self.events.lock().await.recv().await.ok_or_else(|| {
                RemoteRuntimeTransportError::Unavailable {
                    reason: "test event stream closed".to_string(),
                    retryable: false,
                }
            })
        }
    }

    #[async_trait]
    impl RuntimeWirePlacement for ClosedPlacement {
        async fn send(&self, _: RuntimeWireEnvelope) -> Result<(), RemoteRuntimeTransportError> {
            Ok(())
        }

        async fn receive(&self) -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError> {
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
    impl AgentRuntimeDriver for SuccessfulBindDriver {
        async fn describe(
            &self,
            request: DriverDescribeRequest,
        ) -> Result<RuntimeDescriptor, DriverError> {
            FakeDriver.describe(request).await
        }

        async fn bind(&self, request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
            Ok(DriverBinding {
                driver_binding_id: id("driver-binding-host-port"),
                source_thread_id: id("source-thread-host-port"),
                applied_surface_revision: request.surface_revision,
                applied_surface_digest: request.surface_digest,
                applied_tool_set_revision: ToolSetRevision(1),
                applied_tool_set_digest: "tool-set-host-port".to_string(),
                applied_hook_plan_revision: None,
                applied_hook_plan_digest: None,
                applied_hooks: Vec::new(),
            })
        }

        async fn dispatch(
            &self,
            request: DriverCommandEnvelope,
            sink: Arc<dyn DriverEventSink>,
        ) -> Result<DriverDispatchReceipt, DriverError> {
            FakeDriver.dispatch(request, sink).await
        }

        async fn inspect(
            &self,
            query: DriverInspectionQuery,
        ) -> Result<DriverInspection, DriverError> {
            FakeDriver.inspect(query).await
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
                    source_request_id: None,
                    source_entry_index: None,
                    facts: vec![RuntimeJournalFact::Internal(
                        RuntimeEvent::BindingEstablished {
                            binding_id: command.binding_id,
                        },
                    )],
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
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::DriverDescribe(
                    DriverDescribeRequest {
                        service_instance_id: id("service-loopback"),
                    },
                ))),
            })
            .await
            .expect("send loopback describe");
        let RuntimeWirePlacementEvent::Frame(frame) =
            endpoint.receive().await.expect("loopback describe")
        else {
            panic!("expected loopback frame")
        };
        let RuntimeWireFrame::Response {
            request_frame_id,
            response,
        } = &frame.frame
        else {
            panic!("expected response")
        };
        assert_eq!(*request_frame_id, RuntimeWireFrameId(41));
        let RuntimeWireResponse::DriverDescribe(RuntimeWireDriverDescribeResult::Ok(descriptor)) =
            response
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
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::Snapshot(
                    RuntimeSnapshotQuery::Thread {
                        thread_id: id("thread"),
                        at_revision: None,
                    },
                ))),
            })
            .await
            .expect_err("local placement must not host a second Managed Runtime");
        assert!(matches!(
            error,
            RemoteRuntimeTransportError::Protocol { critical: true, .. }
        ));
    }

    #[tokio::test]
    async fn local_host_port_proxy_round_trips_tool_callback_with_frame_correlation() {
        let router = Arc::new(RuntimeWireHostPortRouter::default());
        let host_ports = router.host_ports(Arc::new(TestCredentialBroker));
        let endpoint = RuntimeWireDriverEndpoint::new_with_host_port_router(
            Arc::new(SuccessfulBindDriver),
            router,
        );
        endpoint
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: RuntimeWireFrameId(10),
                critical: true,
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::DriverBind(
                    DriverBindRequest {
                        binding_id: id("binding-host-port"),
                        service_instance_id: id("service-host-port"),
                        surface_revision: SurfaceRevision(1),
                        surface_digest: id("surface-host-port"),
                        intent: DriverBindIntent::Start,
                    },
                ))),
            })
            .await
            .expect("bind request registers reverse route");
        let _ = endpoint.receive().await.expect("bind response");

        let invoke = tokio::spawn(async move {
            host_ports
                .tools
                .invoke(DriverToolInvocation {
                    thread_id: id("thread-host-port"),
                    turn_id: id("turn-host-port"),
                    item_id: id("item-host-port"),
                    binding_id: id("binding-host-port"),
                    generation: RuntimeDriverGeneration(2),
                    source_thread_id: id("source-thread-host-port"),
                    source_turn_id: id("source-turn-host-port"),
                    source_item_id: id("source-item-host-port"),
                    tool_set_revision: ToolSetRevision(1),
                    tool_name: "read".to_string(),
                    arguments: serde_json::json!({"path":"README.md"}),
                    timeout_ms: 1_000,
                    authorization_identity: None,
                })
                .await
        });
        let RuntimeWirePlacementEvent::Frame(request) =
            endpoint.receive().await.expect("reverse HostPort request")
        else {
            panic!("expected HostPort request frame")
        };
        let request_frame_id = request.frame_id;
        assert!(matches!(
            request.frame,
            RuntimeWireFrame::Request(request)
                if matches!(request.as_ref(), RuntimeWireRequest::HostPort(host_port)
                    if matches!(host_port.as_ref(), RuntimeWireHostPortRequest::ToolInvoke(_)))
        ));
        endpoint
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: RuntimeWireFrameId(99),
                critical: true,
                frame: RuntimeWireFrame::Response {
                    request_frame_id,
                    response: RuntimeWireResponse::HostPort(
                        RuntimeWireHostPortResponse::ToolInvoke(Ok(Box::new(
                            DriverToolOutcome::Completed {
                                output: serde_json::json!({"content":"ok"}),
                                is_error: false,
                            },
                        ))),
                    ),
                },
            })
            .await
            .expect("correlated HostPort response");
        assert!(matches!(
            invoke.await.expect("join").expect("tool outcome"),
            DriverToolOutcome::Completed {
                is_error: false,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn failed_driver_bind_removes_reverse_host_port_route() {
        let router = Arc::new(RuntimeWireHostPortRouter::default());
        let host_ports = router.host_ports(Arc::new(TestCredentialBroker));
        let endpoint =
            RuntimeWireDriverEndpoint::new_with_host_port_router(Arc::new(FakeDriver), router);
        endpoint
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: RuntimeWireFrameId(101),
                critical: true,
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::DriverBind(
                    DriverBindRequest {
                        binding_id: id("binding-rejected"),
                        service_instance_id: id("service-rejected"),
                        surface_revision: SurfaceRevision(1),
                        surface_digest: id("surface-rejected"),
                        intent: DriverBindIntent::Start,
                    },
                ))),
            })
            .await
            .expect("bind error is returned as a correlated driver response");
        let _ = endpoint.receive().await.expect("bind response");

        let error = host_ports
            .surfaces
            .materialize(DriverSurfaceRequest {
                binding_id: id("binding-rejected"),
                surface_revision: SurfaceRevision(1),
                surface_digest: id("surface-rejected"),
            })
            .await
            .expect_err("failed bind must not leave a reverse HostPort route");
        assert!(matches!(error, DriverSurfaceError::Stale));
    }

    #[tokio::test]
    async fn remote_driver_reports_lost_when_transport_closes_before_correlation() {
        let driver = Arc::new(RemoteRuntimeDriver {
            instance_id: id("service-remote"),
            generation: RuntimeDriverGeneration(3),
            source_instance_id: id("source-service-remote"),
            source_generation: RuntimeDriverGeneration(8),
            placement: Arc::new(ClosedPlacement),
            host: test_host_ports(),
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
            source_instance_id: id("source-service-remote"),
            source_generation: RuntimeDriverGeneration(8),
            placement,
            host: test_host_ports(),
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
                    presentation_thread_id: id("presentation-thread-1"),
                    binding_id: id("binding-1"),
                    generation: RuntimeDriverGeneration(3),
                    source_thread_id: id("source-thread-1"),
                    runtime_turn_id: None,
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
        assert_eq!(
            sink.events.lock().await[0].generation,
            RuntimeDriverGeneration(3),
            "source placement generation must be normalized to the canonical cloud generation"
        );
    }

    #[tokio::test]
    async fn dispatch_response_waits_for_preceding_driver_event_projection() {
        let (sent_tx, mut sent_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let driver = Arc::new(RemoteRuntimeDriver {
            instance_id: id("service-ordered"),
            generation: RuntimeDriverGeneration(3),
            source_instance_id: id("source-service-ordered"),
            source_generation: RuntimeDriverGeneration(8),
            placement: Arc::new(EpochPlacement {
                sent: sent_tx,
                events: tokio::sync::Mutex::new(event_rx),
            }),
            host: test_host_ports(),
            next_frame_id: AtomicU64::new(1),
            pending: tokio::sync::Mutex::new(HashMap::new()),
            active_bindings: tokio::sync::Mutex::new(HashMap::new()),
            connection_lost: AtomicBool::new(false),
        });
        driver.clone().start_receive_pump();
        let sink = Arc::new(BlockingSink {
            entered: tokio::sync::Semaphore::new(0),
            release: tokio::sync::Semaphore::new(0),
        });
        let mut dispatch = {
            let driver = driver.clone();
            let sink = sink.clone();
            tokio::spawn(async move {
                driver
                    .dispatch(
                        DriverCommandEnvelope {
                            request_id: id("request-ordered"),
                            presentation_thread_id: id("presentation-thread-ordered"),
                            binding_id: id("binding-ordered"),
                            generation: RuntimeDriverGeneration(3),
                            source_thread_id: id("source-thread-ordered"),
                            runtime_turn_id: None,
                            command: RuntimeCommand::ThreadResume {
                                thread_id: id("thread-ordered"),
                            },
                        },
                        sink,
                    )
                    .await
            })
        };
        let request = sent_rx.recv().await.expect("dispatch request");
        event_tx
            .send(RuntimeWirePlacementEvent::Frame(Box::new(
                RuntimeWireEnvelope {
                    protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                    frame_id: RuntimeWireFrameId(40),
                    critical: true,
                    frame: RuntimeWireFrame::Notification(Box::new(
                        RuntimeWireNotification::DriverEvent(DriverEventEnvelope {
                            binding_id: id("binding-ordered"),
                            generation: RuntimeDriverGeneration(8),
                            source_thread_id: id("source-thread-ordered"),
                            source_turn_id: None,
                            source_item_id: None,
                            source_request_id: None,
                            source_entry_index: None,
                            facts: vec![RuntimeJournalFact::Internal(
                                RuntimeEvent::BindingEstablished {
                                    binding_id: id("binding-ordered"),
                                },
                            )],
                        }),
                    )),
                },
            )))
            .expect("preceding event");
        sink.entered.acquire().await.expect("sink entered").forget();
        event_tx
            .send(RuntimeWirePlacementEvent::Frame(Box::new(
                RuntimeWireEnvelope {
                    protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                    frame_id: RuntimeWireFrameId(41),
                    critical: true,
                    frame: RuntimeWireFrame::Response {
                        request_frame_id: request.frame_id,
                        response: RuntimeWireResponse::DriverDispatch(
                            RuntimeWireDriverDispatchResult::Ok(Box::new(DriverDispatchReceipt {
                                request_id: id("request-ordered"),
                                duplicate: false,
                                applied_tool_set: None,
                            })),
                        ),
                    },
                },
            )))
            .expect("dispatch response");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), &mut dispatch)
                .await
                .is_err()
        );
        sink.release.add_permits(1);
        dispatch
            .await
            .expect("dispatch task")
            .expect("dispatch receipt");
    }

    #[tokio::test]
    async fn remote_descriptor_uses_source_coordinates_on_wire_and_canonical_coordinates_at_host() {
        let (sent_tx, mut sent_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let driver = Arc::new(RemoteRuntimeDriver {
            instance_id: id("cloud-proxy-service"),
            generation: RuntimeDriverGeneration(5),
            source_instance_id: id("local-source-service"),
            source_generation: RuntimeDriverGeneration(11),
            placement: Arc::new(EpochPlacement {
                sent: sent_tx,
                events: tokio::sync::Mutex::new(event_rx),
            }),
            host: test_host_ports(),
            next_frame_id: AtomicU64::new(1),
            pending: tokio::sync::Mutex::new(HashMap::new()),
            active_bindings: tokio::sync::Mutex::new(HashMap::new()),
            connection_lost: AtomicBool::new(false),
        });
        driver.clone().start_receive_pump();
        let describe = {
            let driver = driver.clone();
            tokio::spawn(async move {
                driver
                    .describe(DriverDescribeRequest {
                        service_instance_id: id("cloud-proxy-service"),
                    })
                    .await
            })
        };
        let request = sent_rx.recv().await.expect("wire describe request");
        assert!(matches!(
            &request.frame,
            RuntimeWireFrame::Request(request)
                if matches!(request.as_ref(), RuntimeWireRequest::DriverDescribe(
                    DriverDescribeRequest { service_instance_id }
                ) if service_instance_id == &id("local-source-service"))
        ));
        event_tx
            .send(RuntimeWirePlacementEvent::Frame(Box::new(
                RuntimeWireEnvelope {
                    protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                    frame_id: RuntimeWireFrameId(201),
                    critical: true,
                    frame: RuntimeWireFrame::Response {
                        request_frame_id: request.frame_id,
                        response: RuntimeWireResponse::DriverDescribe(
                            RuntimeWireDriverDescribeResult::Ok(Box::new(RuntimeDescriptor {
                                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                                service_instance_id: id("local-source-service"),
                                profile: profile(),
                                profile_digest: id("source-profile"),
                            })),
                        ),
                    },
                },
            )))
            .expect("wire describe response");
        let descriptor = describe
            .await
            .expect("describe task")
            .expect("canonical descriptor");
        assert_eq!(descriptor.service_instance_id, id("cloud-proxy-service"));
    }

    #[tokio::test]
    async fn disconnect_is_exactly_once_lost_and_reconnect_accepts_only_new_work() {
        let (sent_tx, mut sent_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let driver = Arc::new(RemoteRuntimeDriver {
            instance_id: id("service-epoch"),
            generation: RuntimeDriverGeneration(5),
            source_instance_id: id("service-epoch"),
            source_generation: RuntimeDriverGeneration(5),
            placement: Arc::new(EpochPlacement {
                sent: sent_tx,
                events: tokio::sync::Mutex::new(event_rx),
            }),
            host: test_host_ports(),
            next_frame_id: AtomicU64::new(1),
            pending: tokio::sync::Mutex::new(HashMap::new()),
            active_bindings: tokio::sync::Mutex::new(HashMap::new()),
            connection_lost: AtomicBool::new(false),
        });
        driver.clone().start_receive_pump();
        let sink = Arc::new(RecordingSink::default());
        let dispatch = {
            let driver = driver.clone();
            let sink = sink.clone();
            tokio::spawn(async move {
                driver
                    .dispatch(
                        DriverCommandEnvelope {
                            request_id: id("request-epoch-1"),
                            presentation_thread_id: id("presentation-thread-epoch-1"),
                            binding_id: id("binding-epoch-1"),
                            generation: RuntimeDriverGeneration(5),
                            source_thread_id: id("source-epoch-1"),
                            runtime_turn_id: None,
                            command: RuntimeCommand::ThreadResume {
                                thread_id: id("thread-epoch-1"),
                            },
                        },
                        sink,
                    )
                    .await
            })
        };
        let request = sent_rx.recv().await.expect("dispatch request");
        event_tx
            .send(RuntimeWirePlacementEvent::Frame(Box::new(
                RuntimeWireEnvelope {
                    protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                    frame_id: RuntimeWireFrameId(100),
                    critical: true,
                    frame: RuntimeWireFrame::Response {
                        request_frame_id: request.frame_id,
                        response: RuntimeWireResponse::DriverDispatch(
                            RuntimeWireDriverDispatchResult::Ok(Box::new(DriverDispatchReceipt {
                                request_id: id("request-epoch-1"),
                                duplicate: false,
                                applied_tool_set: None,
                            })),
                        ),
                    },
                },
            )))
            .expect("dispatch response");
        dispatch
            .await
            .expect("dispatch task")
            .expect("dispatch receipt");

        event_tx
            .send(RuntimeWirePlacementEvent::Disconnected {
                reason: "socket lost".to_string(),
            })
            .expect("first disconnect");
        event_tx
            .send(RuntimeWirePlacementEvent::Disconnected {
                reason: "duplicate socket lost".to_string(),
            })
            .expect("duplicate disconnect");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if sink.events.lock().await.len() == 1 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Lost event timeout");
        assert!(matches!(
            sink.events.lock().await[0].facts.as_slice(),
            [RuntimeJournalFact::Internal(
                RuntimeEvent::BindingLost { .. }
            )]
        ));

        event_tx
            .send(RuntimeWirePlacementEvent::Reconnected)
            .expect("reconnected");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while driver.connection_lost.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("reconnect state timeout");
        let describe = {
            let driver = driver.clone();
            tokio::spawn(async move {
                driver
                    .describe(DriverDescribeRequest {
                        service_instance_id: id("service-epoch"),
                    })
                    .await
            })
        };
        let describe_request = sent_rx.recv().await.expect("new describe request");
        event_tx
            .send(RuntimeWirePlacementEvent::Frame(Box::new(
                RuntimeWireEnvelope {
                    protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                    frame_id: RuntimeWireFrameId(101),
                    critical: true,
                    frame: RuntimeWireFrame::Response {
                        request_frame_id: describe_request.frame_id,
                        response: RuntimeWireResponse::DriverDescribe(
                            RuntimeWireDriverDescribeResult::Ok(Box::new(RuntimeDescriptor {
                                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                                service_instance_id: id("service-epoch"),
                                profile: profile(),
                                profile_digest: id("remote-profile"),
                            })),
                        ),
                    },
                },
            )))
            .expect("describe response");
        describe
            .await
            .expect("describe task")
            .expect("new request after reconnect");

        event_tx
            .send(RuntimeWirePlacementEvent::Frame(Box::new(
                RuntimeWireEnvelope {
                    protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                    frame_id: RuntimeWireFrameId(102),
                    critical: true,
                    frame: RuntimeWireFrame::Notification(Box::new(
                        RuntimeWireNotification::DriverEvent(DriverEventEnvelope {
                            binding_id: id("binding-epoch-1"),
                            generation: RuntimeDriverGeneration(5),
                            source_thread_id: id("source-epoch-1"),
                            source_turn_id: None,
                            source_item_id: None,
                            source_request_id: None,
                            source_entry_index: None,
                            facts: vec![RuntimeJournalFact::Internal(
                                RuntimeEvent::BindingEstablished {
                                    binding_id: id("binding-epoch-1"),
                                },
                            )],
                        }),
                    )),
                },
            )))
            .expect("late old-binding event");
        tokio::task::yield_now().await;
        assert_eq!(sink.events.lock().await.len(), 1);
    }

    #[test]
    fn remote_wire_and_relay_match_all_three_main_oracle_goldens_strictly() {
        fn normalize(records: &serde_json::Value) -> Vec<NormalizedPresentationEvent> {
            records
                .as_array()
                .expect("scenario records")
                .iter()
                .map(|record| NormalizedPresentationEvent {
                    durability: match record["durability"].as_str().unwrap() {
                        "durable" => StrictDurability::Durable,
                        "ephemeral" => StrictDurability::Ephemeral,
                        other => panic!("unknown durability {other}"),
                    },
                    event: record["event"].clone(),
                })
                .collect()
        }

        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/main-oracle-presentation.json"))
                .expect("parse Remote/Relay Main oracle golden");
        assert_eq!(
            golden["oracle_commit"],
            "957fa9d60ea3d67efa1bb278fe5b376cf0c34598"
        );
        assert_eq!(
            golden["source_sha256"],
            "d2e1cea154e40e8f66aa8e5ec36ef0cd57ebee78332f157a22c639a4db4bbb05"
        );
        let scenarios = golden["scenarios"].as_object().unwrap();
        let expected_source_entry_index = golden["source_entry_index"].as_u64().unwrap() as u32;
        assert_eq!(scenarios.len(), 3);
        for (scenario, expected_records) in scenarios {
            let mut current = Vec::new();
            let mut current_source_entry_indices = Vec::new();
            for (index, record) in expected_records.as_array().unwrap().iter().enumerate() {
                let durability = match record["durability"].as_str().unwrap() {
                    "durable" => PresentationDurability::Durable,
                    "ephemeral" => PresentationDurability::Ephemeral,
                    other => panic!("unknown durability {other}"),
                };
                let protected: agentdash_agent_protocol::BackboneEvent =
                    serde_json::from_value(record["event"].clone())
                        .expect("deserialize protected Main body into owned protocol");
                let envelope = RuntimeWireEnvelope {
                    protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                    frame_id: RuntimeWireFrameId(701 + index as u64),
                    critical: true,
                    frame: RuntimeWireFrame::Notification(Box::new(
                        RuntimeWireNotification::DriverEvent(DriverEventEnvelope {
                            binding_id: id("binding-presentation"),
                            generation: RuntimeDriverGeneration(9),
                            source_thread_id: id("source-presentation"),
                            source_turn_id: Some(id("source-turn-presentation")),
                            source_item_id: None,
                            source_request_id: None,
                            source_entry_index: Some(expected_source_entry_index),
                            facts: vec![RuntimeJournalFact::Presentation(
                                ImmutablePresentationEvent::new(durability, protected),
                            )],
                        }),
                    )),
                };
                let wire_bytes = serde_json::to_vec(&envelope).expect("encode Runtime Wire frame");
                let DecodedRuntimeWireFrame::Known(wire_decoded) =
                    decode_frame(&wire_bytes).expect("decode Runtime Wire frame")
                else {
                    panic!("presentation driver event must be a known frame");
                };
                let relay = RelayMessage::RuntimeWireFrame {
                    id: format!("runtime-wire-{index}"),
                    payload: Box::new(RuntimeRelayFrame {
                        stream_id: RuntimeRelayStreamId("runtime-stream".to_string()),
                        sequence: index as u64 + 1,
                        provenance: RuntimeRelayProvenance {
                            service_definition_id: AgentServiceDefinitionId::new(
                                "native-definition",
                            )
                            .unwrap(),
                            service_instance_id: id("native-instance"),
                            driver_generation: RuntimeDriverGeneration(9),
                            host_incarnation_id: id("host-incarnation"),
                            host_id: "host-1".to_string(),
                            transport_id: AgentRuntimePlacementId::new("transport-1").unwrap(),
                        },
                        envelope: *wire_decoded,
                    }),
                };
                let relay_bytes = serde_json::to_vec(&relay).expect("encode Relay message");
                let RelayMessage::RuntimeWireFrame { payload, .. } =
                    serde_json::from_slice(&relay_bytes).expect("decode Relay message")
                else {
                    panic!("expected relayed Runtime Wire frame");
                };
                let RuntimeWireFrame::Notification(notification) = payload.envelope.frame else {
                    panic!("expected notification frame");
                };
                let RuntimeWireNotification::DriverEvent(event) = *notification else {
                    panic!("expected driver event");
                };
                current_source_entry_indices.push(event.source_entry_index);
                let RuntimeJournalFact::Presentation(presentation) = &event.facts[0] else {
                    panic!("expected presentation fact");
                };
                current.push(NormalizedPresentationEvent {
                    durability: match presentation.durability {
                        PresentationDurability::Durable => StrictDurability::Durable,
                        PresentationDurability::Ephemeral => StrictDurability::Ephemeral,
                    },
                    event: serde_json::to_value(&presentation.event)
                        .expect("serialize relayed protected body"),
                });
            }
            compare_ordered_presentation_events(&normalize(expected_records), &current)
                .unwrap_or_else(|error| panic!("strict parity failed for {scenario}: {error}"));
            assert_eq!(
                expected_records
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|_| Some(expected_source_entry_index))
                    .collect::<Vec<_>>(),
                current_source_entry_indices,
                "source entry coordinates drifted through Runtime Wire/Relay for {scenario}"
            );
        }
    }
}
