use std::{
    collections::BTreeSet,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use agentdash_agent_runtime_wire::{
    RUNTIME_WIRE_PROTOCOL_REVISION, RuntimeWireAgentBindingTarget,
    RuntimeWireAgentChangeNotification, RuntimeWireAgentHostCallbackRequest,
    RuntimeWireAgentHostCallbackResponse, RuntimeWireAgentServiceRequest,
    RuntimeWireAgentServiceResponse, RuntimeWireEnvelope, RuntimeWireFrame, RuntimeWireFrameId,
    RuntimeWireNotification, RuntimeWireRequest, RuntimeWireResponse,
};
use agentdash_agent_service_api::{
    AgentAppliedEffectOutcome, AgentBindingGeneration, AgentCallbackRouteId, AgentChange,
    AgentChangePayload, AgentChangesQuery, AgentCommand, AgentCommandEnvelope, AgentCommandId,
    AgentCommandMeta, AgentContentBlock, AgentEffectIdentity, AgentEffectInspection,
    AgentEffectInspectionState, AgentHookAction, AgentHookDefinitionId, AgentHookInvocation,
    AgentHookPoint, AgentHookTiming, AgentHostCallbackBinding, AgentHostCallbackError,
    AgentHostCallbackMeta, AgentHostCallbacks, AgentIdempotencyKey, AgentInput, AgentInputContent,
    AgentItemBody, AgentItemId, AgentItemPresentation, AgentItemTransition, AgentLifecycleStatus,
    AgentProfileDigest, AgentReadQuery, AgentReceiptState, AgentServiceError,
    AgentServiceErrorCode, AgentServiceInstanceId, AgentSourceCoordinate, AgentSourceCursor,
    AgentSurfaceDigest, AgentSurfaceRevision, AgentSurfaceRoute, AgentToolInvocation,
    AgentToolName, AgentToolResult, AgentTurnId, AppliedAgentCommandReceipt, AppliedAgentSurface,
    AppliedAgentSurfaceReceipt, ApplyBoundAgentSurface, BoundAgentSurface, CompleteAgentService,
};
use agentdash_integration_remote_runtime::{
    RemoteCompleteAgentRegistration, RemoteCompleteAgentService, RemoteRuntimeTransportError,
    RuntimeWireAgentServiceEndpoint, RuntimeWirePlacement, RuntimeWirePlacementEvent,
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
    mismatch_execute_coordinates: AtomicBool,
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
            mismatch_execute_coordinates: AtomicBool::new(false),
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
        let response = match *request {
            RuntimeWireAgentServiceRequest::Execute { target, command } => {
                self.execute_requests.fetch_add(1, Ordering::Relaxed);
                assert_eq!(target.binding_generation, AgentBindingGeneration(9));
                assert_eq!(
                    command.meta.binding_generation,
                    AgentBindingGeneration(9),
                    "proxy must rewrite only at the Runtime Wire boundary"
                );
                RuntimeWireAgentServiceResponse::Execute(Ok(Box::new(
                    agentdash_agent_service_api::AgentCommandReceipt {
                        command_id: command.meta.command_id,
                        effect_id: command.meta.effect_id,
                        source: if self.mismatch_execute_coordinates.load(Ordering::Relaxed) {
                            AgentSourceCoordinate::new("different-source").expect("source")
                        } else {
                            command.source
                        },
                        state: AgentReceiptState::Accepted,
                        snapshot_revision: None,
                        initial_context: None,
                    },
                )))
            }
            RuntimeWireAgentServiceRequest::Inspect { effect_id, .. } => {
                let command_id = AgentCommandId::new("inconsistent-command").expect("command");
                RuntimeWireAgentServiceResponse::Inspect(Ok(Box::new(AgentEffectInspection {
                    effect_id: effect_id.clone(),
                    command_id: Some(command_id.clone()),
                    state: AgentEffectInspectionState::Applied {
                        outcome: AgentAppliedEffectOutcome::Command {
                            receipt: AppliedAgentCommandReceipt {
                                command_id,
                                effect_id: AgentEffectIdentity::new("different-effect")
                                    .expect("effect"),
                                source: AgentSourceCoordinate::new("thread-1").expect("source"),
                                terminal: None,
                                snapshot_revision: None,
                                initial_context: None,
                            },
                        },
                    },
                })))
            }
            RuntimeWireAgentServiceRequest::ApplySurface { target, command } => {
                assert_eq!(target.binding_generation, AgentBindingGeneration(9));
                assert_eq!(
                    command.callbacks.binding_generation,
                    AgentBindingGeneration(9),
                    "proxy must rewrite callback generation at the Runtime Wire boundary"
                );
                RuntimeWireAgentServiceResponse::ApplySurface(Ok(Box::new(
                    AppliedAgentSurfaceReceipt {
                        command_id: command.command_id,
                        effect_id: command.effect_id,
                        source: command.source,
                        applied: AppliedAgentSurface {
                            revision: command.bound_surface.revision,
                            digest: command.bound_surface.digest,
                            contributions: Vec::new(),
                        },
                    },
                )))
            }
            _ => return Ok(()),
        };
        self.inject(
            true,
            RuntimeWireFrame::Response {
                request_frame_id: envelope.frame_id,
                response: RuntimeWireResponse::AgentService(response),
            },
        );
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
        call: agentdash_agent_service_api::AgentHookInvocation,
    ) -> Result<agentdash_agent_service_api::AgentHookDecision, AgentHostCallbackError> {
        self.generations
            .lock()
            .await
            .push(call.meta.binding_generation);
        Ok(agentdash_agent_service_api::AgentHookDecision::Allow)
    }
}

