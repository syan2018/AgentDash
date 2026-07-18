use std::{
    collections::{BTreeMap, BTreeSet},
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
use thiserror::Error;

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
pub struct CompleteAgentCallbackKey {
    pub route_id: AgentCallbackRouteId,
    pub idempotency_key: agentdash_agent_service_api::AgentIdempotencyKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompleteAgentCallbackKind {
    Tool,
    Hook,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompleteAgentCallbackTerminalOutcome {
    Tool {
        result: Result<AgentToolResult, AgentHostCallbackError>,
    },
    Hook {
        result: Result<AgentHookDecision, AgentHostCallbackError>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompleteAgentCallbackReservationState {
    Pending,
    InspectionRequired { reason: String },
    Settled(CompleteAgentCallbackTerminalOutcome),
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentCallbackRecord {
    pub key: CompleteAgentCallbackKey,
    pub kind: CompleteAgentCallbackKind,
    pub request_digest: String,
    pub generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub deadline_at_ms: u64,
    pub state: CompleteAgentCallbackReservationState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompleteAgentCallbackRevision(pub u64);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompleteAgentCallbackFacts {
    pub routes: BTreeMap<AgentCallbackRouteId, CompleteAgentCallbackRoute>,
    pub revoked_routes: BTreeSet<AgentCallbackRouteId>,
    pub callbacks: BTreeMap<CompleteAgentCallbackKey, CompleteAgentCallbackRecord>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompleteAgentCallbackSnapshot {
    pub revision: CompleteAgentCallbackRevision,
    pub facts: CompleteAgentCallbackFacts,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteAgentCallbackCommit {
    pub expected_revision: CompleteAgentCallbackRevision,
    pub facts: CompleteAgentCallbackFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompleteAgentCallbackStoreError {
    #[error("Complete Agent callback revision conflict")]
    Conflict,
    #[error("Complete Agent callback invariant failed: {reason}")]
    Invariant { reason: String },
    #[error("Complete Agent callback persistence failed: {reason}")]
    Persistence { reason: String },
}

/// Host 持有的反向回调路由、reservation 与 outcome 持久化边界。
///
/// 生产 adapter 必须把路由注册或撤销与对应的 Host binding/surface 状态转换放进同一个
/// 数据库事务。回调 reservation 与 outcome 转换形成独立 CAS 聚合；调用平台 handler
/// 之前必须先持久化 reservation。
#[async_trait]
pub trait CompleteAgentCallbackRepository: Send + Sync {
    async fn load(&self) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError>;

    async fn commit(
        &self,
        commit: CompleteAgentCallbackCommit,
    ) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError>;
}

pub fn apply_complete_agent_callback_commit(
    current: &mut CompleteAgentCallbackSnapshot,
    commit: CompleteAgentCallbackCommit,
) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError> {
    if current.revision != commit.expected_revision {
        if current.facts == commit.facts {
            return Ok(current.clone());
        }
        return Err(CompleteAgentCallbackStoreError::Conflict);
    }
    validate_complete_agent_callback_facts(&current.facts, &commit.facts)?;
    current.revision =
        CompleteAgentCallbackRevision(current.revision.0.checked_add(1).ok_or_else(|| {
            CompleteAgentCallbackStoreError::Invariant {
                reason: "callback revision is exhausted".to_owned(),
            }
        })?);
    current.facts = commit.facts;
    Ok(current.clone())
}

pub fn validate_complete_agent_callback_facts(
    current: &CompleteAgentCallbackFacts,
    candidate: &CompleteAgentCallbackFacts,
) -> Result<(), CompleteAgentCallbackStoreError> {
    for (route_id, route) in &candidate.routes {
        if route_id != &route.route_id
            || route.generation.0 == 0
            || route.delivery != AgentSurfaceRoute::AgentNativeCallback
            || route.default_deadline_ms == 0
        {
            return callback_invariant("callback route coordinates are invalid");
        }
    }
    for (route_id, route) in &current.routes {
        if candidate.routes.get(route_id) != Some(route) {
            return callback_invariant("callback route history is immutable");
        }
    }
    if !candidate
        .revoked_routes
        .is_superset(&current.revoked_routes)
        || candidate
            .revoked_routes
            .iter()
            .any(|route_id| !candidate.routes.contains_key(route_id))
    {
        return callback_invariant("callback route revocation history is invalid");
    }
    for (key, record) in &candidate.callbacks {
        if key != &record.key
            || key.route_id != record.key.route_id
            || record.request_digest.trim().is_empty()
            || record.generation.0 == 0
            || record.deadline_at_ms == 0
        {
            return callback_invariant("callback reservation coordinates are invalid");
        }
        let route = candidate.routes.get(&key.route_id).ok_or_else(|| {
            CompleteAgentCallbackStoreError::Invariant {
                reason: "callback reservation has no durable route".to_owned(),
            }
        })?;
        if route.generation != record.generation || route.source != record.source {
            return callback_invariant("callback reservation route fence is inconsistent");
        }
    }
    for (key, record) in &current.callbacks {
        let next = candidate.callbacks.get(key).ok_or_else(|| {
            CompleteAgentCallbackStoreError::Invariant {
                reason: "callback reservation history cannot be removed".to_owned(),
            }
        })?;
        if record.key != next.key
            || record.kind != next.kind
            || record.request_digest != next.request_digest
            || record.generation != next.generation
            || record.source != next.source
            || record.deadline_at_ms != next.deadline_at_ms
            || !callback_state_can_advance(&record.state, &next.state)
        {
            return callback_invariant("callback reservation was rewritten or moved backwards");
        }
    }
    Ok(())
}

fn callback_state_can_advance(
    current: &CompleteAgentCallbackReservationState,
    next: &CompleteAgentCallbackReservationState,
) -> bool {
    current == next
        || matches!(
            (current, next),
            (
                CompleteAgentCallbackReservationState::Pending,
                CompleteAgentCallbackReservationState::InspectionRequired { .. }
                    | CompleteAgentCallbackReservationState::Settled(_)
                    | CompleteAgentCallbackReservationState::Unknown { .. }
            ) | (
                CompleteAgentCallbackReservationState::InspectionRequired { .. },
                CompleteAgentCallbackReservationState::Settled(_)
                    | CompleteAgentCallbackReservationState::Unknown { .. }
            ) | (
                CompleteAgentCallbackReservationState::Unknown { .. },
                CompleteAgentCallbackReservationState::Settled(_)
            )
        )
}

fn callback_invariant<T>(reason: &str) -> Result<T, CompleteAgentCallbackStoreError> {
    Err(CompleteAgentCallbackStoreError::Invariant {
        reason: reason.to_owned(),
    })
}

/// Reverse callback broker for Agent-native Tool/Hook execution.
///
/// Each registered route is bound to one source and generation. Per-idempotency-key serialization
/// persists both successful and failed outcomes, so duplicate/replayed calls do not execute the
/// platform handler twice.
pub struct CompleteAgentCallbackBroker {
    tool_handler: Arc<dyn CompleteAgentToolHandler>,
    hook_handler: Arc<dyn CompleteAgentHookHandler>,
    repository: Arc<dyn CompleteAgentCallbackRepository>,
    clock: Arc<dyn AgentCallbackClock>,
}

impl CompleteAgentCallbackBroker {
    pub fn new(
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
        repository: Arc<dyn CompleteAgentCallbackRepository>,
    ) -> Self {
        Self::with_clock(
            tool_handler,
            hook_handler,
            repository,
            Arc::new(SystemAgentCallbackClock),
        )
    }

    pub fn with_clock(
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
        repository: Arc<dyn CompleteAgentCallbackRepository>,
        clock: Arc<dyn AgentCallbackClock>,
    ) -> Self {
        Self {
            tool_handler,
            hook_handler,
            repository,
            clock,
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
        let snapshot = self.repository.load().await.map_err(store_error)?;
        if let Some(existing) = snapshot.facts.routes.get(&route.route_id) {
            if existing == &route {
                if snapshot.facts.revoked_routes.contains(&route.route_id) {
                    return Err(callback_error(
                        AgentHostCallbackErrorCode::StaleBindingGeneration,
                        "callback route was revoked and cannot be reactivated",
                        false,
                    ));
                }
                return Ok(());
            }
            return Err(callback_error(
                AgentHostCallbackErrorCode::DuplicateConflict,
                "callback route id is already registered with different binding evidence",
                false,
            ));
        }
        let mut facts = snapshot.facts;
        facts.routes.insert(route.route_id.clone(), route);
        self.repository
            .commit(CompleteAgentCallbackCommit {
                expected_revision: snapshot.revision,
                facts,
            })
            .await
            .map_err(store_error)?;
        Ok(())
    }

    pub async fn revoke_route(
        &self,
        route_id: &AgentCallbackRouteId,
        expected_generation: AgentBindingGeneration,
    ) -> Result<(), AgentHostCallbackError> {
        let snapshot = self.repository.load().await.map_err(store_error)?;
        let route = snapshot.facts.routes.get(route_id).ok_or_else(|| {
            callback_error(
                AgentHostCallbackErrorCode::UnknownRoute,
                "callback route is not registered",
                false,
            )
        })?;
        if route.generation != expected_generation {
            return Err(stale_generation_error());
        }
        if snapshot.facts.revoked_routes.contains(route_id) {
            return Ok(());
        }
        let mut facts = snapshot.facts;
        facts.revoked_routes.insert(route_id.clone());
        self.repository
            .commit(CompleteAgentCallbackCommit {
                expected_revision: snapshot.revision,
                facts,
            })
            .await
            .map_err(store_error)?;
        Ok(())
    }

    pub async fn inspect_callback(
        &self,
        key: &CompleteAgentCallbackKey,
    ) -> Result<Option<CompleteAgentCallbackRecord>, AgentHostCallbackError> {
        Ok(self
            .repository
            .load()
            .await
            .map_err(store_error)?
            .facts
            .callbacks
            .get(key)
            .cloned())
    }

    pub async fn mark_inspection_required(
        &self,
        key: &CompleteAgentCallbackKey,
        reason: impl Into<String>,
    ) -> Result<CompleteAgentCallbackRecord, AgentHostCallbackError> {
        self.advance_callback(
            key,
            None,
            CompleteAgentCallbackReservationState::InspectionRequired {
                reason: reason.into(),
            },
        )
        .await
    }

    pub async fn mark_unknown(
        &self,
        key: &CompleteAgentCallbackKey,
        reason: impl Into<String>,
    ) -> Result<CompleteAgentCallbackRecord, AgentHostCallbackError> {
        self.advance_callback(
            key,
            None,
            CompleteAgentCallbackReservationState::Unknown {
                reason: reason.into(),
            },
        )
        .await
    }

    pub async fn reconcile_tool(
        &self,
        key: &CompleteAgentCallbackKey,
        request_digest: &str,
        result: Result<AgentToolResult, AgentHostCallbackError>,
    ) -> Result<CompleteAgentCallbackRecord, AgentHostCallbackError> {
        self.advance_callback(
            key,
            Some((CompleteAgentCallbackKind::Tool, request_digest)),
            CompleteAgentCallbackReservationState::Settled(
                CompleteAgentCallbackTerminalOutcome::Tool { result },
            ),
        )
        .await
    }

    pub async fn reconcile_hook(
        &self,
        key: &CompleteAgentCallbackKey,
        request_digest: &str,
        result: Result<AgentHookDecision, AgentHostCallbackError>,
    ) -> Result<CompleteAgentCallbackRecord, AgentHostCallbackError> {
        self.advance_callback(
            key,
            Some((CompleteAgentCallbackKind::Hook, request_digest)),
            CompleteAgentCallbackReservationState::Settled(
                CompleteAgentCallbackTerminalOutcome::Hook { result },
            ),
        )
        .await
    }

    async fn route_and_key(
        &self,
        meta: &agentdash_agent_service_api::AgentHostCallbackMeta,
    ) -> Result<
        (
            CompleteAgentCallbackSnapshot,
            CompleteAgentCallbackRoute,
            CompleteAgentCallbackKey,
        ),
        AgentHostCallbackError,
    > {
        let snapshot = self.repository.load().await.map_err(store_error)?;
        let route = snapshot
            .facts
            .routes
            .get(&meta.route_id)
            .cloned()
            .ok_or_else(|| {
                callback_error(
                    AgentHostCallbackErrorCode::UnknownRoute,
                    "callback route is not registered",
                    false,
                )
            })?;
        if snapshot.facts.revoked_routes.contains(&meta.route_id) {
            return Err(stale_generation_error());
        }
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
        let key = CompleteAgentCallbackKey {
            route_id: meta.route_id.clone(),
            idempotency_key: meta.idempotency_key.clone(),
        };
        Ok((snapshot, route, key))
    }

    async fn reserve(
        &self,
        snapshot: CompleteAgentCallbackSnapshot,
        record: CompleteAgentCallbackRecord,
    ) -> Result<(), AgentHostCallbackError> {
        let mut facts = snapshot.facts;
        facts.callbacks.insert(record.key.clone(), record);
        self.repository
            .commit(CompleteAgentCallbackCommit {
                expected_revision: snapshot.revision,
                facts,
            })
            .await
            .map_err(store_error)?;
        Ok(())
    }

    async fn advance_callback(
        &self,
        key: &CompleteAgentCallbackKey,
        expected: Option<(CompleteAgentCallbackKind, &str)>,
        state: CompleteAgentCallbackReservationState,
    ) -> Result<CompleteAgentCallbackRecord, AgentHostCallbackError> {
        let snapshot = self.repository.load().await.map_err(store_error)?;
        let mut facts = snapshot.facts;
        let record = facts.callbacks.get_mut(key).ok_or_else(|| {
            callback_error(
                AgentHostCallbackErrorCode::InvalidArgument,
                "callback reservation does not exist",
                false,
            )
        })?;
        if let Some((kind, digest)) = expected
            && (record.kind != kind || record.request_digest != digest)
        {
            return Err(callback_error(
                AgentHostCallbackErrorCode::DuplicateConflict,
                "callback reconciliation does not match the durable reservation",
                false,
            ));
        }
        record.state = state;
        let next = record.clone();
        self.repository
            .commit(CompleteAgentCallbackCommit {
                expected_revision: snapshot.revision,
                facts,
            })
            .await
            .map_err(store_error)?;
        Ok(next)
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
        let (snapshot, route, key) = self.route_and_key(&call.meta).await?;
        let request_digest = request_digest(&call)?;
        if let Some(record) = snapshot.facts.callbacks.get(&key) {
            return replay_tool(record, &request_digest);
        }
        self.ensure_deadline(&route, None, &call.meta)?;
        ensure_tool_is_bound(&route.bound_surface, &call)?;
        self.reserve(
            snapshot,
            CompleteAgentCallbackRecord {
                key: key.clone(),
                kind: CompleteAgentCallbackKind::Tool,
                request_digest: request_digest.clone(),
                generation: call.meta.binding_generation,
                source: call.meta.source.clone(),
                deadline_at_ms: call.meta.deadline_at_ms,
                state: CompleteAgentCallbackReservationState::Pending,
            },
        )
        .await?;
        let result = self.tool_handler.invoke(call).await;
        self.reconcile_tool(&key, &request_digest, result.clone())
            .await?;
        result
    }

    async fn invoke_hook(
        &self,
        call: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        let (snapshot, route, key) = self.route_and_key(&call.meta).await?;
        let request_digest = request_digest(&call)?;
        if let Some(record) = snapshot.facts.callbacks.get(&key) {
            return replay_hook(record, &request_digest);
        }
        let hook_deadline_ms = ensure_hook_is_bound(&route.bound_surface, &call)?;
        self.ensure_deadline(&route, Some(hook_deadline_ms), &call.meta)?;
        self.reserve(
            snapshot,
            CompleteAgentCallbackRecord {
                key: key.clone(),
                kind: CompleteAgentCallbackKind::Hook,
                request_digest: request_digest.clone(),
                generation: call.meta.binding_generation,
                source: call.meta.source.clone(),
                deadline_at_ms: call.meta.deadline_at_ms,
                state: CompleteAgentCallbackReservationState::Pending,
            },
        )
        .await?;
        let result = self
            .hook_handler
            .invoke(call.clone())
            .await
            .and_then(|decision| {
                ensure_hook_decision_allowed(&call, &decision)?;
                Ok(decision)
            });
        self.reconcile_hook(&key, &request_digest, result.clone())
            .await?;
        result
    }
}

fn replay_tool(
    record: &CompleteAgentCallbackRecord,
    request_digest: &str,
) -> Result<AgentToolResult, AgentHostCallbackError> {
    if record.kind != CompleteAgentCallbackKind::Tool || record.request_digest != request_digest {
        return Err(callback_error(
            AgentHostCallbackErrorCode::DuplicateConflict,
            "idempotency key was reused for another callback request",
            false,
        ));
    }
    match &record.state {
        CompleteAgentCallbackReservationState::Settled(
            CompleteAgentCallbackTerminalOutcome::Tool { result },
        ) => result.clone(),
        CompleteAgentCallbackReservationState::Settled(_) => Err(callback_error(
            AgentHostCallbackErrorCode::DuplicateConflict,
            "callback kind does not match the durable outcome",
            false,
        )),
        CompleteAgentCallbackReservationState::Pending => Err(pending_callback_error(
            "callback is durably pending; inspect or reconcile before retry",
        )),
        CompleteAgentCallbackReservationState::InspectionRequired { reason } => {
            Err(pending_callback_error(format!(
                "callback requires inspection before retry: {reason}"
            )))
        }
        CompleteAgentCallbackReservationState::Unknown { reason } => Err(callback_error(
            AgentHostCallbackErrorCode::Unavailable,
            format!("callback result is unknown: {reason}"),
            false,
        )),
    }
}

fn replay_hook(
    record: &CompleteAgentCallbackRecord,
    request_digest: &str,
) -> Result<AgentHookDecision, AgentHostCallbackError> {
    if record.kind != CompleteAgentCallbackKind::Hook || record.request_digest != request_digest {
        return Err(callback_error(
            AgentHostCallbackErrorCode::DuplicateConflict,
            "idempotency key was reused for another callback request",
            false,
        ));
    }
    match &record.state {
        CompleteAgentCallbackReservationState::Settled(
            CompleteAgentCallbackTerminalOutcome::Hook { result },
        ) => result.clone(),
        CompleteAgentCallbackReservationState::Settled(_) => Err(callback_error(
            AgentHostCallbackErrorCode::DuplicateConflict,
            "callback kind does not match the durable outcome",
            false,
        )),
        CompleteAgentCallbackReservationState::Pending => Err(pending_callback_error(
            "callback is durably pending; inspect or reconcile before retry",
        )),
        CompleteAgentCallbackReservationState::InspectionRequired { reason } => {
            Err(pending_callback_error(format!(
                "callback requires inspection before retry: {reason}"
            )))
        }
        CompleteAgentCallbackReservationState::Unknown { reason } => Err(callback_error(
            AgentHostCallbackErrorCode::Unavailable,
            format!("callback result is unknown: {reason}"),
            false,
        )),
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

fn pending_callback_error(message: impl Into<String>) -> AgentHostCallbackError {
    callback_error(AgentHostCallbackErrorCode::Unavailable, message, true)
}

fn store_error(error: CompleteAgentCallbackStoreError) -> AgentHostCallbackError {
    match error {
        CompleteAgentCallbackStoreError::Conflict => pending_callback_error(
            "callback state changed concurrently; inspect durable state before retry",
        ),
        CompleteAgentCallbackStoreError::Invariant { reason } => callback_error(
            AgentHostCallbackErrorCode::Internal,
            format!("callback persistence invariant failed: {reason}"),
            false,
        ),
        CompleteAgentCallbackStoreError::Persistence { reason } => {
            pending_callback_error(format!("callback persistence is unavailable: {reason}"))
        }
    }
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
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FixtureCallbackRepository {
        snapshot: Mutex<CompleteAgentCallbackSnapshot>,
        fail_next_commit: AtomicUsize,
    }

    #[async_trait]
    impl CompleteAgentCallbackRepository for FixtureCallbackRepository {
        async fn load(
            &self,
        ) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError> {
            Ok(self.snapshot.lock().await.clone())
        }

        async fn commit(
            &self,
            commit: CompleteAgentCallbackCommit,
        ) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError> {
            if self.fail_next_commit.swap(0, Ordering::SeqCst) != 0 {
                return Err(CompleteAgentCallbackStoreError::Persistence {
                    reason: "injected callback commit failure".to_owned(),
                });
            }
            let mut snapshot = self.snapshot.lock().await;
            apply_complete_agent_callback_commit(&mut snapshot, commit)
        }
    }

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

    struct FailSettleToolHandler {
        calls: AtomicUsize,
        repository: Arc<FixtureCallbackRepository>,
    }

    #[async_trait]
    impl CompleteAgentToolHandler for FailSettleToolHandler {
        async fn invoke(
            &self,
            _invocation: AgentToolInvocation,
        ) -> Result<AgentToolResult, AgentHostCallbackError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.repository.fail_next_commit.store(1, Ordering::SeqCst);
            Ok(AgentToolResult::Completed {
                output: json!({"recovered": true}),
            })
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
        let repository = Arc::new(FixtureCallbackRepository::default());
        let broker = CompleteAgentCallbackBroker::with_clock(
            tool_handler.clone(),
            Arc::new(AllowHookHandler),
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
        broker
            .register_route(route(AgentBindingGeneration(2)))
            .await
            .expect("route");
        let call = tool_call(AgentBindingGeneration(2), 20);

        let first = broker.invoke_tool(call.clone()).await.expect("first");
        let restarted = CompleteAgentCallbackBroker::with_clock(
            tool_handler.clone(),
            Arc::new(AllowHookHandler),
            repository,
            Arc::new(FixedClock(10)),
        );
        let second = restarted.invoke_tool(call).await.expect("replay");

        assert_eq!(first, second);
        assert_eq!(tool_handler.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stale_generation_and_elapsed_deadline_are_rejected() {
        let repository = Arc::new(FixtureCallbackRepository::default());
        let registering_broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            Arc::new(AllowHookHandler),
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
        registering_broker
            .register_route(route(AgentBindingGeneration(2)))
            .await
            .expect("route");
        let broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            Arc::new(AllowHookHandler),
            repository,
            Arc::new(FixedClock(10)),
        );

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
        let repository = Arc::new(FixtureCallbackRepository::default());
        let broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            hook_handler.clone(),
            repository.clone(),
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
        let restarted = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            hook_handler.clone(),
            repository,
            Arc::new(FixedClock(10)),
        );
        let replay = restarted
            .invoke_hook(call)
            .await
            .expect_err("replayed invalid decision");

        assert_eq!(first.code, AgentHostCallbackErrorCode::Internal);
        assert_eq!(first, replay);
        assert_eq!(hook_handler.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn pending_callback_survives_restart_and_requires_explicit_reconciliation() {
        let repository = Arc::new(FixtureCallbackRepository::default());
        let failing_handler = Arc::new(FailSettleToolHandler {
            calls: AtomicUsize::new(0),
            repository: repository.clone(),
        });
        let broker = CompleteAgentCallbackBroker::with_clock(
            failing_handler.clone(),
            Arc::new(AllowHookHandler),
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
        broker
            .register_route(route(AgentBindingGeneration(2)))
            .await
            .expect("route");
        let call = tool_call(AgentBindingGeneration(2), 20);
        let digest = request_digest(&call).expect("request digest");
        let key = CompleteAgentCallbackKey {
            route_id: call.meta.route_id.clone(),
            idempotency_key: call.meta.idempotency_key.clone(),
        };

        let unsettled = broker
            .invoke_tool(call.clone())
            .await
            .expect_err("settle commit fails");
        assert_eq!(unsettled.code, AgentHostCallbackErrorCode::Unavailable);
        assert!(unsettled.retryable);
        assert_eq!(failing_handler.calls.load(Ordering::SeqCst), 1);

        let restarted_handler = Arc::new(CountingToolHandler::default());
        let restarted = CompleteAgentCallbackBroker::with_clock(
            restarted_handler.clone(),
            Arc::new(AllowHookHandler),
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
        let pending = restarted
            .invoke_tool(call.clone())
            .await
            .expect_err("pending callback is not executed again");
        assert_eq!(pending.code, AgentHostCallbackErrorCode::Unavailable);
        assert!(pending.retryable);
        assert_eq!(restarted_handler.calls.load(Ordering::SeqCst), 0);

        restarted
            .mark_inspection_required(&key, "handler outcome must be inspected")
            .await
            .expect("mark inspection");
        restarted
            .mark_unknown(&key, "inspection could not determine the outcome")
            .await
            .expect("mark unknown");
        let unknown = restarted
            .invoke_tool(call.clone())
            .await
            .expect_err("unknown callback is not executed again");
        assert_eq!(unknown.code, AgentHostCallbackErrorCode::Unavailable);
        assert!(!unknown.retryable);
        assert!(matches!(
            restarted
                .inspect_callback(&key)
                .await
                .expect("inspect callback")
                .expect("callback")
                .state,
            CompleteAgentCallbackReservationState::Unknown { .. }
        ));

        restarted
            .reconcile_tool(
                &key,
                &digest,
                Ok(AgentToolResult::Completed {
                    output: json!({"recovered": true}),
                }),
            )
            .await
            .expect("explicit reconciliation");
        assert_eq!(
            restarted.invoke_tool(call).await.expect("settled replay"),
            AgentToolResult::Completed {
                output: json!({"recovered": true}),
            }
        );
        assert_eq!(restarted_handler.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn hook_callback_cannot_exceed_its_bound_payload_deadline() {
        let broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            Arc::new(AllowHookHandler),
            Arc::new(FixtureCallbackRepository::default()),
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
