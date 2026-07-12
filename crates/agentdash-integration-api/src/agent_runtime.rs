use std::{collections::BTreeMap, fmt, sync::Arc};

use agentdash_agent_runtime_contract::{
    AgentRuntimeDriver, ConfigurationBoundary, ContextBlock, ContextCandidateId,
    ContextCheckpointId, ContextCompactionId, ContextDigest, ContextFidelity, ContextRecipe,
    ContextRevision, DriverItemId, DriverThreadId, DriverTurnId, HookAction, HookDefinitionId,
    HookExecutionSite, HookFailurePolicy, HookPlanDigest, HookPlanRevision, HookPoint,
    InstructionChannel, MaterializedContext, RuntimeBindingId, RuntimeDriverGeneration,
    RuntimeInteractionId, RuntimeItemId, RuntimeProfile, RuntimeServiceInstanceId, RuntimeThreadId,
    RuntimeTurnId, SemanticStrength, SurfaceDigest, SurfaceRevision, ToolChannel, ToolSetRevision,
    WorkspaceCapability,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use ts_rs::TS;

pub fn agent_service_schema_digest(value: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};

    fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => {
                let mut entries = object.iter().collect::<Vec<_>>();
                entries.sort_by(|left, right| left.0.cmp(right.0));
                serde_json::Value::Object(
                    entries
                        .into_iter()
                        .map(|(key, value)| (key.clone(), canonicalize(value)))
                        .collect(),
                )
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(canonicalize).collect())
            }
            other => other.clone(),
        }
    }

    let canonical = serde_json::to_vec(&canonicalize(value))
        .expect("JSON value canonical serialization cannot fail");
    format!("sha256:{:x}", Sha256::digest(canonical))
}

use crate::AuthIdentity;

macro_rules! integration_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, InvalidAgentServiceId> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(InvalidAgentServiceId {
                        type_name: stringify!($name),
                    });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{type_name} must not be empty")]
pub struct InvalidAgentServiceId {
    type_name: &'static str,
}

