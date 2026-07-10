use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{
    ContextRevision, EventSequence, RuntimeCommandKind, RuntimeOperationId, RuntimeRevision,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum OperationConflictKind {
    OperationIdReused,
    IdempotencyKeyReused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeExecuteError {
    #[error("command {command:?} is unsupported: {reason}")]
    Unsupported {
        command: RuntimeCommandKind,
        reason: String,
    },
    #[error("runtime is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("expected thread revision {expected:?}, actual {actual:?}")]
    RevisionConflict {
        expected: RuntimeRevision,
        actual: RuntimeRevision,
    },
    #[error("operation identity conflicts with accepted operation {existing_operation_id}")]
    OperationConflict {
        existing_operation_id: RuntimeOperationId,
        conflict: OperationConflictKind,
    },
    #[error("context compaction operation {operation_id} is already active")]
    ContextCompactionInProgress { operation_id: RuntimeOperationId },
    #[error("command is invalid: {reason}")]
    InvalidCommand { reason: String },
    #[error("runtime binding is incompatible: {reason}")]
    Incompatible { reason: String },
    #[error("operation acceptance failed: {reason}")]
    Persistence { reason: String, retryable: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeSnapshotError {
    #[error("thread was not found")]
    NotFound,
    #[error("snapshot revision {requested:?} is unavailable; current revision is {current:?}")]
    RevisionUnavailable {
        requested: RuntimeRevision,
        current: RuntimeRevision,
    },
    #[error(
        "context revision {requested:?} is unavailable; current context revision is {current:?}"
    )]
    ContextRevisionUnavailable {
        requested: ContextRevision,
        current: ContextRevision,
    },
    #[error("context snapshot is inconsistent: {code:?}")]
    InconsistentContext {
        code: ContextSnapshotConsistencyCode,
    },
    #[error("snapshot is unavailable: {reason}")]
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextSnapshotConsistencyCode {
    ProjectionHeadRevisionMismatch,
    HeadCheckpointMissing,
    HeadCheckpointMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeSubscribeError {
    #[error("thread was not found")]
    NotFound,
    #[error("event cursor is invalid")]
    InvalidCursor,
    #[error(
        "event cursor {requested:?} precedes the earliest retained event {earliest_available:?}"
    )]
    CursorGap {
        requested: EventSequence,
        earliest_available: EventSequence,
        latest_available: EventSequence,
    },
    #[error("event stream is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
}
