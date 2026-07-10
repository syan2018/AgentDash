use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    ContextCheckpointId, EventSequence, RuntimeBindingId, RuntimeInteractionId, RuntimeItemId,
    RuntimeOperationId, RuntimeRevision, RuntimeThreadId, RuntimeTurnId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeThreadStatus {
    Active,
    Suspended,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInteractionTerminal {
    Resolved,
    Expired,
    Cancelled,
    Failed,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeItemContent {
    UserMessage {
        input: Vec<crate::RuntimeInput>,
    },
    AgentMessage {
        text: String,
    },
    ToolCall {
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        name: String,
        output: serde_json::Value,
    },
    SystemContextChange {
        checkpoint_id: ContextCheckpointId,
    },
    ContextCompaction {
        checkpoint_id: ContextCheckpointId,
    },
    Reasoning {
        text: String,
    },
    Plan {
        steps: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
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
    },
    ItemDelta {
        turn_id: RuntimeTurnId,
        item_id: RuntimeItemId,
        delta: String,
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
        interaction_kind: RuntimeInteractionKind,
        prompt: String,
    },
    InteractionTerminal {
        turn_id: RuntimeTurnId,
        interaction_id: RuntimeInteractionId,
        terminal: RuntimeInteractionTerminal,
    },
    ContextCheckpointPrepared {
        checkpoint_id: ContextCheckpointId,
    },
    ContextCheckpointActivated {
        checkpoint_id: ContextCheckpointId,
    },
}

impl RuntimeEvent {
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::ItemDelta { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeEventEnvelope {
    pub thread_id: RuntimeThreadId,
    pub sequence: Option<EventSequence>,
    pub revision: RuntimeRevision,
    pub event: RuntimeEvent,
}

impl RuntimeEventEnvelope {
    pub fn is_authoritative(&self) -> bool {
        !self.event.is_transient()
    }
}
