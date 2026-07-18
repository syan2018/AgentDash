use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use agentdash_agent_runtime_wire::{
    RUNTIME_WIRE_COMPLETE_AGENT_TARGET_REVISION, RuntimeWireAck, RuntimeWireAgentBindingTarget,
    RuntimeWireAgentChangeNotification, RuntimeWireAgentHostCallbackRequest,
    RuntimeWireAgentHostCallbackResponse, RuntimeWireAgentServiceDescribeRequest,
    RuntimeWireAgentServiceRequest, RuntimeWireAgentServiceResponse, RuntimeWireEnvelope,
    RuntimeWireFrame, RuntimeWireFrameId, RuntimeWireNotification, RuntimeWireRequest,
    RuntimeWireResponse,
};
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentChange, AgentChangePage, AgentChangesQuery, AgentCommandEnvelope,
    AgentCommandReceipt, AgentEffectIdentity, AgentEffectInspection, AgentHookDecision,
    AgentHookInvocation, AgentHostCallbackError, AgentHostCallbackErrorCode, AgentHostCallbacks,
    AgentReadQuery, AgentServiceDescriptor, AgentServiceError, AgentServiceErrorCode,
    AgentServiceInstanceId, AgentSnapshot, AgentSourceCoordinate, AgentToolInvocation,
    AgentToolResult, AppliedAgentSurfaceReceipt, ApplyBoundAgentSurface, CompleteAgentService,
    CreateAgentCommand, ForkAgentCommand, ForkAgentReceipt, ResumeAgentCommand,
    RevokeBoundAgentSurface,
};
use async_trait::async_trait;

use crate::{RemoteRuntimeTransportError, RuntimeWirePlacement, RuntimeWirePlacementEvent};

type PendingResponse =
    tokio::sync::oneshot::Sender<Result<RuntimeWireAgentServiceResponse, AgentServiceError>>;

/// Complete Agent proxy bound to one remote service instance.
///
/// The local binding generation is the Host-owned fence exposed to callers. The target carries
/// the source placement generation used on Runtime Wire. Mutating commands are validated against
/// the local fence and rewritten exactly once at this boundary.
pub struct RemoteCompleteAgentService {
    local_binding_generation: AgentBindingGeneration,
    target: RuntimeWireAgentBindingTarget,
    placement: Arc<dyn RuntimeWirePlacement>,
    callbacks: Arc<dyn AgentHostCallbacks>,
    next_frame_id: AtomicU64,
    pending: tokio::sync::Mutex<HashMap<u64, PendingResponse>>,
    cached_effects:
        tokio::sync::Mutex<HashMap<AgentEffectIdentity, RuntimeWireAgentServiceResponse>>,
    callback_effects: tokio::sync::Mutex<HashMap<AgentEffectIdentity, ProxyCallbackEffectState>>,
    pushed_changes: tokio::sync::Mutex<HashMap<AgentSourceCoordinate, Vec<AgentChange>>>,
    pushed_gaps: tokio::sync::Mutex<HashSet<AgentSourceCoordinate>>,
    last_inbound_frame_id: tokio::sync::Mutex<Option<RuntimeWireFrameId>>,
    connection_lost: AtomicBool,
}

