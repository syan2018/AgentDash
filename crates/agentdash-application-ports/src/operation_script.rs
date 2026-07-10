use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_domain::operation::OperationRef;
pub use agentdash_domain::operation::{
    OperationEffect, OperationOriginRef, OperationPrincipalRef, OperationReplayPolicy,
    OperationScopeRef,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const RHAI_V1_DIALECT: &str = "rhai_v1";
pub const OPERATION_SCRIPT_HOST_API_V1: u16 = 1;

/// Server-resolved execution authority. It intentionally cannot be deserialized from a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationScriptExecutionContext {
    pub principal: OperationPrincipalRef,
    pub scope: OperationScopeRef,
    pub authority_revision: String,
    pub granted_capabilities: BTreeSet<String>,
    pub origin: OperationOriginRef,
    pub trace_id: String,
    pub attachment_ref: Option<String>,
}

/// Resolved from the caller's current canonical Operation surface, never client-authored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationScriptAllowedOperation {
    pub operation_ref: OperationRef,
    pub descriptor_digest: String,
    pub effect: OperationEffect,
    pub replay_policy: OperationReplayPolicy,
    pub recursive_operation_script: bool,
}

impl OperationScriptAllowedOperation {
    pub fn script_key(&self) -> String {
        format!(
            "{}:{}:{}:v{}",
            self.operation_ref.provider.namespace,
            self.operation_ref.provider.provider_key,
            self.operation_ref.operation_key,
            self.operation_ref.contract_version
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationScriptLimits {
    pub timeout_ms: u64,
    pub max_source_bytes: usize,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_rhai_operations: u64,
    pub max_call_levels: usize,
    pub max_string_size: usize,
    pub max_array_size: usize,
    pub max_map_size: usize,
    pub max_operation_calls: usize,
    pub max_parallel_operations: usize,
}

impl Default for OperationScriptLimits {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            max_source_bytes: 256 * 1024,
            max_input_bytes: 1024 * 1024,
            max_output_bytes: 1024 * 1024,
            max_rhai_operations: 100_000,
            max_call_levels: 32,
            max_string_size: 1024 * 1024,
            max_array_size: 1_000,
            max_map_size: 500,
            max_operation_calls: 32,
            max_parallel_operations: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationScriptProgram {
    pub dialect: String,
    pub host_api_version: u16,
    pub source: String,
    pub input: Value,
    pub allowed_operations: Vec<OperationScriptAllowedOperation>,
    pub limits: OperationScriptLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationScriptPreflightRequest {
    pub program: OperationScriptProgram,
    pub context: OperationScriptExecutionContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationScriptPreflightToken {
    pub plan_id: Uuid,
    pub binding_digest: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationScriptPreflightResult {
    pub token: OperationScriptPreflightToken,
    pub source_digest: String,
    pub manifest_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationScriptRunRequest {
    pub program: OperationScriptProgram,
    pub context: OperationScriptExecutionContext,
    pub token: OperationScriptPreflightToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationScriptCallStatus {
    Succeeded,
    Failed,
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationScriptCallEvidence {
    pub call_index: usize,
    pub operation_ref: OperationRef,
    pub child_trace_id: String,
    pub status: OperationScriptCallStatus,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationScriptResultAccess {
    pub principal: OperationPrincipalRef,
    pub scope: OperationScopeRef,
    pub authority_revision: String,
    pub required_capabilities: BTreeSet<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationScriptResultRef {
    pub result_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationScriptResultValue {
    Inline {
        value: Value,
    },
    Ref {
        result_ref: OperationScriptResultRef,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationScriptRunOutcome {
    pub execution_id: Uuid,
    pub plan_id: Uuid,
    pub value: OperationScriptResultValue,
    pub calls: Vec<OperationScriptCallEvidence>,
    pub partial: bool,
    pub outcome_unknown: bool,
    pub result_access: OperationScriptResultAccess,
}

#[derive(Debug, Clone)]
pub struct OperationScriptOperationCall {
    pub execution_id: Uuid,
    pub call_index: usize,
    pub operation_ref: OperationRef,
    pub input: Value,
    pub context: OperationScriptExecutionContext,
    pub parent_trace_id: String,
    pub child_trace_id: String,
    pub deadline: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationScriptOperationResult {
    pub value: Value,
    pub outcome_unknown: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OperationScriptError {
    #[error("OperationScript 请求无效: {field}: {reason}")]
    InvalidRequest { field: &'static str, reason: String },
    #[error("OperationScript preflight plan 无效: {reason}")]
    InvalidPlan { reason: &'static str },
    #[error("OperationScript preflight token 已过期")]
    TokenExpired,
    #[error("OperationScript worker capacity 已满")]
    CapacityExceeded,
    #[error("OperationScript 已取消")]
    Cancelled,
    #[error("OperationScript 超过 deadline")]
    DeadlineExceeded,
    #[error("OperationScript 中断: {reason}, outcome_unknown={outcome_unknown}")]
    ExecutionInterrupted {
        reason: &'static str,
        outcome_unknown: bool,
    },
    #[error("OperationScript 编译失败: {diagnostic}")]
    Compile { diagnostic: String },
    #[error("OperationScript 执行失败: {diagnostic}")]
    Runtime { diagnostic: String },
    #[error("OperationScript 未允许 Operation: {operation_key}")]
    OperationDenied { operation_key: String },
    #[error("OperationScript Operation 调用数超限: {maximum}")]
    CallLimitExceeded { maximum: usize },
    #[error("OperationScript 并行调用数超限: {maximum}")]
    ParallelLimitExceeded { maximum: usize },
    #[error("OperationScript 输出大小超限: actual={actual}, maximum={maximum}")]
    OutputLimitExceeded { actual: usize, maximum: usize },
    #[error("OperationScript nested Operation 失败: {code}")]
    NestedOperation { code: String, outcome_unknown: bool },
    #[error(
        "OperationScript 执行失败: {diagnostic}, partial={partial}, outcome_unknown={outcome_unknown}"
    )]
    ExecutionFailed {
        diagnostic: String,
        calls: Vec<OperationScriptCallEvidence>,
        partial: bool,
        outcome_unknown: bool,
    },
    #[error("OperationScript 内部错误: {code}")]
    Internal { code: &'static str },
}

#[async_trait]
pub trait OperationScriptResultStore: Send + Sync {
    async fn put(
        &self,
        value: Value,
        access: OperationScriptResultAccess,
    ) -> Result<OperationScriptResultRef, OperationScriptError>;

    async fn resolve(
        &self,
        result_ref: &OperationScriptResultRef,
        current_context: &OperationScriptExecutionContext,
        cancel: CancellationToken,
    ) -> Result<Option<Value>, OperationScriptError>;
}

#[async_trait]
pub trait OperationScriptOperationExecutor: Send + Sync {
    async fn execute(
        &self,
        call: OperationScriptOperationCall,
        cancel: CancellationToken,
    ) -> Result<OperationScriptOperationResult, OperationScriptError>;
}

#[async_trait]
pub trait OperationScriptEngine: Send + Sync {
    async fn preflight(
        &self,
        request: OperationScriptPreflightRequest,
        cancel: CancellationToken,
    ) -> Result<OperationScriptPreflightResult, OperationScriptError>;

    async fn run(
        &self,
        request: OperationScriptRunRequest,
        operation_executor: Arc<dyn OperationScriptOperationExecutor>,
        cancel: CancellationToken,
    ) -> Result<OperationScriptRunOutcome, OperationScriptError>;

    /// Resolves a scoped result after a trusted host rebuilds current authority.
    async fn resolve_result(
        &self,
        result_ref: &OperationScriptResultRef,
        current_context: &OperationScriptExecutionContext,
        cancel: CancellationToken,
    ) -> Result<Option<Value>, OperationScriptError>;
}
