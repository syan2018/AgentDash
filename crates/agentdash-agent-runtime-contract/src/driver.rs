use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{
    ContextCandidateId, ContextCheckpointId, DriverBindingId, DriverItemId, DriverRequestId,
    DriverThreadId, DriverTurnId, HookPlanDigest, HookPlanRevision, HookPoint, ProfileDigest,
    RuntimeBindingId, RuntimeCommand, RuntimeJournalFact, RuntimeProfile, RuntimeServiceInstanceId,
    RuntimeSurfaceDescriptor, RuntimeTurnId, SurfaceDigest, SurfaceRevision, ToolSetRevision,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverDescribeRequest {
    pub service_instance_id: RuntimeServiceInstanceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeDescriptor {
    pub protocol_revision: u32,
    pub service_instance_id: RuntimeServiceInstanceId,
    pub profile: RuntimeProfile,
    pub profile_digest: ProfileDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverBindRequest {
    pub binding_id: RuntimeBindingId,
    pub service_instance_id: RuntimeServiceInstanceId,
    pub surface_revision: SurfaceRevision,
    pub surface_digest: SurfaceDigest,
    pub intent: DriverBindIntent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriverBindIntent {
    Start,
    Resume {
        source_thread_id: DriverThreadId,
    },
    Fork {
        source_thread_id: DriverThreadId,
        through_source_turn_id: Option<DriverTurnId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverBinding {
    pub driver_binding_id: DriverBindingId,
    pub source_thread_id: DriverThreadId,
    pub applied_surface_revision: SurfaceRevision,
    pub applied_surface_digest: SurfaceDigest,
    pub applied_tool_set_revision: ToolSetRevision,
    pub applied_tool_set_digest: String,
    pub applied_hook_plan_revision: Option<HookPlanRevision>,
    pub applied_hook_plan_digest: Option<HookPlanDigest>,
    pub applied_hooks: Vec<DriverHookApplyStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverHookApplyStatus {
    pub point: HookPoint,
    pub acknowledged: bool,
    pub artifact_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverCommandEnvelope {
    pub request_id: DriverRequestId,
    /// Managed Runtime operation that owns this delivery. Drivers must preserve
    /// it across acceptance, terminal emission, duplicate dispatch and recovery.
    pub operation_id: crate::RuntimeOperationId,
    pub presentation_thread_id: crate::PresentationThreadId,
    pub binding_id: RuntimeBindingId,
    pub generation: crate::RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
    /// Managed Runtime 为会产生新 Turn 的命令分配的 canonical identity。
    /// Driver 只把自己的 source turn 映射到该 identity，不再创建第二个 Runtime Turn。
    pub runtime_turn_id: Option<RuntimeTurnId>,
    /// Session-visible turn identity carried by the protected presentation
    /// protocol. It is distinct from both canonical Runtime and vendor turns.
    pub presentation_turn_id: Option<crate::PresentationTurnId>,
    pub command: RuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverDispatchReceipt {
    pub request_id: DriverRequestId,
    pub duplicate: bool,
    pub applied_tool_set: Option<DriverToolSetApplyReceipt>,
    pub applied_surface: Option<DriverSurfaceApplyReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverSurfaceApplyReceipt {
    pub descriptor: RuntimeSurfaceDescriptor,
    pub applied_hooks: Vec<DriverHookApplyStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverToolSetApplyReceipt {
    pub revision: ToolSetRevision,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverEventEnvelope {
    pub binding_id: RuntimeBindingId,
    pub generation: crate::RuntimeDriverGeneration,
    /// Accepted Runtime command that caused this emission. Long-lived binding
    /// events may omit it; command/turn events preserve it explicitly.
    pub operation_id: Option<crate::RuntimeOperationId>,
    pub source_thread_id: DriverThreadId,
    pub source_turn_id: Option<DriverTurnId>,
    pub source_item_id: Option<DriverItemId>,
    /// Original vendor request identity. Numeric JSON-RPC ids use their exact
    /// decimal representation; string ids are preserved byte-for-byte.
    pub source_request_id: Option<String>,
    /// Producer-owned entry order for this source emission. Different entry
    /// indices must be emitted in different driver envelopes.
    pub source_entry_index: Option<u32>,
    /// Ordered facts produced from one source event. Internal state facts and
    /// immutable presentation facts are committed in this exact order.
    pub facts: Vec<RuntimeJournalFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriverInspectionQuery {
    Binding { driver_binding_id: DriverBindingId },
    CompactionActivation { candidate_id: ContextCandidateId },
    Checkpoint { checkpoint_id: ContextCheckpointId },
    ThreadProjection { source_thread_id: DriverThreadId },
    ContextRead { source_thread_id: DriverThreadId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverProjectedItem {
    pub source_turn_id: DriverTurnId,
    pub source_item_id: DriverItemId,
    pub content: crate::RuntimeItemContent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriverInspection {
    Binding {
        active: bool,
    },
    CompactionActivation {
        applied: bool,
        digest: Option<String>,
        driver_context_revision: Option<crate::DriverContextRevision>,
    },
    Checkpoint {
        available: bool,
        digest: Option<String>,
    },
    ThreadProjection {
        source_thread_id: DriverThreadId,
        items: Vec<DriverProjectedItem>,
        fidelity: crate::ContextFidelity,
    },
    ContextRead {
        source_thread_id: DriverThreadId,
        fidelity: crate::ContextFidelity,
        digest: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriverError {
    #[error("driver does not support the command: {reason}")]
    Unsupported { reason: String },
    #[error("driver rejected the command before acceptance: {reason}")]
    Rejected { reason: String },
    #[error("driver is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("driver binding generation is stale")]
    StaleGeneration,
    #[error("driver protocol violation: {reason}")]
    ProtocolViolation { reason: String, critical: bool },
    #[error("driver result is lost: {reason}")]
    Lost { reason: String, retryable: bool },
    #[error("managed runtime already terminalized the driver stream: {reason}")]
    Terminalized { reason: String },
}

#[async_trait]
pub trait DriverEventSink: Send + Sync {
    async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError>;
}

#[async_trait]
pub trait AgentRuntimeDriver: Send + Sync {
    async fn describe(
        &self,
        request: DriverDescribeRequest,
    ) -> Result<RuntimeDescriptor, DriverError>;

    async fn bind(&self, request: DriverBindRequest) -> Result<DriverBinding, DriverError>;

    async fn dispatch(
        &self,
        command: DriverCommandEnvelope,
        sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, DriverError>;

    async fn inspect(&self, query: DriverInspectionQuery) -> Result<DriverInspection, DriverError>;
}
