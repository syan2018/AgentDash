use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{RuntimeCommandKind, RuntimeRevision};

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
    #[error("snapshot is unavailable: {reason}")]
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeSubscribeError {
    #[error("thread was not found")]
    NotFound,
    #[error("event cursor is invalid")]
    InvalidCursor,
    #[error("event stream is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
}