impl RemoteCompleteAgentService {
    pub fn new(
        local_binding_generation: AgentBindingGeneration,
        target: RuntimeWireAgentBindingTarget,
        placement: Arc<dyn RuntimeWirePlacement>,
        callbacks: Arc<dyn AgentHostCallbacks>,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            local_binding_generation,
            target,
            placement,
            callbacks,
            next_frame_id: AtomicU64::new(1),
            pending: tokio::sync::Mutex::new(HashMap::new()),
            cached_effects: tokio::sync::Mutex::new(HashMap::new()),
            callback_effects: tokio::sync::Mutex::new(HashMap::new()),
            pushed_changes: tokio::sync::Mutex::new(HashMap::new()),
            pushed_gaps: tokio::sync::Mutex::new(HashSet::new()),
            last_inbound_frame_id: tokio::sync::Mutex::new(None),
            connection_lost: AtomicBool::new(false),
        });
        service.clone().start_receive_pump();
        service
    }

    pub fn target(&self) -> &RuntimeWireAgentBindingTarget {
        &self.target
    }

    fn start_receive_pump(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                match self.placement.receive().await {
                    Ok(RuntimeWirePlacementEvent::Frame(envelope)) => {
                        if let Err(error) = self.handle_inbound(*envelope).await {
                            self.fail_connection(error).await;
                            break;
                        }
                    }
                    Ok(RuntimeWirePlacementEvent::Reconnected) => {
                        // A new proxy/binding must be created after connection loss. Reusing this
                        // object could let an old placement generation advance the current Host.
                        if self.connection_lost.load(Ordering::Acquire) {
                            break;
                        }
                    }
                    Ok(RuntimeWirePlacementEvent::Disconnected { reason }) => {
                        self.fail_connection(unavailable(
                            format!("remote Complete Agent disconnected: {reason}"),
                            true,
                        ))
                        .await;
                        self.placement.acknowledge_disconnect().await;
                        break;
                    }
                    Err(error) => {
                        self.fail_connection(transport_error(error)).await;
                        break;
                    }
                }
            }
        });
    }

    async fn handle_inbound(
        self: &Arc<Self>,
        envelope: RuntimeWireEnvelope,
    ) -> Result<(), AgentServiceError> {
        if envelope.protocol_revision != RUNTIME_WIRE_COMPLETE_AGENT_TARGET_REVISION {
            return Err(protocol(
                "remote Complete Agent used an unsupported Runtime Wire revision",
            ));
        }

        let mut last = self.last_inbound_frame_id.lock().await;
        if let Some(previous) = *last {
            if envelope.frame_id <= previous {
                drop(last);
                if envelope.critical {
                    self.send_ack(previous).await?;
                }
                return Ok(());
            }
            if envelope.frame_id.0 != previous.0 + 1 {
                return Err(protocol(format!(
                    "remote Complete Agent frame gap: expected {}, received {}",
                    previous.0 + 1,
                    envelope.frame_id.0
                )));
            }
        } else if envelope.frame_id.0 != 1 {
            return Err(protocol(format!(
                "remote Complete Agent stream must start at frame 1, received {}",
                envelope.frame_id.0
            )));
        }
        *last = Some(envelope.frame_id);
        drop(last);

        let inbound_frame_id = envelope.frame_id;
        let should_ack = envelope.critical && !matches!(&envelope.frame, RuntimeWireFrame::Ack(_));
        match envelope.frame {
            RuntimeWireFrame::Response {
                request_frame_id,
                response: RuntimeWireResponse::AgentService(response),
            } => {
                if let Some(pending) = self.pending.lock().await.remove(&request_frame_id.0) {
                    let _ = pending.send(Ok(response));
                }
            }
            RuntimeWireFrame::Notification(notification) => {
                let RuntimeWireNotification::AgentChange(notification) = *notification else {
                    return Err(protocol(
                        "remote Complete Agent stream received a foreign notification",
                    ));
                };
                self.record_change(*notification).await?;
            }
            RuntimeWireFrame::Request(request) => {
                let RuntimeWireRequest::AgentHostCallback(callback) = *request else {
                    return Err(protocol(
                        "remote Complete Agent stream received a foreign reverse request",
                    ));
                };
                let service = Arc::clone(self);
                tokio::spawn(async move {
                    let response = service.invoke_callback_idempotent(*callback).await;
                    if let Err(error) = service
                        .send_frame(
                            true,
                            RuntimeWireFrame::Response {
                                request_frame_id: inbound_frame_id,
                                response: RuntimeWireResponse::AgentHostCallback(response),
                            },
                        )
                        .await
                    {
                        service.fail_connection(error).await;
                    }
                });
            }
            RuntimeWireFrame::Ack(_) => return Ok(()),
            RuntimeWireFrame::Response { .. } => {
                return Err(protocol(
                    "remote Complete Agent response family does not match its request",
                ));
            }
        }
        if should_ack {
            self.send_ack(inbound_frame_id).await?;
        }
        Ok(())
    }

    async fn invoke_callback_idempotent(
        &self,
        callback: RuntimeWireAgentHostCallbackRequest,
    ) -> RuntimeWireAgentHostCallbackResponse {
        let effect_id = callback_effect_id(&callback);
        let follower = {
            let mut effects = self.callback_effects.lock().await;
            match effects.get_mut(&effect_id) {
                Some(ProxyCallbackEffectState::Settled { request, response }) => {
                    return if request == &callback {
                        response.clone()
                    } else {
                        callback_error_response(&callback, callback_duplicate_conflict())
                    };
                }
                Some(ProxyCallbackEffectState::InFlight { request, waiters }) => {
                    if request != &callback {
                        return callback_error_response(&callback, callback_duplicate_conflict());
                    }
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    waiters.push(tx);
                    Some(rx)
                }
                None => {
                    effects.insert(
                        effect_id.clone(),
                        ProxyCallbackEffectState::InFlight {
                            request: callback.clone(),
                            waiters: Vec::new(),
                        },
                    );
                    None
                }
            }
        };
        if let Some(follower) = follower {
            return follower.await.unwrap_or_else(|_| {
                callback_error_response(
                    &callback,
                    host_callback_error(
                        AgentHostCallbackErrorCode::Unavailable,
                        "shared Host callback result correlation was lost",
                        true,
                    ),
                )
            });
        }

        let response = self.invoke_callback(callback.clone()).await;
        let waiters = {
            let mut effects = self.callback_effects.lock().await;
            let waiters = match effects.remove(&effect_id) {
                Some(ProxyCallbackEffectState::InFlight { waiters, .. }) => waiters,
                _ => Vec::new(),
            };
            effects.insert(
                effect_id,
                ProxyCallbackEffectState::Settled {
                    request: callback,
                    response: response.clone(),
                },
            );
            waiters
        };
        for waiter in waiters {
            let _ = waiter.send(response.clone());
        }
        response
    }

    async fn invoke_callback(
        &self,
        callback: RuntimeWireAgentHostCallbackRequest,
    ) -> RuntimeWireAgentHostCallbackResponse {
        if callback.binding_generation() != self.target.binding_generation {
            let error = agentdash_agent_service_api::AgentHostCallbackError::new(
                agentdash_agent_service_api::AgentHostCallbackErrorCode::StaleBindingGeneration,
                "remote callback carries a stale source binding generation",
                false,
            );
            return match callback {
                RuntimeWireAgentHostCallbackRequest::Tool(_) => {
                    RuntimeWireAgentHostCallbackResponse::Tool(Err(error))
                }
                RuntimeWireAgentHostCallbackRequest::Hook(_) => {
                    RuntimeWireAgentHostCallbackResponse::Hook(Err(error))
                }
            };
        }
        let deadline = match callback_deadline(&callback) {
            Ok(deadline) => deadline,
            Err(error) => return callback_error_response(&callback, error),
        };
        match callback {
            RuntimeWireAgentHostCallbackRequest::Tool(mut invocation) => {
                invocation.meta.binding_generation = self.local_binding_generation;
                let result =
                    tokio::time::timeout(deadline, self.callbacks.invoke_tool(invocation)).await;
                RuntimeWireAgentHostCallbackResponse::Tool(match result {
                    Ok(result) => result.map(Box::new),
                    Err(_) => Err(callback_deadline_error()),
                })
            }
            RuntimeWireAgentHostCallbackRequest::Hook(mut invocation) => {
                invocation.meta.binding_generation = self.local_binding_generation;
                let result =
                    tokio::time::timeout(deadline, self.callbacks.invoke_hook(invocation)).await;
                RuntimeWireAgentHostCallbackResponse::Hook(match result {
                    Ok(result) => result.map(Box::new),
                    Err(_) => Err(callback_deadline_error()),
                })
            }
        }
    }

    async fn record_change(
        &self,
        notification: RuntimeWireAgentChangeNotification,
    ) -> Result<(), AgentServiceError> {
        if notification.target != self.target {
            return Err(stale_generation(
                "remote change carries a stale Complete Agent binding target",
            ));
        }
        let mut changes = self.pushed_changes.lock().await;
        let source_changes = changes.entry(notification.source.clone()).or_default();
        if source_changes
            .iter()
            .any(|change| change.cursor == notification.change.cursor)
        {
            return Ok(());
        }
        if matches!(
            &notification.change.payload,
            agentdash_agent_service_api::AgentChangePayload::SnapshotInvalidated { .. }
        ) {
            self.pushed_gaps
                .lock()
                .await
                .insert(notification.source.clone());
        }
        source_changes.push(notification.change);
        Ok(())
    }

    async fn send_ack(&self, through: RuntimeWireFrameId) -> Result<(), AgentServiceError> {
        self.send_frame(
            false,
            RuntimeWireFrame::Ack(RuntimeWireAck {
                through_frame_id: through,
            }),
        )
        .await
    }

    async fn send_frame(
        &self,
        critical: bool,
        frame: RuntimeWireFrame,
    ) -> Result<(), AgentServiceError> {
        self.placement
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_COMPLETE_AGENT_TARGET_REVISION,
                frame_id: RuntimeWireFrameId(self.next_frame_id.fetch_add(1, Ordering::Relaxed)),
                critical,
                frame,
            })
            .await
            .map_err(transport_error)
    }

    async fn request(
        &self,
        request: RuntimeWireAgentServiceRequest,
    ) -> Result<RuntimeWireAgentServiceResponse, AgentServiceError> {
        if self.connection_lost.load(Ordering::Acquire) {
            return Err(unavailable(
                "remote Complete Agent placement is disconnected",
                true,
            ));
        }
        request
            .validate_generation()
            .map_err(|error| stale_generation(error.to_string()))?;
        let frame_id = RuntimeWireFrameId(self.next_frame_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().await.insert(frame_id.0, tx);
        if let Err(error) = self
            .placement
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_COMPLETE_AGENT_TARGET_REVISION,
                frame_id,
                critical: true,
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::AgentService(
                    Box::new(request),
                ))),
            })
            .await
        {
            self.pending.lock().await.remove(&frame_id.0);
            let error = transport_error(error);
            self.fail_connection(error.clone()).await;
            return Err(error);
        }
        rx.await
            .map_err(|_| unavailable("remote Complete Agent response correlation was lost", true))?
    }

    async fn fail_connection(&self, error: AgentServiceError) {
        if self.connection_lost.swap(true, Ordering::AcqRel) {
            return;
        }
        let pending = std::mem::take(&mut *self.pending.lock().await);
        for (_, sender) in pending {
            let _ = sender.send(Err(error.clone()));
        }
    }

    async fn cached(
        &self,
        effect_id: &AgentEffectIdentity,
    ) -> Option<RuntimeWireAgentServiceResponse> {
        self.cached_effects.lock().await.get(effect_id).cloned()
    }

    async fn cache(
        &self,
        effect_id: AgentEffectIdentity,
        response: RuntimeWireAgentServiceResponse,
    ) {
        if response_succeeded(&response) {
            self.cached_effects.lock().await.insert(effect_id, response);
        }
    }

    fn validate_local_generation(
        &self,
        received: AgentBindingGeneration,
    ) -> Result<(), AgentServiceError> {
        if received != self.local_binding_generation {
            return Err(stale_generation(format!(
                "expected local binding generation {:?}, received {received:?}",
                self.local_binding_generation
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl CompleteAgentService for RemoteCompleteAgentService {
    async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
        match self
            .request(RuntimeWireAgentServiceRequest::Describe(
                RuntimeWireAgentServiceDescribeRequest {
                    service_instance_id: self.target.service_instance_id.clone(),
                },
            ))
            .await?
        {
            RuntimeWireAgentServiceResponse::Describe(result) => result.map(|value| *value),
            _ => Err(protocol("describe received a mismatched response")),
        }
    }

    async fn create(
        &self,
        mut command: CreateAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        self.validate_local_generation(command.meta.binding_generation)?;
        if let Some(RuntimeWireAgentServiceResponse::Create(result)) =
            self.cached(&command.meta.effect_id).await
        {
            return result.map(|value| *value);
        }
        let effect_id = command.meta.effect_id.clone();
        command.meta.binding_generation = self.target.binding_generation;
        let response = self
            .request(RuntimeWireAgentServiceRequest::Create {
                target: self.target.clone(),
                command,
            })
            .await?;
        let result = match &response {
            RuntimeWireAgentServiceResponse::Create(result) => result.clone().map(|value| *value),
            _ => Err(protocol("create received a mismatched response")),
        };
        self.cache(effect_id, response).await;
        result
    }

    async fn resume(
        &self,
        mut command: ResumeAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        self.validate_local_generation(command.meta.binding_generation)?;
        if let Some(RuntimeWireAgentServiceResponse::Resume(result)) =
            self.cached(&command.meta.effect_id).await
        {
            return result.map(|value| *value);
        }
        let effect_id = command.meta.effect_id.clone();
        command.meta.binding_generation = self.target.binding_generation;
        let response = self
            .request(RuntimeWireAgentServiceRequest::Resume {
                target: self.target.clone(),
                command,
            })
            .await?;
        let result = match &response {
            RuntimeWireAgentServiceResponse::Resume(result) => result.clone().map(|value| *value),
            _ => Err(protocol("resume received a mismatched response")),
        };
        self.cache(effect_id, response).await;
        result
    }

    async fn fork(
        &self,
        mut command: ForkAgentCommand,
    ) -> Result<ForkAgentReceipt, AgentServiceError> {
        self.validate_local_generation(command.meta.binding_generation)?;
        if let Some(RuntimeWireAgentServiceResponse::Fork(result)) =
            self.cached(&command.meta.effect_id).await
        {
            return result.map(|value| *value);
        }
        let effect_id = command.meta.effect_id.clone();
        command.meta.binding_generation = self.target.binding_generation;
        let response = self
            .request(RuntimeWireAgentServiceRequest::Fork {
                target: self.target.clone(),
                command,
            })
            .await?;
        let result = match &response {
            RuntimeWireAgentServiceResponse::Fork(result) => result.clone().map(|value| *value),
            _ => Err(protocol("fork received a mismatched response")),
        };
        self.cache(effect_id, response).await;
        result
    }

    async fn execute(
        &self,
        mut command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        self.validate_local_generation(command.meta.binding_generation)?;
        if let Some(RuntimeWireAgentServiceResponse::Execute(result)) =
            self.cached(&command.meta.effect_id).await
        {
            return result.map(|value| *value);
        }
        let effect_id = command.meta.effect_id.clone();
        command.meta.binding_generation = self.target.binding_generation;
        let response = self
            .request(RuntimeWireAgentServiceRequest::Execute {
                target: self.target.clone(),
                command,
            })
            .await?;
        let result = match &response {
            RuntimeWireAgentServiceResponse::Execute(result) => result.clone().map(|value| *value),
            _ => Err(protocol("execute received a mismatched response")),
        };
        self.cache(effect_id, response).await;
        result
    }

    async fn read(&self, query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
        let source = query.source.clone();
        let snapshot = match self
            .request(RuntimeWireAgentServiceRequest::Read {
                target: self.target.clone(),
                query,
            })
            .await?
        {
            RuntimeWireAgentServiceResponse::Read(result) => result.map(|value| *value),
            _ => Err(protocol("read received a mismatched response")),
        }?;
        self.pushed_gaps.lock().await.remove(&source);
        self.pushed_changes.lock().await.remove(&source);
        Ok(snapshot)
    }

    async fn changes(
        &self,
        query: AgentChangesQuery,
    ) -> Result<AgentChangePage, AgentServiceError> {
        if self.pushed_gaps.lock().await.contains(&query.source) {
            let next = self
                .pushed_changes
                .lock()
                .await
                .get(&query.source)
                .and_then(|changes| changes.last())
                .map(|change| change.cursor.clone());
            return Ok(AgentChangePage {
                source: query.source,
                changes: Vec::new(),
                next,
                gap: true,
            });
        }
        let buffered = {
            let changes = self.pushed_changes.lock().await;
            changes.get(&query.source).cloned()
        };
        if let Some(changes) = buffered
            && !changes.is_empty()
        {
            let start = match &query.after {
                Some(after) => changes
                    .iter()
                    .position(|change| &change.cursor == after)
                    .map(|index| index + 1),
                None => Some(0),
            };
            let Some(start) = start else {
                return Ok(AgentChangePage {
                    source: query.source,
                    changes: Vec::new(),
                    next: None,
                    gap: true,
                });
            };
            let page = changes
                .into_iter()
                .skip(start)
                .take(query.limit as usize)
                .collect::<Vec<_>>();
            return Ok(AgentChangePage {
                source: query.source,
                next: page.last().map(|change| change.cursor.clone()),
                changes: page,
                gap: false,
            });
        }
        match self
            .request(RuntimeWireAgentServiceRequest::Changes {
                target: self.target.clone(),
                query,
            })
            .await?
        {
            RuntimeWireAgentServiceResponse::Changes(result) => result.map(|value| *value),
            _ => Err(protocol("changes received a mismatched response")),
        }
    }

    async fn inspect(
        &self,
        effect_id: AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, AgentServiceError> {
        match self
            .request(RuntimeWireAgentServiceRequest::Inspect {
                target: self.target.clone(),
                effect_id,
            })
            .await?
        {
            RuntimeWireAgentServiceResponse::Inspect(result) => result.map(|value| *value),
            _ => Err(protocol("inspect received a mismatched response")),
        }
    }

    async fn apply_surface(
        &self,
        mut command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
        self.validate_local_generation(command.callbacks.binding_generation)?;
        if let Some(RuntimeWireAgentServiceResponse::ApplySurface(result)) =
            self.cached(&command.effect_id).await
        {
            return result.map(|value| *value);
        }
        let effect_id = command.effect_id.clone();
        command.callbacks.binding_generation = self.target.binding_generation;
        let response = self
            .request(RuntimeWireAgentServiceRequest::ApplySurface {
                target: self.target.clone(),
                command,
            })
            .await?;
        let result = match &response {
            RuntimeWireAgentServiceResponse::ApplySurface(result) => {
                result.clone().map(|value| *value)
            }
            _ => Err(protocol("apply surface received a mismatched response")),
        };
        self.cache(effect_id, response).await;
        result
    }

    async fn revoke_surface(
        &self,
        mut command: RevokeBoundAgentSurface,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        self.validate_local_generation(command.binding_generation)?;
        if let Some(RuntimeWireAgentServiceResponse::RevokeSurface(result)) =
            self.cached(&command.effect_id).await
        {
            return result.map(|value| *value);
        }
        let effect_id = command.effect_id.clone();
        command.binding_generation = self.target.binding_generation;
        let response = self
            .request(RuntimeWireAgentServiceRequest::RevokeSurface {
                target: self.target.clone(),
                command,
            })
            .await?;
        let result = match &response {
            RuntimeWireAgentServiceResponse::RevokeSurface(result) => {
                result.clone().map(|value| *value)
            }
            _ => Err(protocol("revoke surface received a mismatched response")),
        };
        self.cache(effect_id, response).await;
        result
    }
}

type PendingHostCallback = tokio::sync::oneshot::Sender<RuntimeWireAgentHostCallbackResponse>;

enum HostCallbackEffectState {
    InFlight {
        request: RuntimeWireAgentHostCallbackRequest,
        waiters: Vec<
            tokio::sync::oneshot::Sender<
                Result<RuntimeWireAgentHostCallbackResponse, AgentHostCallbackError>,
            >,
        >,
    },
    Settled {
        request: RuntimeWireAgentHostCallbackRequest,
        result: Result<RuntimeWireAgentHostCallbackResponse, AgentHostCallbackError>,
    },
}

enum ProxyCallbackEffectState {
    InFlight {
        request: RuntimeWireAgentHostCallbackRequest,
        waiters: Vec<tokio::sync::oneshot::Sender<RuntimeWireAgentHostCallbackResponse>>,
    },
    Settled {
        request: RuntimeWireAgentHostCallbackRequest,
        response: RuntimeWireAgentHostCallbackResponse,
    },
}

/// Source-side reverse callback client backed by one Complete Agent Runtime Wire stream.
#[derive(Clone)]
pub struct RuntimeWireAgentHostCallbackClient {
    target: RuntimeWireAgentBindingTarget,
    next_frame_id: Arc<AtomicU64>,
    pending: Arc<tokio::sync::Mutex<HashMap<u64, PendingHostCallback>>>,
    effects: Arc<tokio::sync::Mutex<HashMap<AgentEffectIdentity, HostCallbackEffectState>>>,
    outbound: Arc<tokio::sync::RwLock<tokio::sync::mpsc::UnboundedSender<RuntimeWireEnvelope>>>,
}

impl RuntimeWireAgentHostCallbackClient {
    async fn invoke(
        &self,
        request: RuntimeWireAgentHostCallbackRequest,
    ) -> Result<RuntimeWireAgentHostCallbackResponse, AgentHostCallbackError> {
        if request.binding_generation() != self.target.binding_generation {
            return Err(host_callback_error(
                AgentHostCallbackErrorCode::StaleBindingGeneration,
                "source callback carries a stale endpoint binding generation",
                false,
            ));
        }

        let effect_id = callback_effect_id(&request);
        let follower = {
            let mut effects = self.effects.lock().await;
            match effects.get_mut(&effect_id) {
                Some(HostCallbackEffectState::Settled {
                    request: existing,
                    result,
                }) => {
                    return if existing == &request {
                        result.clone()
                    } else {
                        Err(callback_duplicate_conflict())
                    };
                }
                Some(HostCallbackEffectState::InFlight {
                    request: existing,
                    waiters,
                }) => {
                    if existing != &request {
                        return Err(callback_duplicate_conflict());
                    }
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    waiters.push(tx);
                    Some(rx)
                }
                None => {
                    effects.insert(
                        effect_id.clone(),
                        HostCallbackEffectState::InFlight {
                            request: request.clone(),
                            waiters: Vec::new(),
                        },
                    );
                    None
                }
            }
        };
        if let Some(follower) = follower {
            return follower.await.map_err(|_| {
                host_callback_error(
                    AgentHostCallbackErrorCode::Unavailable,
                    "shared callback result correlation was lost",
                    true,
                )
            })?;
        }

        let result = self.request(request.clone()).await;
        let waiters = {
            let mut effects = self.effects.lock().await;
            let waiters = match effects.remove(&effect_id) {
                Some(HostCallbackEffectState::InFlight { waiters, .. }) => waiters,
                _ => Vec::new(),
            };
            effects.insert(
                effect_id,
                HostCallbackEffectState::Settled {
                    request,
                    result: result.clone(),
                },
            );
            waiters
        };
        for waiter in waiters {
            let _ = waiter.send(result.clone());
        }
        result
    }

    async fn request(
        &self,
        request: RuntimeWireAgentHostCallbackRequest,
    ) -> Result<RuntimeWireAgentHostCallbackResponse, AgentHostCallbackError> {
        let deadline = callback_deadline(&request)?;
        let frame_id = RuntimeWireFrameId(self.next_frame_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().await.insert(frame_id.0, tx);
        if self
            .outbound
            .read()
            .await
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_COMPLETE_AGENT_TARGET_REVISION,
                frame_id,
                critical: true,
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::AgentHostCallback(
                    Box::new(request),
                ))),
            })
            .is_err()
        {
            self.pending.lock().await.remove(&frame_id.0);
            return Err(host_callback_error(
                AgentHostCallbackErrorCode::Unavailable,
                "Complete Agent callback stream is closed",
                true,
            ));
        }
        match tokio::time::timeout(deadline, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(host_callback_error(
                AgentHostCallbackErrorCode::Unavailable,
                "Complete Agent callback response correlation was lost",
                true,
            )),
            Err(_) => {
                self.pending.lock().await.remove(&frame_id.0);
                Err(callback_deadline_error())
            }
        }
    }
}

#[async_trait]
impl AgentHostCallbacks for RuntimeWireAgentHostCallbackClient {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        match self
            .invoke(RuntimeWireAgentHostCallbackRequest::Tool(call))
            .await?
        {
            RuntimeWireAgentHostCallbackResponse::Tool(result) => result.map(|value| *value),
            RuntimeWireAgentHostCallbackResponse::Hook(_) => Err(host_callback_error(
                AgentHostCallbackErrorCode::Internal,
                "tool callback received a hook response",
                false,
            )),
        }
    }

    async fn invoke_hook(
        &self,
        call: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        match self
            .invoke(RuntimeWireAgentHostCallbackRequest::Hook(call))
            .await?
        {
            RuntimeWireAgentHostCallbackResponse::Hook(result) => result.map(|value| *value),
            RuntimeWireAgentHostCallbackResponse::Tool(_) => Err(host_callback_error(
                AgentHostCallbackErrorCode::Internal,
                "hook callback received a tool response",
                false,
            )),
        }
    }
}

#[derive(Default)]
struct PublishedChangeState {
    last_sequence: Option<u64>,
    cursors: HashSet<agentdash_agent_service_api::AgentSourceCursor>,
}

/// Local Runtime Wire terminator for one concrete Complete Agent implementation.
pub struct RuntimeWireAgentServiceEndpoint {
    service_instance_id: AgentServiceInstanceId,
    binding_generation: AgentBindingGeneration,
    service: Arc<dyn CompleteAgentService>,
    next_frame_id: Arc<AtomicU64>,
    pending_callbacks: Arc<tokio::sync::Mutex<HashMap<u64, PendingHostCallback>>>,
    callback_effects:
        Arc<tokio::sync::Mutex<HashMap<AgentEffectIdentity, HostCallbackEffectState>>>,
    published_changes: tokio::sync::Mutex<HashMap<AgentSourceCoordinate, PublishedChangeState>>,
    outbound_tx: Arc<tokio::sync::RwLock<tokio::sync::mpsc::UnboundedSender<RuntimeWireEnvelope>>>,
    outbound_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<RuntimeWireEnvelope>>,
}

impl RuntimeWireAgentServiceEndpoint {
    pub fn new(
        service_instance_id: AgentServiceInstanceId,
        binding_generation: AgentBindingGeneration,
        service: Arc<dyn CompleteAgentService>,
    ) -> Self {
        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            service_instance_id,
            binding_generation,
            service,
            next_frame_id: Arc::new(AtomicU64::new(1)),
            pending_callbacks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            callback_effects: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            published_changes: tokio::sync::Mutex::new(HashMap::new()),
            outbound_tx: Arc::new(tokio::sync::RwLock::new(outbound_tx)),
            outbound_rx: tokio::sync::Mutex::new(outbound_rx),
        }
    }

    pub fn host_callbacks(&self) -> Arc<dyn AgentHostCallbacks> {
        Arc::new(RuntimeWireAgentHostCallbackClient {
            target: self.target(),
            next_frame_id: self.next_frame_id.clone(),
            pending: self.pending_callbacks.clone(),
            effects: self.callback_effects.clone(),
            outbound: self.outbound_tx.clone(),
        })
    }

    pub fn target(&self) -> RuntimeWireAgentBindingTarget {
        RuntimeWireAgentBindingTarget {
            service_instance_id: self.service_instance_id.clone(),
            binding_generation: self.binding_generation,
        }
    }

    /// Closes the current outbound stream so producers receive an explicit send failure.
    pub async fn disconnect_outbound(&self) {
        self.outbound_rx.lock().await.close();
    }

    /// Installs a fresh outbound stream after transport reconnection.
    pub async fn reconnect_outbound(&self) {
        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        *self.outbound_tx.write().await = outbound_tx;
        *self.outbound_rx.lock().await = outbound_rx;
    }

    /// Publishes one source-owned ordered change.
    ///
    /// The sequence is adapter-owned and local to the source. A discontinuity emits one typed
    /// snapshot invalidation instead of presenting the following change as a contiguous tail.
    pub async fn publish_change(
        &self,
        source_sequence: u64,
        source: AgentSourceCoordinate,
        change: AgentChange,
    ) -> Result<(), AgentServiceError> {
        let mut states = self.published_changes.lock().await;
        let state = states.entry(source.clone()).or_default();
        if state.cursors.contains(&change.cursor) {
            return if state.last_sequence == Some(source_sequence) {
                Ok(())
            } else {
                Err(protocol(
                    "Complete Agent source cursor was replayed at a different sequence",
                ))
            };
        }
        if let Some(last) = state.last_sequence
            && source_sequence <= last
        {
            return Err(protocol(
                "Complete Agent source change sequence moved backwards",
            ));
        }
        let expected = state.last_sequence.map_or(1, |last| last + 1);
        let change = if source_sequence == expected {
            change
        } else {
            AgentChange {
                cursor: change.cursor,
                source_revision: change.source_revision,
                occurred_at_ms: change.occurred_at_ms,
                payload: agentdash_agent_service_api::AgentChangePayload::SnapshotInvalidated {
                    reason: format!(
                        "Complete Agent source change gap: expected {expected}, received {source_sequence}"
                    ),
                },
            }
        };
        let cursor = change.cursor.clone();
        self.outbound_tx
            .read()
            .await
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_COMPLETE_AGENT_TARGET_REVISION,
                frame_id: RuntimeWireFrameId(self.next_frame_id.fetch_add(1, Ordering::Relaxed)),
                critical: true,
                frame: RuntimeWireFrame::Notification(Box::new(
                    RuntimeWireNotification::AgentChange(Box::new(
                        RuntimeWireAgentChangeNotification {
                            target: self.target(),
                            source,
                            change,
                        },
                    )),
                )),
            })
            .map_err(|_| unavailable("Complete Agent change stream is closed", true))?;
        state.last_sequence = Some(source_sequence);
        state.cursors.insert(cursor);
        Ok(())
    }

    fn response(
        &self,
        request_frame_id: RuntimeWireFrameId,
        response: RuntimeWireAgentServiceResponse,
    ) -> RuntimeWireEnvelope {
        RuntimeWireEnvelope {
            protocol_revision: RUNTIME_WIRE_COMPLETE_AGENT_TARGET_REVISION,
            frame_id: RuntimeWireFrameId(self.next_frame_id.fetch_add(1, Ordering::Relaxed)),
            critical: true,
            frame: RuntimeWireFrame::Response {
                request_frame_id,
                response: RuntimeWireResponse::AgentService(response),
            },
        }
    }

    fn validate_target(
        &self,
        target: &RuntimeWireAgentBindingTarget,
    ) -> Result<(), AgentServiceError> {
        if target.service_instance_id != self.service_instance_id {
            return Err(AgentServiceError::new(
                AgentServiceErrorCode::NotFound,
                "Complete Agent service instance is not registered on this endpoint",
                false,
            ));
        }
        if target.binding_generation != self.binding_generation {
            return Err(stale_generation(
                "Complete Agent request carries a stale endpoint generation",
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl RuntimeWirePlacement for RuntimeWireAgentServiceEndpoint {
    async fn send(&self, envelope: RuntimeWireEnvelope) -> Result<(), RemoteRuntimeTransportError> {
        if envelope.protocol_revision != RUNTIME_WIRE_COMPLETE_AGENT_TARGET_REVISION {
            return Err(RemoteRuntimeTransportError::Protocol {
                reason: "unsupported Complete Agent Runtime Wire target revision".to_owned(),
                critical: true,
            });
        }
        match envelope.frame {
            RuntimeWireFrame::Ack(_) => return Ok(()),
            RuntimeWireFrame::Response {
                request_frame_id,
                response: RuntimeWireResponse::AgentHostCallback(response),
            } => {
                if let Some(pending) = self
                    .pending_callbacks
                    .lock()
                    .await
                    .remove(&request_frame_id.0)
                {
                    let _ = pending.send(response);
                }
                Ok(())
            }
            RuntimeWireFrame::Request(request) => {
                let RuntimeWireRequest::AgentService(request) = *request else {
                    return Err(RemoteRuntimeTransportError::Protocol {
                        reason: "Complete Agent endpoint accepts AgentService requests only"
                            .to_owned(),
                        critical: true,
                    });
                };
                let response = self.dispatch(*request).await;
                self.outbound_tx
                    .read()
                    .await
                    .send(self.response(envelope.frame_id, response))
                    .map_err(|_| RemoteRuntimeTransportError::Unavailable {
                        reason: "Complete Agent endpoint receiver is closed".to_owned(),
                        retryable: true,
                    })
            }
            _ => Err(RemoteRuntimeTransportError::Protocol {
                reason:
                    "Complete Agent endpoint accepts service requests, callback responses, and acknowledgements only"
                        .to_owned(),
                critical: true,
            }),
        }
    }

    async fn receive(&self) -> Result<RuntimeWirePlacementEvent, RemoteRuntimeTransportError> {
        self.outbound_rx
            .lock()
            .await
            .recv()
            .await
            .map(|envelope| RuntimeWirePlacementEvent::Frame(Box::new(envelope)))
            .ok_or_else(|| RemoteRuntimeTransportError::Unavailable {
                reason: "Complete Agent endpoint closed".to_owned(),
                retryable: true,
            })
    }
}

impl RuntimeWireAgentServiceEndpoint {
    async fn dispatch(
        &self,
        request: RuntimeWireAgentServiceRequest,
    ) -> RuntimeWireAgentServiceResponse {
        if let Err(error) = request.validate_generation() {
            return response_error(request, stale_generation(error.to_string()));
        }
        if let Some(target) = request.target()
            && let Err(error) = self.validate_target(target)
        {
            return response_error(request, error);
        }
        match request {
            RuntimeWireAgentServiceRequest::Describe(request) => {
                let result = if request.service_instance_id == self.service_instance_id {
                    self.service.describe().await.map(Box::new)
                } else {
                    Err(AgentServiceError::new(
                        AgentServiceErrorCode::NotFound,
                        "Complete Agent service instance is not registered on this endpoint",
                        false,
                    ))
                };
                RuntimeWireAgentServiceResponse::Describe(result)
            }
            RuntimeWireAgentServiceRequest::Create { command, .. } => {
                RuntimeWireAgentServiceResponse::Create(
                    self.service.create(command).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::Resume { command, .. } => {
                RuntimeWireAgentServiceResponse::Resume(
                    self.service.resume(command).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::Fork { command, .. } => {
                RuntimeWireAgentServiceResponse::Fork(
                    self.service.fork(command).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::Execute { command, .. } => {
                RuntimeWireAgentServiceResponse::Execute(
                    self.service.execute(command).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::Read { query, .. } => {
                RuntimeWireAgentServiceResponse::Read(self.service.read(query).await.map(Box::new))
            }
            RuntimeWireAgentServiceRequest::Changes { query, .. } => {
                RuntimeWireAgentServiceResponse::Changes(
                    self.service.changes(query).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::Inspect { effect_id, .. } => {
                RuntimeWireAgentServiceResponse::Inspect(
                    self.service.inspect(effect_id).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::ApplySurface { command, .. } => {
                RuntimeWireAgentServiceResponse::ApplySurface(
                    self.service.apply_surface(command).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::RevokeSurface { command, .. } => {
                RuntimeWireAgentServiceResponse::RevokeSurface(
                    self.service.revoke_surface(command).await.map(Box::new),
                )
            }
        }
    }
}

fn response_error(
    request: RuntimeWireAgentServiceRequest,
    error: AgentServiceError,
) -> RuntimeWireAgentServiceResponse {
    match request {
        RuntimeWireAgentServiceRequest::Describe(_) => {
            RuntimeWireAgentServiceResponse::Describe(Err(error))
        }
        RuntimeWireAgentServiceRequest::Create { .. } => {
            RuntimeWireAgentServiceResponse::Create(Err(error))
        }
        RuntimeWireAgentServiceRequest::Resume { .. } => {
            RuntimeWireAgentServiceResponse::Resume(Err(error))
        }
        RuntimeWireAgentServiceRequest::Fork { .. } => {
            RuntimeWireAgentServiceResponse::Fork(Err(error))
        }
        RuntimeWireAgentServiceRequest::Execute { .. } => {
            RuntimeWireAgentServiceResponse::Execute(Err(error))
        }
        RuntimeWireAgentServiceRequest::Read { .. } => {
            RuntimeWireAgentServiceResponse::Read(Err(error))
        }
        RuntimeWireAgentServiceRequest::Changes { .. } => {
            RuntimeWireAgentServiceResponse::Changes(Err(error))
        }
        RuntimeWireAgentServiceRequest::Inspect { .. } => {
            RuntimeWireAgentServiceResponse::Inspect(Err(error))
        }
        RuntimeWireAgentServiceRequest::ApplySurface { .. } => {
            RuntimeWireAgentServiceResponse::ApplySurface(Err(error))
        }
        RuntimeWireAgentServiceRequest::RevokeSurface { .. } => {
            RuntimeWireAgentServiceResponse::RevokeSurface(Err(error))
        }
    }
}

fn response_succeeded(response: &RuntimeWireAgentServiceResponse) -> bool {
    match response {
        RuntimeWireAgentServiceResponse::Describe(result) => result.is_ok(),
        RuntimeWireAgentServiceResponse::Create(result)
        | RuntimeWireAgentServiceResponse::Resume(result)
        | RuntimeWireAgentServiceResponse::Execute(result)
        | RuntimeWireAgentServiceResponse::RevokeSurface(result) => result.is_ok(),
        RuntimeWireAgentServiceResponse::Fork(result) => result.is_ok(),
        RuntimeWireAgentServiceResponse::Read(result) => result.is_ok(),
        RuntimeWireAgentServiceResponse::Changes(result) => result.is_ok(),
        RuntimeWireAgentServiceResponse::Inspect(result) => result.is_ok(),
        RuntimeWireAgentServiceResponse::ApplySurface(result) => result.is_ok(),
    }
}

fn callback_effect_id(request: &RuntimeWireAgentHostCallbackRequest) -> AgentEffectIdentity {
    match request {
        RuntimeWireAgentHostCallbackRequest::Tool(invocation) => invocation.meta.effect_id.clone(),
        RuntimeWireAgentHostCallbackRequest::Hook(invocation) => invocation.meta.effect_id.clone(),
    }
}

fn callback_deadline(
    request: &RuntimeWireAgentHostCallbackRequest,
) -> Result<std::time::Duration, AgentHostCallbackError> {
    let deadline_at_ms = match request {
        RuntimeWireAgentHostCallbackRequest::Tool(invocation) => invocation.meta.deadline_at_ms,
        RuntimeWireAgentHostCallbackRequest::Hook(invocation) => invocation.meta.deadline_at_ms,
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64);
    let remaining = deadline_at_ms.saturating_sub(now_ms);
    if remaining == 0 {
        return Err(callback_deadline_error());
    }
    Ok(std::time::Duration::from_millis(remaining))
}

fn callback_deadline_error() -> AgentHostCallbackError {
    host_callback_error(
        AgentHostCallbackErrorCode::DeadlineExceeded,
        "Complete Agent Host callback deadline exceeded",
        false,
    )
}

fn callback_duplicate_conflict() -> AgentHostCallbackError {
    host_callback_error(
        AgentHostCallbackErrorCode::DuplicateConflict,
        "callback effect identity was reused with a different request",
        false,
    )
}

fn callback_error_response(
    request: &RuntimeWireAgentHostCallbackRequest,
    error: AgentHostCallbackError,
) -> RuntimeWireAgentHostCallbackResponse {
    match request {
        RuntimeWireAgentHostCallbackRequest::Tool(_) => {
            RuntimeWireAgentHostCallbackResponse::Tool(Err(error))
        }
        RuntimeWireAgentHostCallbackRequest::Hook(_) => {
            RuntimeWireAgentHostCallbackResponse::Hook(Err(error))
        }
    }
}

fn host_callback_error(
    code: AgentHostCallbackErrorCode,
    message: impl Into<String>,
    retryable: bool,
) -> AgentHostCallbackError {
    AgentHostCallbackError::new(code, message, retryable)
}

fn stale_generation(message: impl Into<String>) -> AgentServiceError {
    AgentServiceError::new(
        AgentServiceErrorCode::StaleBindingGeneration,
        message,
        false,
    )
}

fn protocol(message: impl Into<String>) -> AgentServiceError {
    AgentServiceError::new(AgentServiceErrorCode::ProtocolViolation, message, false)
}

fn unavailable(message: impl Into<String>, retryable: bool) -> AgentServiceError {
    AgentServiceError::new(AgentServiceErrorCode::Unavailable, message, retryable)
}

fn transport_error(error: RemoteRuntimeTransportError) -> AgentServiceError {
    match error {
        RemoteRuntimeTransportError::Unavailable { reason, retryable } => {
            unavailable(reason, retryable)
        }
        RemoteRuntimeTransportError::Protocol { reason, .. } => protocol(reason),
    }
}
