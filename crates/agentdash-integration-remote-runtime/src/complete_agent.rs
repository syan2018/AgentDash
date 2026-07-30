use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use agentdash_agent_runtime_contract::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentChange, AgentChangePage, AgentChangesQuery,
    AgentCommandEnvelope, AgentCommandReceipt, AgentContextQuery, AgentContextSnapshot,
    AgentEffectIdentity, AgentEffectInspection, AgentHookDecision, AgentHookInvocation,
    AgentHostCallbackError, AgentHostCallbackErrorCode, AgentHostCallbacks, AgentLiveBatch,
    AgentLiveBatchStream, AgentLiveStreamError, AgentReadQuery, AgentServiceDescriptor,
    AgentServiceError, AgentServiceErrorCode, AgentServiceInstanceId, AgentSnapshot,
    AgentSourceCoordinate, AgentToolExecutionEvent, AgentToolExecutionSequence,
    AgentToolExecutionStream, AgentToolInvocation, AgentToolResult, AppliedAgentSurfaceReceipt,
    ApplyBoundAgentSurface, CompleteAgentService, CreateAgentCommand, ForkAgentCommand,
    ForkAgentReceipt, ResumeAgentCommand, RevokeBoundAgentSurface,
};
use agentdash_agent_runtime_wire::{
    RUNTIME_WIRE_PROTOCOL_REVISION, RuntimeWireAck, RuntimeWireAgentBindingTarget,
    RuntimeWireAgentChangeNotification, RuntimeWireAgentHostCallbackRequest,
    RuntimeWireAgentHostCallbackResponse, RuntimeWireAgentLiveBatchNotification,
    RuntimeWireAgentLiveEvent, RuntimeWireAgentServiceDescribeRequest,
    RuntimeWireAgentServiceRequest, RuntimeWireAgentServiceResponse,
    RuntimeWireAgentToolExecutionEvent, RuntimeWireEnvelope, RuntimeWireFrame, RuntimeWireFrameId,
    RuntimeWireNotification, RuntimeWireRequest, RuntimeWireResponse,
};
use async_trait::async_trait;
use thiserror::Error;

use crate::{RemoteRuntimeTransportError, RuntimeWirePlacement, RuntimeWirePlacementEvent};

type PendingResponse =
    tokio::sync::oneshot::Sender<Result<RuntimeWireAgentServiceResponse, AgentServiceError>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RuntimeWireFrameAllocationError {
    #[error("Runtime Wire frame identity space is exhausted")]
    Exhausted,
}

fn allocate_frame_id(
    last_allocated: &AtomicU64,
) -> Result<RuntimeWireFrameId, RuntimeWireFrameAllocationError> {
    let previous = last_allocated
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| RuntimeWireFrameAllocationError::Exhausted)?;
    Ok(RuntimeWireFrameId(previous + 1))
}

/// Complete Agent proxy bound to one remote service instance.
///
/// The local binding generation is the Host-owned fence exposed to callers. The target carries
/// the source placement generation used on Runtime Wire. Mutating commands are validated against
/// the local fence and rewritten exactly once at this boundary.
pub struct RemoteCompleteAgentService {
    target: RuntimeWireAgentBindingTarget,
    placement: Arc<dyn RuntimeWirePlacement>,
    callbacks: Arc<dyn AgentHostCallbacks>,
    next_frame_id: AtomicU64,
    pending: tokio::sync::Mutex<HashMap<u64, PendingResponse>>,
    cached_effects:
        tokio::sync::Mutex<HashMap<AgentEffectIdentity, RuntimeWireAgentServiceResponse>>,
    callback_effects: tokio::sync::Mutex<HashMap<AgentEffectIdentity, ProxyCallbackEffectState>>,
    callback_generations: tokio::sync::Mutex<HashMap<AgentCallbackRouteId, AgentBindingGeneration>>,
    pushed_changes: tokio::sync::Mutex<HashMap<AgentSourceCoordinate, Vec<AgentChange>>>,
    pushed_gaps: tokio::sync::Mutex<HashSet<AgentSourceCoordinate>>,
    live_channels: tokio::sync::Mutex<
        HashMap<
            AgentSourceCoordinate,
            tokio::sync::broadcast::Sender<Result<AgentLiveBatch, AgentLiveStreamError>>,
        >,
    >,
    outbound_order: tokio::sync::Mutex<()>,
    causal_outbound_order: tokio::sync::Mutex<()>,
    last_inbound_frame_id: tokio::sync::Mutex<Option<RuntimeWireFrameId>>,
    connection_lost: AtomicBool,
}

