use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentHookAction, AgentHookDecision,
    AgentHookInvocation, AgentHostCallbackBinding, AgentHostCallbackError,
    AgentHostCallbackErrorCode, AgentHostCallbacks, AgentSourceCoordinate,
    AgentSurfaceContributionPayload, AgentSurfaceRoute, AgentToolInvocation, AgentToolResult,
    BoundAgentSurface,
};
use async_trait::async_trait;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

#[async_trait]
pub trait CompleteAgentToolHandler: Send + Sync {
    async fn invoke(
        &self,
        invocation: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError>;
}

#[async_trait]
pub trait CompleteAgentHookHandler: Send + Sync {
    async fn invoke(
        &self,
        invocation: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError>;
}

pub trait AgentCallbackClock: Send + Sync {
    fn now_ms(&self) -> u64;
}

#[derive(Debug, Default)]
pub struct SystemAgentCallbackClock;

impl AgentCallbackClock for SystemAgentCallbackClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentCallbackRoute {
    pub route_id: AgentCallbackRouteId,
    pub generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub delivery: AgentSurfaceRoute,
    pub default_deadline_ms: u64,
    pub bound_surface: BoundAgentSurface,
}

impl CompleteAgentCallbackRoute {
    pub fn from_binding(
        binding: AgentHostCallbackBinding,
        source: AgentSourceCoordinate,
        bound_surface: BoundAgentSurface,
    ) -> Result<Self, AgentHostCallbackError> {
        if binding.delivery != AgentSurfaceRoute::AgentNativeCallback
            || binding.default_deadline_ms == 0
        {
            return Err(callback_error(
                AgentHostCallbackErrorCode::InvalidArgument,
                "reverse callback binding requires Agent-native delivery and a positive deadline",
                false,
            ));
        }
        Ok(Self {
            route_id: binding.route_id,
            generation: binding.binding_generation,
            source,
            delivery: binding.delivery,
            default_deadline_ms: binding.default_deadline_ms,
            bound_surface,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CallbackKey {
    route_id: AgentCallbackRouteId,
    idempotency_key: agentdash_agent_service_api::AgentIdempotencyKey,
}

#[derive(Debug, Clone)]
enum CachedCallback {
    Tool {
        request_digest: String,
        result: Result<AgentToolResult, AgentHostCallbackError>,
    },
    Hook {
        request_digest: String,
        result: Result<AgentHookDecision, AgentHostCallbackError>,
    },
}

#[derive(Default)]
struct CallbackState {
    routes: BTreeMap<AgentCallbackRouteId, CompleteAgentCallbackRoute>,
    results: BTreeMap<CallbackKey, CachedCallback>,
    locks: BTreeMap<CallbackKey, Arc<Mutex<()>>>,
}

/// Reverse callback broker for Agent-native Tool/Hook execution.
///
/// Each registered route is bound to one source and generation. Per-idempotency-key serialization
/// persists both successful and failed outcomes, so duplicate/replayed calls do not execute the
/// platform handler twice.
pub struct CompleteAgentCallbackBroker {
    tool_handler: Arc<dyn CompleteAgentToolHandler>,
    hook_handler: Arc<dyn CompleteAgentHookHandler>,
    clock: Arc<dyn AgentCallbackClock>,
    state: Mutex<CallbackState>,
}

impl CompleteAgentCallbackBroker {
    pub fn new(
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
    ) -> Self {
        Self::with_clock(
            tool_handler,
            hook_handler,
            Arc::new(SystemAgentCallbackClock),
        )
    }

    pub fn with_clock(
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
        clock: Arc<dyn AgentCallbackClock>,
    ) -> Self {
        Self {
            tool_handler,
            hook_handler,
            clock,
            state: Mutex::new(CallbackState::default()),
        }
    }

    pub async fn register_route(
        &self,
        route: CompleteAgentCallbackRoute,
    ) -> Result<(), AgentHostCallbackError> {
        if route.generation.0 == 0
            || route.delivery != AgentSurfaceRoute::AgentNativeCallback
            || route.default_deadline_ms == 0
        {
            return Err(callback_error(
                AgentHostCallbackErrorCode::InvalidArgument,
                "callback route requires positive generation/deadline and Agent-native delivery",
                false,
            ));
        }
        let mut state = self.state.lock().await;
        if let Some(existing) = state.routes.get(&route.route_id) {
            if existing == &route {
                return Ok(());
            }
            return Err(callback_error(
                AgentHostCallbackErrorCode::DuplicateConflict,
                "callback route id is already registered with different binding evidence",
                false,
            ));
        }
        state.routes.insert(route.route_id.clone(), route);
        Ok(())
    }

    pub async fn revoke_route(
        &self,
        route_id: &AgentCallbackRouteId,
        expected_generation: AgentBindingGeneration,
    ) -> Result<(), AgentHostCallbackError> {
        let mut state = self.state.lock().await;
        let route = state.routes.get(route_id).ok_or_else(|| {
            callback_error(
                AgentHostCallbackErrorCode::UnknownRoute,
                "callback route is not registered",
                false,
            )
        })?;
        if route.generation != expected_generation {
            return Err(stale_generation_error());
        }
        state.routes.remove(route_id);
        Ok(())
    }

    async fn route_and_lock(
        &self,
        meta: &agentdash_agent_service_api::AgentHostCallbackMeta,
    ) -> Result<(CompleteAgentCallbackRoute, CallbackKey, Arc<Mutex<()>>), AgentHostCallbackError>
    {
        let mut state = self.state.lock().await;
        let route = state.routes.get(&meta.route_id).cloned().ok_or_else(|| {
            callback_error(
                AgentHostCallbackErrorCode::UnknownRoute,
                "callback route is not registered",
                false,
            )
        })?;
        if route.generation != meta.binding_generation {
            return Err(stale_generation_error());
        }
        if route.source != meta.source {
            return Err(callback_error(
                AgentHostCallbackErrorCode::InvalidArgument,
                "callback source does not match the registered route",
                false,
            ));
        }
        let key = CallbackKey {
            route_id: meta.route_id.clone(),
            idempotency_key: meta.idempotency_key.clone(),
        };
        let lock = state
            .locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        Ok((route, key, lock))
    }

    fn ensure_deadline(
        &self,
        route: &CompleteAgentCallbackRoute,
        semantic_deadline_ms: Option<u64>,
        meta: &agentdash_agent_service_api::AgentHostCallbackMeta,
    ) -> Result<(), AgentHostCallbackError> {
        let now_ms = self.clock.now_ms();
        if now_ms >= meta.deadline_at_ms {
            return Err(callback_error(
                AgentHostCallbackErrorCode::DeadlineExceeded,
                "callback semantic deadline elapsed before execution",
                false,
            ));
        }
        let maximum_duration_ms = semantic_deadline_ms
            .map_or(route.default_deadline_ms, |deadline_ms| {
                route.default_deadline_ms.min(deadline_ms)
            });
        if maximum_duration_ms == 0 {
            return Err(callback_error(
                AgentHostCallbackErrorCode::InvalidArgument,
                "callback semantic deadline must be greater than zero",
                false,
            ));
        }
        if meta.deadline_at_ms > now_ms.saturating_add(maximum_duration_ms) {
            return Err(callback_error(
                AgentHostCallbackErrorCode::InvalidArgument,
                "callback deadline exceeds the bound route or contribution semantic deadline",
                false,
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl AgentHostCallbacks for CompleteAgentCallbackBroker {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        let (route, key, key_lock) = self.route_and_lock(&call.meta).await?;
        let _guard = key_lock.lock().await;
        let request_digest = request_digest(&call)?;
        if let Some(cached) = self.state.lock().await.results.get(&key).cloned() {
            return match cached {
                CachedCallback::Tool {
                    request_digest: existing,
                    result,
                } if existing == request_digest => result,
                _ => Err(callback_error(
                    AgentHostCallbackErrorCode::DuplicateConflict,
                    "idempotency key was reused for another callback request",
                    false,
                )),
            };
        }
        self.ensure_deadline(&route, None, &call.meta)?;
        ensure_tool_is_bound(&route.bound_surface, &call)?;

        let result = self.tool_handler.invoke(call).await;
        self.state.lock().await.results.insert(
            key,
            CachedCallback::Tool {
                request_digest,
                result: result.clone(),
            },
        );
        result
    }

    async fn invoke_hook(
        &self,
        call: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        let (route, key, key_lock) = self.route_and_lock(&call.meta).await?;
        let _guard = key_lock.lock().await;
        let request_digest = request_digest(&call)?;
        if let Some(cached) = self.state.lock().await.results.get(&key).cloned() {
            return match cached {
                CachedCallback::Hook {
                    request_digest: existing,
                    result,
                } if existing == request_digest => result,
                _ => Err(callback_error(
                    AgentHostCallbackErrorCode::DuplicateConflict,
                    "idempotency key was reused for another callback request",
                    false,
                )),
            };
        }
        let hook_deadline_ms = ensure_hook_is_bound(&route.bound_surface, &call)?;
        self.ensure_deadline(&route, Some(hook_deadline_ms), &call.meta)?;

        let result = self
            .hook_handler
            .invoke(call.clone())
            .await
            .and_then(|decision| {
                ensure_hook_decision_allowed(&call, &decision)?;
                Ok(decision)
            });
        self.state.lock().await.results.insert(
            key,
            CachedCallback::Hook {
                request_digest,
                result: result.clone(),
            },
        );
        result
    }
}

fn ensure_tool_is_bound(
    surface: &BoundAgentSurface,
    call: &AgentToolInvocation,
) -> Result<(), AgentHostCallbackError> {
    let found = surface.contributions.iter().any(|contribution| {
        contribution.route == AgentSurfaceRoute::AgentNativeCallback
            && contribution.semantics.required_causal_route()
                == Some(AgentSurfaceRoute::AgentNativeCallback)
            && matches!(
                &contribution.payload,
                AgentSurfaceContributionPayload::Tool { name, .. } if name == &call.tool
            )
    });
    if !found {
        return Err(callback_error(
            AgentHostCallbackErrorCode::Unsupported,
            "tool is not bound to this Agent-native callback route",
            false,
        ));
    }
    Ok(())
}

fn ensure_hook_is_bound(
    surface: &BoundAgentSurface,
    call: &AgentHookInvocation,
) -> Result<u64, AgentHostCallbackError> {
    let deadline_ms = surface.contributions.iter().find_map(|contribution| {
        if contribution.route != AgentSurfaceRoute::AgentNativeCallback
            || contribution.semantics.required_causal_route()
                != Some(AgentSurfaceRoute::AgentNativeCallback)
        {
            return None;
        }
        match &contribution.payload {
            AgentSurfaceContributionPayload::Hook {
                definition_id,
                point,
                timing,
                actions,
                deadline_ms,
            } if definition_id == &call.definition_id
                && point == &call.point
                && timing == &call.timing
                && &call.allowed_actions == actions =>
            {
                Some(*deadline_ms)
            }
            _ => None,
        }
    });
    let Some(deadline_ms) = deadline_ms else {
        return Err(callback_error(
            AgentHostCallbackErrorCode::Unsupported,
            "hook is not bound to this Agent-native callback route",
            false,
        ));
    };
    if deadline_ms == 0 {
        return Err(callback_error(
            AgentHostCallbackErrorCode::InvalidArgument,
            "bound hook semantic deadline must be greater than zero",
            false,
        ));
    }
    Ok(deadline_ms)
}

fn ensure_hook_decision_allowed(
    call: &AgentHookInvocation,
    decision: &AgentHookDecision,
) -> Result<(), AgentHostCallbackError> {
    let action = match decision {
        AgentHookDecision::Allow | AgentHookDecision::Deny { .. } => AgentHookAction::AllowOrDeny,
        AgentHookDecision::ReplaceInput { .. } => AgentHookAction::RewriteInput,
        AgentHookDecision::ReplaceResult { .. } => AgentHookAction::RewriteResult,
        AgentHookDecision::AddContext { .. } => AgentHookAction::AddContext,
        AgentHookDecision::EmitEffect { .. } => AgentHookAction::EmitEffect,
    };
    if !call.allowed_actions.contains(&action) {
        return Err(callback_error(
            AgentHostCallbackErrorCode::Internal,
            "hook handler returned a decision outside the bound semantic actions",
            false,
        ));
    }
    Ok(())
}

fn request_digest(value: &impl Serialize) -> Result<String, AgentHostCallbackError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        callback_error(
            AgentHostCallbackErrorCode::InvalidArgument,
            format!("callback request cannot be encoded: {error}"),
            false,
        )
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn stale_generation_error() -> AgentHostCallbackError {
    callback_error(
        AgentHostCallbackErrorCode::StaleBindingGeneration,
        "callback binding generation is stale",
        false,
    )
}

fn callback_error(
    code: AgentHostCallbackErrorCode,
    message: impl Into<String>,
    retryable: bool,
) -> AgentHostCallbackError {
    AgentHostCallbackError::new(code, message, retryable)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use agentdash_agent_service_api::{
        AgentEffectIdentity, AgentHookBlockingSemantics, AgentHookDefinitionId,
        AgentHookEffectKind, AgentHookMutationKind, AgentHookPoint, AgentHookSemanticFacet,
        AgentHookTiming, AgentHostCallbackMeta, AgentIdempotencyKey, AgentPayloadDigest,
        AgentProfileDigest, AgentSurfaceContributionPayload, AgentSurfaceDigest,
        AgentSurfaceRevision, AgentSurfaceSemanticFacet, AgentToolDelivery, AgentToolName,
        AgentToolSemanticFacet, AgentToolUpdateSemantics, AgentTurnId,
        BoundAgentSurfaceContribution, SemanticFidelity,
    };
    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct CountingToolHandler {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl CompleteAgentToolHandler for CountingToolHandler {
        async fn invoke(
            &self,
            _invocation: AgentToolInvocation,
        ) -> Result<AgentToolResult, AgentHostCallbackError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(AgentToolResult::Completed {
                output: json!({"ok": true}),
            })
        }
    }

    struct AllowHookHandler;

    #[async_trait]
    impl CompleteAgentHookHandler for AllowHookHandler {
        async fn invoke(
            &self,
            _invocation: AgentHookInvocation,
        ) -> Result<AgentHookDecision, AgentHostCallbackError> {
            Ok(AgentHookDecision::Allow)
        }
    }

    #[derive(Default)]
    struct InvalidHookHandler {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl CompleteAgentHookHandler for InvalidHookHandler {
        async fn invoke(
            &self,
            _invocation: AgentHookInvocation,
        ) -> Result<AgentHookDecision, AgentHostCallbackError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(AgentHookDecision::ReplaceInput { input: json!({}) })
        }
    }

    struct FixedClock(u64);

    impl AgentCallbackClock for FixedClock {
        fn now_ms(&self) -> u64 {
            self.0
        }
    }

    #[tokio::test]
    async fn duplicate_tool_callback_replays_one_result() {
        let tool_handler = Arc::new(CountingToolHandler::default());
        let broker = CompleteAgentCallbackBroker::with_clock(
            tool_handler.clone(),
            Arc::new(AllowHookHandler),
            Arc::new(FixedClock(10)),
        );
        broker
            .register_route(route(AgentBindingGeneration(2)))
            .await
            .expect("route");
        let call = tool_call(AgentBindingGeneration(2), 20);

        let first = broker.invoke_tool(call.clone()).await.expect("first");
        let second = broker.invoke_tool(call).await.expect("replay");

        assert_eq!(first, second);
        assert_eq!(tool_handler.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stale_generation_and_elapsed_deadline_are_rejected() {
        let broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            Arc::new(AllowHookHandler),
            Arc::new(FixedClock(10)),
        );
        broker
            .register_route(route(AgentBindingGeneration(2)))
            .await
            .expect("route");

        let stale = broker
            .invoke_tool(tool_call(AgentBindingGeneration(1), 20))
            .await
            .expect_err("stale");
        assert_eq!(
            stale.code,
            AgentHostCallbackErrorCode::StaleBindingGeneration
        );

        let expired = broker
            .invoke_tool(tool_call(AgentBindingGeneration(2), 10))
            .await
            .expect_err("expired");
        assert_eq!(expired.code, AgentHostCallbackErrorCode::DeadlineExceeded);

        let overlong = broker
            .invoke_tool(tool_call(AgentBindingGeneration(2), 21))
            .await
            .expect_err("overlong");
        assert_eq!(overlong.code, AgentHostCallbackErrorCode::InvalidArgument);
    }

    #[tokio::test]
    async fn hook_decision_outside_bound_actions_is_rejected_and_replayed_once() {
        let hook_handler = Arc::new(InvalidHookHandler::default());
        let broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            hook_handler.clone(),
            Arc::new(FixedClock(10)),
        );
        broker
            .register_route(hook_route())
            .await
            .expect("hook route");
        let call = hook_call();

        let first = broker
            .invoke_hook(call.clone())
            .await
            .expect_err("invalid decision");
        let replay = broker
            .invoke_hook(call)
            .await
            .expect_err("replayed invalid decision");

        assert_eq!(first.code, AgentHostCallbackErrorCode::Internal);
        assert_eq!(first, replay);
        assert_eq!(hook_handler.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn hook_callback_cannot_exceed_its_bound_payload_deadline() {
        let broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            Arc::new(AllowHookHandler),
            Arc::new(FixedClock(10)),
        );
        broker
            .register_route(hook_route_with_deadlines(100, 10))
            .await
            .expect("hook route");

        let error = broker
            .invoke_hook(hook_call_with_deadline(21))
            .await
            .expect_err("deadline exceeds bound hook semantics");

        assert_eq!(error.code, AgentHostCallbackErrorCode::InvalidArgument);
    }

    #[test]
    fn callback_route_is_created_only_from_agent_native_apply_binding() {
        let binding = AgentHostCallbackBinding {
            route_id: AgentCallbackRouteId::new("route").expect("route"),
            binding_generation: AgentBindingGeneration(2),
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms: 10,
        };
        let callback_route = CompleteAgentCallbackRoute::from_binding(
            binding,
            AgentSourceCoordinate::new("source").expect("source"),
            route(AgentBindingGeneration(2)).bound_surface,
        )
        .expect("callback route");
        assert_eq!(
            callback_route.delivery,
            AgentSurfaceRoute::AgentNativeCallback
        );

        let rejected = CompleteAgentCallbackRoute::from_binding(
            AgentHostCallbackBinding {
                route_id: AgentCallbackRouteId::new("wrong-route").expect("route"),
                binding_generation: AgentBindingGeneration(2),
                delivery: AgentSurfaceRoute::RuntimeToolBroker,
                default_deadline_ms: 10,
            },
            AgentSourceCoordinate::new("source").expect("source"),
            route(AgentBindingGeneration(2)).bound_surface,
        )
        .expect_err("non-callback delivery");
        assert_eq!(rejected.code, AgentHostCallbackErrorCode::InvalidArgument);
    }

    fn route(generation: AgentBindingGeneration) -> CompleteAgentCallbackRoute {
        CompleteAgentCallbackRoute {
            route_id: AgentCallbackRouteId::new("route").expect("route"),
            generation,
            source: AgentSourceCoordinate::new("source").expect("source"),
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms: 10,
            bound_surface: BoundAgentSurface {
                revision: AgentSurfaceRevision(1),
                digest: AgentSurfaceDigest::new("surface").expect("surface"),
                offer_profile_digest: AgentProfileDigest::new("profile").expect("profile"),
                contributions: vec![BoundAgentSurfaceContribution {
                    key: "tool:test".to_owned(),
                    required: true,
                    route: AgentSurfaceRoute::AgentNativeCallback,
                    fidelity: SemanticFidelity::Exact,
                    semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                        delivery: AgentToolDelivery::AgentNativeCallback,
                        invocation: SemanticFidelity::Exact,
                        update: AgentToolUpdateSemantics::BindingOnly,
                    }),
                    payload: AgentSurfaceContributionPayload::Tool {
                        name: AgentToolName::new("test").expect("tool"),
                        description: "test".to_owned(),
                        input_schema: json!({"type": "object"}),
                        output_schema: None,
                    },
                    payload_digest: AgentPayloadDigest::new("payload").expect("payload"),
                }],
            },
        }
    }

    fn tool_call(generation: AgentBindingGeneration, deadline_at_ms: u64) -> AgentToolInvocation {
        AgentToolInvocation {
            meta: AgentHostCallbackMeta {
                route_id: AgentCallbackRouteId::new("route").expect("route"),
                binding_generation: generation,
                source: AgentSourceCoordinate::new("source").expect("source"),
                turn_id: AgentTurnId::new("turn").expect("turn"),
                item_id: None,
                interaction_id: None,
                effect_id: AgentEffectIdentity::new("effect").expect("effect"),
                idempotency_key: AgentIdempotencyKey::new("idem").expect("idempotency"),
                deadline_at_ms,
            },
            tool: AgentToolName::new("test").expect("tool"),
            arguments: json!({}),
        }
    }

    fn hook_route() -> CompleteAgentCallbackRoute {
        hook_route_with_deadlines(10, 10)
    }

    fn hook_route_with_deadlines(
        default_deadline_ms: u64,
        hook_deadline_ms: u64,
    ) -> CompleteAgentCallbackRoute {
        CompleteAgentCallbackRoute {
            route_id: AgentCallbackRouteId::new("hook-route").expect("route"),
            generation: AgentBindingGeneration(2),
            source: AgentSourceCoordinate::new("source").expect("source"),
            delivery: AgentSurfaceRoute::AgentNativeCallback,
            default_deadline_ms,
            bound_surface: BoundAgentSurface {
                revision: AgentSurfaceRevision(1),
                digest: AgentSurfaceDigest::new("hook-surface").expect("surface"),
                offer_profile_digest: AgentProfileDigest::new("profile").expect("profile"),
                contributions: vec![BoundAgentSurfaceContribution {
                    key: "hook:test".to_owned(),
                    required: true,
                    route: AgentSurfaceRoute::AgentNativeCallback,
                    fidelity: SemanticFidelity::Exact,
                    semantics: AgentSurfaceSemanticFacet::Hook(AgentHookSemanticFacet {
                        point: AgentHookPoint::BeforeTool,
                        timing: AgentHookTiming::Before,
                        blocking: AgentHookBlockingSemantics::Blocking {
                            fidelity: SemanticFidelity::Exact,
                        },
                        mutations: BTreeMap::<AgentHookMutationKind, SemanticFidelity>::new(),
                        effects: BTreeMap::<AgentHookEffectKind, SemanticFidelity>::new(),
                    }),
                    payload: AgentSurfaceContributionPayload::Hook {
                        definition_id: AgentHookDefinitionId::new("hook").expect("hook"),
                        point: AgentHookPoint::BeforeTool,
                        timing: AgentHookTiming::Before,
                        actions: BTreeSet::from([AgentHookAction::AllowOrDeny]),
                        deadline_ms: hook_deadline_ms,
                    },
                    payload_digest: AgentPayloadDigest::new("hook-payload").expect("payload"),
                }],
            },
        }
    }

    fn hook_call() -> AgentHookInvocation {
        hook_call_with_deadline(20)
    }

    fn hook_call_with_deadline(deadline_at_ms: u64) -> AgentHookInvocation {
        AgentHookInvocation {
            meta: AgentHostCallbackMeta {
                route_id: AgentCallbackRouteId::new("hook-route").expect("route"),
                binding_generation: AgentBindingGeneration(2),
                source: AgentSourceCoordinate::new("source").expect("source"),
                turn_id: AgentTurnId::new("turn").expect("turn"),
                item_id: None,
                interaction_id: None,
                effect_id: AgentEffectIdentity::new("hook-effect").expect("effect"),
                idempotency_key: AgentIdempotencyKey::new("hook-idem").expect("idempotency"),
                deadline_at_ms,
            },
            definition_id: AgentHookDefinitionId::new("hook").expect("hook"),
            point: AgentHookPoint::BeforeTool,
            timing: AgentHookTiming::Before,
            allowed_actions: BTreeSet::from([AgentHookAction::AllowOrDeny]),
            input: json!({}),
        }
    }
}