#[derive(Default)]
struct ReentrantCallbacks {
    nested: OnceLock<Arc<dyn AgentHostCallbacks>>,
    tools: Mutex<Vec<String>>,
}

impl ReentrantCallbacks {
    fn bind_nested(&self, callbacks: Arc<dyn AgentHostCallbacks>) {
        self.nested
            .set(callbacks)
            .map_err(|_| ())
            .expect("bind nested callback client once");
    }
}

#[async_trait]
impl AgentHostCallbacks for ReentrantCallbacks {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.tools.lock().await.push(call.tool.as_str().to_owned());
        if call.tool.as_str() == "endpoint-tool" {
            self.nested
                .get()
                .expect("nested callback client")
                .invoke_tool(AgentToolInvocation {
                    meta: AgentHostCallbackMeta {
                        binding_generation: AgentBindingGeneration(9),
                        effect_id: AgentEffectIdentity::new("nested-effect").expect("effect"),
                        idempotency_key: AgentIdempotencyKey::new("nested-idempotency")
                            .expect("idempotency"),
                        deadline_at_ms: deadline_after_ms(1_000),
                        ..call.meta
                    },
                    tool: AgentToolName::new("nested-tool").expect("tool"),
                    arguments: json!({"nested": true}),
                })
                .await?;
        }
        Ok(AgentToolResult::Completed {
            output: json!({"ok": true}),
        })
    }

    async fn invoke_hook(
        &self,
        _: AgentHookInvocation,
    ) -> Result<agentdash_agent_service_api::AgentHookDecision, AgentHostCallbackError> {
        Ok(agentdash_agent_service_api::AgentHookDecision::Allow)
    }
}

#[derive(Default)]
struct BlockingCallbacks {
    invocations: AtomicU64,
}

#[async_trait]
impl AgentHostCallbacks for BlockingCallbacks {
    async fn invoke_tool(
        &self,
        _: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.invocations.fetch_add(1, Ordering::Relaxed);
        std::future::pending().await
    }

