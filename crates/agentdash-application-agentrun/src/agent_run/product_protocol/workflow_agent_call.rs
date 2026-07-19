#[cfg(test)]
use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeCommand, ManagedRuntimeCommandEnvelope, ManagedRuntimeContentBlock,
    ManagedRuntimeContextAuthority, ManagedRuntimeContextProvenance,
    ManagedRuntimeInitialContextContribution, ManagedRuntimeInitialContextContributionContent,
    ManagedRuntimeInitialContextMode, ManagedRuntimeInitialContextPackage,
    ManagedRuntimeOperationEvidence, ManagedRuntimeOperationStatus,
    ManagedRuntimeSourceBindingEvidence, RuntimeContextContributionId, RuntimeContextPackageId,
    RuntimeContextSourceRef, RuntimeContextSourceRevision, RuntimeIdempotencyKey,
    RuntimeOperationId, RuntimePayloadDigest, RuntimeThreadId,
};
use agentdash_application_workflow::{
    WorkflowAgentCallContentBlock, WorkflowAgentCallDispatchError,
    WorkflowAgentCallDispatchOutcome, WorkflowAgentCallDispatchPort, WorkflowAgentCallMailboxState,
    WorkflowAgentCallRequest, WorkflowAgentCallTargetIntent,
};
use agentdash_domain::agent_run_target::AgentRunTarget;
use agentdash_domain::workflow::WorkflowAgentCallSourceBindingRef;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::ProductManagedRuntimeCommandAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAgentCallProductPhase {
    MaterializeTarget,
    CreateRuntime,
    ActivateRuntime,
    CommitBinding,
    SubmitInput,
}

impl WorkflowAgentCallProductPhase {
    const CREATE_NEW_ORDER: [Self; 5] = [
        Self::MaterializeTarget,
        Self::CreateRuntime,
        Self::ActivateRuntime,
        Self::CommitBinding,
        Self::SubmitInput,
    ];
    const CONTINUE_CURRENT_ORDER: [Self; 1] = [Self::SubmitInput];

