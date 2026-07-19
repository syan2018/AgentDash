use std::{
    collections::BTreeMap,
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
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    CompleteAgentBindingId, CompleteAgentBindingState, CompleteAgentHostFacts,
    CompleteAgentHostStoreError, SharedCompleteAgentHostRepository,
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

#[async_trait]
pub trait CompleteAgentToolHandler: Send + Sync {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentToolCallback,
    ) -> Result<AgentToolResult, AgentHostCallbackError>;
}

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CompleteAgentCallbackKey {
    pub route_id: AgentCallbackRouteId,
    pub idempotency_key: agentdash_agent_service_api::AgentIdempotencyKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompleteAgentCallbackKind {
    Tool,
    Hook,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompleteAgentCallbackTerminalOutcome {
    Tool {
        result: Result<AgentToolResult, AgentHostCallbackError>,
    },
    Hook {
        result: Result<AgentHookDecision, AgentHostCallbackError>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "outcome", rename_all = "snake_case")]
pub enum CompleteAgentCallbackReservationState {
    Pending,
    InspectionRequired { reason: String },
    Settled(CompleteAgentCallbackTerminalOutcome),
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentCallbackRecord {
    pub key: CompleteAgentCallbackKey,
    pub kind: CompleteAgentCallbackKind,
    pub request_digest: String,
    pub generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub bound_surface_digest: AgentSurfaceDigest,
    pub deadline_at_ms: u64,
    pub state: CompleteAgentCallbackReservationState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CompleteAgentCallbackRevision(pub u64);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentCallbackFacts {
    #[serde(with = "callback_records")]
    pub callbacks: BTreeMap<CompleteAgentCallbackKey, CompleteAgentCallbackRecord>,
}

mod callback_records {
    use super::*;

    pub fn serialize<S>(
        callbacks: &BTreeMap<CompleteAgentCallbackKey, CompleteAgentCallbackRecord>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        callbacks.values().collect::<Vec<_>>().serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<BTreeMap<CompleteAgentCallbackKey, CompleteAgentCallbackRecord>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let records = Vec::<CompleteAgentCallbackRecord>::deserialize(deserializer)?;
        let mut callbacks = BTreeMap::new();
        for record in records {
            if callbacks.insert(record.key.clone(), record).is_some() {
                return Err(serde::de::Error::custom(
                    "duplicate Complete Agent callback key",
                ));
            }
        }
        Ok(callbacks)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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

/// 反向回调 reservation 与 outcome 的持久化边界。
///
/// route 与 tombstone 属于 `CompleteAgentHostRepository` 聚合。回调 reservation 与
/// outcome 转换形成独立 CAS 聚合；调用平台 handler 之前必须先持久化引用已提交 route
/// generation 与 bound-surface digest 的 reservation。
#[async_trait]
pub trait CompleteAgentCallbackRepository: Send + Sync {
    async fn load(&self) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError>;

    async fn commit(
        &self,
        commit: CompleteAgentCallbackCommit,
    ) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError>;
}

pub fn encode_complete_agent_callback_snapshot(
    snapshot: &CompleteAgentCallbackSnapshot,
) -> Result<serde_json::Value, CompleteAgentCallbackStoreError> {
    serde_json::to_value(snapshot).map_err(|error| CompleteAgentCallbackStoreError::Persistence {
        reason: format!("failed to encode Complete Agent callback snapshot: {error}"),
    })
}

pub fn decode_complete_agent_callback_snapshot(
    value: serde_json::Value,
) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError> {
    let snapshot: CompleteAgentCallbackSnapshot =
        serde_json::from_value(value).map_err(|error| {
            CompleteAgentCallbackStoreError::Invariant {
                reason: format!("failed to decode Complete Agent callback snapshot: {error}"),
            }
        })?;
    validate_complete_agent_callback_facts(&snapshot.facts, &snapshot.facts)?;
    Ok(snapshot)
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
    for (key, record) in &candidate.callbacks {
        let terminal_kind_matches = matches!(
            (&record.kind, &record.state),
            (
                CompleteAgentCallbackKind::Tool,
                CompleteAgentCallbackReservationState::Settled(
                    CompleteAgentCallbackTerminalOutcome::Tool { .. },
                ),
            ) | (
                CompleteAgentCallbackKind::Hook,
                CompleteAgentCallbackReservationState::Settled(
                    CompleteAgentCallbackTerminalOutcome::Hook { .. },
                ),
            ) | (
                _,
                CompleteAgentCallbackReservationState::Pending
                    | CompleteAgentCallbackReservationState::InspectionRequired { .. }
                    | CompleteAgentCallbackReservationState::Unknown { .. },
            )
        );
        if key.route_id.as_str().trim().is_empty()
            || key.idempotency_key.as_str().trim().is_empty()
            || record.source.as_str().trim().is_empty()
            || record.bound_surface_digest.as_str().trim().is_empty()
            || key != &record.key
            || key.route_id != record.key.route_id
            || record.request_digest.trim().is_empty()
            || record.generation.0 == 0
            || record.deadline_at_ms == 0
            || !terminal_kind_matches
        {
            return callback_invariant("callback reservation coordinates are invalid");
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
            || record.bound_surface_digest != next.bound_surface_digest
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
/// 每条已提交 Host route 绑定唯一 source、generation 与 surface digest。按幂等键持久化
/// 成功和失败 outcome，使重复或重放调用不会再次执行平台 handler。
pub struct CompleteAgentCallbackBroker {
    tool_handler: Arc<dyn CompleteAgentToolHandler>,
    hook_handler: Arc<dyn CompleteAgentHookHandler>,
    host_repository: SharedCompleteAgentHostRepository,
    repository: Arc<dyn CompleteAgentCallbackRepository>,
    clock: Arc<dyn AgentCallbackClock>,
}

impl CompleteAgentCallbackBroker {
    pub fn new(
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
        host_repository: SharedCompleteAgentHostRepository,
        repository: Arc<dyn CompleteAgentCallbackRepository>,
    ) -> Self {
        Self::with_clock(
            tool_handler,
            hook_handler,
            host_repository,
            repository,
            Arc::new(SystemAgentCallbackClock),
        )
    }

    pub fn with_clock(
        tool_handler: Arc<dyn CompleteAgentToolHandler>,
        hook_handler: Arc<dyn CompleteAgentHookHandler>,
        host_repository: SharedCompleteAgentHostRepository,
        repository: Arc<dyn CompleteAgentCallbackRepository>,
        clock: Arc<dyn AgentCallbackClock>,
    ) -> Self {
        Self {
            tool_handler,
            hook_handler,
            host_repository,
            repository,
            clock,
        }
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
            ResolvedCompleteAgentCallbackContext,
        ),
        AgentHostCallbackError,
    > {
        let host_snapshot = self
            .host_repository
            .load()
            .await
            .map_err(host_store_error)?;
        let route = host_snapshot
            .facts
            .callback_routes
            .get(&meta.route_id)
            .cloned()
            .ok_or_else(|| {
                callback_error(
                    AgentHostCallbackErrorCode::UnknownRoute,
                    "callback route is not registered",
                    false,
                )
            })?;
        if host_snapshot
            .facts
            .revoked_callback_routes
            .contains(&meta.route_id)
        {
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
        let context = resolve_callback_context(&host_snapshot.facts, &route)?;
        let snapshot = self.repository.load().await.map_err(store_error)?;
        Ok((snapshot, route, key, context))
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

    async fn handler_deadline_budget(
        &self,
        key: &CompleteAgentCallbackKey,
        deadline_at_ms: u64,
    ) -> Result<Duration, AgentHostCallbackError> {
        let now_ms = self.clock.now_ms();
        let Some(remaining_ms) = deadline_at_ms
            .checked_sub(now_ms)
            .filter(|value| *value > 0)
        else {
            self.mark_inspection_required(
                key,
                "callback deadline elapsed after durable reservation; handler outcome requires inspection",
            )
            .await?;
            return Err(callback_error(
                AgentHostCallbackErrorCode::DeadlineExceeded,
                "callback deadline elapsed after durable reservation; outcome requires inspection",
                false,
            ));
        };
        Ok(Duration::from_millis(remaining_ms))
    }

    async fn quarantine_handler_timeout(
        &self,
        key: &CompleteAgentCallbackKey,
    ) -> Result<(), AgentHostCallbackError> {
        self.mark_inspection_required(
            key,
            "callback handler crossed its absolute deadline; side-effect outcome requires inspection",
        )
        .await?;
        Ok(())
    }

    async fn ensure_handler_completed_before_deadline(
        &self,
        key: &CompleteAgentCallbackKey,
        deadline_at_ms: u64,
    ) -> Result<(), AgentHostCallbackError> {
        if self.clock.now_ms() < deadline_at_ms {
            return Ok(());
        }
        self.quarantine_handler_timeout(key).await?;
        Err(callback_error(
            AgentHostCallbackErrorCode::DeadlineExceeded,
            "callback handler crossed its absolute deadline; outcome requires inspection",
            false,
        ))
    }
}

#[async_trait]
impl AgentHostCallbacks for CompleteAgentCallbackBroker {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        let (snapshot, route, key, context) = self.route_and_key(&call.meta).await?;
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
                bound_surface_digest: route.bound_surface.digest.clone(),
                deadline_at_ms: call.meta.deadline_at_ms,
                state: CompleteAgentCallbackReservationState::Pending,
            },
        )
        .await?;
        let deadline_at_ms = call.meta.deadline_at_ms;
        let budget = self.handler_deadline_budget(&key, deadline_at_ms).await?;
        let result = match tokio::time::timeout(
            budget,
            self.tool_handler.invoke(ResolvedCompleteAgentToolCallback {
                context,
                invocation: call,
            }),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                self.quarantine_handler_timeout(&key).await?;
                return Err(callback_error(
                    AgentHostCallbackErrorCode::DeadlineExceeded,
                    "callback handler crossed its absolute deadline; outcome requires inspection",
                    false,
                ));
            }
        };
        self.ensure_handler_completed_before_deadline(&key, deadline_at_ms)
            .await?;
        self.reconcile_tool(&key, &request_digest, result.clone())
            .await?;
        result
    }

    async fn invoke_hook(
        &self,
        call: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        let (snapshot, route, key, context) = self.route_and_key(&call.meta).await?;
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
                bound_surface_digest: route.bound_surface.digest.clone(),
                deadline_at_ms: call.meta.deadline_at_ms,
                state: CompleteAgentCallbackReservationState::Pending,
            },
        )
        .await?;
        let budget = self
            .handler_deadline_budget(&key, call.meta.deadline_at_ms)
            .await?;
        let result = match tokio::time::timeout(
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
            Err(_) => {
                self.quarantine_handler_timeout(&key).await?;
                return Err(callback_error(
                    AgentHostCallbackErrorCode::DeadlineExceeded,
                    "callback handler crossed its absolute deadline; outcome requires inspection",
                    false,
                ));
            }
        };
        self.ensure_handler_completed_before_deadline(&key, call.meta.deadline_at_ms)
            .await?;
        self.reconcile_hook(&key, &request_digest, result.clone())
            .await?;
        result
    }
}

fn resolve_callback_context(
    facts: &CompleteAgentHostFacts,
    route: &CompleteAgentCallbackRoute,
) -> Result<ResolvedCompleteAgentCallbackContext, AgentHostCallbackError> {
    let binding = facts.bindings.get(&route.binding_id).ok_or_else(|| {
        callback_invariant_error("callback route has no committed owning binding")
    })?;
    let applied_surface = binding.applied_surface.as_ref().ok_or_else(|| {
        callback_invariant_error("callback binding has no committed applied surface")
    })?;
    if binding.id != route.binding_id
        || binding.state != CompleteAgentBindingState::Available
        || binding.generation != route.generation
        || binding.source != route.source
        || binding.bound_surface != route.bound_surface
        || facts.source_coordinates.get(&binding.id) != Some(&binding.source)
        || !binding.bound_surface.accepts_applied(applied_surface)
    {
        return Err(callback_invariant_error(
            "callback route, binding, source, and applied surface facts are inconsistent",
        ));
    }

    let mut targets = facts.runtime_targets.iter().filter(|(thread_id, target)| {
        *thread_id == &target.runtime_thread_id
            && target.callbacks.route_id == route.route_id
            && target.callbacks.binding_generation == route.generation
            && target.callbacks.delivery == route.delivery
            && target.callbacks.default_deadline_ms == route.default_deadline_ms
            && target.service_instance_id == binding.service_instance_id
            && target.generation == binding.generation
            && target.profile_digest == binding.profile_digest
            && target.bound_surface == binding.bound_surface
    });
    let Some((runtime_thread_id, target)) = targets.next() else {
        return Err(callback_invariant_error(
            "callback facts do not resolve one Runtime target",
        ));
    };
    if targets.next().is_some() {
        return Err(callback_invariant_error(
            "callback facts resolve multiple Runtime targets",
        ));
    }

    Ok(ResolvedCompleteAgentCallbackContext {
        runtime_thread_id: runtime_thread_id.clone(),
        binding_id: binding.id.clone(),
        binding_generation: binding.generation,
        source: binding.source.clone(),
        service_instance_id: target.service_instance_id.clone(),
        profile_digest: target.profile_digest.clone(),
        bound_surface_revision: binding.bound_surface.revision,
        bound_surface_digest: binding.bound_surface.digest.clone(),
        bound_surface_offer_profile_digest: binding.bound_surface.offer_profile_digest.clone(),
        applied_surface_revision: applied_surface.revision,
        applied_surface_digest: applied_surface.digest.clone(),
    })
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
        // `Allow` is also the neutral continue/no-op result for non-blocking mutation/effect
        // hooks. It does not claim that the surface exposes a blocking policy action.
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

fn host_store_error(error: CompleteAgentHostStoreError) -> AgentHostCallbackError {
    match error {
        CompleteAgentHostStoreError::Conflict { .. } => pending_callback_error(
            "callback route state changed concurrently; inspect durable Host state before retry",
        ),
        CompleteAgentHostStoreError::Invariant { reason } => callback_error(
            AgentHostCallbackErrorCode::Internal,
            format!("callback Host invariant failed: {reason}"),
            false,
        ),
        CompleteAgentHostStoreError::Persistence { reason } => pending_callback_error(format!(
            "callback Host persistence is unavailable: {reason}"
        )),
    }
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

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    };

    use agentdash_agent_service_api::{
        AgentEffectIdentity, AgentHookBlockingSemantics, AgentHookDefinitionId,
        AgentHookEffectKind, AgentHookMutationKind, AgentHookPoint, AgentHookSemanticFacet,
        AgentHookTiming, AgentHostCallbackMeta, AgentIdempotencyKey, AgentPayloadDigest,
        AgentProfileDigest, AgentSurfaceContributionPayload, AgentSurfaceDigest,
        AgentSurfaceRevision, AgentSurfaceSemanticFacet, AgentToolDelivery, AgentToolName,
        AgentToolSemanticFacet, AgentToolUpdateSemantics, AgentTurnId, AppliedAgentSurface,
        AppliedAgentSurfaceContribution, AppliedContributionStatus, BoundAgentSurfaceContribution,
        SemanticFidelity,
    };
    use serde_json::json;
    use tokio::sync::Mutex;

    use super::*;
    use crate::{
        CompleteAgentBinding, CompleteAgentHostCommit, CompleteAgentHostRepository,
        CompleteAgentHostSnapshot, CompleteAgentRuntimeTarget, apply_complete_agent_host_commit,
    };

    #[test]
    fn neutral_allow_is_valid_for_non_blocking_hook_actions() {
        let mut call = hook_call();
        call.allowed_actions = BTreeSet::from([AgentHookAction::EmitEffect]);

        ensure_hook_decision_allowed(&call, &AgentHookDecision::Allow)
            .expect("neutral allow must not fabricate blocking capability");
    }

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
    struct FixtureCallbackRouteRepository {
        snapshot: Mutex<CompleteAgentHostSnapshot>,
    }

    impl FixtureCallbackRouteRepository {
        fn with_route(route: CompleteAgentCallbackRoute) -> Self {
            let mut snapshot = CompleteAgentHostSnapshot::default();
            let service_instance_id =
                AgentServiceInstanceId::new("service").expect("service instance");
            let runtime_thread_id = RuntimeThreadId::new("runtime-thread").expect("Runtime thread");
            let binding = CompleteAgentBinding {
                id: route.binding_id.clone(),
                service_instance_id: service_instance_id.clone(),
                generation: route.generation,
                source: route.source.clone(),
                profile_digest: route.bound_surface.offer_profile_digest.clone(),
                bound_surface: route.bound_surface.clone(),
                applied_surface: Some(applied_surface(&route.bound_surface)),
                state: CompleteAgentBindingState::Available,
            };
            snapshot.facts.bindings.insert(binding.id.clone(), binding);
            snapshot
                .facts
                .source_coordinates
                .insert(route.binding_id.clone(), route.source.clone());
            snapshot.facts.runtime_targets.insert(
                runtime_thread_id.clone(),
                CompleteAgentRuntimeTarget {
                    runtime_thread_id,
                    service_instance_id,
                    generation: route.generation,
                    profile_digest: route.bound_surface.offer_profile_digest.clone(),
                    bound_surface: route.bound_surface.clone(),
                    callbacks: AgentHostCallbackBinding {
                        route_id: route.route_id.clone(),
                        binding_generation: route.generation,
                        delivery: route.delivery,
                        default_deadline_ms: route.default_deadline_ms,
                    },
                },
            );
            snapshot
                .facts
                .callback_routes
                .insert(route.route_id.clone(), route);
            Self {
                snapshot: Mutex::new(snapshot),
            }
        }
    }

    #[async_trait]
    impl CompleteAgentHostRepository for FixtureCallbackRouteRepository {
        async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
            Ok(self.snapshot.lock().await.clone())
        }

        async fn commit(
            &self,
            commit: CompleteAgentHostCommit,
        ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
            let mut snapshot = self.snapshot.lock().await;
            apply_complete_agent_host_commit(&mut snapshot, commit)
        }
    }

    #[derive(Default)]
    struct CountingToolHandler {
        calls: AtomicUsize,
        callbacks: Mutex<Vec<ResolvedCompleteAgentToolCallback>>,
    }

    #[async_trait]
    impl CompleteAgentToolHandler for CountingToolHandler {
        async fn invoke(
            &self,
            callback: ResolvedCompleteAgentToolCallback,
        ) -> Result<AgentToolResult, AgentHostCallbackError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.callbacks.lock().await.push(callback);
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
            _callback: ResolvedCompleteAgentHookCallback,
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
            _callback: ResolvedCompleteAgentToolCallback,
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
        callbacks: Mutex<Vec<ResolvedCompleteAgentHookCallback>>,
    }

    #[async_trait]
    impl CompleteAgentHookHandler for InvalidHookHandler {
        async fn invoke(
            &self,
            callback: ResolvedCompleteAgentHookCallback,
        ) -> Result<AgentHookDecision, AgentHostCallbackError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.callbacks.lock().await.push(callback);
            Ok(AgentHookDecision::ReplaceInput { input: json!({}) })
        }
    }

    #[derive(Default)]
    struct DeadlineCrossingToolHandler {
        calls: AtomicUsize,
        late_completions: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl CompleteAgentToolHandler for DeadlineCrossingToolHandler {
        async fn invoke(
            &self,
            _callback: ResolvedCompleteAgentToolCallback,
        ) -> Result<AgentToolResult, AgentHostCallbackError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let late_completions = self.late_completions.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                late_completions.fetch_add(1, Ordering::SeqCst);
            });
            std::future::pending().await
        }
    }

    #[derive(Default)]
    struct DeadlineCrossingHookHandler {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl CompleteAgentHookHandler for DeadlineCrossingHookHandler {
        async fn invoke(
            &self,
            _callback: ResolvedCompleteAgentHookCallback,
        ) -> Result<AgentHookDecision, AgentHostCallbackError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::future::pending().await
        }
    }

    struct DeadlineCrossingSuccessToolHandler {
        calls: AtomicUsize,
        clock: Arc<MutableClock>,
    }

    #[async_trait]
    impl CompleteAgentToolHandler for DeadlineCrossingSuccessToolHandler {
        async fn invoke(
            &self,
            _callback: ResolvedCompleteAgentToolCallback,
        ) -> Result<AgentToolResult, AgentHostCallbackError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.clock.set(20);
            Ok(AgentToolResult::Completed {
                output: json!({"too_late": true}),
            })
        }
    }

    struct FixedClock(u64);

    impl AgentCallbackClock for FixedClock {
        fn now_ms(&self) -> u64 {
            self.0
        }
    }

    struct MutableClock(AtomicU64);

    impl MutableClock {
        fn new(now_ms: u64) -> Self {
            Self(AtomicU64::new(now_ms))
        }

        fn set(&self, now_ms: u64) {
            self.0.store(now_ms, Ordering::SeqCst);
        }
    }

    impl AgentCallbackClock for MutableClock {
        fn now_ms(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn duplicate_tool_callback_replays_one_result() {
        let tool_handler = Arc::new(CountingToolHandler::default());
        let repository = Arc::new(FixtureCallbackRepository::default());
        let host_repository = Arc::new(FixtureCallbackRouteRepository::with_route(route(
            AgentBindingGeneration(2),
        )));
        let broker = CompleteAgentCallbackBroker::with_clock(
            tool_handler.clone(),
            Arc::new(AllowHookHandler),
            host_repository.clone(),
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
        let call = tool_call(AgentBindingGeneration(2), 20);
        let expected_call = call.clone();

        let first = broker.invoke_tool(call.clone()).await.expect("first");
        let restarted = CompleteAgentCallbackBroker::with_clock(
            tool_handler.clone(),
            Arc::new(AllowHookHandler),
            host_repository,
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
        let second = restarted.invoke_tool(call).await.expect("replay");

        assert_eq!(first, second);
        assert_eq!(tool_handler.calls.load(Ordering::SeqCst), 1);
        let callbacks = tool_handler.callbacks.lock().await;
        assert_eq!(callbacks.len(), 1);
        assert_eq!(callbacks[0].invocation, expected_call);
        assert_eq!(
            callbacks[0].context,
            ResolvedCompleteAgentCallbackContext {
                runtime_thread_id: RuntimeThreadId::new("runtime-thread").expect("Runtime thread"),
                binding_id: CompleteAgentBindingId::new("binding").expect("binding"),
                binding_generation: AgentBindingGeneration(2),
                source: AgentSourceCoordinate::new("source").expect("source"),
                service_instance_id: AgentServiceInstanceId::new("service")
                    .expect("service instance"),
                profile_digest: AgentProfileDigest::new("profile").expect("profile"),
                bound_surface_revision: AgentSurfaceRevision(1),
                bound_surface_digest: AgentSurfaceDigest::new("surface").expect("surface"),
                bound_surface_offer_profile_digest: AgentProfileDigest::new("profile")
                    .expect("profile"),
                applied_surface_revision: AgentSurfaceRevision(1),
                applied_surface_digest: AgentSurfaceDigest::new("surface").expect("surface"),
            }
        );
        drop(callbacks);

        let snapshot = repository.load().await.expect("callback snapshot");
        let encoded =
            encode_complete_agent_callback_snapshot(&snapshot).expect("encode callback snapshot");
        let decoded =
            decode_complete_agent_callback_snapshot(encoded).expect("decode callback snapshot");
        assert_eq!(decoded, snapshot);
        assert!(decoded.facts.callbacks.values().any(|record| {
            matches!(
                record.state,
                CompleteAgentCallbackReservationState::Settled(
                    CompleteAgentCallbackTerminalOutcome::Tool { .. }
                )
            )
        }));

        let mut wrong_kind = snapshot;
        wrong_kind
            .facts
            .callbacks
            .values_mut()
            .next()
            .expect("callback")
            .kind = CompleteAgentCallbackKind::Hook;
        let encoded = serde_json::to_value(wrong_kind).expect("encode invalid callback snapshot");
        assert!(matches!(
            decode_complete_agent_callback_snapshot(encoded),
            Err(CompleteAgentCallbackStoreError::Invariant { .. })
        ));
    }

    #[tokio::test]
    async fn unresolved_or_ambiguous_runtime_target_never_reaches_handler() {
        let missing_handler = Arc::new(CountingToolHandler::default());
        let missing_callbacks = Arc::new(FixtureCallbackRepository::default());
        let missing_host = Arc::new(FixtureCallbackRouteRepository::with_route(route(
            AgentBindingGeneration(2),
        )));
        missing_host
            .snapshot
            .lock()
            .await
            .facts
            .runtime_targets
            .clear();
        let missing_broker = CompleteAgentCallbackBroker::with_clock(
            missing_handler.clone(),
            Arc::new(AllowHookHandler),
            missing_host,
            missing_callbacks.clone(),
            Arc::new(FixedClock(10)),
        );

        let missing = missing_broker
            .invoke_tool(tool_call(AgentBindingGeneration(2), 20))
            .await
            .expect_err("missing Runtime target");
        assert_eq!(missing.code, AgentHostCallbackErrorCode::Internal);
        assert_eq!(missing_handler.calls.load(Ordering::SeqCst), 0);
        assert!(
            missing_callbacks
                .load()
                .await
                .expect("callback snapshot")
                .facts
                .callbacks
                .is_empty()
        );

        let ambiguous_handler = Arc::new(CountingToolHandler::default());
        let ambiguous_callbacks = Arc::new(FixtureCallbackRepository::default());
        let ambiguous_host = Arc::new(FixtureCallbackRouteRepository::with_route(route(
            AgentBindingGeneration(2),
        )));
        {
            let mut snapshot = ambiguous_host.snapshot.lock().await;
            let mut duplicate = snapshot
                .facts
                .runtime_targets
                .values()
                .next()
                .expect("Runtime target")
                .clone();
            let duplicate_thread =
                RuntimeThreadId::new("runtime-thread-duplicate").expect("Runtime thread");
            duplicate.runtime_thread_id = duplicate_thread.clone();
            snapshot
                .facts
                .runtime_targets
                .insert(duplicate_thread, duplicate);
        }
        let ambiguous_broker = CompleteAgentCallbackBroker::with_clock(
            ambiguous_handler.clone(),
            Arc::new(AllowHookHandler),
            ambiguous_host,
            ambiguous_callbacks.clone(),
            Arc::new(FixedClock(10)),
        );

        let ambiguous = ambiguous_broker
            .invoke_tool(tool_call(AgentBindingGeneration(2), 20))
            .await
            .expect_err("ambiguous Runtime target");
        assert_eq!(ambiguous.code, AgentHostCallbackErrorCode::Internal);
        assert_eq!(ambiguous_handler.calls.load(Ordering::SeqCst), 0);
        assert!(
            ambiguous_callbacks
                .load()
                .await
                .expect("callback snapshot")
                .facts
                .callbacks
                .is_empty()
        );
    }

    #[tokio::test]
    async fn missing_or_tombstoned_host_route_never_reserves_or_replays_callback() {
        let tool_handler = Arc::new(CountingToolHandler::default());
        let callback_repository = Arc::new(FixtureCallbackRepository::default());
        let missing_host_repository = Arc::new(FixtureCallbackRouteRepository::default());
        let missing_broker = CompleteAgentCallbackBroker::with_clock(
            tool_handler.clone(),
            Arc::new(AllowHookHandler),
            missing_host_repository,
            callback_repository.clone(),
            Arc::new(FixedClock(10)),
        );
        let call = tool_call(AgentBindingGeneration(2), 20);

        let missing = missing_broker
            .invoke_tool(call.clone())
            .await
            .expect_err("missing route");
        assert_eq!(missing.code, AgentHostCallbackErrorCode::UnknownRoute);
        assert!(
            callback_repository
                .load()
                .await
                .expect("callback snapshot")
                .facts
                .callbacks
                .is_empty(),
            "a missing durable Host route cannot create a callback reservation"
        );

        let host_repository = Arc::new(FixtureCallbackRouteRepository::with_route(route(
            AgentBindingGeneration(2),
        )));
        let broker = CompleteAgentCallbackBroker::with_clock(
            tool_handler.clone(),
            Arc::new(AllowHookHandler),
            host_repository.clone(),
            callback_repository.clone(),
            Arc::new(FixedClock(10)),
        );
        broker
            .invoke_tool(call.clone())
            .await
            .expect("initial callback");
        host_repository
            .snapshot
            .lock()
            .await
            .facts
            .revoked_callback_routes
            .insert(call.meta.route_id.clone());

        let restarted = CompleteAgentCallbackBroker::with_clock(
            tool_handler.clone(),
            Arc::new(AllowHookHandler),
            host_repository,
            callback_repository,
            Arc::new(FixedClock(10)),
        );
        let revoked = restarted
            .invoke_tool(call)
            .await
            .expect_err("tombstoned route");
        assert_eq!(
            revoked.code,
            AgentHostCallbackErrorCode::StaleBindingGeneration
        );
        assert_eq!(
            tool_handler.calls.load(Ordering::SeqCst),
            1,
            "the tombstone fence wins over a previously settled callback outcome"
        );
    }

    #[tokio::test]
    async fn stale_generation_and_elapsed_deadline_are_rejected() {
        let repository = Arc::new(FixtureCallbackRepository::default());
        let host_repository = Arc::new(FixtureCallbackRouteRepository::with_route(route(
            AgentBindingGeneration(2),
        )));
        let tool_handler = Arc::new(CountingToolHandler::default());
        let broker = CompleteAgentCallbackBroker::with_clock(
            tool_handler.clone(),
            Arc::new(AllowHookHandler),
            host_repository,
            repository.clone(),
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
        assert_eq!(
            tool_handler.calls.load(Ordering::SeqCst),
            0,
            "stale or malformed deadline evidence must not reach the platform handler"
        );
        assert!(
            repository
                .load()
                .await
                .expect("callback snapshot")
                .facts
                .callbacks
                .is_empty(),
            "rejected callback evidence must not create a durable reservation"
        );
    }

    #[tokio::test]
    async fn hook_decision_outside_bound_actions_is_rejected_and_replayed_once() {
        let hook_handler = Arc::new(InvalidHookHandler::default());
        let repository = Arc::new(FixtureCallbackRepository::default());
        let host_repository = Arc::new(FixtureCallbackRouteRepository::with_route(hook_route()));
        let broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            hook_handler.clone(),
            host_repository.clone(),
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
        let call = hook_call();
        let expected_call = call.clone();

        let first = broker
            .invoke_hook(call.clone())
            .await
            .expect_err("invalid decision");
        let restarted = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            hook_handler.clone(),
            host_repository,
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
        let callbacks = hook_handler.callbacks.lock().await;
        assert_eq!(callbacks.len(), 1);
        assert_eq!(callbacks[0].invocation, expected_call);
        assert_eq!(
            callbacks[0].context.runtime_thread_id,
            RuntimeThreadId::new("runtime-thread").expect("Runtime thread")
        );
        assert_eq!(
            callbacks[0].context.binding_id,
            CompleteAgentBindingId::new("binding").expect("binding")
        );
    }

    #[tokio::test]
    async fn pending_callback_survives_restart_and_requires_explicit_reconciliation() {
        let repository = Arc::new(FixtureCallbackRepository::default());
        let host_repository = Arc::new(FixtureCallbackRouteRepository::with_route(route(
            AgentBindingGeneration(2),
        )));
        let failing_handler = Arc::new(FailSettleToolHandler {
            calls: AtomicUsize::new(0),
            repository: repository.clone(),
        });
        let broker = CompleteAgentCallbackBroker::with_clock(
            failing_handler.clone(),
            Arc::new(AllowHookHandler),
            host_repository.clone(),
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
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
            host_repository,
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
    async fn tool_handler_crossing_deadline_is_quarantined_across_restart_and_late_completion() {
        let repository = Arc::new(FixtureCallbackRepository::default());
        let host_repository = Arc::new(FixtureCallbackRouteRepository::with_route(route(
            AgentBindingGeneration(2),
        )));
        let handler = Arc::new(DeadlineCrossingToolHandler::default());
        let broker = CompleteAgentCallbackBroker::with_clock(
            handler.clone(),
            Arc::new(AllowHookHandler),
            host_repository.clone(),
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
        let call = tool_call(AgentBindingGeneration(2), 20);
        let key = CompleteAgentCallbackKey {
            route_id: call.meta.route_id.clone(),
            idempotency_key: call.meta.idempotency_key.clone(),
        };

        let timeout = broker
            .invoke_tool(call.clone())
            .await
            .expect_err("handler must be bounded by the absolute callback deadline");
        assert_eq!(timeout.code, AgentHostCallbackErrorCode::DeadlineExceeded);
        assert!(!timeout.retryable);
        assert_eq!(handler.calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            broker
                .inspect_callback(&key)
                .await
                .expect("inspect callback")
                .expect("callback")
                .state,
            CompleteAgentCallbackReservationState::InspectionRequired { .. }
        ));

        let restarted_handler = Arc::new(CountingToolHandler::default());
        let restarted = CompleteAgentCallbackBroker::with_clock(
            restarted_handler.clone(),
            Arc::new(AllowHookHandler),
            host_repository,
            repository,
            Arc::new(FixedClock(10)),
        );
        let replay = restarted
            .invoke_tool(call.clone())
            .await
            .expect_err("an inspection-required effect must never be re-executed");
        assert_eq!(replay.code, AgentHostCallbackErrorCode::Unavailable);
        assert!(replay.retryable);
        assert_eq!(restarted_handler.calls.load(Ordering::SeqCst), 0);

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(handler.late_completions.load(Ordering::SeqCst), 1);
        assert!(matches!(
            restarted
                .inspect_callback(&key)
                .await
                .expect("inspect callback after late completion")
                .expect("callback")
                .state,
            CompleteAgentCallbackReservationState::InspectionRequired { .. }
        ));
        restarted
            .invoke_tool(call)
            .await
            .expect_err("a late side effect cannot reopen execution");
        assert_eq!(restarted_handler.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn handler_returning_success_after_absolute_deadline_cannot_settle_success() {
        let repository = Arc::new(FixtureCallbackRepository::default());
        let clock = Arc::new(MutableClock::new(10));
        let handler = Arc::new(DeadlineCrossingSuccessToolHandler {
            calls: AtomicUsize::new(0),
            clock: clock.clone(),
        });
        let broker = CompleteAgentCallbackBroker::with_clock(
            handler.clone(),
            Arc::new(AllowHookHandler),
            Arc::new(FixtureCallbackRouteRepository::with_route(route(
                AgentBindingGeneration(2),
            ))),
            repository,
            clock,
        );
        let call = tool_call(AgentBindingGeneration(2), 20);
        let key = CompleteAgentCallbackKey {
            route_id: call.meta.route_id.clone(),
            idempotency_key: call.meta.idempotency_key.clone(),
        };

        let timeout = broker
            .invoke_tool(call)
            .await
            .expect_err("late success is an uncertain side effect, not a settled success");
        assert_eq!(timeout.code, AgentHostCallbackErrorCode::DeadlineExceeded);
        assert_eq!(handler.calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            broker
                .inspect_callback(&key)
                .await
                .expect("inspect callback")
                .expect("callback")
                .state,
            CompleteAgentCallbackReservationState::InspectionRequired { .. }
        ));
    }

    #[tokio::test]
    async fn hook_handler_crossing_deadline_uses_the_same_effect_quarantine() {
        let repository = Arc::new(FixtureCallbackRepository::default());
        let hook_handler = Arc::new(DeadlineCrossingHookHandler::default());
        let host_repository = Arc::new(FixtureCallbackRouteRepository::with_route(hook_route()));
        let broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            hook_handler.clone(),
            host_repository.clone(),
            repository.clone(),
            Arc::new(FixedClock(10)),
        );
        let call = hook_call();

        let timeout = broker
            .invoke_hook(call.clone())
            .await
            .expect_err("hook handler must be bounded by the absolute callback deadline");
        assert_eq!(timeout.code, AgentHostCallbackErrorCode::DeadlineExceeded);
        assert!(!timeout.retryable);
        assert_eq!(hook_handler.calls.load(Ordering::SeqCst), 1);

        let restarted_handler = Arc::new(DeadlineCrossingHookHandler::default());
        let restarted = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            restarted_handler.clone(),
            host_repository,
            repository,
            Arc::new(FixedClock(10)),
        );
        let replay = restarted
            .invoke_hook(call)
            .await
            .expect_err("inspection-required hook effect must not be re-executed");
        assert_eq!(replay.code, AgentHostCallbackErrorCode::Unavailable);
        assert!(replay.retryable);
        assert_eq!(restarted_handler.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn hook_callback_cannot_exceed_its_bound_payload_deadline() {
        let host_repository = Arc::new(FixtureCallbackRouteRepository::with_route(
            hook_route_with_deadlines(100, 10),
        ));
        let broker = CompleteAgentCallbackBroker::with_clock(
            Arc::new(CountingToolHandler::default()),
            Arc::new(AllowHookHandler),
            host_repository,
            Arc::new(FixtureCallbackRepository::default()),
            Arc::new(FixedClock(10)),
        );

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
            CompleteAgentBindingId::new("binding").expect("binding"),
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
            CompleteAgentBindingId::new("binding").expect("binding"),
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
            binding_id: CompleteAgentBindingId::new("binding").expect("binding"),
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

    fn applied_surface(surface: &BoundAgentSurface) -> AppliedAgentSurface {
        AppliedAgentSurface {
            revision: surface.revision,
            digest: surface.digest.clone(),
            contributions: surface
                .contributions
                .iter()
                .map(|contribution| AppliedAgentSurfaceContribution {
                    key: contribution.key.clone(),
                    route: contribution.route,
                    fidelity: contribution.fidelity,
                    semantics: contribution.semantics.clone(),
                    payload_digest: contribution.payload_digest.clone(),
                    status: AppliedContributionStatus::Applied,
                    evidence: Some("fixture-applied".to_owned()),
                })
                .collect(),
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
            binding_id: CompleteAgentBindingId::new("binding").expect("binding"),
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
