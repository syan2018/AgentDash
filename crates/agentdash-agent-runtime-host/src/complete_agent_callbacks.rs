use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_agent_service_api::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentHookAction, AgentHookDecision,
    AgentHookInvocation, AgentHostCallbackBinding, AgentHostCallbackError,
    AgentHostCallbackErrorCode, AgentHostCallbacks, AgentProfileDigest, AgentServiceInstanceId,
    AgentSourceCoordinate, AgentSurfaceContributionPayload, AgentSurfaceDigest,
    AgentSurfaceRevision, AgentSurfaceRoute, AgentToolInvocation, AgentToolResult,
    BoundAgentSurface,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    CompleteAgentBinding, CompleteAgentBindingId, CompleteAgentBindingState, CompleteAgentHost,
    CompleteAgentRuntimeTarget,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCompleteAgentCallbackContext {
    pub runtime_thread_id: RuntimeThreadId,
    pub binding_id: CompleteAgentBindingId,
    pub binding_generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub service_instance_id: AgentServiceInstanceId,
    pub profile_digest: AgentProfileDigest,
    pub bound_surface_revision: AgentSurfaceRevision,
    pub bound_surface_digest: AgentSurfaceDigest,
    pub bound_surface_offer_profile_digest: AgentProfileDigest,
    pub applied_surface_revision: AgentSurfaceRevision,
    pub applied_surface_digest: AgentSurfaceDigest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCompleteAgentToolCallback {
    pub context: ResolvedCompleteAgentCallbackContext,
    pub invocation: AgentToolInvocation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCompleteAgentHookCallback {
    pub context: ResolvedCompleteAgentCallbackContext,
    pub invocation: AgentHookInvocation,
}

/// Actual Tool owner boundary.
///
/// `invocation.meta.idempotency_key` is stable across retries. The handler or the concrete
/// executor it delegates to owns effect inspection and receipt replay; the Host does not mirror
/// handler outcomes.
#[async_trait]
pub trait CompleteAgentToolHandler: Send + Sync {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentToolCallback,
    ) -> Result<AgentToolResult, AgentHostCallbackError>;
}

/// Actual Hook owner boundary.
///
/// A handler that produces side effects must use `invocation.meta.idempotency_key` as its own
/// stable effect identity. Pure decision handlers can recompute the same decision on retry.
#[async_trait]
pub trait CompleteAgentHookHandler: Send + Sync {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentHookCallback,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentCallbackRoute {
    pub route_id: AgentCallbackRouteId,
    pub binding_id: CompleteAgentBindingId,
    pub generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub delivery: AgentSurfaceRoute,
    pub default_deadline_ms: u64,
    pub bound_surface: BoundAgentSurface,
}

impl CompleteAgentCallbackRoute {
    pub fn from_binding(
        binding_id: CompleteAgentBindingId,
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
            binding_id,
            generation: binding.binding_generation,
            source,
            delivery: binding.delivery,
            default_deadline_ms: binding.default_deadline_ms,
            bound_surface,
        })
    }
}

/// Process-local reverse callback router.
///
/// The broker validates the current Host route and semantic deadline, then calls the actual
/// handler owner. It deliberately keeps no callback reservation, outcome, or replay ledger.
pub struct CompleteAgentCallbackBroker {
    tool_handler: Arc<dyn CompleteAgentToolHandler>,
    hook_handler: Arc<dyn CompleteAgentHookHandler>,
    host: Arc<CompleteAgentHost>,
    clock: Arc<dyn AgentCallbackClock>,
}

impl CompleteAgentCallbackBroker {
    pub fn new(
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
        host: Arc<CompleteAgentHost>,
    ) -> Self {
        Self::with_clock(
            tool_handler,
            hook_handler,
            host,
            Arc::new(SystemAgentCallbackClock),
        )
    }

    pub fn with_clock(
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
        host: Arc<CompleteAgentHost>,
        clock: Arc<dyn AgentCallbackClock>,
    ) -> Self {
        Self {
            tool_handler,
            hook_handler,
            host,
            clock,
        }
    }

    async fn route_and_context(
        &self,
        meta: &agentdash_agent_service_api::AgentHostCallbackMeta,
    ) -> Result<
        (
            CompleteAgentCallbackRoute,
            ResolvedCompleteAgentCallbackContext,
        ),
        AgentHostCallbackError,
    > {
        let (route, binding, target) = self.host.resolve_callback_route(meta).await?;
        let context = resolve_callback_context(&route, &binding, &target)?;
        Ok((route, context))
    }