impl RemoteCompleteAgentService {
    pub fn new(
        target: RuntimeWireAgentBindingTarget,
        placement: Arc<dyn RuntimeWirePlacement>,
        callbacks: Arc<dyn AgentHostCallbacks>,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            target,
            placement,
            callbacks,
            next_frame_id: AtomicU64::new(0),
            pending: tokio::sync::Mutex::new(HashMap::new()),
            cached_effects: tokio::sync::Mutex::new(HashMap::new()),
            callback_effects: tokio::sync::Mutex::new(HashMap::new()),
            callback_generations: tokio::sync::Mutex::new(HashMap::new()),
            pushed_changes: tokio::sync::Mutex::new(HashMap::new()),
            pushed_gaps: tokio::sync::Mutex::new(HashSet::new()),
            live_channels: tokio::sync::Mutex::new(HashMap::new()),
            outbound_order: tokio::sync::Mutex::new(()),
            causal_outbound_order: tokio::sync::Mutex::new(()),
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
        if envelope.protocol_revision != RUNTIME_WIRE_PROTOCOL_REVISION {
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
            let expected = previous
                .0
                .checked_add(1)
                .ok_or_else(|| protocol(RuntimeWireFrameAllocationError::Exhausted.to_string()))?;
            if envelope.frame_id.0 != expected {
                return Err(protocol(format!(
                    "remote Complete Agent frame gap: expected {}, received {}",
                    expected, envelope.frame_id.0
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
            RuntimeWireFrame::Notification(notification) => match *notification {
                RuntimeWireNotification::AgentChange(notification) => {
                    self.record_change(*notification).await?;
                }
                RuntimeWireNotification::AgentLiveBatch(notification) => {
                    self.record_live_event(*notification).await?;
                }
                RuntimeWireNotification::Heartbeat { .. } => {
                    return Err(protocol(
                        "remote Complete Agent stream received a foreign notification",
                    ));
                }
            },
            RuntimeWireFrame::Request(request) => {
                let RuntimeWireRequest::AgentHostCallback(callback) = *request else {
                    return Err(protocol(
                        "remote Complete Agent stream received a foreign reverse request",
                    ));
                };
                let service = Arc::clone(self);
                tokio::spawn(async move {
                    if let Err(error) = service
                        .serve_callback_idempotent(inbound_frame_id, *callback)
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

    async fn serve_callback_idempotent(
        &self,
        request_frame_id: RuntimeWireFrameId,
        callback: RuntimeWireAgentHostCallbackRequest,
    ) -> Result<(), AgentServiceError> {
        let effect_id = callback_effect_id(&callback);
        let replay = {
            let mut effects = self.callback_effects.lock().await;
            match effects.get_mut(&effect_id) {
                Some(ProxyCallbackEffectState::Settled { request, responses }) => {
                    Some(if request == &callback {
                        responses.clone()
                    } else {
                        callback_failure_responses(
                            &callback,
                            "tool_callback_duplicate_conflict",
                            callback_duplicate_conflict(),
                        )
                    })
                }
                Some(ProxyCallbackEffectState::InFlight {
                    request,
                    waiting_request_frame_ids,
                }) => {
                    if request != &callback {
                        Some(callback_failure_responses(
                            &callback,
                            "tool_callback_duplicate_conflict",
                            callback_duplicate_conflict(),
                        ))
                    } else {
                        waiting_request_frame_ids.push(request_frame_id);
                        return Ok(());
                    }
                }
                None => {
                    effects.insert(
                        effect_id.clone(),
                        ProxyCallbackEffectState::InFlight {
                            request: callback.clone(),
                            waiting_request_frame_ids: Vec::new(),
                        },
                    );
                    None
                }
            }
        };
        if let Some(responses) = replay {
            return self
                .replay_callback_responses(request_frame_id, &responses)
                .await;
        }

        let responses = self
            .execute_callback(request_frame_id, callback.clone())
            .await?;
        let waiting_request_frame_ids = {
            let mut effects = self.callback_effects.lock().await;
            let waiting_request_frame_ids = match effects.remove(&effect_id) {
                Some(ProxyCallbackEffectState::InFlight {
                    waiting_request_frame_ids,
                    ..
                }) => waiting_request_frame_ids,
                _ => Vec::new(),
            };
            effects.insert(
                effect_id,
                ProxyCallbackEffectState::Settled {
                    request: callback,
                    responses: responses.clone(),
                },
            );
            waiting_request_frame_ids
        };
        for waiting_request_frame_id in waiting_request_frame_ids {
            self.replay_callback_responses(waiting_request_frame_id, &responses)
                .await?;
        }
        Ok(())
    }

    async fn replay_callback_responses(
        &self,
        request_frame_id: RuntimeWireFrameId,
        responses: &[RuntimeWireAgentHostCallbackResponse],
    ) -> Result<(), AgentServiceError> {
        for response in responses {
            self.send_callback_response(request_frame_id, response.clone())
                .await?;
        }
        Ok(())
    }

    async fn send_callback_response(
        &self,
        request_frame_id: RuntimeWireFrameId,
        response: RuntimeWireAgentHostCallbackResponse,
    ) -> Result<(), AgentServiceError> {
        self.send_frame(
            true,
            RuntimeWireFrame::Response {
                request_frame_id,
                response: RuntimeWireResponse::AgentHostCallback(response),
            },
        )
        .await
    }

    async fn execute_callback(
        &self,
        request_frame_id: RuntimeWireFrameId,
        callback: RuntimeWireAgentHostCallbackRequest,
    ) -> Result<Vec<RuntimeWireAgentHostCallbackResponse>, AgentServiceError> {
        if callback.binding_generation() != self.target.binding_generation {
            let error = AgentHostCallbackError::new(
                AgentHostCallbackErrorCode::StaleBindingGeneration,
                "remote callback carries a stale source binding generation",
                false,
            );
            let responses =
                callback_failure_responses(&callback, "stale_binding_generation", error);
            self.replay_callback_responses(request_frame_id, &responses)
                .await?;
            return Ok(responses);
        }
        let deadline = match callback_deadline(&callback) {
            Ok(deadline) => tokio::time::Instant::now() + deadline,
            Err(error) => {
                let responses =
                    callback_failure_responses(&callback, "tool_callback_deadline_exceeded", error);
                self.replay_callback_responses(request_frame_id, &responses)
                    .await?;
                return Ok(responses);
            }
        };
        let route_id = match &callback {
            RuntimeWireAgentHostCallbackRequest::Tool(invocation) => &invocation.meta.route_id,
            RuntimeWireAgentHostCallbackRequest::Hook(invocation) => &invocation.meta.route_id,
        };
        let Some(local_generation) = self
            .callback_generations
            .lock()
            .await
            .get(route_id)
            .copied()
        else {
            let error = AgentHostCallbackError::new(
                AgentHostCallbackErrorCode::StaleBindingGeneration,
                "remote callback route has no exact local generation mapping",
                false,
            );
            let responses =
                callback_failure_responses(&callback, "stale_binding_generation", error);
            self.replay_callback_responses(request_frame_id, &responses)
                .await?;
            return Ok(responses);
        };
        match callback {
            RuntimeWireAgentHostCallbackRequest::Tool(mut invocation) => {
                invocation.meta.binding_generation = local_generation;
                self.execute_tool_callback(request_frame_id, invocation, deadline)
                    .await
            }
            RuntimeWireAgentHostCallbackRequest::Hook(mut invocation) => {
                invocation.meta.binding_generation = local_generation;
                let result =
                    tokio::time::timeout_at(deadline, self.callbacks.invoke_hook(invocation)).await;
                let response = RuntimeWireAgentHostCallbackResponse::Hook(match result {
                    Ok(result) => result.map(Box::new),
                    Err(_) => Err(callback_deadline_error()),
                });
                self.send_callback_response(request_frame_id, response.clone())
                    .await?;
                Ok(vec![response])
            }
        }
    }

    async fn execute_tool_callback(
        &self,
        request_frame_id: RuntimeWireFrameId,
        invocation: AgentToolInvocation,
        deadline: tokio::time::Instant,
    ) -> Result<Vec<RuntimeWireAgentHostCallbackResponse>, AgentServiceError> {
        let mut stream =
            match tokio::time::timeout_at(deadline, self.callbacks.invoke_tool(invocation.clone()))
                .await
            {
                Ok(Ok(stream)) => stream,
                Ok(Err(error)) => {
                    let responses = tool_failure_responses(
                        &invocation,
                        "tool_callback_rejected",
                        error.message,
                    );
                    self.replay_callback_responses(request_frame_id, &responses)
                        .await?;
                    return Ok(responses);
                }
                Err(_) => {
                    let responses = tool_failure_responses(
                        &invocation,
                        "tool_callback_deadline_exceeded",
                        "Complete Agent Host callback deadline exceeded",
                    );
                    self.replay_callback_responses(request_frame_id, &responses)
                        .await?;
                    return Ok(responses);
                }
            };

        let mut responses = Vec::new();
        let mut started = false;
        let mut last_update_index = 0_u64;
        loop {
            let next = tokio::time::timeout_at(deadline, stream.next()).await;
            let event = match next {
                Ok(Ok(Some(AgentToolExecutionEvent::Started))) if !started => {
                    started = true;
                    AgentToolExecutionEvent::Started
                }
                Ok(Ok(Some(AgentToolExecutionEvent::Progress {
                    update_index,
                    output,
                }))) if started && last_update_index.checked_add(1) == Some(update_index) => {
                    last_update_index = update_index;
                    AgentToolExecutionEvent::Progress {
                        update_index,
                        output,
                    }
                }
                Ok(Ok(Some(AgentToolExecutionEvent::Completed { result }))) if started => {
                    let response = tool_event_response(
                        &invocation,
                        AgentToolExecutionEvent::Completed { result },
                    );
                    self.send_callback_response(request_frame_id, response.clone())
                        .await?;
                    responses.push(response);
                    return Ok(responses);
                }
                Ok(Ok(None)) => {
                    if !started {
                        let response =
                            tool_event_response(&invocation, AgentToolExecutionEvent::Started);
                        self.send_callback_response(request_frame_id, response.clone())
                            .await?;
                        responses.push(response);
                    }
                    AgentToolExecutionEvent::Completed {
                        result: AgentToolResult::Failed {
                            code: "tool_callback_transport_lost".to_owned(),
                            message: "Host tool callback stream ended before a terminal event"
                                .to_owned(),
                        },
                    }
                }
                Err(_) => {
                    if !started {
                        let response =
                            tool_event_response(&invocation, AgentToolExecutionEvent::Started);
                        self.send_callback_response(request_frame_id, response.clone())
                            .await?;
                        responses.push(response);
                    }
                    AgentToolExecutionEvent::Completed {
                        result: AgentToolResult::Failed {
                            code: "tool_callback_deadline_exceeded".to_owned(),
                            message: "Host tool callback crossed its absolute deadline".to_owned(),
                        },
                    }
                }
                Ok(Err(error)) => {
                    if !started {
                        let response =
                            tool_event_response(&invocation, AgentToolExecutionEvent::Started);
                        self.send_callback_response(request_frame_id, response.clone())
                            .await?;
                        responses.push(response);
                    }
                    AgentToolExecutionEvent::Completed {
                        result: AgentToolResult::Failed {
                            code: "tool_callback_unavailable".to_owned(),
                            message: error.message,
                        },
                    }
                }
                _ => {
                    if !started {
                        let response =
                            tool_event_response(&invocation, AgentToolExecutionEvent::Started);
                        self.send_callback_response(request_frame_id, response.clone())
                            .await?;
                        responses.push(response);
                    }
                    AgentToolExecutionEvent::Completed {
                        result: AgentToolResult::Failed {
                            code: "tool_callback_protocol_violation".to_owned(),
                            message: "Host tool callback lifecycle is out of order".to_owned(),
                        },
                    }
                }
            };
            let terminal = matches!(event, AgentToolExecutionEvent::Completed { .. });
            let response = tool_event_response(&invocation, event);
            self.send_callback_response(request_frame_id, response.clone())
                .await?;
            responses.push(response);
            if terminal {
                return Ok(responses);
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
            agentdash_agent_runtime_contract::AgentChangePayload::SnapshotInvalidated { .. }
        ) {
            self.pushed_gaps
                .lock()
                .await
                .insert(notification.source.clone());
        }
        source_changes.push(notification.change);
        Ok(())
    }

    async fn record_live_event(
        &self,
        notification: RuntimeWireAgentLiveBatchNotification,
    ) -> Result<(), AgentServiceError> {
        if notification.target != self.target {
            return Err(stale_generation(
                "remote live batch carries a stale Complete Agent binding target",
            ));
        }
        let (source, event) = match notification.event {
            RuntimeWireAgentLiveEvent::Batch { batch } => {
                let source = batch.source.clone();
                (source, Ok(*batch))
            }
            RuntimeWireAgentLiveEvent::Lagged { source, skipped } => {
                (source, Err(AgentLiveStreamError::Lagged { skipped }))
            }
            RuntimeWireAgentLiveEvent::Protocol { source, message } => {
                (source, Err(AgentLiveStreamError::Protocol { message }))
            }
            RuntimeWireAgentLiveEvent::Unavailable { source, message } => {
                (source, Err(AgentLiveStreamError::Unavailable { message }))
            }
        };
        if let Some(sender) = self.live_channels.lock().await.get(&source) {
            let _ = sender.send(event);
        }
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
        // Acknowledgements and callback responses are causally downstream of a frame the
        // placement has already delivered. They use a separate ordered lane so a re-entrant
        // callback can complete while its originating service request is still awaiting return.
        let _causal_outbound_order = self.causal_outbound_order.lock().await;
        self.placement
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id: allocate_frame_id(&self.next_frame_id)
                    .map_err(frame_allocation_service_error)?,
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
        let outbound_order = self.outbound_order.lock().await;
        let mut pending = self.pending.lock().await;
        let frame_id =
            allocate_frame_id(&self.next_frame_id).map_err(frame_allocation_service_error)?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        pending.insert(frame_id.0, tx);
        drop(pending);
        if let Err(error) = self
            .placement
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
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
        drop(outbound_order);
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
        let channels = std::mem::take(&mut *self.live_channels.lock().await);
        for (source, sender) in channels {
            let _ = sender.send(Err(AgentLiveStreamError::Unavailable {
                message: format!("{}: {}", source.as_str(), error.message),
            }));
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
        if received.0 == 0 {
            return Err(stale_generation(format!(
                "local binding generation must be positive, received {received:?}"
            )));
        }
        Ok(())
    }

    async fn remember_callback_generation(
        &self,
        route_id: AgentCallbackRouteId,
        generation: AgentBindingGeneration,
    ) -> Result<(), AgentServiceError> {
        self.validate_local_generation(generation)?;
        let mut generations = self.callback_generations.lock().await;
        if let Some(existing) = generations.get(&route_id) {
            return if *existing == generation {
                Ok(())
            } else {
                Err(stale_generation(
                    "callback route was reused with a different local binding generation",
                ))
            };
        }
        generations.insert(route_id, generation);
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
            return result.and_then(|value| {
                validate_command_receipt(
                    &command.meta.command_id,
                    &command.meta.effect_id,
                    command.requested_source.as_ref(),
                    *value,
                )
            });
        }
        let effect_id = command.meta.effect_id.clone();
        let command_id = command.meta.command_id.clone();
        let requested_source = command.requested_source.clone();
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
        }
        .and_then(|receipt| {
            validate_command_receipt(&command_id, &effect_id, requested_source.as_ref(), receipt)
        });
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
            return result.and_then(|value| {
                validate_command_receipt(
                    &command.meta.command_id,
                    &command.meta.effect_id,
                    Some(&command.source),
                    *value,
                )
            });
        }
        let effect_id = command.meta.effect_id.clone();
        let command_id = command.meta.command_id.clone();
        let source = command.source.clone();
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
        }
        .and_then(|receipt| {
            validate_command_receipt(&command_id, &effect_id, Some(&source), receipt)
        });
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
            return result
                .map(|value| *value)
                .and_then(|receipt| validate_fork_receipt(&command, receipt));
        }
        let effect_id = command.meta.effect_id.clone();
        let expected = command.clone();
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
        }
        .and_then(|receipt| validate_fork_receipt(&expected, receipt));
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
            return result.and_then(|value| {
                validate_command_receipt(
                    &command.meta.command_id,
                    &command.meta.effect_id,
                    Some(&command.source),
                    *value,
                )
            });
        }
        let effect_id = command.meta.effect_id.clone();
        let command_id = command.meta.command_id.clone();
        let source = command.source.clone();
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
        }
        .and_then(|receipt| {
            validate_command_receipt(&command_id, &effect_id, Some(&source), receipt)
        });
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

    async fn context(
        &self,
        query: AgentContextQuery,
    ) -> Result<AgentContextSnapshot, AgentServiceError> {
        match self
            .request(RuntimeWireAgentServiceRequest::Context {
                target: self.target.clone(),
                query,
            })
            .await?
        {
            RuntimeWireAgentServiceResponse::Context(result) => result.map(|value| *value),
            _ => Err(protocol("context received a mismatched response")),
        }
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

    async fn live_batches(
        &self,
        source: AgentSourceCoordinate,
    ) -> Result<Box<dyn AgentLiveBatchStream>, AgentServiceError> {
        let receiver = {
            let mut channels = self.live_channels.lock().await;
            channels
                .entry(source.clone())
                .or_insert_with(|| tokio::sync::broadcast::channel(1024).0)
                .subscribe()
        };
        match self
            .request(RuntimeWireAgentServiceRequest::SubscribeLive {
                target: self.target.clone(),
                source: source.clone(),
            })
            .await?
        {
            RuntimeWireAgentServiceResponse::SubscribeLive(result) => {
                let subscribed = result.map(|value| *value)?;
                if subscribed != source {
                    return Err(protocol(
                        "subscribe live received a mismatched source coordinate",
                    ));
                }
            }
            _ => return Err(protocol("subscribe live received a mismatched response")),
        }
        Ok(Box::new(RemoteAgentLiveBatchStream { receiver }))
    }

    async fn inspect(
        &self,
        effect_id: AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, AgentServiceError> {
        let expected_effect_id = effect_id.clone();
        let inspection = match self
            .request(RuntimeWireAgentServiceRequest::Inspect {
                target: self.target.clone(),
                effect_id,
            })
            .await?
        {
            RuntimeWireAgentServiceResponse::Inspect(result) => result.map(|value| *value)?,
            _ => return Err(protocol("inspect received a mismatched response")),
        };
        validate_inspection(&expected_effect_id, inspection)
    }

    async fn apply_surface(
        &self,
        mut command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
        self.remember_callback_generation(
            command.callbacks.route_id.clone(),
            command.callbacks.binding_generation,
        )
        .await?;
        if let Some(RuntimeWireAgentServiceResponse::ApplySurface(result)) =
            self.cached(&command.effect_id).await
        {
            return result
                .map(|value| *value)
                .and_then(|receipt| validate_surface_receipt(&command, receipt));
        }
        let effect_id = command.effect_id.clone();
        let expected = command.clone();
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
        }
        .and_then(|receipt| validate_surface_receipt(&expected, receipt));
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
            return result.and_then(|value| {
                validate_command_receipt(
                    &command.command_id,
                    &command.effect_id,
                    Some(&command.source),
                    *value,
                )
            });
        }
        let effect_id = command.effect_id.clone();
        let command_id = command.command_id.clone();
        let source = command.source.clone();
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
        }
        .and_then(|receipt| {
            validate_command_receipt(&command_id, &effect_id, Some(&source), receipt)
        });
        self.cache(effect_id, response).await;
        result
    }
}

struct RemoteAgentLiveBatchStream {
    receiver: tokio::sync::broadcast::Receiver<Result<AgentLiveBatch, AgentLiveStreamError>>,
}

#[async_trait]
impl AgentLiveBatchStream for RemoteAgentLiveBatchStream {
    async fn next(&mut self) -> Result<Option<AgentLiveBatch>, AgentLiveStreamError> {
        match self.receiver.recv().await {
            Ok(event) => event.map(Some),
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                Err(AgentLiveStreamError::Lagged { skipped })
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                Err(AgentLiveStreamError::Unavailable {
                    message: "remote Complete Agent live lane closed".to_owned(),
                })
            }
        }
    }
}

enum PendingHostCallback {
    Tool(tokio::sync::mpsc::UnboundedSender<RuntimeWireAgentToolExecutionEvent>),
    Hook(tokio::sync::oneshot::Sender<RuntimeWireAgentHostCallbackResponse>),
}

enum ProxyCallbackEffectState {
    InFlight {
        request: RuntimeWireAgentHostCallbackRequest,
        waiting_request_frame_ids: Vec<RuntimeWireFrameId>,
    },
    Settled {
        request: RuntimeWireAgentHostCallbackRequest,
        responses: Vec<RuntimeWireAgentHostCallbackResponse>,
    },
}

/// Source-side reverse callback client backed by one Complete Agent Runtime Wire stream.
#[derive(Clone)]
pub struct RuntimeWireAgentHostCallbackClient {
    target: RuntimeWireAgentBindingTarget,
    next_frame_id: Arc<AtomicU64>,
    pending: Arc<tokio::sync::Mutex<HashMap<u64, PendingHostCallback>>>,
    outbound: Arc<tokio::sync::RwLock<tokio::sync::mpsc::UnboundedSender<RuntimeWireEnvelope>>>,
    outbound_order: Arc<tokio::sync::Mutex<()>>,
}

impl RuntimeWireAgentHostCallbackClient {
    fn failed_tool_stream(
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Box<dyn AgentToolExecutionStream> {
        AgentToolExecutionSequence::completed(AgentToolResult::Failed {
            code: code.into(),
            message: message.into(),
        })
    }

    async fn invoke_hook_callback(
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

        let deadline = callback_deadline(&request)?;
        let _outbound_order = self.outbound_order.lock().await;
        let frame_id =
            allocate_frame_id(&self.next_frame_id).map_err(frame_allocation_callback_error)?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(frame_id.0, PendingHostCallback::Hook(tx));
        if self
            .outbound
            .read()
            .await
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
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
        drop(_outbound_order);
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
    ) -> Result<Box<dyn AgentToolExecutionStream>, AgentHostCallbackError> {
        if call.meta.binding_generation != self.target.binding_generation {
            return Ok(Self::failed_tool_stream(
                "stale_binding_generation",
                "source callback carries a stale endpoint binding generation",
            ));
        }
        let deadline =
            match callback_deadline(&RuntimeWireAgentHostCallbackRequest::Tool(call.clone())) {
                Ok(deadline) => tokio::time::Instant::now() + deadline,
                Err(error) => {
                    return Ok(Self::failed_tool_stream(
                        "tool_callback_deadline_exceeded",
                        error.message,
                    ));
                }
            };
        let _outbound_order = self.outbound_order.lock().await;
        let frame_id = match allocate_frame_id(&self.next_frame_id) {
            Ok(frame_id) => frame_id,
            Err(error) => {
                return Ok(Self::failed_tool_stream(
                    "tool_callback_unavailable",
                    error.to_string(),
                ));
            }
        };
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.pending
            .lock()
            .await
            .insert(frame_id.0, PendingHostCallback::Tool(tx));
        if self
            .outbound
            .read()
            .await
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id,
                critical: true,
                frame: RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::AgentHostCallback(
                    Box::new(RuntimeWireAgentHostCallbackRequest::Tool(call.clone())),
                ))),
            })
            .is_err()
        {
            self.pending.lock().await.remove(&frame_id.0);
            return Ok(Self::failed_tool_stream(
                "tool_callback_unavailable",
                "Complete Agent callback stream is closed",
            ));
        }
        drop(_outbound_order);
        Ok(Box::new(RuntimeWireToolExecutionStream {
            call,
            request_frame_id: frame_id,
            receiver: rx,
            pending: self.pending.clone(),
            deadline,
            started: false,
            last_update_index: 0,
            terminal: false,
            queued: VecDeque::new(),
        }))
    }

    async fn invoke_hook(
        &self,
        call: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        match self
            .invoke_hook_callback(RuntimeWireAgentHostCallbackRequest::Hook(call))
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

struct RuntimeWireToolExecutionStream {
    call: AgentToolInvocation,
    request_frame_id: RuntimeWireFrameId,
    receiver: tokio::sync::mpsc::UnboundedReceiver<RuntimeWireAgentToolExecutionEvent>,
    pending: Arc<tokio::sync::Mutex<HashMap<u64, PendingHostCallback>>>,
    deadline: tokio::time::Instant,
    started: bool,
    last_update_index: u64,
    terminal: bool,
    queued: VecDeque<AgentToolExecutionEvent>,
}

impl RuntimeWireToolExecutionStream {
    fn fail(&mut self, code: &str, message: impl Into<String>) -> AgentToolExecutionEvent {
        self.terminal = true;
        AgentToolExecutionEvent::Completed {
            result: AgentToolResult::Failed {
                code: code.to_owned(),
                message: message.into(),
            },
        }
    }

    fn validate(&mut self, frame: RuntimeWireAgentToolExecutionEvent) -> AgentToolExecutionEvent {
        if frame.effect_id != self.call.meta.effect_id
            || frame.item_id != self.call.meta.item_id
            || frame.tool != self.call.tool
        {
            return self.fail(
                "tool_callback_correlation_mismatch",
                "Runtime Wire tool callback coordinates do not match the request",
            );
        }
        match frame.event {
            AgentToolExecutionEvent::Started if !self.started => {
                self.started = true;
                AgentToolExecutionEvent::Started
            }
            AgentToolExecutionEvent::Progress {
                update_index,
                output,
            } if self.started && self.last_update_index.checked_add(1) == Some(update_index) => {
                self.last_update_index = update_index;
                AgentToolExecutionEvent::Progress {
                    update_index,
                    output,
                }
            }
            AgentToolExecutionEvent::Completed { result } if self.started => {
                self.terminal = true;
                AgentToolExecutionEvent::Completed { result }
            }
            _ => self.fail(
                "tool_callback_protocol_violation",
                "Runtime Wire tool callback lifecycle is out of order",
            ),
        }
    }
}

#[async_trait]
impl AgentToolExecutionStream for RuntimeWireToolExecutionStream {
    async fn next(&mut self) -> Result<Option<AgentToolExecutionEvent>, AgentHostCallbackError> {
        if let Some(event) = self.queued.pop_front() {
            return Ok(Some(event));
        }
        if self.terminal {
            return Ok(None);
        }
        let received = tokio::time::timeout_at(self.deadline, self.receiver.recv()).await;
        let event = match received {
            Ok(Some(frame)) => {
                let event = self.validate(frame);
                if self.terminal {
                    self.pending.lock().await.remove(&self.request_frame_id.0);
                }
                if self.terminal && !self.started {
                    self.started = true;
                    self.queued.push_back(event);
                    AgentToolExecutionEvent::Started
                } else {
                    event
                }
            }
            Ok(None) => {
                if !self.started {
                    self.started = true;
                    let terminal = self.fail(
                        "tool_callback_transport_lost",
                        "Runtime Wire tool callback ended before a terminal event",
                    );
                    self.queued.push_back(terminal);
                    AgentToolExecutionEvent::Started
                } else {
                    self.fail(
                        "tool_callback_transport_lost",
                        "Runtime Wire tool callback ended before a terminal event",
                    )
                }
            }
            Err(_) => {
                self.pending.lock().await.remove(&self.request_frame_id.0);
                if !self.started {
                    self.started = true;
                    let terminal = self.fail(
                        "tool_callback_deadline_exceeded",
                        "Runtime Wire tool callback crossed its absolute deadline",
                    );
                    self.queued.push_back(terminal);
                    AgentToolExecutionEvent::Started
                } else {
                    self.fail(
                        "tool_callback_deadline_exceeded",
                        "Runtime Wire tool callback crossed its absolute deadline",
                    )
                }
            }
        };
        Ok(Some(event))
    }
}

#[derive(Default)]
struct PublishedChangeState {
    last_sequence: Option<u64>,
    cursors: HashSet<agentdash_agent_runtime_contract::AgentSourceCursor>,
}

/// Local Runtime Wire terminator for one concrete Complete Agent implementation.
pub struct RuntimeWireAgentServiceEndpoint {
    service_instance_id: AgentServiceInstanceId,
    binding_generation: AgentBindingGeneration,
    service: Arc<dyn CompleteAgentService>,
    next_frame_id: Arc<AtomicU64>,
    pending_callbacks: Arc<tokio::sync::Mutex<HashMap<u64, PendingHostCallback>>>,
    published_changes: tokio::sync::Mutex<HashMap<AgentSourceCoordinate, PublishedChangeState>>,
    live_subscriptions: Arc<tokio::sync::Mutex<HashSet<AgentSourceCoordinate>>>,
    outbound_tx: Arc<tokio::sync::RwLock<tokio::sync::mpsc::UnboundedSender<RuntimeWireEnvelope>>>,
    outbound_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<RuntimeWireEnvelope>>,
    outbound_order: Arc<tokio::sync::Mutex<()>>,
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
            next_frame_id: Arc::new(AtomicU64::new(0)),
            pending_callbacks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            published_changes: tokio::sync::Mutex::new(HashMap::new()),
            live_subscriptions: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            outbound_tx: Arc::new(tokio::sync::RwLock::new(outbound_tx)),
            outbound_rx: tokio::sync::Mutex::new(outbound_rx),
            outbound_order: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn host_callbacks(&self) -> Arc<dyn AgentHostCallbacks> {
        Arc::new(RuntimeWireAgentHostCallbackClient {
            target: self.target(),
            next_frame_id: self.next_frame_id.clone(),
            pending: self.pending_callbacks.clone(),
            outbound: self.outbound_tx.clone(),
            outbound_order: self.outbound_order.clone(),
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
        let _outbound_order = self.outbound_order.lock().await;
        self.outbound_rx.lock().await.close();
        self.pending_callbacks.lock().await.clear();
    }

    /// Installs a fresh outbound stream after transport reconnection.
    pub async fn reconnect_outbound(&self) {
        let _outbound_order = self.outbound_order.lock().await;
        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        *self.outbound_tx.write().await = outbound_tx;
        *self.outbound_rx.lock().await = outbound_rx;
    }

    async fn subscribe_live(
        &self,
        source: AgentSourceCoordinate,
    ) -> Result<AgentSourceCoordinate, AgentServiceError> {
        {
            let mut subscriptions = self.live_subscriptions.lock().await;
            if !subscriptions.insert(source.clone()) {
                return Ok(source);
            }
        }
        let mut stream = match self.service.live_batches(source.clone()).await {
            Ok(stream) => stream,
            Err(error) => {
                self.live_subscriptions.lock().await.remove(&source);
                return Err(error);
            }
        };
        let target = self.target();
        let outbound = self.outbound_tx.clone();
        let next_frame_id = self.next_frame_id.clone();
        let subscriptions = self.live_subscriptions.clone();
        let outbound_order = self.outbound_order.clone();
        let task_source = source.clone();
        tokio::spawn(async move {
            loop {
                let event = match stream.next().await {
                    Ok(Some(batch)) if batch.source == task_source => {
                        RuntimeWireAgentLiveEvent::Batch {
                            batch: Box::new(batch),
                        }
                    }
                    Ok(Some(_)) => RuntimeWireAgentLiveEvent::Protocol {
                        source: task_source.clone(),
                        message: "Complete Agent live stream changed source coordinate".to_owned(),
                    },
                    Ok(None) => RuntimeWireAgentLiveEvent::Unavailable {
                        source: task_source.clone(),
                        message: "Complete Agent live stream closed".to_owned(),
                    },
                    Err(AgentLiveStreamError::Lagged { skipped }) => {
                        RuntimeWireAgentLiveEvent::Lagged {
                            source: task_source.clone(),
                            skipped,
                        }
                    }
                    Err(AgentLiveStreamError::Protocol { message }) => {
                        RuntimeWireAgentLiveEvent::Protocol {
                            source: task_source.clone(),
                            message,
                        }
                    }
                    Err(AgentLiveStreamError::Unavailable { message }) => {
                        RuntimeWireAgentLiveEvent::Unavailable {
                            source: task_source.clone(),
                            message,
                        }
                    }
                };
                let terminal = !matches!(event, RuntimeWireAgentLiveEvent::Batch { .. });
                let _outbound_order = outbound_order.lock().await;
                let frame_id = match allocate_frame_id(&next_frame_id) {
                    Ok(frame_id) => frame_id,
                    Err(_) => break,
                };
                if outbound
                    .read()
                    .await
                    .send(RuntimeWireEnvelope {
                        protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                        frame_id,
                        critical: true,
                        frame: RuntimeWireFrame::Notification(Box::new(
                            RuntimeWireNotification::AgentLiveBatch(Box::new(
                                RuntimeWireAgentLiveBatchNotification {
                                    target: target.clone(),
                                    event,
                                },
                            )),
                        )),
                    })
                    .is_err()
                {
                    break;
                }
                if terminal {
                    break;
                }
            }
            subscriptions.lock().await.remove(&task_source);
        });
        Ok(source)
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
                payload:
                    agentdash_agent_runtime_contract::AgentChangePayload::SnapshotInvalidated {
                        reason: format!(
                            "Complete Agent source change gap: expected {expected}, received {source_sequence}"
                        ),
                    },
            }
        };
        let cursor = change.cursor.clone();
        let _outbound_order = self.outbound_order.lock().await;
        let frame_id =
            allocate_frame_id(&self.next_frame_id).map_err(frame_allocation_service_error)?;
        self.outbound_tx
            .read()
            .await
            .send(RuntimeWireEnvelope {
                protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                frame_id,
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
        if envelope.protocol_revision != RUNTIME_WIRE_PROTOCOL_REVISION {
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
                let mut pending = self.pending_callbacks.lock().await;
                match response {
                    RuntimeWireAgentHostCallbackResponse::Tool(event) => {
                        let terminal =
                            matches!(event.event, AgentToolExecutionEvent::Completed { .. });
                        let sender = match pending.get(&request_frame_id.0) {
                            Some(PendingHostCallback::Tool(sender)) => sender.clone(),
                            Some(PendingHostCallback::Hook(_)) => {
                                return Err(RemoteRuntimeTransportError::Protocol {
                                    reason:
                                        "tool callback event used a hook callback correlation"
                                            .to_owned(),
                                    critical: true,
                                });
                            }
                            None => return Ok(()),
                        };
                        if terminal {
                            pending.remove(&request_frame_id.0);
                        }
                        let _ = sender.send(*event);
                    }
                    response @ RuntimeWireAgentHostCallbackResponse::Hook(_) => {
                        match pending.remove(&request_frame_id.0) {
                            Some(PendingHostCallback::Hook(sender)) => {
                                let _ = sender.send(response);
                            }
                            Some(PendingHostCallback::Tool(_)) => {
                                return Err(RemoteRuntimeTransportError::Protocol {
                                    reason:
                                        "hook callback response used a tool callback correlation"
                                            .to_owned(),
                                    critical: true,
                                });
                            }
                            None => {}
                        }
                    }
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
                let _outbound_order = self.outbound_order.lock().await;
                let frame_id = allocate_frame_id(&self.next_frame_id).map_err(|error| {
                    RemoteRuntimeTransportError::Protocol {
                        reason: error.to_string(),
                        critical: true,
                    }
                })?;
                self.outbound_tx
                    .read()
                    .await
                    .send(RuntimeWireEnvelope {
                        protocol_revision: RUNTIME_WIRE_PROTOCOL_REVISION,
                        frame_id,
                        critical: true,
                        frame: RuntimeWireFrame::Response {
                            request_frame_id: envelope.frame_id,
                            response: RuntimeWireResponse::AgentService(response),
                        },
                    })
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
            RuntimeWireAgentServiceRequest::Context { query, .. } => {
                RuntimeWireAgentServiceResponse::Context(
                    self.service.context(query).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::Changes { query, .. } => {
                RuntimeWireAgentServiceResponse::Changes(
                    self.service.changes(query).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::SubscribeLive { source, .. } => {
                RuntimeWireAgentServiceResponse::SubscribeLive(
                    self.subscribe_live(source).await.map(Box::new),
                )
            }
            RuntimeWireAgentServiceRequest::Inspect { effect_id, .. } => {
                let expected_effect_id = effect_id.clone();
                let result = self
                    .service
                    .inspect(effect_id)
                    .await
                    .and_then(|inspection| validate_inspection(&expected_effect_id, inspection))
                    .map(Box::new);
                RuntimeWireAgentServiceResponse::Inspect(result)
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

fn validate_inspection(
    expected_effect_id: &AgentEffectIdentity,
    inspection: AgentEffectInspection,
) -> Result<AgentEffectInspection, AgentServiceError> {
    if &inspection.effect_id != expected_effect_id || !inspection.validate() {
        return Err(protocol(
            "remote Complete Agent returned an inconsistent effect inspection",
        ));
    }
    Ok(inspection)
}

fn validate_command_receipt(
    expected_command_id: &agentdash_agent_runtime_contract::AgentCommandId,
    expected_effect_id: &AgentEffectIdentity,
    expected_source: Option<&AgentSourceCoordinate>,
    receipt: AgentCommandReceipt,
) -> Result<AgentCommandReceipt, AgentServiceError> {
    if &receipt.command_id != expected_command_id
        || &receipt.effect_id != expected_effect_id
        || expected_source.is_some_and(|source| source != &receipt.source)
    {
        return Err(protocol(
            "remote Complete Agent returned a command receipt for different coordinates",
        ));
    }
    Ok(receipt)
}

fn validate_fork_receipt(
    command: &ForkAgentCommand,
    receipt: ForkAgentReceipt,
) -> Result<ForkAgentReceipt, AgentServiceError> {
    if receipt.command_id != command.meta.command_id
        || receipt.effect_id != command.meta.effect_id
        || receipt.parent_source != command.source
        || receipt.cutoff != command.cutoff
        || command
            .requested_child_source
            .as_ref()
            .is_some_and(|source| receipt.child_source.as_ref() != Some(source))
    {
        return Err(protocol(
            "remote Complete Agent returned a fork receipt for different coordinates",
        ));
    }
    Ok(receipt)
}

fn validate_surface_receipt(
    command: &ApplyBoundAgentSurface,
    receipt: AppliedAgentSurfaceReceipt,
) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
    if receipt.command_id != command.command_id
        || receipt.effect_id != command.effect_id
        || receipt.source != command.source
        || receipt.applied.revision != command.bound_surface.revision
        || receipt.applied.digest != command.bound_surface.digest
    {
        return Err(protocol(
            "remote Complete Agent returned a surface receipt for different coordinates",
        ));
    }
    Ok(receipt)
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
        RuntimeWireAgentServiceRequest::Context { .. } => {
            RuntimeWireAgentServiceResponse::Context(Err(error))
        }
        RuntimeWireAgentServiceRequest::Changes { .. } => {
            RuntimeWireAgentServiceResponse::Changes(Err(error))
        }
        RuntimeWireAgentServiceRequest::SubscribeLive { .. } => {
            RuntimeWireAgentServiceResponse::SubscribeLive(Err(error))
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
        RuntimeWireAgentServiceResponse::Context(result) => result.is_ok(),
        RuntimeWireAgentServiceResponse::Changes(result) => result.is_ok(),
        RuntimeWireAgentServiceResponse::SubscribeLive(result) => result.is_ok(),
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

fn frame_allocation_service_error(error: RuntimeWireFrameAllocationError) -> AgentServiceError {
    unavailable(error.to_string(), false)
}

fn frame_allocation_callback_error(
    error: RuntimeWireFrameAllocationError,
) -> AgentHostCallbackError {
    host_callback_error(
        AgentHostCallbackErrorCode::Unavailable,
        error.to_string(),
        false,
    )
}

fn callback_failure_responses(
    request: &RuntimeWireAgentHostCallbackRequest,
    tool_code: &str,
    error: AgentHostCallbackError,
) -> Vec<RuntimeWireAgentHostCallbackResponse> {
    match request {
        RuntimeWireAgentHostCallbackRequest::Tool(invocation) => {
            tool_failure_responses(invocation, tool_code, error.message)
        }
        RuntimeWireAgentHostCallbackRequest::Hook(_) => {
            vec![RuntimeWireAgentHostCallbackResponse::Hook(Err(error))]
        }
    }
}

fn tool_failure_responses(
    invocation: &AgentToolInvocation,
    code: &str,
    message: impl Into<String>,
) -> Vec<RuntimeWireAgentHostCallbackResponse> {
    vec![
        tool_event_response(invocation, AgentToolExecutionEvent::Started),
        tool_event_response(
            invocation,
            AgentToolExecutionEvent::Completed {
                result: AgentToolResult::Failed {
                    code: code.to_owned(),
                    message: message.into(),
                },
            },
        ),
    ]
}

fn tool_event_response(
    invocation: &AgentToolInvocation,
    event: AgentToolExecutionEvent,
) -> RuntimeWireAgentHostCallbackResponse {
    RuntimeWireAgentHostCallbackResponse::Tool(Box::new(RuntimeWireAgentToolExecutionEvent {
        effect_id: invocation.meta.effect_id.clone(),
        item_id: invocation.meta.item_id.clone(),
        tool: invocation.tool.clone(),
        event,
    }))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_allocator_uses_the_full_u64_domain_then_reports_exhaustion() {
        let allocator = AtomicU64::new(u64::MAX - 1);

        assert_eq!(
            allocate_frame_id(&allocator),
            Ok(RuntimeWireFrameId(u64::MAX))
        );
        assert_eq!(
            allocate_frame_id(&allocator),
            Err(RuntimeWireFrameAllocationError::Exhausted)
        );
        assert_eq!(allocator.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn frame_allocator_starts_at_one_without_reserving_a_wire_coordinate() {
        let allocator = AtomicU64::new(0);

        assert_eq!(allocate_frame_id(&allocator), Ok(RuntimeWireFrameId(1)));
        assert_eq!(allocate_frame_id(&allocator), Ok(RuntimeWireFrameId(2)));
    }
}
