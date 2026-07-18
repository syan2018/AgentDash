use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{
    ManagedRuntimeChangePage, ManagedRuntimeContentBlock, ManagedRuntimeOperationStatus,
    ManagedRuntimeSnapshot, RuntimeChangeSequence, RuntimeIdempotencyKey, RuntimeInteractionId,
    RuntimeOperationId, RuntimeProjectionRevision, RuntimeThreadId, RuntimeTurnId,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeInteractionResponse {
    Approved,
    Denied {
        reason: Option<String>,
    },
    UserInput {
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Structured {
        schema: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeCommand {
    SubmitInput {
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Steer {
        expected_turn_id: RuntimeTurnId,
        content: Vec<ManagedRuntimeContentBlock>,
    },
    Interrupt {
        expected_turn_id: RuntimeTurnId,
    },
    RequestCompaction,
    ResolveInteraction {
        interaction_id: RuntimeInteractionId,
        response: ManagedRuntimeInteractionResponse,
    },
    Close,
    Fork {
        child_thread_id: RuntimeThreadId,
        through_completed_turn_id: Option<RuntimeTurnId>,
    },
}

impl ManagedRuntimeCommand {
    pub fn kind(&self) -> crate::ManagedRuntimeCommandKind {
        match self {
            Self::SubmitInput { .. } => crate::ManagedRuntimeCommandKind::SubmitInput,
            Self::Steer { .. } => crate::ManagedRuntimeCommandKind::Steer,
            Self::Interrupt { .. } => crate::ManagedRuntimeCommandKind::Interrupt,
            Self::RequestCompaction => crate::ManagedRuntimeCommandKind::RequestCompaction,
            Self::ResolveInteraction { .. } => crate::ManagedRuntimeCommandKind::ResolveInteraction,
            Self::Close => crate::ManagedRuntimeCommandKind::Close,
            Self::Fork { .. } => crate::ManagedRuntimeCommandKind::Fork,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeCommandEnvelope {
    pub operation_id: RuntimeOperationId,
    pub idempotency_key: RuntimeIdempotencyKey,
    pub thread_id: RuntimeThreadId,
    pub expected_revision: Option<RuntimeProjectionRevision>,
    pub command: ManagedRuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeOperationReceipt {
    pub operation_id: RuntimeOperationId,
    pub thread_id: RuntimeThreadId,
    pub accepted_revision: RuntimeProjectionRevision,
    pub status: ManagedRuntimeOperationStatus,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeReadRequest {
    pub thread_id: RuntimeThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeChangesRequest {
    pub thread_id: RuntimeThreadId,
    pub after: Option<RuntimeChangeSequence>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeGatewayError {
    #[error("managed Runtime command conflicts with revision {actual:?}")]
    Conflict { actual: RuntimeProjectionRevision },
    #[error("managed Runtime thread was not found")]
    NotFound,
    #[error("managed Runtime command is unavailable: {reason}")]
    Unavailable { reason: String },
    #[error("managed Runtime request is invalid: {reason}")]
    Invalid { reason: String },
    #[error("managed Runtime persistence failed: {reason}")]
    Persistence { reason: String },
}

#[async_trait]
pub trait ManagedAgentRuntimeGateway: Send + Sync {
    async fn execute(
        &self,
        command: ManagedRuntimeCommandEnvelope,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError>;

    async fn read(
        &self,
        request: ManagedRuntimeReadRequest,
    ) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeGatewayError>;

    async fn changes(
        &self,
        request: ManagedRuntimeChangesRequest,
    ) -> Result<ManagedRuntimeChangePage, ManagedRuntimeGatewayError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeContractSchema {
    pub command: ManagedRuntimeCommandEnvelope,
    pub operation_receipt: ManagedRuntimeOperationReceipt,
    pub read: ManagedRuntimeReadRequest,
    pub changes: ManagedRuntimeChangesRequest,
    pub error: ManagedRuntimeGatewayError,
    pub snapshot: ManagedRuntimeSnapshot,
    pub change_page: ManagedRuntimeChangePage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_schema_contains_command_read_and_change_families() {
        let schema = schemars::schema_for!(ManagedRuntimeContractSchema);
        let schema = serde_json::to_string(&schema).expect("serialize Runtime schema");
        for family in [
            "ManagedRuntimeCommandEnvelope",
            "ManagedRuntimeOperationReceipt",
            "ManagedRuntimeReadRequest",
            "ManagedRuntimeChangesRequest",
            "ManagedRuntimeSnapshot",
            "ManagedRuntimeChangePage",
        ] {
            assert!(schema.contains(family), "missing {family}");
        }
    }
}