    fn ensure_deadline(
        &self,
        route: &CompleteAgentCallbackRoute,
        semantic_deadline_ms: Option<u64>,
        meta: &agentdash_agent_service_api::AgentHostCallbackMeta,
    ) -> Result<Duration, AgentHostCallbackError> {
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
        Ok(Duration::from_millis(meta.deadline_at_ms - now_ms))
    }
}

#[async_trait]
impl AgentHostCallbacks for CompleteAgentCallbackBroker {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        let (route, context) = self.route_and_context(&call.meta).await?;
        ensure_tool_is_bound(&route.bound_surface, &call)?;
        let budget = self.ensure_deadline(&route, None, &call.meta)?;
        match tokio::time::timeout(
            budget,
            self.tool_handler.invoke(ResolvedCompleteAgentToolCallback {
                context,
                invocation: call,
            }),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(callback_error(
                AgentHostCallbackErrorCode::DeadlineExceeded,
                "callback handler crossed its absolute deadline",
                false,
            )),
        }
    }

    async fn invoke_hook(
        &self,
        call: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        let (route, context) = self.route_and_context(&call.meta).await?;
        let semantic_deadline_ms = ensure_hook_is_bound(&route.bound_surface, &call)?;
        let budget = self.ensure_deadline(&route, Some(semantic_deadline_ms), &call.meta)?;
        match tokio::time::timeout(
            budget,
            self.hook_handler.invoke(ResolvedCompleteAgentHookCallback {
                context,
                invocation: call.clone(),
            }),
        )
        .await
        {
            Ok(result) => result.and_then(|decision| {
                ensure_hook_decision_allowed(&call, &decision)?;
                Ok(decision)
            }),
            Err(_) => Err(callback_error(
                AgentHostCallbackErrorCode::DeadlineExceeded,
                "callback handler crossed its absolute deadline",
                false,
            )),
        }
    }
}

fn resolve_callback_context(
    route: &CompleteAgentCallbackRoute,
    binding: &CompleteAgentBinding,
    target: &CompleteAgentRuntimeTarget,
) -> Result<ResolvedCompleteAgentCallbackContext, AgentHostCallbackError> {
    let applied_surface = binding.applied_surface.as_ref().ok_or_else(|| {
        callback_invariant_error("callback binding has no applied surface in this Host incarnation")
    })?;
    if binding.id != route.binding_id
        || binding.state != CompleteAgentBindingState::Available
        || binding.generation != route.generation
        || binding.source != route.source
        || binding.bound_surface != route.bound_surface
        || !binding.bound_surface.accepts_applied(applied_surface)
        || target.callbacks.route_id != route.route_id
        || target.callbacks.binding_generation != route.generation
        || target.target != binding.target
        || target.generation != binding.generation
        || target.bound_surface != binding.bound_surface
    {
        return Err(callback_invariant_error(
            "callback route, binding, target, and applied surface are inconsistent",
        ));
    }
    Ok(ResolvedCompleteAgentCallbackContext {
        runtime_thread_id: target.runtime_thread_id.clone(),
        binding_id: binding.id.clone(),
        binding_generation: binding.generation,
        source: binding.source.clone(),
        service_instance_id: target.target.logical_instance_id.clone(),
        profile_digest: target.profile_digest.clone(),
        bound_surface_revision: binding.bound_surface.revision,
        bound_surface_digest: binding.bound_surface.digest.clone(),
        bound_surface_offer_profile_digest: binding.bound_surface.offer_profile_digest.clone(),
        applied_surface_revision: applied_surface.revision,
        applied_surface_digest: applied_surface.digest.clone(),
    })
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
        AgentHookDecision::Allow => return Ok(()),
        AgentHookDecision::Deny { .. } => AgentHookAction::AllowOrDeny,
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

fn callback_invariant_error(message: impl Into<String>) -> AgentHostCallbackError {
    callback_error(AgentHostCallbackErrorCode::Internal, message, false)
}

fn callback_error(
    code: AgentHostCallbackErrorCode,
    message: impl Into<String>,
    retryable: bool,
) -> AgentHostCallbackError {
    AgentHostCallbackError::new(code, message, retryable)
}