    async fn invoke_hook(
        &self,
        _: AgentHookInvocation,
    ) -> Result<agentdash_agent_service_api::AgentHookDecision, AgentHostCallbackError> {
        self.invocations.fetch_add(1, Ordering::Relaxed);
        std::future::pending().await
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

fn deadline_after_ms(offset: u64) -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64
        + offset
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

async fn apply_callback_route(proxy: &RemoteCompleteAgentService, route: &str, generation: u64) {
    proxy
        .apply_surface(ApplyBoundAgentSurface {
            command_id: AgentCommandId::new(format!("apply-{route}")).expect("command"),
            effect_id: AgentEffectIdentity::new(format!("apply-effect-{route}")).expect("effect"),
            idempotency_key: AgentIdempotencyKey::new(format!("apply-idem-{route}"))
                .expect("idempotency"),
            source: AgentSourceCoordinate::new("thread-1").expect("source"),
            bound_surface: BoundAgentSurface {
                revision: AgentSurfaceRevision(1),
                digest: AgentSurfaceDigest::new(format!("surface-{route}")).expect("digest"),
                offer_profile_digest: AgentProfileDigest::new("profile").expect("profile"),
                contributions: Vec::new(),
            },
            callbacks: AgentHostCallbackBinding {
                route_id: AgentCallbackRouteId::new(route).expect("route"),
                binding_generation: AgentBindingGeneration(generation),
                delivery: AgentSurfaceRoute::AgentNativeCallback,
                default_deadline_ms: u64::MAX,
            },
        })
        .await
        .expect("apply callback route");
}

fn tool_invocation(
    effect: &str,
    generation: u64,
    deadline_at_ms: u64,
    tool: &str,
) -> AgentToolInvocation {
    AgentToolInvocation {
        meta: AgentHostCallbackMeta {
            route_id: AgentCallbackRouteId::new(format!("route-{effect}")).expect("route"),
            binding_generation: AgentBindingGeneration(generation),
            source: AgentSourceCoordinate::new("thread-1").expect("source"),
            turn_id: AgentTurnId::new("turn-1").expect("turn"),
            item_id: None,
            interaction_id: None,
            effect_id: AgentEffectIdentity::new(effect).expect("effect"),
            idempotency_key: AgentIdempotencyKey::new(format!("idempotency-{effect}"))
                .expect("idempotency"),
            deadline_at_ms,
        },
        tool: AgentToolName::new(tool).expect("tool"),
        arguments: json!({"effect": effect}),
    }
}

#[derive(Default)]
struct EndpointTracerService {
    callbacks: OnceLock<Arc<dyn AgentHostCallbacks>>,
    executions: AtomicU64,
    callback_results: Mutex<Vec<AgentToolResult>>,
    inspection: Mutex<Option<AgentEffectInspection>>,
}

impl EndpointTracerService {
    fn bind_callbacks(&self, callbacks: Arc<dyn AgentHostCallbacks>) {
        self.callbacks
            .set(callbacks)
            .map_err(|_| ())
            .expect("bind endpoint callbacks once");
    }

    fn unsupported() -> AgentServiceError {
        AgentServiceError::new(
            AgentServiceErrorCode::Unsupported,
            "unused tracer operation",
            false,
        )
    }
}

#[async_trait]
impl CompleteAgentService for EndpointTracerService {
    async fn describe(
        &self,
    ) -> Result<agentdash_agent_service_api::AgentServiceDescriptor, AgentServiceError> {
        Err(Self::unsupported())
    }

    async fn create(
        &self,
        _: agentdash_agent_service_api::CreateAgentCommand,
    ) -> Result<agentdash_agent_service_api::AgentCommandReceipt, AgentServiceError> {
        Err(Self::unsupported())
    }

    async fn resume(
        &self,
        _: agentdash_agent_service_api::ResumeAgentCommand,
    ) -> Result<agentdash_agent_service_api::AgentCommandReceipt, AgentServiceError> {
        Err(Self::unsupported())
    }

    async fn fork(
        &self,
        _: agentdash_agent_service_api::ForkAgentCommand,
    ) -> Result<agentdash_agent_service_api::ForkAgentReceipt, AgentServiceError> {
        Err(Self::unsupported())
    }

    async fn execute(
        &self,
        command: AgentCommandEnvelope,
    ) -> Result<agentdash_agent_service_api::AgentCommandReceipt, AgentServiceError> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        let callbacks = self.callbacks.get().expect("endpoint callbacks");
        let callback_meta = AgentHostCallbackMeta {
            route_id: AgentCallbackRouteId::new("endpoint-route").expect("route"),
            binding_generation: command.meta.binding_generation,
            source: command.source.clone(),
            turn_id: AgentTurnId::new("endpoint-turn").expect("turn"),
            item_id: None,
            interaction_id: None,
            effect_id: AgentEffectIdentity::new("endpoint-tool-effect").expect("effect"),
            idempotency_key: AgentIdempotencyKey::new("endpoint-tool-idempotency")
                .expect("idempotency"),
            deadline_at_ms: u64::MAX,
        };
        let tool = AgentToolInvocation {
            meta: callback_meta.clone(),
            tool: AgentToolName::new("endpoint-tool").expect("tool"),
            arguments: json!({"value": 1}),
        };
        let (first, replay) = tokio::join!(
            callbacks.invoke_tool(tool.clone()),
            callbacks.invoke_tool(tool)
        );
        let first = first.map_err(|error| {
            AgentServiceError::new(
                AgentServiceErrorCode::Unavailable,
                error.to_string(),
                error.retryable,
            )
        })?;
        let replay = replay.map_err(|error| {
            AgentServiceError::new(
                AgentServiceErrorCode::Unavailable,
                error.to_string(),
                error.retryable,
            )
        })?;
        assert_eq!(first, replay, "endpoint must replay the callback result");
        self.callback_results.lock().await.push(first);

        callbacks
            .invoke_hook(AgentHookInvocation {
                meta: AgentHostCallbackMeta {
                    effect_id: AgentEffectIdentity::new("endpoint-hook-effect").expect("effect"),
                    idempotency_key: AgentIdempotencyKey::new("endpoint-hook-idempotency")
                        .expect("idempotency"),
                    ..callback_meta
                },
                definition_id: AgentHookDefinitionId::new("endpoint-hook").expect("hook"),
                point: AgentHookPoint::BeforeTurn,
                timing: AgentHookTiming::Before,
                allowed_actions: BTreeSet::from([AgentHookAction::AllowOrDeny]),
                input: json!({"value": 2}),
            })
            .await
            .map_err(|error| {
                AgentServiceError::new(
                    AgentServiceErrorCode::Unavailable,
                    error.to_string(),
                    error.retryable,
                )
            })?;

        let receipt = agentdash_agent_service_api::AgentCommandReceipt {
            command_id: command.meta.command_id,
            effect_id: command.meta.effect_id,
            source: command.source,
            state: AgentReceiptState::Accepted,
            snapshot_revision: None,
            initial_context: None,
        };
        *self.inspection.lock().await = Some(AgentEffectInspection {
            effect_id: receipt.effect_id.clone(),
            command_id: Some(receipt.command_id.clone()),
            state: AgentEffectInspectionState::Applied {
                outcome: AgentAppliedEffectOutcome::Command {
                    receipt: AppliedAgentCommandReceipt {
                        command_id: receipt.command_id.clone(),
                        effect_id: receipt.effect_id.clone(),
                        source: receipt.source.clone(),
                        terminal: None,
                        snapshot_revision: None,
                        initial_context: None,
                    },
                },
            },
        });
        Ok(receipt)
    }

    async fn read(
        &self,
        _: AgentReadQuery,
    ) -> Result<agentdash_agent_service_api::AgentSnapshot, AgentServiceError> {
        Err(Self::unsupported())
    }

    async fn changes(
        &self,
        query: AgentChangesQuery,
    ) -> Result<agentdash_agent_service_api::AgentChangePage, AgentServiceError> {
        Ok(agentdash_agent_service_api::AgentChangePage {
            source: query.source,
            changes: Vec::new(),
            next: query.after,
            gap: false,
        })
    }

    async fn inspect(
        &self,
        effect_id: AgentEffectIdentity,
    ) -> Result<agentdash_agent_service_api::AgentEffectInspection, AgentServiceError> {
        self.inspection
            .lock()
            .await
            .clone()
            .filter(|inspection| inspection.effect_id == effect_id)
            .ok_or_else(Self::unsupported)
    }

    async fn apply_surface(
        &self,
        command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
        Ok(AppliedAgentSurfaceReceipt {
            command_id: command.command_id,
            effect_id: command.effect_id,
            source: command.source,
            applied: AppliedAgentSurface {
                revision: command.bound_surface.revision,
                digest: command.bound_surface.digest,
                contributions: Vec::new(),
            },
        })
    }

    async fn revoke_surface(
        &self,
        _: agentdash_agent_service_api::RevokeBoundAgentSurface,
    ) -> Result<agentdash_agent_service_api::AgentCommandReceipt, AgentServiceError> {
        Err(Self::unsupported())
    }
}

fn endpoint_tracer() -> (
    Arc<EndpointTracerService>,
    Arc<RuntimeWireAgentServiceEndpoint>,
    Arc<RecordingCallbacks>,
    Arc<RemoteCompleteAgentService>,
) {
    let host_callbacks = Arc::new(RecordingCallbacks::default());
    let (source_service, endpoint, proxy) = endpoint_tracer_with_callbacks(host_callbacks.clone());
    (source_service, endpoint, host_callbacks, proxy)
}

fn endpoint_tracer_with_callbacks(
    host_callbacks: Arc<dyn AgentHostCallbacks>,
) -> (
    Arc<EndpointTracerService>,
    Arc<RuntimeWireAgentServiceEndpoint>,
    Arc<RemoteCompleteAgentService>,
) {
    let source_service = Arc::new(EndpointTracerService::default());
    let endpoint = Arc::new(RuntimeWireAgentServiceEndpoint::new(
        target().service_instance_id,
        AgentBindingGeneration(9),
        source_service.clone(),
    ));
    source_service.bind_callbacks(endpoint.host_callbacks());
    let proxy =
        RemoteCompleteAgentService::new(endpoint.target(), endpoint.clone(), host_callbacks);
    (source_service, endpoint, proxy)
}

#[tokio::test]
async fn registration_preserves_caller_service_identity_and_complete_agent_target() {
    let callbacks = Arc::new(RecordingCallbacks::default());
    let source_service = Arc::new(EndpointTracerService::default());
    let endpoint = Arc::new(RuntimeWireAgentServiceEndpoint::new(
        target().service_instance_id,
        AgentBindingGeneration(9),
        source_service,
    ));
    let registration = RemoteCompleteAgentRegistration::new(
        AgentServiceInstanceId::new("enterprise-agent").expect("instance"),
        endpoint.target(),
        endpoint,
        callbacks,
    );

    assert_eq!(registration.instance_id().as_str(), "enterprise-agent");
    let (instance_id, _) = registration.into_parts();
    assert_eq!(instance_id.as_str(), "enterprise-agent");
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
async fn real_endpoint_round_trips_tool_hook_and_replays_duplicate_callback_result() {
    let (source_service, endpoint, host_callbacks, proxy) = endpoint_tracer();
    apply_callback_route(&proxy, "endpoint-route", 3).await;
    let command = execute("endpoint-roundtrip", 3);
    let effect_id = command.meta.effect_id.clone();

    let first = proxy
        .execute(command.clone())
        .await
        .expect("remote execute");
    let replay = proxy.execute(command).await.expect("remote execute replay");
    let inspection = proxy.inspect(effect_id).await.expect("remote inspect");

    assert_eq!(first, replay);
    assert!(inspection.validate());
    assert!(matches!(
        inspection.state,
        AgentEffectInspectionState::Applied {
            outcome: AgentAppliedEffectOutcome::Command { .. }
        }
    ));
    assert_eq!(source_service.executions.load(Ordering::Relaxed), 1);
    assert_eq!(source_service.callback_results.lock().await.len(), 1);
    assert_eq!(
        host_callbacks.generations.lock().await.as_slice(),
        &[AgentBindingGeneration(3), AgentBindingGeneration(3)],
        "duplicate tool callback must reuse the endpoint result while hook remains a distinct request"
    );

    let stale = endpoint
        .host_callbacks()
        .invoke_tool(AgentToolInvocation {
            meta: AgentHostCallbackMeta {
                route_id: AgentCallbackRouteId::new("stale-route").expect("route"),
                binding_generation: AgentBindingGeneration(8),
                source: AgentSourceCoordinate::new("thread-1").expect("source"),
                turn_id: AgentTurnId::new("turn-1").expect("turn"),
                item_id: None,
                interaction_id: None,
                effect_id: AgentEffectIdentity::new("stale-effect").expect("effect"),
                idempotency_key: AgentIdempotencyKey::new("stale-idempotency")
                    .expect("idempotency"),
                deadline_at_ms: u64::MAX,
            },
            tool: AgentToolName::new("stale-tool").expect("tool"),
            arguments: json!({}),
        })
        .await
        .expect_err("stale source generation");
    assert_eq!(
        stale.code,
        agentdash_agent_service_api::AgentHostCallbackErrorCode::StaleBindingGeneration
    );
    assert_eq!(host_callbacks.generations.lock().await.len(), 2);
}

#[tokio::test]
async fn callback_effects_are_reentrant_and_different_effects_do_not_share_an_await_lock() {
    let host_callbacks = Arc::new(ReentrantCallbacks::default());
    let (source_service, endpoint, proxy) = endpoint_tracer_with_callbacks(host_callbacks.clone());
    host_callbacks.bind_nested(endpoint.host_callbacks());
    apply_callback_route(&proxy, "endpoint-route", 3).await;

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        proxy.execute(execute("reentrant", 3)),
    )
    .await
    .expect("nested callback must not deadlock")
    .expect("reentrant execute");

    assert_eq!(source_service.executions.load(Ordering::Relaxed), 1);
    assert_eq!(
        host_callbacks.tools.lock().await.as_slice(),
        &["endpoint-tool".to_owned(), "nested-tool".to_owned()]
    );
}

#[tokio::test]
async fn source_callback_deadline_clears_pending_and_replays_typed_timeout() {
    let source_service = Arc::new(EndpointTracerService::default());
    let endpoint = Arc::new(RuntimeWireAgentServiceEndpoint::new(
        target().service_instance_id,
        AgentBindingGeneration(9),
        source_service,
    ));
    let callbacks = endpoint.host_callbacks();
    let call = tool_invocation(
        "source-deadline-effect",
        9,
        deadline_after_ms(20),
        "deadline-tool",
    );

    let error = callbacks
        .invoke_tool(call.clone())
        .await
        .expect_err("missing callback response must time out");
    assert_eq!(
        error.code,
        agentdash_agent_service_api::AgentHostCallbackErrorCode::DeadlineExceeded
    );
    let outbound = endpoint.receive().await.expect("one callback request");
    assert!(matches!(
        outbound,
        RuntimeWirePlacementEvent::Frame(envelope)
            if matches!(
                &envelope.frame,
                RuntimeWireFrame::Request(request)
                    if matches!(**request, RuntimeWireRequest::AgentHostCallback(_))
            )
    ));

    let replay = callbacks
        .invoke_tool(call)
        .await
        .expect_err("deadline result is stable by effect");
    assert_eq!(
        replay.code,
        agentdash_agent_service_api::AgentHostCallbackErrorCode::DeadlineExceeded
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), endpoint.receive())
            .await
            .is_err(),
        "settled timeout must not emit a duplicate callback request"
    );
}

