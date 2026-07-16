use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    BindingEpoch, ContextActivationId, ContextCandidateId, ContextCheckpointId,
    ContextCompactionId, ContextDigest, ContextRevision, DriverContextRevision, DriverThreadId,
    EventSequence, HookDefinitionId, HookEffectId, HookPlanDigest, HookPlanRevision, HookPoint,
    HookRunId, IdempotencyKey, ProfileDigest, RuntimeBindingId, RuntimeDriverGeneration,
    RuntimeInteractionId, RuntimeItemId, RuntimeOperationId, RuntimeProfile,
    RuntimeRecoveryIntentId, RuntimeRevision, RuntimeThreadId, RuntimeTransientEventId,
    RuntimeTransientSequence, RuntimeTurnId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum HookRunDecision {
    Continue,
    Block,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum HookRunTerminal {
    Completed,
    Blocked,
    Failed,
    Stopped,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeThreadStatus {
    Active,
    Suspended,
    Desynchronized,
    Closed,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTurnTerminal {
    Completed,
    Interrupted,
    Refused,
    LimitReached,
    Failed,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeOperationTerminal {
    Succeeded,
    Failed {
        retryable: bool,
        message: Option<String>,
    },
    Lost {
        retryable: bool,
        message: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeItemTerminal {
    Completed { final_content: RuntimeItemContent },
    Failed { message: Option<String> },
    Cancelled { message: Option<String> },
    Lost { message: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProtocolViolationCode {
    DriverOperationAcceptance,
    DriverRuntimeOwnedContextEvent,
    DriverRuntimeOwnedHookEvent,
    DriverRuntimeOwnedBindingEvent,
    InvalidLifecycleTransition,
    DuplicateTerminal,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInteractionKind {
    CommandApproval,
    FileChangeApproval,
    PermissionApproval,
    UserInputRequest,
    McpElicitation,
    DynamicToolExecution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePermissionApprovalRequest {
    pub item_id: String,
    pub target: RuntimePermissionApprovalTarget,
    pub reason: Option<String>,
    pub started_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimePermissionApprovalTarget {
    ToolInvocation {
        capability_key: Option<String>,
        tool_name: String,
        arguments: serde_json::Value,
        details: Option<serde_json::Value>,
    },
    Hook {
        hook_run_id: String,
    },
    WorkspacePermissions {
        cwd: String,
        permissions: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeInteractionRequest {
    CommandApproval { params: Box<agentdash_agent_protocol::generated::codex_v2::command_execution_request_approval_params::CommandExecutionRequestApprovalParams> },
    FileChangeApproval { params: Box<agentdash_agent_protocol::generated::codex_v2::file_change_request_approval_params::FileChangeRequestApprovalParams> },
    PermissionApproval { params: Box<RuntimePermissionApprovalRequest> },
    UserInputRequest { params: Box<agentdash_agent_protocol::generated::codex_v2::tool_request_user_input_params::ToolRequestUserInputParams> },
    McpElicitation { params: Box<agentdash_agent_protocol::generated::codex_v2::mcp_server_elicitation_request_params::McpServerElicitationRequestParams> },
    DynamicToolExecution { params: Box<agentdash_agent_protocol::generated::codex_v2::dynamic_tool_call_params::DynamicToolCallParams> },
}

impl RuntimeInteractionRequest {
    pub fn kind(&self) -> RuntimeInteractionKind {
        match self {
            Self::CommandApproval { .. } => RuntimeInteractionKind::CommandApproval,
            Self::FileChangeApproval { .. } => RuntimeInteractionKind::FileChangeApproval,
            Self::PermissionApproval { .. } => RuntimeInteractionKind::PermissionApproval,
            Self::UserInputRequest { .. } => RuntimeInteractionKind::UserInputRequest,
            Self::McpElicitation { .. } => RuntimeInteractionKind::McpElicitation,
            Self::DynamicToolExecution { .. } => RuntimeInteractionKind::DynamicToolExecution,
        }
    }

    pub fn tool_permission_approval(
        item_id: &str,
        capability_key: Option<String>,
        tool_name: String,
        arguments: serde_json::Value,
        details: Option<serde_json::Value>,
        reason: String,
    ) -> Self {
        Self::PermissionApproval {
            params: Box::new(RuntimePermissionApprovalRequest {
                item_id: item_id.to_string(),
                target: RuntimePermissionApprovalTarget::ToolInvocation {
                    capability_key,
                    tool_name,
                    arguments,
                    details,
                },
                reason: Some(reason),
                started_at_ms: current_epoch_millis(),
            }),
        }
    }

    pub fn hook_approval(item_id: &str, hook_run_id: String, reason: String) -> Self {
        Self::PermissionApproval {
            params: Box::new(RuntimePermissionApprovalRequest {
                item_id: item_id.to_string(),
                target: RuntimePermissionApprovalTarget::Hook { hook_run_id },
                reason: Some(reason),
                started_at_ms: current_epoch_millis(),
            }),
        }
    }

    pub fn workspace_permission_approval(
        item_id: String,
        cwd: String,
        permissions: serde_json::Value,
        reason: Option<String>,
        started_at_ms: i64,
    ) -> Self {
        Self::PermissionApproval {
            params: Box::new(RuntimePermissionApprovalRequest {
                item_id,
                target: RuntimePermissionApprovalTarget::WorkspacePermissions { cwd, permissions },
                reason,
                started_at_ms,
            }),
        }
    }

    pub fn temporary_command_approval(
        thread_id: &str,
        turn_id: &str,
        item_id: &str,
        command: &str,
    ) -> Self {
        Self::CommandApproval {
            params: Box::new(
                serde_json::from_value(serde_json::json!({
                    "approvalId": null, "command": command, "commandActions": [], "cwd": null,
                    "itemId": item_id, "reason": "approve?", "startedAtMs": 0,
                    "threadId": thread_id, "turnId": turn_id
                }))
                .expect("temporary owned command approval"),
            ),
        }
    }

    pub fn temporary_user_input(
        thread_id: &str,
        turn_id: &str,
        item_id: &str,
        prompt: &str,
    ) -> Self {
        Self::UserInputRequest { params: Box::new(serde_json::from_value(serde_json::json!({
            "autoResolutionMs": null, "itemId": item_id,
            "questions": [{"id":"input", "header":"Input", "question":prompt, "options":null, "isOther":false, "isSecret":false}],
            "threadId": thread_id, "turnId": turn_id
        })).expect("temporary owned user input")) }
    }

    pub fn temporary_dynamic_interaction(thread_id: &str, turn_id: &str, call_id: &str) -> Self {
        Self::DynamicToolExecution {
            params: Box::new(
                serde_json::from_value(serde_json::json!({
                    "arguments": {}, "callId": call_id, "namespace": null,
                    "threadId": thread_id, "tool": "dynamic_tool", "turnId": turn_id
                }))
                .expect("temporary owned dynamic tool call"),
            ),
        }
    }
}

fn current_epoch_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(i64::MAX, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInteractionTerminal {
    Resolved,
    Expired,
    Cancelled,
    Failed,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct RuntimeItemContent(pub Box<agentdash_agent_protocol::AgentDashThreadItem>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum ToolProtocolProjection {
    Command,
    FileChange,
    FsRead,
    FsGrep,
    FsGlob,
    Mcp { server_key: String },
    Dynamic { namespace: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ToolPresentationEmitter {
    VendorStream,
    ToolBroker,
}

impl RuntimeItemContent {
    pub fn new(item: agentdash_agent_protocol::AgentDashThreadItem) -> Self {
        Self(Box::new(item))
    }

    pub fn item(&self) -> &agentdash_agent_protocol::AgentDashThreadItem {
        &self.0
    }

    pub fn agent_message_text(&self) -> Option<&str> {
        match self.item() {
            agentdash_agent_protocol::AgentDashThreadItem::Codex(
                agentdash_agent_protocol::CodexThreadItem::AgentMessage { text, .. },
            ) => Some(text),
            _ => None,
        }
    }

    pub fn agent_message(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::codex_json(serde_json::json!({
            "type": "agentMessage", "id": id.into(), "text": text.into()
        }))
    }

    pub fn reasoning(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::codex_json(serde_json::json!({
            "type": "reasoning", "id": id.into(), "summary": [text.into()]
        }))
    }

    pub fn user_message(id: impl Into<String>, input: Vec<crate::RuntimeInput>) -> Self {
        let content = input
            .into_iter()
            .filter_map(|input| match input {
                crate::RuntimeInput::UserInput { block } => Some(block),
                crate::RuntimeInput::Structured { .. } => None,
            })
            .collect::<Vec<_>>();
        Self::codex_json(
            serde_json::json!({"type":"userMessage", "id":id.into(), "content":content}),
        )
    }

    fn codex_json(value: serde_json::Value) -> Self {
        let item = serde_json::from_value(value).expect("runtime-owned Codex item constructor");
        Self::new(agentdash_agent_protocol::AgentDashThreadItem::Codex(item))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeConversationDelta {
    AgentMessage {
        delta: String,
    },
    ReasoningText {
        delta: String,
    },
    ReasoningSummary {
        delta: String,
    },
    CommandOutput {
        delta: String,
    },
    FileChangeOutput {
        delta: String,
    },
    Plan {
        delta: String,
    },
    McpProgress {
        message: String,
    },
    ToolProgress {
        content_items: Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeConversationError {
    pub code: Option<String>,
    pub message: String,
    pub retryable: bool,
    pub details: Option<RuntimeConversationErrorDetails>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeConversationErrorDetails {
    pub error_type: Option<String>,
    pub http_status: Option<u16>,
    pub request_id: Option<String>,
    pub metadata: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct RuntimeProviderStatus {
    pub phase: RuntimeProviderPhase,
    pub attempt: u32,
    pub max_attempts: u32,
    pub will_retry: bool,
    pub delay_ms: Option<u64>,
    pub reason_code: Option<String>,
    pub message: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProviderPhase {
    Connecting,
    ConnectedWaitingFirstDelta,
    Streaming,
    RetryScheduled,
    Retrying,
    Failed,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeEvent {
    OperationAccepted {
        operation_id: RuntimeOperationId,
    },
    OperationTerminal {
        operation_id: RuntimeOperationId,
        terminal: RuntimeOperationTerminal,
    },
    BindingEstablished {
        binding_id: RuntimeBindingId,
    },
    BindingLost {
        binding_id: RuntimeBindingId,
        reason: String,
    },
    BindingReestablished {
        recovery_intent_id: RuntimeRecoveryIntentId,
        binding_epoch: BindingEpoch,
        old_binding_id: RuntimeBindingId,
        old_driver_generation: RuntimeDriverGeneration,
        new_binding_id: RuntimeBindingId,
        new_driver_generation: RuntimeDriverGeneration,
        source_thread_id: DriverThreadId,
        profile_digest: ProfileDigest,
        bound_profile: Box<RuntimeProfile>,
    },
    ProtocolViolation {
        code: RuntimeProtocolViolationCode,
        message: String,
        critical: bool,
    },
    ThreadStatusChanged {
        status: RuntimeThreadStatus,
    },
    TurnStarted {
        turn_id: RuntimeTurnId,
        presentation_turn_id: crate::PresentationTurnId,
    },
    TurnTerminal {
        turn_id: RuntimeTurnId,
        terminal: RuntimeTurnTerminal,
        message: Option<String>,
        diagnostic: Option<agentdash_agent_protocol::RuntimeTerminalDiagnostic>,
    },
    ItemStarted {
        turn_id: RuntimeTurnId,
        item_id: RuntimeItemId,
        initial_content: RuntimeItemContent,
    },
    ConversationDelta {
        turn_id: RuntimeTurnId,
        item_id: RuntimeItemId,
        delta: RuntimeConversationDelta,
    },
    TokenUsageUpdated {
        turn_id: RuntimeTurnId,
        usage: RuntimeTokenUsage,
    },
    ConversationError {
        turn_id: Option<RuntimeTurnId>,
        error: RuntimeConversationError,
    },
    ProviderStatus {
        turn_id: RuntimeTurnId,
        status: RuntimeProviderStatus,
    },
    ItemTerminal {
        turn_id: RuntimeTurnId,
        item_id: RuntimeItemId,
        terminal: RuntimeItemTerminal,
    },
    InteractionRequested {
        turn_id: RuntimeTurnId,
        item_id: Option<RuntimeItemId>,
        interaction_id: RuntimeInteractionId,
        request: RuntimeInteractionRequest,
    },
    InteractionTerminal {
        turn_id: RuntimeTurnId,
        interaction_id: RuntimeInteractionId,
        terminal: RuntimeInteractionTerminal,
    },
    ContextCheckpointPrepared {
        checkpoint_id: ContextCheckpointId,
        candidate_id: ContextCandidateId,
        compaction_id: ContextCompactionId,
    },
    ContextActivationApplied {
        activation_id: ContextActivationId,
        candidate_id: ContextCandidateId,
        digest: ContextDigest,
        driver_context_revision: DriverContextRevision,
    },
    ContextCompactionTerminal {
        compaction_id: ContextCompactionId,
        operation_id: RuntimeOperationId,
        terminal: RuntimeOperationTerminal,
        context_revision: ContextRevision,
    },
    ContextCheckpointActivated {
        checkpoint_id: ContextCheckpointId,
        candidate_id: ContextCandidateId,
        activation_id: ContextActivationId,
        compaction_id: ContextCompactionId,
        context_revision: ContextRevision,
        digest: ContextDigest,
    },
    DriverContextCompactedOpaque,
    HookRunAccepted {
        hook_run_id: HookRunId,
        definition_id: HookDefinitionId,
        point: HookPoint,
        plan_revision: HookPlanRevision,
        plan_digest: HookPlanDigest,
        operation_id: Option<RuntimeOperationId>,
        turn_id: Option<RuntimeTurnId>,
        item_id: Option<RuntimeItemId>,
        interaction_id: Option<RuntimeInteractionId>,
    },
    HookRunStarted {
        hook_run_id: HookRunId,
    },
    HookRunTerminal {
        hook_run_id: HookRunId,
        terminal: HookRunTerminal,
        decision: HookRunDecision,
        message: Option<String>,
        effect_ids: Vec<HookEffectId>,
    },
    HookPlanBound {
        plan_revision: HookPlanRevision,
        plan_digest: HookPlanDigest,
    },
}

impl RuntimeEvent {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::ConversationDelta { .. } | Self::ProviderStatus { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeEventEnvelope {
    pub thread_id: RuntimeThreadId,
    /// Authoritative producer timestamp used by presentation projections.
    pub occurred_at_ms: u64,
    pub sequence: Option<EventSequence>,
    /// Present only for live, non-durable events. Together these coordinates form a stable
    /// reconnect cursor inside one binding generation; they are never persisted as journal facts.
    pub transient: Option<RuntimeTransientCoordinate>,
    pub revision: RuntimeRevision,
    pub event: RuntimeEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTransientCoordinate {
    pub binding_id: RuntimeBindingId,
    pub stream_generation: RuntimeDriverGeneration,
    pub sequence: RuntimeTransientSequence,
    pub event_id: RuntimeTransientEventId,
    pub turn_id: Option<RuntimeTurnId>,
}

/// Presentation durability is part of the observable session contract. It is
/// deliberately stored beside the owned event rather than inferred from a
/// journal cursor at read time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum PresentationDurability {
    Durable,
    Ephemeral,
}

/// The presentation body produced at a connector/tool/application boundary.
///
/// `event` is the protected body. Runtime coordinates and persistence metadata
/// must never be injected into or used to rewrite it after construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ImmutablePresentationEvent {
    pub durability: PresentationDurability,
    pub event: agentdash_agent_protocol::BackboneEvent,
}

impl ImmutablePresentationEvent {
    pub const fn new(
        durability: PresentationDurability,
        event: agentdash_agent_protocol::BackboneEvent,
    ) -> Self {
        Self { durability, event }
    }
}

/// Correlation data owned by Managed Runtime. Source identifiers coexist with
/// canonical identifiers here and never replace identifiers inside the event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimePresentationCoordinate {
    pub runtime_turn_id: Option<RuntimeTurnId>,
    /// Session-visible turn identity. Runtime/source identifiers are correlation
    /// coordinates and must never be substituted for this value at API projection.
    pub presentation_turn_id: Option<crate::PresentationTurnId>,
    pub runtime_item_id: Option<RuntimeItemId>,
    pub interaction_id: Option<RuntimeInteractionId>,
    pub source_thread_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub source_item_id: Option<String>,
    pub source_request_id: Option<String>,
    /// Producer-owned presentation entry order from the source session stream.
    /// Persistence and journal projection must preserve it verbatim.
    pub source_entry_index: Option<u32>,
}

/// One producer-owned presentation fact and its source correlation metadata.
/// Batches remain atomic and ordered while every fact retains its own entry index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimePresentationInput {
    pub coordinate: RuntimePresentationCoordinate,
    pub event: ImmutablePresentationEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimePresentationAppendRequest {
    pub runtime_thread_id: crate::RuntimeThreadId,
    pub producer: String,
    pub idempotency_key: crate::IdempotencyKey,
    pub events: Vec<RuntimePresentationInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTransientPresentationAppendRequest {
    pub runtime_thread_id: crate::RuntimeThreadId,
    pub producer: String,
    pub events: Vec<RuntimePresentationInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimePresentationAppendReceipt {
    pub first_sequence: crate::EventSequence,
    pub last_sequence: crate::EventSequence,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimePresentationAppendError {
    #[error("presentation append request is invalid: {0}")]
    Invalid(String),
    #[error("presentation append idempotency identity was reused with different events")]
    IdempotencyConflict,
    #[error("runtime thread was not found")]
    ThreadNotFound,
    #[error("runtime presentation append is temporarily unavailable")]
    Unavailable,
}

/// Allowlisted carrier metadata around a journal fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeCarrierMetadata {
    pub thread_id: RuntimeThreadId,
    pub recorded_at_ms: u64,
    pub sequence: Option<EventSequence>,
    pub transient: Option<RuntimeTransientCoordinate>,
    pub revision: RuntimeRevision,
    pub operation_id: Option<RuntimeOperationId>,
    /// Canonical append idempotency identity. It never replaces a producer
    /// request identity in `coordinate.source_request_id`.
    pub append_idempotency_key: Option<IdempotencyKey>,
    pub binding_id: Option<RuntimeBindingId>,
    pub coordinate: RuntimePresentationCoordinate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum RuntimeJournalFact {
    Presentation(ImmutablePresentationEvent),
    Internal(RuntimeEvent),
}

impl RuntimeJournalFact {
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Presentation(event) => event.durability == PresentationDurability::Ephemeral,
            Self::Internal(event) => event.is_transient(),
        }
    }

    pub const fn as_presentation(&self) -> Option<&ImmutablePresentationEvent> {
        match self {
            Self::Presentation(event) => Some(event),
            Self::Internal(_) => None,
        }
    }
}

/// One authoritative journal record. Presentation facts and internal Runtime
/// facts share ordering/transaction semantics without becoming two copies of
/// the same business fact.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub struct RuntimeJournalRecord {
    carrier: RuntimeCarrierMetadata,
    fact: RuntimeJournalFact,
}

impl<'de> Deserialize<'de> for RuntimeJournalRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        struct WireRecord {
            carrier: RuntimeCarrierMetadata,
            fact: RuntimeJournalFact,
        }

        let record = WireRecord::deserialize(deserializer)?;
        Self::new(record.carrier, record.fact).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeJournalRecordError {
    #[error("a runtime journal carrier must contain exactly one durable or transient cursor")]
    InvalidCursor,
    #[error("durable presentation facts require a durable sequence")]
    DurablePresentationRequiresSequence,
    #[error("ephemeral presentation facts require a transient coordinate")]
    EphemeralPresentationRequiresTransient,
    #[error("transient conversation facts must carry a complete presentation event")]
    TransientInternalFact,
    #[error("runtime-internal facts require a durable sequence")]
    InternalFactRequiresSequence,
}

impl RuntimeJournalRecord {
    pub fn new(
        carrier: RuntimeCarrierMetadata,
        fact: RuntimeJournalFact,
    ) -> Result<Self, RuntimeJournalRecordError> {
        if carrier.sequence.is_some() == carrier.transient.is_some() {
            return Err(RuntimeJournalRecordError::InvalidCursor);
        }
        if matches!(&fact, RuntimeJournalFact::Internal(event) if event.is_transient()) {
            return Err(RuntimeJournalRecordError::TransientInternalFact);
        }
        if matches!(&fact, RuntimeJournalFact::Internal(_)) && carrier.sequence.is_none() {
            return Err(RuntimeJournalRecordError::InternalFactRequiresSequence);
        }
        if let RuntimeJournalFact::Presentation(event) = &fact {
            match event.durability {
                PresentationDurability::Durable if carrier.sequence.is_none() => {
                    return Err(RuntimeJournalRecordError::DurablePresentationRequiresSequence);
                }
                PresentationDurability::Ephemeral if carrier.transient.is_none() => {
                    return Err(RuntimeJournalRecordError::EphemeralPresentationRequiresTransient);
                }
                PresentationDurability::Durable | PresentationDurability::Ephemeral => {}
            }
        }
        Ok(Self { carrier, fact })
    }

    pub const fn as_presentation(&self) -> Option<&ImmutablePresentationEvent> {
        self.fact.as_presentation()
    }

    pub const fn carrier(&self) -> &RuntimeCarrierMetadata {
        &self.carrier
    }

    /// Attach the Runtime operation that atomically published this durable fact.
    ///
    /// Command-owned presentation records use this correlation when a driver reconstructs a
    /// transcript before accepting that same command, preventing the current user input from
    /// being replayed once from the journal and then appended again by `prompt`.
    pub fn with_operation_id(mut self, operation_id: RuntimeOperationId) -> Self {
        self.carrier.operation_id = Some(operation_id);
        self
    }

    pub const fn fact(&self) -> &RuntimeJournalFact {
        &self.fact
    }

    /// Build the durable journal representation of an internal Runtime fact.
    ///
    /// Internal consumers may later project this record back to an
    /// `RuntimeEventEnvelope`, but persistence always stores the journal record
    /// itself so presentation and internal facts share one ordered source.
    pub fn from_internal_envelope(
        envelope: RuntimeEventEnvelope,
    ) -> Result<Self, RuntimeJournalRecordError> {
        let RuntimeEventEnvelope {
            thread_id,
            occurred_at_ms,
            sequence,
            transient,
            revision,
            event,
        } = envelope;
        Self::new(
            RuntimeCarrierMetadata {
                thread_id,
                recorded_at_ms: occurred_at_ms,
                sequence,
                transient,
                revision,
                operation_id: None,
                append_idempotency_key: None,
                binding_id: None,
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    presentation_turn_id: None,
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: None,
                    source_turn_id: None,
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: None,
                },
            },
            RuntimeJournalFact::Internal(event),
        )
    }

    /// Typed internal view over the single journal fact source.
    pub fn to_internal_envelope(&self) -> Option<RuntimeEventEnvelope> {
        let RuntimeJournalFact::Internal(event) = &self.fact else {
            return None;
        };
        Some(RuntimeEventEnvelope {
            thread_id: self.carrier.thread_id.clone(),
            occurred_at_ms: self.carrier.recorded_at_ms,
            sequence: self.carrier.sequence,
            transient: self.carrier.transient.clone(),
            revision: self.carrier.revision,
            event: event.clone(),
        })
    }
}

pub fn presentation_events(
    records: &[RuntimeJournalRecord],
) -> impl Iterator<Item = &ImmutablePresentationEvent> {
    records
        .iter()
        .filter_map(RuntimeJournalRecord::as_presentation)
}

impl RuntimeEventEnvelope {
    pub fn is_authoritative(&self) -> bool {
        self.sequence.is_some() && self.transient.is_none() && !self.event.is_transient()
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;

    #[test]
    fn runtime_user_item_preserves_standard_blocks_and_keeps_structured_context_hidden() {
        let expected = vec![
            serde_json::json!({
                "type": "text",
                "text": "ask @agent",
                "text_elements": [{
                    "byteRange": { "start": 4, "end": 10 },
                    "placeholder": null
                }]
            }),
            serde_json::json!({ "type": "image", "detail": null, "url": "https://example.test/image.png" }),
            serde_json::json!({ "type": "localImage", "path": "C:/workspace/image.png" }),
            serde_json::json!({ "type": "skill", "name": "review", "path": "C:/skills/review/SKILL.md" }),
            serde_json::json!({ "type": "mention", "name": "main.rs", "path": "C:/workspace/src/main.rs" }),
        ];
        let mut input = expected
            .iter()
            .cloned()
            .map(|value| {
                crate::RuntimeInput::user_input(
                    serde_json::from_value(value).expect("generated Codex UserInput"),
                )
            })
            .collect::<Vec<_>>();
        input.push(crate::RuntimeInput::Structured {
            schema: "agentdash.context.v1".to_string(),
            value: serde_json::json!({ "hidden": true }),
        });

        let item = RuntimeItemContent::user_message("user-item", input);

        let agentdash_agent_protocol::AgentDashThreadItem::Codex(
            agentdash_agent_protocol::CodexThreadItem::UserMessage { content, .. },
        ) = item.item()
        else {
            panic!("expected a typed Codex user message");
        };
        assert_eq!(
            serde_json::to_value(content).expect("serialize user content"),
            serde_json::Value::Array(expected)
        );
    }

    fn presentation_event(durability: PresentationDurability) -> ImmutablePresentationEvent {
        let event = serde_json::from_value(serde_json::json!({
            "type": "item_completed",
            "payload": {
                "item": {
                    "type": "dynamicToolCall",
                    "id": "source-item-1",
                    "namespace": null,
                    "tool": "fixture",
                    "arguments": { "explicit_null": null, "ordered": [1, 2] },
                    "status": "completed",
                    "contentItems": null,
                    "success": true,
                    "durationMs": null
                },
                "threadId": "source-thread-1",
                "turnId": "source-turn-1",
                "completedAtMs": 1712345678901_i64
            }
        }))
        .expect("deserialize owned presentation event");
        ImmutablePresentationEvent::new(durability, event)
    }

    fn carrier(sequence: Option<u64>, transient: bool) -> RuntimeCarrierMetadata {
        RuntimeCarrierMetadata {
            thread_id: RuntimeThreadId::new("runtime-thread-1").expect("thread id"),
            recorded_at_ms: 9_999,
            sequence: sequence.map(EventSequence),
            transient: transient.then(|| RuntimeTransientCoordinate {
                binding_id: RuntimeBindingId::new("binding-1").expect("binding id"),
                stream_generation: RuntimeDriverGeneration(3),
                sequence: RuntimeTransientSequence(4),
                event_id: RuntimeTransientEventId::new("event-1").expect("event id"),
                turn_id: Some(RuntimeTurnId::new("runtime-turn-1").expect("turn id")),
            }),
            revision: RuntimeRevision(7),
            operation_id: Some(RuntimeOperationId::new("operation-1").expect("operation id")),
            append_idempotency_key: None,
            binding_id: Some(RuntimeBindingId::new("binding-1").expect("binding id")),
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: Some(
                    RuntimeTurnId::new("runtime-turn-1").expect("runtime turn id"),
                ),
                presentation_turn_id: Some(
                    crate::PresentationTurnId::new("presentation-turn-1")
                        .expect("presentation turn id"),
                ),
                runtime_item_id: Some(
                    RuntimeItemId::new("runtime-item-1").expect("runtime item id"),
                ),
                interaction_id: Some(
                    RuntimeInteractionId::new("interaction-1").expect("interaction id"),
                ),
                source_thread_id: Some("source-thread-1".to_string()),
                source_turn_id: Some("source-turn-1".to_string()),
                source_item_id: Some("source-item-1".to_string()),
                source_request_id: Some("source-request-1".to_string()),
                source_entry_index: Some(17),
            },
        }
    }

    #[test]
    fn presentation_body_round_trips_without_null_or_timestamp_loss() {
        let event = presentation_event(PresentationDurability::Durable);
        let before = serde_json::to_value(&event.event).expect("serialize protected body");
        let encoded = serde_json::to_vec(&event).expect("serialize presentation event");
        let decoded: ImmutablePresentationEvent =
            serde_json::from_slice(&encoded).expect("deserialize presentation event");
        let after = serde_json::to_value(&decoded.event).expect("serialize decoded body");

        assert_eq!(after, before);
        assert_eq!(
            after.pointer("/payload/item/arguments/explicit_null"),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(
            after
                .pointer("/payload/completedAtMs")
                .and_then(serde_json::Value::as_i64),
            Some(1_712_345_678_901)
        );
    }

    #[test]
    fn changing_runtime_coordinates_does_not_change_protected_body() {
        let event = presentation_event(PresentationDurability::Durable);
        let first = RuntimeJournalRecord::new(
            carrier(Some(1), false),
            RuntimeJournalFact::Presentation(event.clone()),
        )
        .expect("first carrier");
        let mut changed = carrier(Some(99), false);
        changed.thread_id = RuntimeThreadId::new("runtime-thread-2").expect("thread id");
        changed.coordinate.runtime_item_id =
            Some(RuntimeItemId::new("runtime-item-99").expect("item id"));
        changed.coordinate.source_entry_index = Some(99);
        let second = RuntimeJournalRecord::new(changed, RuntimeJournalFact::Presentation(event))
            .expect("second carrier");

        assert_eq!(
            serde_json::to_value(&first.as_presentation().expect("presentation").event)
                .expect("serialize first body"),
            serde_json::to_value(&second.as_presentation().expect("presentation").event)
                .expect("serialize second body")
        );
        assert_eq!(first.carrier().coordinate.source_entry_index, Some(17));
        assert_eq!(second.carrier().coordinate.source_entry_index, Some(99));
    }

    #[test]
    fn internal_records_are_excluded_from_presentation_iteration() {
        let records = vec![
            RuntimeJournalRecord::new(
                carrier(Some(1), false),
                RuntimeJournalFact::Internal(RuntimeEvent::ThreadStatusChanged {
                    status: RuntimeThreadStatus::Active,
                }),
            )
            .expect("internal record"),
            RuntimeJournalRecord::new(
                carrier(Some(2), false),
                RuntimeJournalFact::Presentation(presentation_event(
                    PresentationDurability::Durable,
                )),
            )
            .expect("presentation record"),
        ];

        let presentation = presentation_events(&records).collect::<Vec<_>>();
        assert_eq!(presentation.len(), 1);
        assert!(matches!(
            presentation[0].event,
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(_)
        ));
    }

    #[test]
    fn durability_must_match_the_carrier_cursor() {
        assert!(matches!(
            RuntimeJournalRecord::new(
                carrier(None, true),
                RuntimeJournalFact::Presentation(presentation_event(
                    PresentationDurability::Durable,
                )),
            ),
            Err(RuntimeJournalRecordError::DurablePresentationRequiresSequence)
        ));
        assert!(matches!(
            RuntimeJournalRecord::new(
                carrier(Some(1), false),
                RuntimeJournalFact::Presentation(presentation_event(
                    PresentationDurability::Ephemeral,
                )),
            ),
            Err(RuntimeJournalRecordError::EphemeralPresentationRequiresTransient)
        ));
    }

    #[test]
    fn transient_runtime_summary_cannot_enter_the_journal_carrier() {
        let result = RuntimeJournalRecord::new(
            carrier(None, true),
            RuntimeJournalFact::Internal(RuntimeEvent::ConversationDelta {
                turn_id: RuntimeTurnId::new("runtime-turn-1").expect("turn id"),
                item_id: RuntimeItemId::new("runtime-item-1").expect("item id"),
                delta: RuntimeConversationDelta::AgentMessage {
                    delta: "token".to_string(),
                },
            }),
        );
        assert!(matches!(
            result,
            Err(RuntimeJournalRecordError::TransientInternalFact)
        ));
    }

    #[test]
    fn durable_internal_fact_cannot_occupy_a_transient_cursor() {
        let result = RuntimeJournalRecord::new(
            carrier(None, true),
            RuntimeJournalFact::Internal(RuntimeEvent::ThreadStatusChanged {
                status: RuntimeThreadStatus::Active,
            }),
        );
        assert!(matches!(
            result,
            Err(RuntimeJournalRecordError::InternalFactRequiresSequence)
        ));
    }

    #[test]
    fn deserialize_rejects_an_internal_fact_on_a_transient_cursor() {
        let value = serde_json::json!({
            "carrier": carrier(None, true),
            "fact": {
                "kind": "internal",
                "payload": {
                    "kind": "thread_status_changed",
                    "status": "active"
                }
            }
        });
        let error = serde_json::from_value::<RuntimeJournalRecord>(value)
            .expect_err("invalid wire record must be rejected");
        assert!(error.to_string().contains("durable sequence"));
    }
}
