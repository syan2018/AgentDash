use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::{
    ContextCandidateId, ContextCheckpointId, DriverBindingId, DriverItemId, DriverRequestId,
    DriverThreadId, DriverTurnId, ProfileDigest, RuntimeBindingId, RuntimeCommand, RuntimeEvent,
    RuntimeProfile, RuntimeServiceInstanceId, SurfaceDigest, SurfaceRevision,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverBinding {
    pub driver_binding_id: DriverBindingId,
    pub source_thread_id: DriverThreadId,
    pub applied_surface_revision: SurfaceRevision,
    pub applied_surface_digest: SurfaceDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverCommandEnvelope {
    pub request_id: DriverRequestId,
    pub binding_id: RuntimeBindingId,
    pub generation: crate::RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
    pub command: RuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverDispatchReceipt {
    pub request_id: DriverRequestId,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverEventEnvelope {
    pub binding_id: RuntimeBindingId,
    pub generation: crate::RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
    pub source_turn_id: Option<DriverTurnId>,
    pub source_item_id: Option<DriverItemId>,
    pub event: RuntimeEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriverInspectionQuery {
    Binding { driver_binding_id: DriverBindingId },
    CompactionActivation { candidate_id: ContextCandidateId },
    Checkpoint { checkpoint_id: ContextCheckpointId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriverInspection {
    Binding {
        active: bool,
    },
    CompactionActivation {
        applied: bool,
        digest: Option<String>,
    },
    Checkpoint {
        available: bool,
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
        sink: &dyn DriverEventSink,
    ) -> Result<DriverDispatchReceipt, DriverError>;

    async fn inspect(&self, query: DriverInspectionQuery) -> Result<DriverInspection, DriverError>;
}