#[tokio::test]
async fn proxy_deadline_and_effect_ledger_prevent_duplicate_host_side_effects() {
    let placement = LoopbackPlacement::new();
    let host = Arc::new(BlockingCallbacks::default());
    let proxy = RemoteCompleteAgentService::new(target(), placement.clone(), host.clone());
    apply_callback_route(&proxy, "route-proxy-deadline-effect", 3).await;
    let call = RuntimeWireAgentHostCallbackRequest::Tool(tool_invocation(
        "proxy-deadline-effect",
        9,
        deadline_after_ms(20),
        "blocked-tool",
    ));
    placement.inject(
        true,
        RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::AgentHostCallback(Box::new(
            call.clone(),
        )))),
    );
    placement.inject(
        true,
        RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::AgentHostCallback(Box::new(
            call.clone(),
        )))),
    );
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    wait_until(|| {
        placement.sent.try_lock().is_ok_and(|sent| {
            sent.iter()
                .filter(|frame| matches!(
                    &frame.frame,
                    RuntimeWireFrame::Response {
                        response: RuntimeWireResponse::AgentHostCallback(
                            RuntimeWireAgentHostCallbackResponse::Tool(Err(error))
                        ),
                        ..
                    } if error.code
                        == agentdash_agent_service_api::AgentHostCallbackErrorCode::DeadlineExceeded
                ))
                .count()
                >= 2
        })
    })
    .await;
    assert_eq!(host.invocations.load(Ordering::Relaxed), 1);

    placement.inject(
        true,
        RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::AgentHostCallback(Box::new(
            call.clone(),
        )))),
    );
    wait_until(|| {
        placement.sent.try_lock().is_ok_and(|sent| {
            sent.iter()
                .filter(|frame| matches!(
                    &frame.frame,
                    RuntimeWireFrame::Response {
                        response: RuntimeWireResponse::AgentHostCallback(
                            RuntimeWireAgentHostCallbackResponse::Tool(Err(error))
                        ),
                        ..
                    } if error.code
                        == agentdash_agent_service_api::AgentHostCallbackErrorCode::DeadlineExceeded
                ))
                .count()
                >= 3
        })
    })
    .await;
    assert_eq!(
        host.invocations.load(Ordering::Relaxed),
        1,
        "new-frame replay must use the proxy effect ledger"
    );

    let mut conflict = match call {
        RuntimeWireAgentHostCallbackRequest::Tool(call) => call,
        RuntimeWireAgentHostCallbackRequest::Hook(_) => unreachable!(),
    };
    conflict.arguments = json!({"different": true});
    placement.inject(
        true,
        RuntimeWireFrame::Request(Box::new(RuntimeWireRequest::AgentHostCallback(Box::new(
            RuntimeWireAgentHostCallbackRequest::Tool(conflict),
        )))),
    );
    wait_until(|| {
        placement.sent.try_lock().is_ok_and(|sent| {
            sent.iter().any(|frame| matches!(
                &frame.frame,
                RuntimeWireFrame::Response {
                    response: RuntimeWireResponse::AgentHostCallback(
                        RuntimeWireAgentHostCallbackResponse::Tool(Err(error))
                    ),
                    ..
                } if error.code
                    == agentdash_agent_service_api::AgentHostCallbackErrorCode::DuplicateConflict
            ))
        })
    })
    .await;
    assert_eq!(host.invocations.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn real_endpoint_change_producer_deduplicates_cursor_and_surfaces_source_gap() {
    let (_, endpoint, _, proxy) = endpoint_tracer();
    let source = AgentSourceCoordinate::new("thread-1").expect("source");
    let first = AgentChange {
        cursor: AgentSourceCursor::new("cursor-1").expect("cursor"),
        source_revision: None,
        occurred_at_ms: 1,
        payload: AgentChangePayload::ItemTransitioned {
            turn_id: AgentTurnId::new("turn-1").expect("turn"),
            item_id: AgentItemId::new("item-1").expect("item"),
            transition: AgentItemTransition::Started {
                presentation: AgentItemPresentation::new(
                    AgentItemBody::AgentMessage {
                        content: vec![AgentContentBlock::Text {
                            text: "started".to_owned(),
                        }],
                        phase: None,
                    },
                    Some(1),
                    Some(1),
                    None,
                )
                .expect("presentation"),
            },
        },
    };
    endpoint
        .publish_change(1, source.clone(), first.clone())
        .await
        .expect("publish first");
    endpoint
        .publish_change(1, source.clone(), first)
        .await
        .expect("duplicate cursor");

    let first_page = {
        let mut observed = None;
        for _ in 0..100 {
            let page = proxy
                .changes(AgentChangesQuery {
                    source: source.clone(),
                    after: None,
                    limit: 16,
                })
                .await
                .expect("changes");
            if page.changes.len() == 1 {
                observed = Some(page);
                break;
            }
            tokio::task::yield_now().await;
        }
        observed.expect("endpoint notification reached proxy")
    };
    assert!(!first_page.gap);
    assert_eq!(first_page.changes.len(), 1);
    assert!(matches!(
        first_page.changes[0].payload,
        AgentChangePayload::ItemTransitioned { .. }
    ));

    endpoint
        .publish_change(
            3,
            source.clone(),
            AgentChange {
                cursor: AgentSourceCursor::new("cursor-3").expect("cursor"),
                source_revision: None,
                occurred_at_ms: 3,
                payload: AgentChangePayload::LifecycleChanged {
                    status: AgentLifecycleStatus::Closed,
                },
            },
        )
        .await
        .expect("publish source gap");

    let gap = {
        let mut observed = None;
        for _ in 0..100 {
            let page = proxy
                .changes(AgentChangesQuery {
                    source: source.clone(),
                    after: first_page.next.clone(),
                    limit: 16,
                })
                .await
                .expect("changes after gap");
            if page.gap {
                observed = Some(page);
                break;
            }
            tokio::task::yield_now().await;
        }
        observed.expect("source gap reached proxy")
    };
    assert!(gap.gap);
    assert!(gap.changes.is_empty());
    assert_eq!(
        gap.next.as_ref().map(AgentSourceCursor::as_str),
        Some("cursor-3")
    );
}

#[tokio::test]
async fn failed_change_send_does_not_commit_sequence_or_cursor_before_retry() {
    let endpoint = Arc::new(RuntimeWireAgentServiceEndpoint::new(
        target().service_instance_id,
        AgentBindingGeneration(9),
        Arc::new(EndpointTracerService::default()),
    ));
    let source = AgentSourceCoordinate::new("thread-1").expect("source");
    let change = AgentChange {
        cursor: AgentSourceCursor::new("retry-cursor-1").expect("cursor"),
        source_revision: None,
        occurred_at_ms: 1,
        payload: AgentChangePayload::LifecycleChanged {
            status: AgentLifecycleStatus::Active,
        },
    };
    endpoint.disconnect_outbound().await;
    endpoint
        .publish_change(1, source.clone(), change.clone())
        .await
        .expect_err("closed receiver must reject the change");

    endpoint.reconnect_outbound().await;
    endpoint
        .publish_change(1, source, change)
        .await
        .expect("same change must be emitted after reconnect");
    let notification = endpoint.receive().await.expect("retried notification");
    assert!(matches!(
        notification,
        RuntimeWirePlacementEvent::Frame(envelope)
            if matches!(
                &envelope.frame,
                RuntimeWireFrame::Notification(notification)
                    if matches!(**notification, RuntimeWireNotification::AgentChange(_))
            )
    ));
}

#[tokio::test]
async fn proxy_rewrites_generation_and_replays_completed_duplicate_without_second_effect() {
    let placement = LoopbackPlacement::new();
    let callbacks = Arc::new(RecordingCallbacks::default());
    let proxy = RemoteCompleteAgentService::new(target(), placement.clone(), callbacks);
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
async fn recovered_local_generation_is_rewritten_and_zero_is_fenced() {
    let placement = LoopbackPlacement::new();
    let proxy = RemoteCompleteAgentService::new(
        target(),
        placement.clone(),
        Arc::new(RecordingCallbacks::default()),
    );

    proxy
        .execute(execute("recovered", 4))
        .await
        .expect("Host-fenced recovered generation");
    let error = proxy
        .execute(execute("zero", 0))
        .await
        .expect_err("zero generation");

    assert_eq!(error.code, AgentServiceErrorCode::StaleBindingGeneration);
    assert_eq!(placement.execute_requests.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn proxy_rejects_a_success_receipt_for_different_command_coordinates() {
    let placement = LoopbackPlacement::new();
    placement
        .mismatch_execute_coordinates
        .store(true, Ordering::Relaxed);
    let proxy = RemoteCompleteAgentService::new(
        target(),
        placement,
        Arc::new(RecordingCallbacks::default()),
    );

    let error = proxy
        .execute(execute("mismatched-receipt", 3))
        .await
        .expect_err("foreign source receipt must be rejected");

    assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
}

#[tokio::test]
async fn proxy_rejects_an_inconsistent_typed_inspection() {
    let placement = LoopbackPlacement::new();
    let proxy = RemoteCompleteAgentService::new(
        target(),
        placement,
        Arc::new(RecordingCallbacks::default()),
    );

    let error = proxy
        .inspect(AgentEffectIdentity::new("inspect-effect").expect("effect"))
        .await
        .expect_err("inconsistent inspection must be rejected");

    assert_eq!(error.code, AgentServiceErrorCode::ProtocolViolation);
}

#[tokio::test]
async fn pushed_change_replay_is_idempotent_and_frame_gap_loses_the_binding() {
    let placement = LoopbackPlacement::new();
    let proxy = RemoteCompleteAgentService::new(
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
    let proxy = RemoteCompleteAgentService::new(target(), placement.clone(), callbacks.clone());
    apply_callback_route(&proxy, "route-1", 3).await;
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
            request_frame_id: RuntimeWireFrameId(2),
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