    fn slug(self) -> &'static str {
        match self {
            Self::MaterializeTarget => "materialize-target",
            Self::CreateRuntime => "create-runtime",
            Self::ActivateRuntime => "activate-runtime",
            Self::CommitBinding => "commit-binding",
            Self::SubmitInput => "submit-input",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowAgentCallProductPhaseIdentity {
    pub effect_id: String,
    pub runtime_operation_id: Option<RuntimeOperationId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowAgentCallProductPhaseReceipt {
    pub phase: WorkflowAgentCallProductPhase,
    pub identity: WorkflowAgentCallProductPhaseIdentity,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowAgentCallProductSaga {
    pub request: WorkflowAgentCallRequest,
    pub runtime_thread_id: RuntimeThreadId,
    pub version: u64,
    pub receipts: Vec<WorkflowAgentCallProductPhaseReceipt>,
    pub in_flight: Option<WorkflowAgentCallProductPhase>,
    pub source_binding: Option<ManagedRuntimeSourceBindingEvidence>,
    pub mailbox_state: Option<WorkflowAgentCallMailboxState>,
}

impl WorkflowAgentCallProductSaga {
    pub fn prepare(request: WorkflowAgentCallRequest) -> Result<Self, String> {
        if !request.validate_payload_digest() {
            return Err("Workflow AgentCall payload digest is invalid".to_owned());
        }
        let (runtime_thread_id, source_binding) = match &request.target_intent {
            WorkflowAgentCallTargetIntent::CreateNew { .. } => (
                RuntimeThreadId::new(format!(
                    "workflow-agent-call:{}",
                    request.identity.request_id
                ))
                .map_err(|error| error.to_string())?,
                None,
            ),
            WorkflowAgentCallTargetIntent::ContinueCurrent {
                runtime_thread_id,
                source_binding,
                ..
            } => {
                if source_binding.activated_at_revision.is_none() {
                    return Err(
                        "ContinueCurrent authority does not prove Runtime activation".to_owned(),
                    );
                }
                (
                    RuntimeThreadId::new(runtime_thread_id.clone())
                        .map_err(|error| error.to_string())?,
                    Some(runtime_binding_from_workflow(source_binding)?),
                )
            }
        };
        Ok(Self {
            request,
            runtime_thread_id,
            version: 0,
            receipts: Vec::new(),
            in_flight: None,
            source_binding,
            mailbox_state: None,
        })
    }

    pub fn target(&self) -> &AgentRunTarget {
        self.request.target_intent.target()
    }

    fn phase_identity(
        &self,
        phase: WorkflowAgentCallProductPhase,
    ) -> Result<WorkflowAgentCallProductPhaseIdentity, String> {
        let effect_id = format!("{}:{}", self.request.identity.request_id, phase.slug());
        let runtime_operation_id = match phase {
            WorkflowAgentCallProductPhase::CreateRuntime
            | WorkflowAgentCallProductPhase::ActivateRuntime
            | WorkflowAgentCallProductPhase::SubmitInput => Some(
                RuntimeOperationId::new(effect_id.clone()).map_err(|error| error.to_string())?,
            ),
            WorkflowAgentCallProductPhase::MaterializeTarget
            | WorkflowAgentCallProductPhase::CommitBinding => None,
        };
        Ok(WorkflowAgentCallProductPhaseIdentity {
            effect_id,
            runtime_operation_id,
        })
    }

    fn next_phase(&self) -> Option<WorkflowAgentCallProductPhase> {
        self.phase_plan()
            .iter()
            .copied()
            .find(|phase| !self.receipts.iter().any(|receipt| receipt.phase == *phase))
    }

    fn phase_plan(&self) -> &'static [WorkflowAgentCallProductPhase] {
        match self.request.target_intent {
            WorkflowAgentCallTargetIntent::CreateNew { .. } => {
                &WorkflowAgentCallProductPhase::CREATE_NEW_ORDER
            }
            WorkflowAgentCallTargetIntent::ContinueCurrent { .. } => {
                &WorkflowAgentCallProductPhase::CONTINUE_CURRENT_ORDER
            }
        }
    }

    fn mark_dispatched(&mut self, phase: WorkflowAgentCallProductPhase) -> Result<(), String> {
        if self.next_phase() != Some(phase) {
            return Err("Workflow AgentCall phase order drifted".to_owned());
        }
        if self.in_flight.is_some_and(|in_flight| in_flight != phase) {
            return Err("another Workflow AgentCall phase is already in flight".to_owned());
        }
        self.in_flight = Some(phase);
        Ok(())
    }

    fn mark_applied(
        &mut self,
        phase: WorkflowAgentCallProductPhase,
        accepted: bool,
        source_binding: Option<ManagedRuntimeSourceBindingEvidence>,
    ) -> Result<(), String> {
        if self.in_flight != Some(phase) {
            return Err("Workflow AgentCall phase was not durably dispatched".to_owned());
        }
        let identity = self.phase_identity(phase)?;
        match (phase, source_binding.as_ref()) {
            (WorkflowAgentCallProductPhase::CreateRuntime, Some(binding))
                if binding.activated_at_revision.is_none() => {}
            (WorkflowAgentCallProductPhase::ActivateRuntime, Some(binding))
                if binding.activated_at_revision.is_some() => {}
            (WorkflowAgentCallProductPhase::SubmitInput, None)
            | (WorkflowAgentCallProductPhase::MaterializeTarget, None)
            | (WorkflowAgentCallProductPhase::CommitBinding, None) => {}
            _ => {
                return Err(format!(
                    "Workflow AgentCall phase {phase:?} returned invalid binding evidence"
                ));
            }
        }
        self.receipts.push(WorkflowAgentCallProductPhaseReceipt {
            phase,
            identity,
            accepted,
        });
        self.in_flight = None;
        if let Some(binding) = source_binding {
            if let Some(existing) = self.source_binding.as_ref() {
                if existing.source_ref != binding.source_ref
                    || existing.committed_at_revision != binding.committed_at_revision
                    || existing.applied_surface_revision != binding.applied_surface_revision
                    || (existing.activated_at_revision.is_some()
                        && existing.activated_at_revision != binding.activated_at_revision)
                {
                    return Err("Workflow AgentCall Runtime binding drifted".to_owned());
                }
            }
            self.source_binding = Some(binding);
        }
        if phase == WorkflowAgentCallProductPhase::SubmitInput {
            self.mailbox_state = Some(if accepted {
                WorkflowAgentCallMailboxState::Queued
            } else {
                WorkflowAgentCallMailboxState::Submitted
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowAgentCallProductRepositoryError {
    #[error("Workflow AgentCall saga payload conflicts with prepared history")]
    PayloadConflict,
    #[error("Workflow AgentCall saga version conflicts")]
    VersionConflict,
    #[error("Workflow AgentCall saga persistence failed: {0}")]
    Persistence(String),
}

/// W8 durable storage contract. The adapter migration is mechanical from this
/// record: one row per request identity, with the whole saga body protected by
/// the scalar `version` CAS.
pub const WORKFLOW_AGENT_CALL_PRODUCT_SAGA_TABLE: &str = "workflow_agent_call_product_sagas";
pub const WORKFLOW_AGENT_CALL_PRODUCT_SAGA_PRIMARY_KEY: &[&str] = &["request_id"];
pub const WORKFLOW_AGENT_CALL_PRODUCT_SAGA_UNIQUE_KEYS: &[&[&str]] = &[&[
    "lifecycle_run_id",
    "orchestration_id",
    "node_path",
    "attempt",
]];
pub const WORKFLOW_AGENT_CALL_PRODUCT_SAGA_COLUMNS: &[&str] = &[
    "request_id:text not null",
    "lifecycle_run_id:uuid not null",
    "orchestration_id:uuid not null",
    "node_path:text not null",
    "attempt:bigint not null",
    "payload_digest:text not null",
    "request:jsonb not null",
    "target_run_id:uuid not null",
    "target_agent_id:uuid not null",
    "runtime_thread_id:text not null",
    "phase_plan:jsonb not null",
    "receipts:jsonb not null",
    "in_flight:text null",
    "source_binding:jsonb null",
    "mailbox_state:text null",
    "version:bigint not null",
    "created_at:timestamptz not null",
    "updated_at:timestamptz not null",
];
pub const WORKFLOW_AGENT_CALL_PRODUCT_EFFECT_TABLE: &str = "workflow_agent_call_product_effects";
pub const WORKFLOW_AGENT_CALL_PRODUCT_EFFECT_PRIMARY_KEY: &[&str] = &["effect_id"];
pub const WORKFLOW_AGENT_CALL_PRODUCT_EFFECT_UNIQUE_KEYS: &[&[&str]] =
    &[&["request_id", "phase"], &["runtime_operation_id"]];
pub const WORKFLOW_AGENT_CALL_PRODUCT_EFFECT_COLUMNS: &[&str] = &[
    "effect_id:text not null",
    "request_id:text not null references workflow_agent_call_product_sagas(request_id)",
    "phase:text not null",
    "runtime_operation_id:text null",
    "payload_digest:text not null",
    "state:text not null",
    "target_run_id:uuid not null",
    "target_agent_id:uuid not null",
    "runtime_thread_id:text not null",
    "evidence:jsonb null",
    "created_at:timestamptz not null",
    "updated_at:timestamptz not null",
];

#[async_trait]
pub trait WorkflowAgentCallProductSagaRepository: Send + Sync {
    /// Transaction contract: insert the full prepared row with version=0, or
    /// lock/read the primary-key row and return it only when identity,
    /// payload_digest, request JSON, target and RuntimeThreadId are identical.
    async fn prepare(
        &self,
        saga: WorkflowAgentCallProductSaga,
    ) -> Result<WorkflowAgentCallProductSaga, WorkflowAgentCallProductRepositoryError>;

    /// Transaction contract: `UPDATE ... SET <complete saga>, version=version+1
    /// WHERE request_id=? AND version=?`; zero rows is VersionConflict. Receipts
    /// must retain unique `(request_id, phase)` identities, Runtime operation
    /// ids are unique, binding evidence never regresses, and Submit acceptance
    /// is durable before Accepted is returned.
    async fn save(
        &self,
        expected_version: u64,
        saga: WorkflowAgentCallProductSaga,
    ) -> Result<WorkflowAgentCallProductSaga, WorkflowAgentCallProductRepositoryError>;
}

#[cfg(test)]
#[derive(Default)]
pub struct InMemoryWorkflowAgentCallProductSagaRepository {
    sagas: tokio::sync::Mutex<BTreeMap<String, WorkflowAgentCallProductSaga>>,
}

#[cfg(test)]
impl InMemoryWorkflowAgentCallProductSagaRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn load(&self, request_id: &str) -> Option<WorkflowAgentCallProductSaga> {
        self.sagas.lock().await.get(request_id).cloned()
    }
}

#[async_trait]
#[cfg(test)]
impl WorkflowAgentCallProductSagaRepository for InMemoryWorkflowAgentCallProductSagaRepository {
    async fn prepare(
        &self,
        saga: WorkflowAgentCallProductSaga,
    ) -> Result<WorkflowAgentCallProductSaga, WorkflowAgentCallProductRepositoryError> {
        let request_id = saga.request.identity.request_id.clone();
        let mut sagas = self.sagas.lock().await;
        if let Some(existing) = sagas.get(&request_id) {
            if existing.request.payload_digest != saga.request.payload_digest
                || existing.request != saga.request
                || existing.runtime_thread_id != saga.runtime_thread_id
            {
                return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
            }
            return Ok(existing.clone());
        }
        sagas.insert(request_id, saga.clone());
        Ok(saga)
    }

    async fn save(
        &self,
        expected_version: u64,
        mut saga: WorkflowAgentCallProductSaga,
    ) -> Result<WorkflowAgentCallProductSaga, WorkflowAgentCallProductRepositoryError> {
        let request_id = saga.request.identity.request_id.clone();
        let mut sagas = self.sagas.lock().await;
        let existing = sagas.get(&request_id).ok_or_else(|| {
            WorkflowAgentCallProductRepositoryError::Persistence(
                "prepared saga does not exist".to_owned(),
            )
        })?;
        if existing.version != expected_version {
            return Err(WorkflowAgentCallProductRepositoryError::VersionConflict);
        }
        if existing.request != saga.request || existing.runtime_thread_id != saga.runtime_thread_id
        {
            return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
        }
        saga.version = expected_version + 1;
        sagas.insert(request_id, saga.clone());
        Ok(saga)
    }
}

#[async_trait]
pub trait WorkflowAgentCallProductGraphPort: Send + Sync {
    async fn materialize_target(
        &self,
        request: &WorkflowAgentCallRequest,
        effect_id: &str,
    ) -> Result<(), String>;

    async fn commit_runtime_binding(
        &self,
        request_id: &str,
        payload_digest: &str,
        target: &AgentRunTarget,
        runtime_thread_id: &RuntimeThreadId,
        binding: &ManagedRuntimeSourceBindingEvidence,
        effect_id: &str,
    ) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAgentCallTargetMaterialization {
    pub request_id: String,
    pub payload_digest: String,
    pub target: AgentRunTarget,
    pub effect_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAgentCallBindingCommit {
    pub request_id: String,
    pub payload_digest: String,
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub binding: ManagedRuntimeSourceBindingEvidence,
    pub effect_id: String,
}

/// Durable Product graph contract implemented in W8. Each method is one
/// transaction: insert the effect ledger identity, apply the graph mutation,
/// and return the identical committed fact on replay. An existing effect_id
/// with different payload is a permanent conflict.
#[async_trait]
pub trait WorkflowAgentCallProductGraphRepository: Send + Sync {
    async fn materialize_target_idempotent(
        &self,
        mutation: WorkflowAgentCallTargetMaterialization,
    ) -> Result<WorkflowAgentCallTargetMaterialization, String>;

    async fn commit_runtime_binding_idempotent(
        &self,
        mutation: WorkflowAgentCallBindingCommit,
    ) -> Result<WorkflowAgentCallBindingCommit, String>;
}

#[derive(Clone)]
pub struct DurableWorkflowAgentCallProductGraphAdapter {
    repository: Arc<dyn WorkflowAgentCallProductGraphRepository>,
}

impl DurableWorkflowAgentCallProductGraphAdapter {
    pub fn new(repository: Arc<dyn WorkflowAgentCallProductGraphRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl WorkflowAgentCallProductGraphPort for DurableWorkflowAgentCallProductGraphAdapter {
    async fn materialize_target(
        &self,
        request: &WorkflowAgentCallRequest,
        effect_id: &str,
    ) -> Result<(), String> {
        let expected = WorkflowAgentCallTargetMaterialization {
            request_id: request.identity.request_id.clone(),
            payload_digest: request.payload_digest.clone(),
            target: request.target_intent.target().clone(),
            effect_id: effect_id.to_owned(),
        };
        let committed = self
            .repository
            .materialize_target_idempotent(expected.clone())
            .await?;
        if committed != expected {
            return Err("Product graph materialization evidence drifted".to_owned());
        }
        Ok(())
    }

    async fn commit_runtime_binding(
        &self,
        request_id: &str,
        payload_digest: &str,
        target: &AgentRunTarget,
        runtime_thread_id: &RuntimeThreadId,
        binding: &ManagedRuntimeSourceBindingEvidence,
        effect_id: &str,
    ) -> Result<(), String> {
        let expected = WorkflowAgentCallBindingCommit {
            request_id: request_id.to_owned(),
            payload_digest: payload_digest.to_owned(),
            target: target.clone(),
            runtime_thread_id: runtime_thread_id.clone(),
            binding: binding.clone(),
            effect_id: effect_id.to_owned(),
        };
        let committed = self
            .repository
            .commit_runtime_binding_idempotent(expected.clone())
            .await?;
        if committed != expected {
            return Err("Product graph binding evidence drifted".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowAgentCallRuntimeOutcome {
    Unknown,
    Accepted,
    Succeeded {
        source_binding: Option<ManagedRuntimeSourceBindingEvidence>,
    },
    Failed {
        reason: String,
    },
}

#[async_trait]
pub trait WorkflowAgentCallRuntimePort: Send + Sync {
    async fn execute(
        &self,
        saga: &WorkflowAgentCallProductSaga,
        phase: WorkflowAgentCallProductPhase,
        identity: &WorkflowAgentCallProductPhaseIdentity,
    ) -> Result<WorkflowAgentCallRuntimeOutcome, String>;

    async fn inspect(
        &self,
        saga: &WorkflowAgentCallProductSaga,
        phase: WorkflowAgentCallProductPhase,
        identity: &WorkflowAgentCallProductPhaseIdentity,
    ) -> Result<WorkflowAgentCallRuntimeOutcome, String>;
}

#[derive(Clone)]
pub struct ProductWorkflowAgentCallRuntimeAdapter {
    runtime: ProductManagedRuntimeCommandAdapter,
}

impl ProductWorkflowAgentCallRuntimeAdapter {
    pub fn new(runtime: ProductManagedRuntimeCommandAdapter) -> Self {
        Self { runtime }
    }

    fn command(
        saga: &WorkflowAgentCallProductSaga,
        phase: WorkflowAgentCallProductPhase,
        identity: &WorkflowAgentCallProductPhaseIdentity,
    ) -> Result<ManagedRuntimeCommandEnvelope, String> {
        let operation_id = identity
            .runtime_operation_id
            .clone()
            .ok_or_else(|| "Runtime phase is missing its operation identity".to_owned())?;
        let command = match phase {
            WorkflowAgentCallProductPhase::CreateRuntime => ManagedRuntimeCommand::Create {
                initial_context: Some(Self::initial_context(saga)?),
            },
            WorkflowAgentCallProductPhase::ActivateRuntime => ManagedRuntimeCommand::Activate,
            WorkflowAgentCallProductPhase::SubmitInput => ManagedRuntimeCommand::SubmitInput {
                content: saga
                    .request
                    .input
                    .iter()
                    .map(|block| match block {
                        WorkflowAgentCallContentBlock::Text { text } => {
                            ManagedRuntimeContentBlock::Text { text: text.clone() }
                        }
                        WorkflowAgentCallContentBlock::Structured { schema, value } => {
                            ManagedRuntimeContentBlock::Structured {
                                schema: schema.clone(),
                                value: value.clone(),
                            }
                        }
                    })
                    .collect(),
            },
            WorkflowAgentCallProductPhase::MaterializeTarget
            | WorkflowAgentCallProductPhase::CommitBinding => {
                return Err("Product graph phase is not a Runtime command".to_owned());
            }
        };
        Ok(ManagedRuntimeCommandEnvelope {
            idempotency_key: RuntimeIdempotencyKey::new(format!("idempotency:{operation_id}"))
                .map_err(|error| error.to_string())?,
            operation_id,
            thread_id: saga.runtime_thread_id.clone(),
            expected_revision: None,
            command,
        })
    }

    fn initial_context(
        saga: &WorkflowAgentCallProductSaga,
    ) -> Result<ManagedRuntimeInitialContextPackage, String> {
        let payload_digest = RuntimePayloadDigest::new(saga.request.payload_digest.clone())
            .map_err(|error| error.to_string())?;
        let provenance = ManagedRuntimeContextProvenance {
            authority: ManagedRuntimeContextAuthority::Workflow,
            source: RuntimeContextSourceRef::new(format!(
                "workflow-agent-call:{}",
                saga.request.identity.request_id
            ))
            .map_err(|error| error.to_string())?,
            revision: RuntimeContextSourceRevision::new(saga.request.payload_digest.clone())
                .map_err(|error| error.to_string())?,
            digest: payload_digest,
        };
        let mut contribution = ManagedRuntimeInitialContextContribution {
            contribution_id: RuntimeContextContributionId::new("workflow-agent-call-procedure")
                .map_err(|error| error.to_string())?,
            digest: RuntimePayloadDigest::new("pending").expect("non-empty digest placeholder"),
            content: ManagedRuntimeInitialContextContributionContent::WorkflowContext {
                schema: "agentdash.workflow.agent-call.procedure.v1".to_owned(),
                value: serde_json::json!({
                    "procedure_key": saga.request.procedure_key,
                    "contract": saga.request.procedure_contract,
                    "target": saga.target(),
                }),
                provenance,
            },
        };
        contribution.digest = contribution.calculated_digest();
        let mut package = ManagedRuntimeInitialContextPackage {
            package_id: RuntimeContextPackageId::new(format!(
                "workflow-agent-call-context:{}",
                saga.request.identity.request_id
            ))
            .map_err(|error| error.to_string())?,
            schema_version: 1,
            mode: ManagedRuntimeInitialContextMode::WorkflowOnly,
            contributions: vec![contribution],
            digest: RuntimePayloadDigest::new("pending").expect("non-empty digest placeholder"),
        };
        package.digest = package.calculated_digest();
        Ok(package)
    }

    fn map_observation(
        phase: WorkflowAgentCallProductPhase,
        status: ManagedRuntimeOperationStatus,
        evidence: Option<ManagedRuntimeOperationEvidence>,
    ) -> Result<WorkflowAgentCallRuntimeOutcome, String> {
        match status {
            ManagedRuntimeOperationStatus::Accepted | ManagedRuntimeOperationStatus::Running => {
                Ok(WorkflowAgentCallRuntimeOutcome::Accepted)
            }
            ManagedRuntimeOperationStatus::Succeeded => match phase {
                WorkflowAgentCallProductPhase::CreateRuntime => {
                    let Some(ManagedRuntimeOperationEvidence::Create { binding, .. }) = evidence
                    else {
                        return Err(
                            "succeeded Workflow AgentCall Create is missing binding evidence"
                                .to_owned(),
                        );
                    };
                    Ok(WorkflowAgentCallRuntimeOutcome::Succeeded {
                        source_binding: Some(binding),
                    })
                }
                WorkflowAgentCallProductPhase::ActivateRuntime => {
                    let Some(ManagedRuntimeOperationEvidence::Activate { binding }) = evidence
                    else {
                        return Err(
                            "succeeded Workflow AgentCall Activate is missing binding evidence"
                                .to_owned(),
                        );
                    };
                    if binding.activated_at_revision.is_none() {
                        return Err(
                            "Workflow AgentCall Activate did not prove activation".to_owned()
                        );
                    }
                    Ok(WorkflowAgentCallRuntimeOutcome::Succeeded {
                        source_binding: Some(binding),
                    })
                }
                WorkflowAgentCallProductPhase::SubmitInput => {
                    if evidence.is_some() {
                        return Err(
                            "Workflow AgentCall SubmitInput returned foreign evidence".to_owned()
                        );
                    }
                    Ok(WorkflowAgentCallRuntimeOutcome::Succeeded {
                        source_binding: None,
                    })
                }
                WorkflowAgentCallProductPhase::MaterializeTarget
                | WorkflowAgentCallProductPhase::CommitBinding => {
                    Err("Product graph phase cannot return Runtime evidence".to_owned())
                }
            },
            ManagedRuntimeOperationStatus::Failed
            | ManagedRuntimeOperationStatus::Interrupted
            | ManagedRuntimeOperationStatus::Lost => Ok(WorkflowAgentCallRuntimeOutcome::Failed {
                reason: format!("Workflow AgentCall Runtime operation ended with {status:?}"),
            }),
        }
    }
}

#[async_trait]
impl WorkflowAgentCallRuntimePort for ProductWorkflowAgentCallRuntimeAdapter {
    async fn execute(
        &self,
        saga: &WorkflowAgentCallProductSaga,
        phase: WorkflowAgentCallProductPhase,
        identity: &WorkflowAgentCallProductPhaseIdentity,
    ) -> Result<WorkflowAgentCallRuntimeOutcome, String> {
        let observation = self
            .runtime
            .execute(Self::command(saga, phase, identity)?)
            .await?;
        Self::map_observation(phase, observation.status, observation.evidence)
    }

    async fn inspect(
        &self,
        saga: &WorkflowAgentCallProductSaga,
        phase: WorkflowAgentCallProductPhase,
        identity: &WorkflowAgentCallProductPhaseIdentity,
    ) -> Result<WorkflowAgentCallRuntimeOutcome, String> {
        let operation_id = identity
            .runtime_operation_id
            .as_ref()
            .ok_or_else(|| "Runtime phase is missing its operation identity".to_owned())?;
        let Some(observation) = self
            .runtime
            .inspect(&saga.runtime_thread_id, operation_id)
            .await?
        else {
            return Ok(WorkflowAgentCallRuntimeOutcome::Unknown);
        };
        Self::map_observation(phase, observation.status, observation.evidence)
    }
}

#[derive(Clone)]
pub struct ProductWorkflowAgentCallDispatchService {
    repository: Arc<dyn WorkflowAgentCallProductSagaRepository>,
    product_graph: Arc<dyn WorkflowAgentCallProductGraphPort>,
    runtime: Arc<dyn WorkflowAgentCallRuntimePort>,
}

impl ProductWorkflowAgentCallDispatchService {
    #[cfg(test)]
    pub fn new(
        repository: Arc<dyn WorkflowAgentCallProductSagaRepository>,
        product_graph: Arc<dyn WorkflowAgentCallProductGraphPort>,
        runtime: Arc<dyn WorkflowAgentCallRuntimePort>,
    ) -> Self {
        Self {
            repository,
            product_graph,
            runtime,
        }
    }

    async fn save(
        &self,
        expected_version: u64,
        saga: WorkflowAgentCallProductSaga,
    ) -> Result<WorkflowAgentCallProductSaga, WorkflowAgentCallDispatchError> {
        self.repository
            .save(expected_version, saga)
            .await
            .map_err(repository_dispatch_error)
    }

    async fn dispatch_inner(
        &self,
        request: WorkflowAgentCallRequest,
    ) -> Result<WorkflowAgentCallDispatchOutcome, WorkflowAgentCallDispatchError> {
        if !request.validate_payload_digest() {
            return Err(WorkflowAgentCallDispatchError::new(
                "agent_call_payload_digest_invalid",
                "Workflow AgentCall payload digest 无效",
                false,
            ));
        }
        let prepared = WorkflowAgentCallProductSaga::prepare(request).map_err(|message| {
            WorkflowAgentCallDispatchError::new(
                "agent_call_product_prepare_invalid",
                message,
                false,
            )
        })?;
        let mut saga = self
            .repository
            .prepare(prepared)
            .await
            .map_err(repository_dispatch_error)?;

        loop {
            let Some(phase) = saga.next_phase() else {
                return Ok(WorkflowAgentCallDispatchOutcome::Accepted {
                    target: saga.target().clone(),
                    runtime_thread_id: saga.runtime_thread_id.to_string(),
                    source_binding: workflow_binding_from_runtime(
                        saga.source_binding.as_ref().ok_or_else(|| {
                            WorkflowAgentCallDispatchError::new(
                                "agent_call_runtime_binding_missing",
                                "accepted AgentCall 缺少 Runtime source binding",
                                false,
                            )
                        })?,
                    ),
                    mailbox_state: saga
                        .mailbox_state
                        .unwrap_or(WorkflowAgentCallMailboxState::Submitted),
                });
            };
            let identity = saga.phase_identity(phase).map_err(invalid_saga)?;
            let was_in_flight = saga.in_flight == Some(phase);
            if !was_in_flight {
                let expected_version = saga.version;
                saga.mark_dispatched(phase).map_err(invalid_saga)?;
                saga = self.save(expected_version, saga).await?;
            }

            match phase {
                WorkflowAgentCallProductPhase::MaterializeTarget => {
                    self.product_graph
                        .materialize_target(&saga.request, &identity.effect_id)
                        .await
                        .map_err(retryable_product_effect)?;
                    let expected_version = saga.version;
                    saga.mark_applied(phase, false, None)
                        .map_err(invalid_saga)?;
                    saga = self.save(expected_version, saga).await?;
                }
                WorkflowAgentCallProductPhase::CommitBinding => {
                    let binding = saga.source_binding.clone().ok_or_else(|| {
                        WorkflowAgentCallDispatchError::new(
                            "agent_call_runtime_binding_missing",
                            "binding commit 前缺少 Runtime Create/Activate evidence",
                            false,
                        )
                    })?;
                    self.product_graph
                        .commit_runtime_binding(
                            &saga.request.identity.request_id,
                            &saga.request.payload_digest,
                            saga.target(),
                            &saga.runtime_thread_id,
                            &binding,
                            &identity.effect_id,
                        )
                        .await
                        .map_err(retryable_product_effect)?;
                    let expected_version = saga.version;
                    saga.mark_applied(phase, false, None)
                        .map_err(invalid_saga)?;
                    saga = self.save(expected_version, saga).await?;
                }
                WorkflowAgentCallProductPhase::CreateRuntime
                | WorkflowAgentCallProductPhase::ActivateRuntime
                | WorkflowAgentCallProductPhase::SubmitInput => {
                    let inspected = if was_in_flight {
                        self.runtime.inspect(&saga, phase, &identity).await
                    } else {
                        Ok(WorkflowAgentCallRuntimeOutcome::Unknown)
                    }
                    .map_err(retryable_runtime_effect)?;
                    let outcome = match inspected {
                        WorkflowAgentCallRuntimeOutcome::Unknown => {
                            match self.runtime.execute(&saga, phase, &identity).await {
                                Ok(outcome) => outcome,
                                Err(_) => self
                                    .runtime
                                    .inspect(&saga, phase, &identity)
                                    .await
                                    .map_err(retryable_runtime_effect)?,
                            }
                        }
                        outcome => outcome,
                    };
                    match outcome {
                        WorkflowAgentCallRuntimeOutcome::Unknown
                        | WorkflowAgentCallRuntimeOutcome::Accepted
                            if phase != WorkflowAgentCallProductPhase::SubmitInput =>
                        {
                            return Ok(WorkflowAgentCallDispatchOutcome::Pending);
                        }
                        WorkflowAgentCallRuntimeOutcome::Accepted => {
                            let expected_version = saga.version;
                            saga.mark_applied(phase, true, None).map_err(invalid_saga)?;
                            saga = self.save(expected_version, saga).await?;
                        }
                        WorkflowAgentCallRuntimeOutcome::Succeeded { source_binding } => {
                            let expected_version = saga.version;
                            saga.mark_applied(phase, false, source_binding)
                                .map_err(invalid_saga)?;
                            saga = self.save(expected_version, saga).await?;
                        }
                        WorkflowAgentCallRuntimeOutcome::Failed { reason } => {
                            return Err(WorkflowAgentCallDispatchError::new(
                                "agent_call_runtime_failed",
                                reason,
                                false,
                            ));
                        }
                        WorkflowAgentCallRuntimeOutcome::Unknown => {
                            return Ok(WorkflowAgentCallDispatchOutcome::Pending);
                        }
                    }
                }
            }
        }
    }
}

#[async_trait]
impl WorkflowAgentCallDispatchPort for ProductWorkflowAgentCallDispatchService {
    async fn dispatch(
        &self,
        request: WorkflowAgentCallRequest,
    ) -> Result<WorkflowAgentCallDispatchOutcome, WorkflowAgentCallDispatchError> {
        self.dispatch_inner(request).await
    }
}

/// The only production construction path for Workflow AgentCall dispatch.
/// Callers must provide durable saga and Product graph repositories; the
/// in-memory saga store is compiled for tests only.
pub fn build_durable_workflow_agent_call_dispatch(
    saga_repository: Arc<dyn WorkflowAgentCallProductSagaRepository>,
    product_graph_repository: Arc<dyn WorkflowAgentCallProductGraphRepository>,
    managed_runtime: ProductManagedRuntimeCommandAdapter,
) -> Arc<dyn WorkflowAgentCallDispatchPort> {
    let graph = Arc::new(DurableWorkflowAgentCallProductGraphAdapter::new(
        product_graph_repository,
    ));
    let runtime = Arc::new(ProductWorkflowAgentCallRuntimeAdapter::new(managed_runtime));
    Arc::new(ProductWorkflowAgentCallDispatchService {
        repository: saga_repository,
        product_graph: graph,
        runtime,
    })
}

fn repository_dispatch_error(
    error: WorkflowAgentCallProductRepositoryError,
) -> WorkflowAgentCallDispatchError {
    match error {
        WorkflowAgentCallProductRepositoryError::PayloadConflict => {
            WorkflowAgentCallDispatchError::new(
                "agent_call_payload_conflict",
                error.to_string(),
                false,
            )
        }
        WorkflowAgentCallProductRepositoryError::VersionConflict => {
            WorkflowAgentCallDispatchError::new(
                "agent_call_saga_version_conflict",
                error.to_string(),
                true,
            )
        }
        WorkflowAgentCallProductRepositoryError::Persistence(_) => {
            WorkflowAgentCallDispatchError::new(
                "agent_call_saga_persistence_failed",
                error.to_string(),
                true,
            )
        }
    }
}

fn invalid_saga(message: String) -> WorkflowAgentCallDispatchError {
    WorkflowAgentCallDispatchError::new("agent_call_saga_invalid", message, false)
}

fn retryable_product_effect(message: String) -> WorkflowAgentCallDispatchError {
    WorkflowAgentCallDispatchError::new("agent_call_product_effect_unavailable", message, true)
}

fn retryable_runtime_effect(message: String) -> WorkflowAgentCallDispatchError {
    WorkflowAgentCallDispatchError::new("agent_call_runtime_unavailable", message, true)
}

pub fn workflow_agent_call_request_fingerprint(request: &WorkflowAgentCallRequest) -> String {
    let canonical = serde_json::to_vec(request).expect("Workflow AgentCall request serializes");
    format!("sha256:{:x}", Sha256::digest(canonical))
}

fn workflow_binding_from_runtime(
    binding: &ManagedRuntimeSourceBindingEvidence,
) -> WorkflowAgentCallSourceBindingRef {
    WorkflowAgentCallSourceBindingRef {
        source_ref: binding.source_ref.to_string(),
        committed_at_revision: binding.committed_at_revision.0,
        applied_surface_revision: binding.applied_surface_revision.0,
        activated_at_revision: binding.activated_at_revision.map(|revision| revision.0),
    }
}

fn runtime_binding_from_workflow(
    binding: &WorkflowAgentCallSourceBindingRef,
) -> Result<ManagedRuntimeSourceBindingEvidence, String> {
    Ok(ManagedRuntimeSourceBindingEvidence {
        source_ref: agentdash_agent_runtime_contract::RuntimeSourceRef::new(
            binding.source_ref.clone(),
        )
        .map_err(|error| error.to_string())?,
        committed_at_revision: agentdash_agent_runtime_contract::RuntimeProjectionRevision(
            binding.committed_at_revision,
        ),
        applied_surface_revision: agentdash_agent_runtime_contract::SurfaceRevision(
            binding.applied_surface_revision,
        ),
        activated_at_revision: binding
            .activated_at_revision
            .map(agentdash_agent_runtime_contract::RuntimeProjectionRevision),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use agentdash_agent_runtime_contract::{
        RuntimeProjectionRevision, RuntimeSourceRef, SurfaceRevision,
    };
    use agentdash_application_workflow::{
        WorkflowAgentCallIdentity, WorkflowAgentCallTargetIntent,
    };
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct RecordingProductGraph {
        materializations: tokio::sync::Mutex<Vec<String>>,
        commits: tokio::sync::Mutex<Vec<String>>,
    }

    #[derive(Default)]
    struct RecordingDurableGraphRepository {
        bindings: tokio::sync::Mutex<Vec<WorkflowAgentCallBindingCommit>>,
    }

    #[async_trait]
    impl WorkflowAgentCallProductGraphRepository for RecordingDurableGraphRepository {
        async fn materialize_target_idempotent(
            &self,
            mutation: WorkflowAgentCallTargetMaterialization,
        ) -> Result<WorkflowAgentCallTargetMaterialization, String> {
            Ok(mutation)
        }

        async fn commit_runtime_binding_idempotent(
            &self,
            mutation: WorkflowAgentCallBindingCommit,
        ) -> Result<WorkflowAgentCallBindingCommit, String> {
            self.bindings.lock().await.push(mutation.clone());
            Ok(mutation)
        }
    }

    #[async_trait]
    impl WorkflowAgentCallProductGraphPort for RecordingProductGraph {
        async fn materialize_target(
            &self,
            _request: &WorkflowAgentCallRequest,
            effect_id: &str,
        ) -> Result<(), String> {
            self.materializations
                .lock()
                .await
                .push(effect_id.to_owned());
            Ok(())
        }

        async fn commit_runtime_binding(
            &self,
            _request_id: &str,
            _payload_digest: &str,
            _target: &AgentRunTarget,
            _runtime_thread_id: &RuntimeThreadId,
            _binding: &ManagedRuntimeSourceBindingEvidence,
            effect_id: &str,
        ) -> Result<(), String> {
            self.commits.lock().await.push(effect_id.to_owned());
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingRuntime {
        executions: tokio::sync::Mutex<
            Vec<(
                WorkflowAgentCallProductPhase,
                WorkflowAgentCallProductPhaseIdentity,
            )>,
        >,
        observations: tokio::sync::Mutex<
            BTreeMap<WorkflowAgentCallProductPhase, WorkflowAgentCallRuntimeOutcome>,
        >,
        lose_create_receipt_once: tokio::sync::Mutex<bool>,
    }

    impl RecordingRuntime {
        async fn with_create_receipt_loss() -> Self {
            Self {
                lose_create_receipt_once: tokio::sync::Mutex::new(true),
                ..Default::default()
            }
        }

        fn binding(activated: bool) -> ManagedRuntimeSourceBindingEvidence {
            ManagedRuntimeSourceBindingEvidence {
                source_ref: RuntimeSourceRef::new("source:workflow-agent").expect("source"),
                committed_at_revision: RuntimeProjectionRevision(2),
                applied_surface_revision: SurfaceRevision(3),
                activated_at_revision: activated.then_some(RuntimeProjectionRevision(4)),
            }
        }

        fn success(phase: WorkflowAgentCallProductPhase) -> WorkflowAgentCallRuntimeOutcome {
            match phase {
                WorkflowAgentCallProductPhase::CreateRuntime => {
                    WorkflowAgentCallRuntimeOutcome::Succeeded {
                        source_binding: Some(Self::binding(false)),
                    }
                }
                WorkflowAgentCallProductPhase::ActivateRuntime => {
                    WorkflowAgentCallRuntimeOutcome::Succeeded {
                        source_binding: Some(Self::binding(true)),
                    }
                }
                WorkflowAgentCallProductPhase::SubmitInput => {
                    WorkflowAgentCallRuntimeOutcome::Succeeded {
                        source_binding: None,
                    }
                }
                _ => unreachable!("only Runtime phases are recorded"),
            }
        }
    }

    #[async_trait]
    impl WorkflowAgentCallRuntimePort for RecordingRuntime {
        async fn execute(
            &self,
            _saga: &WorkflowAgentCallProductSaga,
            phase: WorkflowAgentCallProductPhase,
            identity: &WorkflowAgentCallProductPhaseIdentity,
        ) -> Result<WorkflowAgentCallRuntimeOutcome, String> {
            self.executions.lock().await.push((phase, identity.clone()));
            let outcome = Self::success(phase);
            self.observations
                .lock()
                .await
                .insert(phase, outcome.clone());
            let mut lose = self.lose_create_receipt_once.lock().await;
            if phase == WorkflowAgentCallProductPhase::CreateRuntime && *lose {
                *lose = false;
                return Err("receipt lost after Runtime accepted command".to_owned());
            }
            Ok(outcome)
        }

        async fn inspect(
            &self,
            _saga: &WorkflowAgentCallProductSaga,
            phase: WorkflowAgentCallProductPhase,
            _identity: &WorkflowAgentCallProductPhaseIdentity,
        ) -> Result<WorkflowAgentCallRuntimeOutcome, String> {
            Ok(self
                .observations
                .lock()
                .await
                .get(&phase)
                .cloned()
                .unwrap_or(WorkflowAgentCallRuntimeOutcome::Unknown))
        }
    }

    fn request() -> WorkflowAgentCallRequest {
        let run_id = Uuid::new_v4();
        WorkflowAgentCallRequest {
            identity: WorkflowAgentCallIdentity {
                request_id: "workflow-agent-call:orchestration:node#1".to_owned(),
                lifecycle_run_id: run_id,
                orchestration_id: Uuid::new_v4(),
                node_path: "node".to_owned(),
                attempt: 1,
            },
            payload_digest: String::new(),
            project_id: Uuid::new_v4(),
            created_by_user_id: "user-1".to_owned(),
            target_intent: WorkflowAgentCallTargetIntent::CreateNew {
                target: AgentRunTarget {
                    run_id,
                    agent_id: Uuid::new_v4(),
                },
            },
            procedure_key: Some("review".to_owned()),
            procedure_contract: Default::default(),
            input: vec![
                WorkflowAgentCallContentBlock::Text {
                    text: "review".to_owned(),
                },
                WorkflowAgentCallContentBlock::Structured {
                    schema: "agentdash.workflow.agent-call.input-port.v1".to_owned(),
                    value: serde_json::json!({
                        "port_key": "context",
                        "value": {"nested": [1, true, null]},
                    }),
                },
            ],
        }
        .with_calculated_payload_digest()
    }

    #[tokio::test]
    async fn prepare_provision_bind_and_submit_survives_receipt_loss_with_same_identity() {
        let repository = Arc::new(InMemoryWorkflowAgentCallProductSagaRepository::new());
        let graph = Arc::new(RecordingProductGraph::default());
        let runtime = Arc::new(RecordingRuntime::with_create_receipt_loss().await);
        let service = ProductWorkflowAgentCallDispatchService::new(
            repository.clone(),
            graph.clone(),
            runtime.clone(),
        );
        let request = request();

        let outcome = service.dispatch(request.clone()).await.expect("dispatch");
        assert!(matches!(
            outcome,
            WorkflowAgentCallDispatchOutcome::Accepted {
                mailbox_state: WorkflowAgentCallMailboxState::Submitted,
                ..
            }
        ));
        let saga = repository
            .load(&request.identity.request_id)
            .await
            .expect("saga");
        assert_eq!(
            saga.receipts
                .iter()
                .map(|receipt| receipt.phase)
                .collect::<Vec<_>>(),
            WorkflowAgentCallProductPhase::CREATE_NEW_ORDER
        );
        assert_eq!(graph.materializations.lock().await.len(), 1);
        assert_eq!(graph.commits.lock().await.len(), 1);
        let first_executions = runtime.executions.lock().await.clone();
        assert_eq!(first_executions.len(), 3);

        let replay = service.dispatch(request).await.expect("idempotent replay");
        assert_eq!(outcome, replay);
        assert_eq!(*runtime.executions.lock().await, first_executions);
    }

    #[tokio::test]
    async fn prepared_request_rejects_payload_digest_conflict_before_product_effects() {
        let repository = Arc::new(InMemoryWorkflowAgentCallProductSagaRepository::new());
        let graph = Arc::new(RecordingProductGraph::default());
        let runtime = Arc::new(RecordingRuntime::default());
        let service = ProductWorkflowAgentCallDispatchService::new(
            repository,
            graph.clone(),
            runtime.clone(),
        );
        let original = request();
        service
            .dispatch(original.clone())
            .await
            .expect("first dispatch");

        let mut conflicting = original;
        conflicting.input.push(WorkflowAgentCallContentBlock::Text {
            text: "different".to_owned(),
        });
        conflicting = conflicting.with_calculated_payload_digest();
        let error = service
            .dispatch(conflicting)
            .await
            .expect_err("payload conflict");
        assert_eq!(error.code, "agent_call_payload_conflict");
        assert_eq!(graph.materializations.lock().await.len(), 1);
        assert_eq!(runtime.executions.lock().await.len(), 3);
    }

    #[tokio::test]
    async fn continue_current_submits_only_to_durable_authority_thread() {
        let repository = Arc::new(InMemoryWorkflowAgentCallProductSagaRepository::new());
        let graph = Arc::new(RecordingProductGraph::default());
        let runtime = Arc::new(RecordingRuntime::default());
        let service = ProductWorkflowAgentCallDispatchService::new(
            repository.clone(),
            graph.clone(),
            runtime.clone(),
        );
        let mut request = request();
        let target = request.target_intent.target().clone();
        request.target_intent = WorkflowAgentCallTargetIntent::ContinueCurrent {
            target,
            runtime_thread_id: "existing-runtime-thread".to_owned(),
            source_binding: workflow_binding_from_runtime(&RecordingRuntime::binding(true)),
        };
        request = request.with_calculated_payload_digest();

        let outcome = service.dispatch(request.clone()).await.expect("dispatch");

        assert!(matches!(
            outcome,
            WorkflowAgentCallDispatchOutcome::Accepted {
                runtime_thread_id,
                ..
            } if runtime_thread_id == "existing-runtime-thread"
        ));
        assert!(graph.materializations.lock().await.is_empty());
        assert!(graph.commits.lock().await.is_empty());
        assert_eq!(
            runtime
                .executions
                .lock()
                .await
                .iter()
                .map(|(phase, _)| *phase)
                .collect::<Vec<_>>(),
            vec![WorkflowAgentCallProductPhase::SubmitInput]
        );
        assert_eq!(
            repository
                .load(&request.identity.request_id)
                .await
                .expect("saga")
                .receipts
                .iter()
                .map(|receipt| receipt.phase)
                .collect::<Vec<_>>(),
            WorkflowAgentCallProductPhase::CONTINUE_CURRENT_ORDER
        );
    }

    #[tokio::test]
    async fn durable_graph_binding_mutation_carries_saga_request_identity() {
        let repository = Arc::new(RecordingDurableGraphRepository::default());
        let adapter = DurableWorkflowAgentCallProductGraphAdapter::new(repository.clone());
        let request = request();
        let target = request.target_intent.target().clone();
        let thread_id = RuntimeThreadId::new("thread:binding").expect("thread");
        let binding = RecordingRuntime::binding(true);

        adapter
            .commit_runtime_binding(
                &request.identity.request_id,
                &request.payload_digest,
                &target,
                &thread_id,
                &binding,
                "effect:commit-binding",
            )
            .await
            .expect("commit");

        assert_eq!(
            repository.bindings.lock().await.as_slice(),
            &[WorkflowAgentCallBindingCommit {
                request_id: request.identity.request_id,
                payload_digest: request.payload_digest,
                target,
                runtime_thread_id: thread_id,
                binding,
                effect_id: "effect:commit-binding".to_owned(),
            }]
        );
    }
}
