use std::collections::BTreeSet;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use ts_rs::TS;

use crate::{
    AgentBindingGeneration, AgentCallbackRouteId, AgentChangePage, AgentChangesQuery,
    AgentCommandEnvelope, AgentCommandReceipt, AgentEffectIdentity, AgentEffectInspection,
    AgentHookAction, AgentHookDefinitionId, AgentHookPoint, AgentHookTiming, AgentIdempotencyKey,
    AgentInteractionId, AgentItemId, AgentLiveEventStream, AgentReadQuery, AgentServiceDescriptor,
    AgentSnapshot, AgentSourceCoordinate, AgentToolName, AgentTurnId, AppliedAgentSurfaceReceipt,
    ApplyBoundAgentSurface, CreateAgentCommand, ForkAgentCommand, ForkAgentReceipt,
    ResumeAgentCommand, RevokeBoundAgentSurface,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentServiceErrorCode {
    InvalidArgument,
    NotFound,
    Conflict,
    Unsupported,
    StaleBindingGeneration,
    DeadlineExceeded,
    Unavailable,
    ProtocolViolation,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize, JsonSchema, TS)]
#[error("{code:?}: {message}")]
#[serde(rename_all = "snake_case")]
pub struct AgentServiceError {
    pub code: AgentServiceErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl AgentServiceError {
    pub fn new(code: AgentServiceErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentHostCallbackMeta {
    pub route_id: AgentCallbackRouteId,
    pub binding_generation: AgentBindingGeneration,
    pub source: AgentSourceCoordinate,
    pub turn_id: AgentTurnId,
    pub item_id: Option<AgentItemId>,
    pub interaction_id: Option<AgentInteractionId>,
    pub effect_id: AgentEffectIdentity,
    pub idempotency_key: AgentIdempotencyKey,
    /// Absolute Unix epoch deadline. The Host must not start a callback after it.
    #[serde(with = "crate::wire_u64")]
    #[schemars(with = "crate::wire_u64::AgentServiceU64")]
    #[ts(type = "AgentServiceU64")]
    pub deadline_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentToolInvocation {
    pub meta: AgentHostCallbackMeta,
    pub tool: AgentToolName,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentToolResult {
    Completed { output: Value },
    Rejected { code: String, message: String },
    Failed { code: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentHookInvocation {
    pub meta: AgentHostCallbackMeta,
    pub definition_id: AgentHookDefinitionId,
    pub point: AgentHookPoint,
    pub timing: AgentHookTiming,
    pub allowed_actions: BTreeSet<AgentHookAction>,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentHookDecision {
    Allow,
    Deny { reason: String },
    ReplaceInput { input: Value },
    ReplaceResult { result: Value },
    AddContext { context: Value },
    EmitEffect { effect: Value },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentHostCallbackErrorCode {
    InvalidArgument,
    UnknownRoute,
    StaleBindingGeneration,
    DeadlineExceeded,
    DuplicateConflict,
    Unsupported,
    Unavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize, JsonSchema, TS)]
#[error("{code:?}: {message}")]
#[serde(rename_all = "snake_case")]
pub struct AgentHostCallbackError {
    pub code: AgentHostCallbackErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl AgentHostCallbackError {
    pub fn new(
        code: AgentHostCallbackErrorCode,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

/// Reverse channel used by an Agent-native Tool or Hook to call the Runtime Host.
///
/// Implementations fence `binding_generation` and enforce the semantic deadline. The stable
/// `idempotency_key` is passed to the actual Tool/Hook owner, which owns effect inspection and
/// receipt replay when the handler can produce side effects.
#[async_trait]
pub trait AgentHostCallbacks: Send + Sync {
    async fn invoke_tool(
        &self,
        call: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError>;

    async fn invoke_hook(
        &self,
        call: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError>;
}

/// Finite Host-to-Agent contract. A Complete Agent remains authoritative for its own history,
/// context/compaction, fork lineage, and native lifecycle.
#[async_trait]
pub trait CompleteAgentService: Send + Sync {
    async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError>;

    async fn create(
        &self,
        command: CreateAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError>;

    async fn resume(
        &self,
        command: ResumeAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError>;

    async fn fork(&self, command: ForkAgentCommand) -> Result<ForkAgentReceipt, AgentServiceError>;

    async fn execute(
        &self,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentServiceError>;

    async fn read(&self, query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError>;

    async fn changes(&self, query: AgentChangesQuery)
    -> Result<AgentChangePage, AgentServiceError>;

    async fn live_events(
        &self,
        _source: AgentSourceCoordinate,
    ) -> Result<Box<dyn AgentLiveEventStream>, AgentServiceError> {
        Err(AgentServiceError::new(
            AgentServiceErrorCode::Unsupported,
            "Complete Agent does not expose process-local live events",
            false,
        ))
    }

    async fn inspect(
        &self,
        identity: AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, AgentServiceError>;

    async fn apply_surface(
        &self,
        command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError>;

    async fn revoke_surface(
        &self,
        command: RevokeBoundAgentSurface,
    ) -> Result<AgentCommandReceipt, AgentServiceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_meta_keeps_effect_and_generation_distinct() {
        let meta = AgentHostCallbackMeta {
            route_id: AgentCallbackRouteId::new("route-1").expect("route"),
            binding_generation: AgentBindingGeneration(7),
            source: AgentSourceCoordinate::new("source-1").expect("source"),
            turn_id: AgentTurnId::new("turn-1").expect("turn"),
            item_id: None,
            interaction_id: None,
            effect_id: AgentEffectIdentity::new("effect-1").expect("effect"),
            idempotency_key: AgentIdempotencyKey::new("idem-1").expect("idempotency"),
            deadline_at_ms: 42,
        };

        assert_eq!(meta.binding_generation, AgentBindingGeneration(7));
        assert_eq!(meta.effect_id.as_str(), "effect-1");
    }

    #[test]
    fn hook_decision_is_typed_not_an_observation() {
        let decision = AgentHookDecision::Deny {
            reason: "policy".to_owned(),
        };
        assert!(matches!(decision, AgentHookDecision::Deny { .. }));
    }
}
