use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{FunctionActivityExecutorSpec, LifecycleGate, NodePortValue, RuntimeNodeError};

pub const WORKFLOW_EXECUTOR_EFFECT_TABLE: &str = "workflow_executor_effects";
pub const WORKFLOW_EXECUTOR_EFFECT_PRIMARY_KEY: &[&str] = &["effect_id"];
pub const WORKFLOW_EXECUTOR_EFFECT_UNIQUE_KEYS: &[&[&str]] = &[
    &[
        "lifecycle_run_id",
        "orchestration_id",
        "node_path",
        "attempt",
        "effect_kind",
    ],
    &["gate_id", "effect_kind"],
];
pub const WORKFLOW_EXECUTOR_EFFECT_COLUMNS: &[&str] = &[
    "effect_id:text not null",
    "effect_kind:text not null",
    "lifecycle_run_id:uuid not null",
    "orchestration_id:uuid not null",
    "node_path:text not null",
    "attempt:bigint not null",
    "payload_digest:text not null",
    "request:jsonb null",
    "state:text not null",
    "gate_id:uuid null",
    "receipt:jsonb null",
    "created_at:timestamptz not null",
    "updated_at:timestamptz not null",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowExecutorEffectIdentity {
    pub effect_id: String,
    pub lifecycle_run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowFunctionTerminalResult {
    Completed { outputs: Vec<NodePortValue> },
    Failed { error: RuntimeNodeError },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowFunctionEffectRequest {
    pub identity: WorkflowExecutorEffectIdentity,
    pub payload_digest: String,
    pub spec: FunctionActivityExecutorSpec,
    pub context: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowFunctionEffectRecord {
    pub request: WorkflowFunctionEffectRequest,
    pub terminal: Option<WorkflowFunctionTerminalResult>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowHumanGateOpenEffect {
    pub identity: WorkflowExecutorEffectIdentity,
    pub payload_digest: String,
    pub gate: LifecycleGate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowHumanGateOpenReceipt {
    pub effect: WorkflowHumanGateOpenEffect,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowHumanGateResolutionEffect {
    pub identity: WorkflowExecutorEffectIdentity,
    pub payload_digest: String,
    pub gate_id: Uuid,
    pub decision: Value,
    pub resolved_by: String,
    pub outputs: Vec<NodePortValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowHumanGateResolutionReceipt {
    pub effect: WorkflowHumanGateResolutionEffect,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowExecutorEffectRepositoryError {
    #[error("Workflow executor effect payload conflict: effect_id={effect_id}")]
    PayloadConflict { effect_id: String },
    #[error("Workflow executor effect persistence failed: {0}")]
    Persistence(String),
}

#[async_trait::async_trait]
pub trait WorkflowExecutorEffectRepository: Send + Sync {
    /// Inserts the Prepared function effect or returns the identical record.
    async fn prepare_function(
        &self,
        request: WorkflowFunctionEffectRequest,
    ) -> Result<WorkflowFunctionEffectRecord, WorkflowExecutorEffectRepositoryError>;

    /// Commits the terminal result once. Replays must return the byte-identical
    /// terminal receipt; a different payload or terminal result is a conflict.
    async fn commit_function_terminal(
        &self,
        request: WorkflowFunctionEffectRequest,
        terminal: WorkflowFunctionTerminalResult,
    ) -> Result<WorkflowFunctionEffectRecord, WorkflowExecutorEffectRepositoryError>;

    async fn get_function(
        &self,
        effect_id: &str,
    ) -> Result<Option<WorkflowFunctionEffectRecord>, WorkflowExecutorEffectRepositoryError>;

    /// Atomically creates the LifecycleGate and its open receipt. The gate id,
    /// correlation, payload digest and stable gate fields define idempotency;
    /// persistence timestamps are repository-owned metadata.
    async fn open_human_gate(
        &self,
        effect: WorkflowHumanGateOpenEffect,
    ) -> Result<WorkflowHumanGateOpenReceipt, WorkflowExecutorEffectRepositoryError>;

    async fn get_human_gate_open(
        &self,
        effect_id: &str,
    ) -> Result<Option<WorkflowHumanGateOpenReceipt>, WorkflowExecutorEffectRepositoryError>;

    /// Atomically resolves the gate and commits the decision receipt.
    async fn resolve_human_gate(
        &self,
        effect: WorkflowHumanGateResolutionEffect,
    ) -> Result<WorkflowHumanGateResolutionReceipt, WorkflowExecutorEffectRepositoryError>;

    async fn get_human_gate_resolution(
        &self,
        gate_id: Uuid,
    ) -> Result<Option<WorkflowHumanGateResolutionReceipt>, WorkflowExecutorEffectRepositoryError>;
}