integration_id!(AgentServiceDefinitionId);
integration_id!(AgentServiceOfferId);
integration_id!(AgentServiceBuildDigest);
integration_id!(AgentServiceSchemaDigest);
integration_id!(AgentRuntimeFactoryKey);
integration_id!(AgentRuntimeCredentialSlot);
integration_id!(AgentRuntimeCredentialRef);
integration_id!(AgentRuntimePlacementId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentServiceProvenance {
    pub definition_id: AgentServiceDefinitionId,
    pub publisher_integration: String,
    pub service_version: String,
    pub build_digest: AgentServiceBuildDigest,
}

/// Integration 随 driver definition 一并交给宿主的静态信任声明。
///
/// 该声明只描述由集成构建、测试并签入的事实，不携带运行期配置或凭据。宿主会据此
/// 构造自己的 conformance verifier，并在 service instance 激活时校验实际证据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRuntimeTrustManifest {
    pub provenance: AgentServiceProvenance,
    pub suite_revision: String,
    pub driver_build_digest: String,
    pub protocol_revision: u32,
    pub verified_profile: RuntimeProfile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CredentialSlotDefinition {
    pub slot: AgentRuntimeCredentialSlot,
    pub purpose: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentServiceDefinition {
    pub provenance: AgentServiceProvenance,
    pub factory_key: AgentRuntimeFactoryKey,
    pub supported_protocol_revisions: Vec<u32>,
    pub config_schema: Value,
    pub config_schema_digest: AgentServiceSchemaDigest,
    pub credential_slots: Vec<CredentialSlotDefinition>,
    pub service_profile_upper_bound: RuntimeProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRuntimePlacement {
    InProcess,
    LocalProcess {
        host_id: String,
    },
    Remote {
        host_id: String,
        transport_id: AgentRuntimePlacementId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ActivatedAgentServiceInstance {
    pub instance_id: RuntimeServiceInstanceId,
    pub instance_revision: u64,
    pub generation: RuntimeDriverGeneration,
    pub definition: AgentServiceDefinition,
    pub config: Value,
    pub credentials: BTreeMap<AgentRuntimeCredentialSlot, AgentRuntimeCredentialRef>,
    pub placement: AgentRuntimePlacement,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CredentialLease {
    pub slot: AgentRuntimeCredentialSlot,
    pub purpose: String,
    pub secret: String,
}

impl fmt::Debug for CredentialLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialLease")
            .field("slot", &self.slot)
            .field("purpose", &self.purpose)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CredentialResolveError {
    #[error("credential reference is unavailable for slot {slot}: {reason}")]
    Unavailable {
        slot: AgentRuntimeCredentialSlot,
        reason: String,
    },
    #[error("credential purpose is not allowed for slot {slot}")]
    PurposeDenied { slot: AgentRuntimeCredentialSlot },
}

#[async_trait]
pub trait AgentRuntimeCredentialBroker: Send + Sync {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        reference: &AgentRuntimeCredentialRef,
        purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverSurfaceRequest {
    pub binding_id: RuntimeBindingId,
    pub surface_revision: SurfaceRevision,
    pub surface_digest: SurfaceDigest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverInstructionSet {
    pub channel: InstructionChannel,
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverContextSurface {
    pub recipe: ContextRecipe,
    pub instructions: Vec<DriverInstructionSet>,
    pub blocks: Vec<ContextBlock>,
    pub digest: ContextDigest,
    pub fidelity: ContextFidelity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
    pub channels: Vec<ToolChannel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverToolSurface {
    pub revision: ToolSetRevision,
    pub digest: String,
    pub tools: Vec<DriverToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverHookBinding {
    pub definition_id: HookDefinitionId,
    pub point: HookPoint,
    pub actions: Vec<HookAction>,
    pub strength: SemanticStrength,
    pub failure_policy: HookFailurePolicy,
    pub required: bool,
    pub site: HookExecutionSite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverHookSurface {
    pub revision: HookPlanRevision,
    pub digest: HookPlanDigest,
    pub artifact_digest: Option<String>,
    pub configuration_boundary: ConfigurationBoundary,
    pub bindings: Vec<DriverHookBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverWorkspaceSurface {
    pub capabilities: Vec<WorkspaceCapability>,
    pub roots: Vec<String>,
}

/// Runtime-owned surface materialized for one immutable binding intent.
///
/// Drivers may cache this value, but must acknowledge exactly the revisions and digests they
/// actually installed. The Integration host never treats a requested digest as proof of apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct MaterializedDriverSurface {
    pub runtime_thread_id: RuntimeThreadId,
    pub revision: SurfaceRevision,
    pub digest: SurfaceDigest,
    pub authorization_identity: Option<AuthIdentity>,
    pub context: DriverContextSurface,
    pub tools: DriverToolSurface,
    pub hooks: DriverHookSurface,
    pub workspace: DriverWorkspaceSurface,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DriverSurfaceError {
    #[error("driver surface is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("driver surface request is stale")]
    Stale,
    #[error("driver surface materialization violated its digest contract: {reason}")]
    InvalidMaterialization { reason: String },
}

#[async_trait]
pub trait AgentRuntimeSurfaceBroker: Send + Sync {
    async fn materialize(
        &self,
        request: DriverSurfaceRequest,
    ) -> Result<MaterializedDriverSurface, DriverSurfaceError>;

    async fn materialize_tool_set(
        &self,
        binding_id: RuntimeBindingId,
        revision: ToolSetRevision,
        digest: &str,
    ) -> Result<DriverToolSurface, DriverSurfaceError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverContextCheckpointRequest {
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub checkpoint_id: ContextCheckpointId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverCompactionActivationRequest {
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub compaction_id: ContextCompactionId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverContextActivation {
    pub candidate_id: ContextCandidateId,
    pub checkpoint_id: ContextCheckpointId,
    pub context_revision: ContextRevision,
    pub materialized: MaterializedContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DriverContextError {
    #[error("driver context is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("driver context request is stale")]
    Stale,
    #[error("driver context does not exist")]
    NotFound,
    #[error("driver context materialization violated its digest contract: {reason}")]
    InvalidMaterialization { reason: String },
}

#[async_trait]
pub trait AgentRuntimeContextBroker: Send + Sync {
    async fn load_checkpoint(
        &self,
        request: DriverContextCheckpointRequest,
    ) -> Result<DriverContextActivation, DriverContextError>;

    async fn compaction_activation(
        &self,
        request: DriverCompactionActivationRequest,
    ) -> Result<DriverContextActivation, DriverContextError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverToolInvocation {
    pub thread_id: RuntimeThreadId,
    pub turn_id: RuntimeTurnId,
    pub item_id: RuntimeItemId,
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
    pub source_turn_id: DriverTurnId,
    pub source_item_id: DriverItemId,
    pub tool_set_revision: ToolSetRevision,
    pub tool_name: String,
    pub arguments: Value,
    pub timeout_ms: u64,
    pub authorization_identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriverToolOutcome {
    Completed {
        output: Value,
        is_error: bool,
    },
    InteractionRequired {
        interaction_id: RuntimeInteractionId,
        reason: String,
    },
    Denied {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DriverToolCallbackError {
    #[error("tool callback is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("tool callback coordinates are stale")]
    Stale,
    #[error("tool callback protocol violation: {reason}")]
    ProtocolViolation { reason: String },
}

#[async_trait]
pub trait AgentRuntimeToolCallback: Send + Sync {
    async fn invoke(
        &self,
        request: DriverToolInvocation,
    ) -> Result<DriverToolOutcome, DriverToolCallbackError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct DriverHookInvocation {
    pub thread_id: RuntimeThreadId,
    pub turn_id: Option<RuntimeTurnId>,
    pub item_id: Option<RuntimeItemId>,
    pub binding_id: RuntimeBindingId,
    pub generation: RuntimeDriverGeneration,
    pub source_thread_id: DriverThreadId,
    pub source_turn_id: Option<DriverTurnId>,
    pub source_item_id: Option<DriverItemId>,
    pub definition_id: HookDefinitionId,
    pub point: HookPoint,
    pub payload: Value,
    pub authorization_identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriverHookDecision {
    Continue {
        payload: Value,
    },
    Block {
        reason: String,
    },
    InteractionRequired {
        interaction_id: RuntimeInteractionId,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DriverHookCallbackError {
    #[error("hook callback is unavailable: {reason}")]
    Unavailable { reason: String, retryable: bool },
    #[error("hook callback coordinates are stale")]
    Stale,
    #[error("hook callback protocol violation: {reason}")]
    ProtocolViolation { reason: String },
}

#[async_trait]
pub trait AgentRuntimeHookCallback: Send + Sync {
    async fn execute(
        &self,
        request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError>;
}

#[derive(Clone)]
pub struct RuntimeDriverHostPorts {
    pub credentials: Arc<dyn AgentRuntimeCredentialBroker>,
    pub surfaces: Arc<dyn AgentRuntimeSurfaceBroker>,
    pub context: Arc<dyn AgentRuntimeContextBroker>,
    pub tools: Arc<dyn AgentRuntimeToolCallback>,
    pub hooks: Arc<dyn AgentRuntimeHookCallback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DriverFactoryError {
    #[error("driver configuration is invalid: {reason}")]
    InvalidConfiguration { reason: String },
    #[error("driver credential is unavailable for slot {slot}: {reason}")]
    CredentialUnavailable {
        slot: AgentRuntimeCredentialSlot,
        reason: String,
    },
    #[error("driver could not be created: {reason}")]
    Unavailable { reason: String, retryable: bool },
}

#[async_trait]
pub trait AgentRuntimeDriverFactory: Send + Sync {
    fn factory_key(&self) -> &AgentRuntimeFactoryKey;

    async fn create(
        &self,
        instance: ActivatedAgentServiceInstance,
        host: RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, DriverFactoryError>;
}

#[derive(Clone)]
pub struct AgentRuntimeDriverContribution {
    pub definition: AgentServiceDefinition,
    pub factory: Arc<dyn AgentRuntimeDriverFactory>,
}
