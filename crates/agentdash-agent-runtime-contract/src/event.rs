use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    BindingEpoch, ContextActivationId, ContextCandidateId, ContextCheckpointId,
    ContextCompactionId, ContextDigest, ContextRevision, DriverContextRevision, DriverThreadId,
    EventSequence, HookDefinitionId, HookEffectId, HookPlanDigest, HookPlanRevision, HookPoint,
    HookRunId, ProfileDigest, RuntimeBindingId, RuntimeDriverGeneration, RuntimeInteractionId,
    RuntimeItemId, RuntimeOperationId, RuntimeProfile, RuntimeRecoveryIntentId, RuntimeRevision,
    RuntimeThreadId, RuntimeTransientEventId, RuntimeTransientSequence, RuntimeTurnId,
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
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeInteractionRequest {
    CommandApproval { params: Box<agentdash_agent_protocol::generated::codex_v2::command_execution_request_approval_params::CommandExecutionRequestApprovalParams> },
    FileChangeApproval { params: Box<agentdash_agent_protocol::generated::codex_v2::file_change_request_approval_params::FileChangeRequestApprovalParams> },
    PermissionApproval { params: Box<agentdash_agent_protocol::generated::codex_v2::permissions_request_approval_params::PermissionsRequestApprovalParams> },
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

    pub fn temporary_permission_approval(
        thread_id: &str,
        turn_id: &str,
        item_id: &str,
        reason: String,
    ) -> Self {
        let cwd = std::env::current_dir()
            .expect("runtime cwd")
            .to_string_lossy()
            .into_owned();
        Self::PermissionApproval {
            params: Box::new(
                serde_json::from_value(serde_json::json!({
                    "cwd": cwd, "itemId": item_id, "permissions": {}, "reason": reason,
                    "startedAtMs": 0, "threadId": thread_id, "turnId": turn_id
                }))
                .expect("temporary owned permission approval"),
            ),
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
        let content = input.into_iter().map(|input| match input {
            crate::RuntimeInput::Text { text } => serde_json::json!({"type":"text", "text":text}),
            crate::RuntimeInput::Image { data_url, .. } => serde_json::json!({"type":"image", "url":data_url}),
            crate::RuntimeInput::FileReference { uri, media_type } => serde_json::json!({
                "type":"mention", "name":media_type.unwrap_or_else(|| "resource".to_string()), "path":uri
            }),
            crate::RuntimeInput::Structured { schema, value } => serde_json::json!({
                "type":"text", "text": serde_json::json!({"schema":schema,"value":value}).to_string()
            }),
        }).collect::<Vec<_>>();
        Self::codex_json(
            serde_json::json!({"type":"userMessage", "id":id.into(), "content":content}),
        )
    }

    /// Temporary W2 bridge; W3 replaces this with tool-owner projectors.
    pub fn temporary_dynamic_tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self::codex_json(serde_json::json!({
            "type":"dynamicToolCall", "id":id.into(), "tool":name.into(), "arguments":arguments,
            "status":"inProgress"
        }))
    }

    /// Temporary W2 bridge; intentionally does not invent a generic terminal output contract.
    pub fn temporary_dynamic_tool_result(
        id: impl Into<String>,
        name: impl Into<String>,
        _output: serde_json::Value,
        failed: bool,
    ) -> Self {
        Self::codex_json(serde_json::json!({
            "type":"dynamicToolCall", "id":id.into(), "tool":name.into(), "arguments":{},
            "status": if failed { "failed" } else { "completed" }, "success": !failed,
            "contentItems": null
        }))
    }

    fn codex_json(value: serde_json::Value) -> Self {
        let item = serde_json::from_value(value).expect("runtime-owned Codex item constructor");
        Self::new(agentdash_agent_protocol::AgentDashThreadItem::Codex(item))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeConversationDelta {
    AgentMessage { delta: String },
    ReasoningText { delta: String },
    ReasoningSummary { delta: String },
    CommandOutput { delta: String },
    FileChangeOutput { delta: String },
    Plan { delta: String },
    McpProgress { message: String },
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
    },
    TurnTerminal {
        turn_id: RuntimeTurnId,
        terminal: RuntimeTurnTerminal,
        message: Option<String>,
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
        matches!(self, Self::ConversationDelta { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeEventEnvelope {
    pub thread_id: RuntimeThreadId,
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

impl RuntimeEventEnvelope {
    pub fn is_authoritative(&self) -> bool {
        self.sequence.is_some() && self.transient.is_none() && !self.event.is_transient()
    }
}
