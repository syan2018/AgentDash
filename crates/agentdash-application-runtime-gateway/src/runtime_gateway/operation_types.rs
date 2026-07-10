use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use agentdash_domain::operation::{OperationProviderRef, OperationRef};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::OperationExecutionError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationEffect {
    Read,
    LocalMutation,
    ExternalSideEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationReplayPolicy {
    NonReplayable,
    Idempotent,
    ReplaySafe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationActorKind {
    User,
    Agent,
    Workflow,
    ExtensionService,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationReadiness {
    Ready,
    Unavailable { code: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationExecutionPolicy {
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
    pub max_inline_output_bytes: usize,
    pub result_ttl_seconds: u64,
}

impl Default for OperationExecutionPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            max_output_bytes: 1024 * 1024,
            max_inline_output_bytes: 64 * 1024,
            result_ttl_seconds: 15 * 60,
        }
    }
}

impl OperationExecutionPolicy {
    pub fn validate(&self) -> Result<(), OperationExecutionError> {
        if self.timeout_ms == 0 {
            return Err(OperationExecutionError::invalid_request(
                "Operation timeout_ms 必须大于 0",
            ));
        }
        if self.max_output_bytes == 0 {
            return Err(OperationExecutionError::invalid_request(
                "Operation max_output_bytes 必须大于 0",
            ));
        }
        if self.max_inline_output_bytes > self.max_output_bytes {
            return Err(OperationExecutionError::invalid_request(
                "Operation inline output 上限不能超过总 output 上限",
            ));
        }
        if self.result_ttl_seconds == 0 {
            return Err(OperationExecutionError::invalid_request(
                "Operation result TTL 必须大于 0",
            ));
        }
        if i64::try_from(self.result_ttl_seconds).is_err() {
            return Err(OperationExecutionError::invalid_request(
                "Operation result TTL 超出可表示范围",
            ));
        }
        Ok(())
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationDescriptor {
    pub operation_ref: OperationRef,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
    pub output_schema: Value,
    pub effect: OperationEffect,
    pub replay_policy: OperationReplayPolicy,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_capabilities: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub actor_visibility: BTreeSet<OperationActorKind>,
    pub execution_policy: OperationExecutionPolicy,
    pub readiness: OperationReadiness,
    pub provenance: OperationProvenance,
    pub dispatch: OperationDispatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationProvenance {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationDispatch {
    pub provider: OperationProviderRef,
    pub route: String,
}

impl OperationDescriptor {
    pub fn validate_identity(&self) -> Result<(), OperationExecutionError> {
        self.operation_ref
            .validate()
            .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
        if self.dispatch.provider != self.operation_ref.provider {
            return Err(OperationExecutionError::invalid_request(
                "Operation dispatch provider 与 OperationRef provider 不一致",
            ));
        }
        if self.title.trim().is_empty() || self.dispatch.route.trim().is_empty() {
            return Err(OperationExecutionError::invalid_request(
                "Operation title 与 dispatch route 不能为空",
            ));
        }
        if self.actor_visibility.is_empty() {
            return Err(OperationExecutionError::invalid_request(
                "Operation actor visibility 不能为空",
            ));
        }
        self.execution_policy.validate()
    }
}

#[derive(Debug, Clone)]
pub struct OperationCatalog {
    descriptors: HashMap<OperationRef, OperationDescriptor>,
}

impl OperationCatalog {
    pub fn try_new(
        descriptors: impl IntoIterator<Item = OperationDescriptor>,
    ) -> Result<Self, OperationExecutionError> {
        let mut catalog = HashMap::new();
        for descriptor in descriptors {
            descriptor.validate_identity()?;
            super::validate_json_schema_definition(&descriptor.input_schema).map_err(
                |message| OperationExecutionError::InvalidDescriptor {
                    field: "input_schema",
                    message,
                },
            )?;
            super::validate_json_schema_definition(&descriptor.output_schema).map_err(
                |message| OperationExecutionError::InvalidDescriptor {
                    field: "output_schema",
                    message,
                },
            )?;
            let operation_ref = descriptor.operation_ref.clone();
            if catalog.insert(operation_ref.clone(), descriptor).is_some() {
                return Err(OperationExecutionError::invalid_request(format!(
                    "Operation catalog 存在重复 exact identity: {}:{}:{}@{}",
                    operation_ref.provider.namespace,
                    operation_ref.provider.provider_key,
                    operation_ref.operation_key,
                    operation_ref.contract_version,
                )));
            }
        }
        Ok(Self {
            descriptors: catalog,
        })
    }

    pub fn get(&self, operation_ref: &OperationRef) -> Option<&OperationDescriptor> {
        self.descriptors.get(operation_ref)
    }

    pub fn descriptors(&self) -> Vec<&OperationDescriptor> {
        let mut descriptors = self.descriptors.values().collect::<Vec<_>>();
        descriptors.sort_by(|left, right| operation_sort_key(left).cmp(&operation_sort_key(right)));
        descriptors
    }
}

fn operation_sort_key(descriptor: &OperationDescriptor) -> (&str, &str, &str, u16) {
    (
        descriptor.operation_ref.provider.namespace.as_str(),
        descriptor.operation_ref.provider.provider_key.as_str(),
        descriptor.operation_ref.operation_key.as_str(),
        descriptor.operation_ref.contract_version,
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationPrincipal {
    User { user_id: String },
    AgentRunAgent { run_id: Uuid, agent_id: Uuid },
    WorkflowNode { run_id: Uuid, node_key: String },
    ExtensionInstallation { installation_id: Uuid },
}

impl OperationPrincipal {
    pub fn actor_kind(&self) -> OperationActorKind {
        match self {
            Self::User { .. } => OperationActorKind::User,
            Self::AgentRunAgent { .. } => OperationActorKind::Agent,
            Self::WorkflowNode { .. } => OperationActorKind::Workflow,
            Self::ExtensionInstallation { .. } => OperationActorKind::ExtensionService,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationScopeRef {
    Project {
        project_id: Uuid,
    },
    InteractionInstance {
        instance_id: Uuid,
    },
    WorkspaceBinding {
        project_id: Uuid,
        workspace_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct OperationAuthorizationScope {
    pub scope_ref: OperationScopeRef,
    pub authority_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationOrigin {
    AgentTool,
    UserWorkshop,
    Canvas {
        definition_id: Uuid,
    },
    Interaction {
        instance_id: Uuid,
    },
    ComponentEvent {
        instance_id: Uuid,
        component_key: String,
    },
    Workflow,
    OperationScriptNested {
        script_invocation_id: String,
    },
    EffectReplay {
        effect_id: Uuid,
    },
    ExtensionService,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationTraceContext {
    pub trace_id: String,
    pub invocation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_invocation_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl OperationTraceContext {
    pub fn root() -> Self {
        Self {
            trace_id: format!("trace-{}", Uuid::new_v4().simple()),
            invocation_id: format!("opinv-{}", Uuid::new_v4().simple()),
            parent_invocation_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn child_of(parent: &Self) -> Self {
        Self {
            trace_id: parent.trace_id.clone(),
            invocation_id: format!("opinv-{}", Uuid::new_v4().simple()),
            parent_invocation_id: Some(parent.invocation_id.clone()),
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationPlacement {
    Cloud,
    LocalBackend { backend_id: String },
}

/// Server-resolved command. This type intentionally does not implement `Deserialize`:
/// browser/iframe inputs must be resolved by a trusted host adapter first.
#[derive(Debug, Clone)]
pub struct OperationExecutionRequest {
    pub operation_ref: OperationRef,
    pub input: Value,
    pub principal: OperationPrincipal,
    pub scope: OperationAuthorizationScope,
    pub origin: OperationOrigin,
    pub trace: OperationTraceContext,
    pub deadline: DateTime<Utc>,
    pub idempotency_key: Option<String>,
    pub attachment_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OperationInvocationEnvelope {
    pub operation_ref: OperationRef,
    pub input: Value,
    pub principal: OperationPrincipal,
    pub scope: OperationAuthorizationScope,
    pub origin: OperationOrigin,
    pub placement: OperationPlacement,
    pub trace: OperationTraceContext,
    pub deadline: DateTime<Utc>,
    pub idempotency_key: Option<String>,
    pub attachment_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActorOperationSurface {
    pub authority_revision: String,
    pub granted_capabilities: BTreeSet<String>,
    pub catalog: OperationCatalog,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationResultValue {
    Inline { value: Value },
    Ref { result_ref: OperationResultRef },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationResultRef {
    pub result_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationResultAccess {
    pub principal: OperationPrincipal,
    pub scope: OperationAuthorizationScope,
    pub required_capabilities: BTreeSet<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ScopedOperationResult {
    pub result_ref: OperationResultRef,
    pub operation_ref: OperationRef,
    pub value: Value,
    pub access: OperationResultAccess,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationExecutionResult {
    pub operation_ref: OperationRef,
    pub trace: OperationTraceContext,
    pub value: OperationResultValue,
    pub output_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationAuditStage {
    Started,
    Admitted,
    Dispatched,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct OperationAuditEvent {
    pub operation_ref: OperationRef,
    pub principal: OperationPrincipal,
    pub scope: OperationAuthorizationScope,
    pub origin: OperationOrigin,
    pub trace: OperationTraceContext,
    pub stage: OperationAuditStage,
    pub outcome_code: Option<String>,
    pub occurred_at: DateTime<Utc>,
}
