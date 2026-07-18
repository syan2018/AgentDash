use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentRunForkRequestId(pub Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunForkParent {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub source_coordinate: String,
    pub through_turn_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreallocatedAgentRunChild {
    pub agent_run_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub presentation_thread_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunForkSagaPhase {
    Requested,
    RuntimeAdmitted,
    AgentForkApplied,
    RuntimeProvisioned,
    ProductGraphCommitted,
    RuntimeActivated,
    Succeeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunForkRuntimeOperation {
    Admit,
    ApplyAgentFork,
    Provision,
    Activate,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentRunForkOperationIdentity {
    pub request_id: AgentRunForkRequestId,
    pub operation: AgentRunForkRuntimeOperation,
    pub child_run_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAgentChildIdentity {
    pub source_coordinate: String,
    pub runtime_agent_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitialContextDeliveryFidelity {
    Unsupported,
    CanonicalRendered,
    TypedNative,
}

impl InitialContextDeliveryFidelity {
    pub fn satisfies(self, minimum: Self) -> bool {
        matches!(
            (self, minimum),
            (Self::TypedNative, _)
                | (
                    Self::CanonicalRendered,
                    Self::CanonicalRendered | Self::Unsupported
                )
                | (Self::Unsupported, Self::Unsupported)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialContextContributionApplicationEvidence {
    pub kind: String,
    pub fidelity: InitialContextDeliveryFidelity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialContextApplicationEvidence {
    pub package_id: Uuid,
    pub package_digest: String,
    pub fidelity: InitialContextDeliveryFidelity,
    pub contribution_fidelity: Vec<InitialContextContributionApplicationEvidence>,
    pub renderer_version: Option<String>,
    pub materialized_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredInitialContextEvidence {
    pub package_id: Uuid,
    pub package_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeForkPhaseEvidence {
    pub child: Option<RuntimeAgentChildIdentity>,
    pub host_binding: Option<String>,
    pub child_history_digest: Option<String>,
    pub context: Option<InitialContextApplicationEvidence>,
    pub receipt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductGraphCommitEvidence {
    pub agent_run_id: Uuid,
    pub child_run_id: Uuid,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub presentation_thread_id: String,
    pub runtime_child: RuntimeAgentChildIdentity,
    pub host_binding: String,
    pub child_history_digest: String,
    pub commit_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableRuntimeDispatch {
    pub identity: AgentRunForkOperationIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LostRuntimeOperation {
    pub identity: AgentRunForkOperationIdentity,
    pub known_child: Option<RuntimeAgentChildIdentity>,
    pub known_host_binding: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentRunForkSagaReceipts {
    pub runtime_admission: Option<String>,
    pub agent_fork: Option<String>,
    pub runtime_provisioning: Option<String>,
    pub runtime_activation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunForkFailure {
    pub phase: AgentRunForkSagaPhase,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunForkSaga {
    request_id: AgentRunForkRequestId,
    parent: AgentRunForkParent,
    child: PreallocatedAgentRunChild,
    phase: AgentRunForkSagaPhase,
    version: u64,
    durable_runtime_dispatch: Option<DurableRuntimeDispatch>,
    runtime_child: Option<RuntimeAgentChildIdentity>,
    host_binding: Option<String>,
    child_history_digest: Option<String>,
    required_initial_context: Option<RequiredInitialContextEvidence>,
    initial_context_evidence: Option<InitialContextApplicationEvidence>,
    receipts: AgentRunForkSagaReceipts,
    graph_commit: Option<ProductGraphCommitEvidence>,
    failed: Option<AgentRunForkFailure>,
    lost: Option<LostRuntimeOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunForkSagaStep {
    DispatchRuntime(AgentRunForkOperationIdentity),
    InspectRuntime(AgentRunForkOperationIdentity),
    CommitProductGraph,
    MarkSucceeded,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeOperationOutcome {
    Applied(RuntimeForkPhaseEvidence),
    Unknown,
    Lost {
        known_child: Option<RuntimeAgentChildIdentity>,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunForkSagaError {
    #[error("fork saga is terminal")]
    Terminal,
    #[error("operation {actual:?} is invalid while saga is in {phase:?}")]
    InvalidOperation {
        phase: AgentRunForkSagaPhase,
        actual: AgentRunForkRuntimeOperation,
    },
    #[error("runtime outcome does not belong to the pending operation identity")]
    OperationIdentityMismatch,
    #[error("runtime dispatch marker does not belong to the current operation")]
    DispatchIdentityMismatch,
    #[error("Runtime/Agent child identity drifted after it was pinned")]
    RuntimeChildIdentityDrift,
    #[error("Host binding identity drifted after it was pinned")]
    HostBindingIdentityDrift,
    #[error("native fork receipt did not include an exact child history digest")]
    MissingChildHistoryDigest,
    #[error("native child history digest drifted after it was pinned")]
    ChildHistoryDigestDrift,
    #[error("product graph commit does not match the preallocated child")]
    ProductGraphIdentityMismatch,
    #[error("product graph can only be committed after runtime provisioning")]
    ProductGraphOutOfOrder,
    #[error("saga can only succeed after runtime activation")]
    SuccessOutOfOrder,
    #[error("a clean fork failure can only terminalize before product graph commit")]
    FailureOutOfOrder,
    #[error("a known native child cannot be terminalized as a clean failure")]
    KnownChildCannotFail,
    #[error("known child loss requires a pinned Runtime/Agent child identity")]
    MissingKnownChild,
    #[error("Runtime activation requires matching applied initial context evidence")]
    InitialContextEvidenceRequired,
}

impl AgentRunForkSaga {
    pub fn requested(
        request_id: AgentRunForkRequestId,
        parent: AgentRunForkParent,
        child: PreallocatedAgentRunChild,
    ) -> Self {
        Self::requested_with_initial_context(request_id, parent, child, None)
    }

    pub fn requested_with_initial_context(
        request_id: AgentRunForkRequestId,
        parent: AgentRunForkParent,
        child: PreallocatedAgentRunChild,
        required_initial_context: Option<RequiredInitialContextEvidence>,
    ) -> Self {
        Self {
            request_id,
            parent,
            child,
            phase: AgentRunForkSagaPhase::Requested,
            version: 0,
            durable_runtime_dispatch: None,
            runtime_child: None,
            host_binding: None,
            child_history_digest: None,
            required_initial_context,
            initial_context_evidence: None,
            receipts: AgentRunForkSagaReceipts::default(),
            graph_commit: None,
            failed: None,
            lost: None,
        }
    }

    pub fn next_step(&self) -> AgentRunForkSagaStep {
        if self.failed.is_some()
            || self.lost.is_some()
            || self.phase == AgentRunForkSagaPhase::Succeeded
        {
            return AgentRunForkSagaStep::Terminal;
        }
        if let Some(dispatch) = &self.durable_runtime_dispatch {
            return AgentRunForkSagaStep::InspectRuntime(dispatch.identity.clone());
        }
        match self.phase {
            AgentRunForkSagaPhase::Requested => AgentRunForkSagaStep::DispatchRuntime(
                self.operation_identity(AgentRunForkRuntimeOperation::Admit),
            ),
            AgentRunForkSagaPhase::RuntimeAdmitted => AgentRunForkSagaStep::DispatchRuntime(
                self.operation_identity(AgentRunForkRuntimeOperation::ApplyAgentFork),
            ),
            AgentRunForkSagaPhase::AgentForkApplied => AgentRunForkSagaStep::DispatchRuntime(
                self.operation_identity(AgentRunForkRuntimeOperation::Provision),
            ),
            AgentRunForkSagaPhase::RuntimeProvisioned => AgentRunForkSagaStep::CommitProductGraph,
            AgentRunForkSagaPhase::ProductGraphCommitted => AgentRunForkSagaStep::DispatchRuntime(
                self.operation_identity(AgentRunForkRuntimeOperation::Activate),
            ),
            AgentRunForkSagaPhase::RuntimeActivated => AgentRunForkSagaStep::MarkSucceeded,
            AgentRunForkSagaPhase::Succeeded => AgentRunForkSagaStep::Terminal,
        }
    }

    pub fn request_id(&self) -> &AgentRunForkRequestId {
        &self.request_id
    }

    pub fn parent(&self) -> &AgentRunForkParent {
        &self.parent
    }

    pub fn child(&self) -> &PreallocatedAgentRunChild {
        &self.child
    }

    pub fn phase(&self) -> AgentRunForkSagaPhase {
        self.phase
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn runtime_child(&self) -> Option<&RuntimeAgentChildIdentity> {
        self.runtime_child.as_ref()
    }

    pub fn host_binding(&self) -> Option<&str> {
        self.host_binding.as_deref()
    }

    pub fn child_history_digest(&self) -> Option<&str> {
        self.child_history_digest.as_deref()
    }

    pub fn durable_runtime_dispatch(&self) -> Option<&DurableRuntimeDispatch> {
        self.durable_runtime_dispatch.as_ref()
    }

    pub fn initial_context_evidence(&self) -> Option<&InitialContextApplicationEvidence> {
        self.initial_context_evidence.as_ref()
    }

    pub fn receipts(&self) -> &AgentRunForkSagaReceipts {
        &self.receipts
    }

    pub fn graph_commit(&self) -> Option<&ProductGraphCommitEvidence> {
        self.graph_commit.as_ref()
    }

    pub fn failure(&self) -> Option<&AgentRunForkFailure> {
        self.failed.as_ref()
    }

    pub fn lost(&self) -> Option<&LostRuntimeOperation> {
        self.lost.as_ref()
    }

    pub fn mark_runtime_dispatched(
        &mut self,
        identity: AgentRunForkOperationIdentity,
    ) -> Result<(), AgentRunForkSagaError> {
        let Some(operation) = self.runtime_operation_for_phase() else {
            return Err(AgentRunForkSagaError::InvalidOperation {
                phase: self.phase,
                actual: identity.operation,
            });
        };
        if self.durable_runtime_dispatch.is_some() || self.operation_identity(operation) != identity
        {
            return Err(AgentRunForkSagaError::DispatchIdentityMismatch);
        }
        self.durable_runtime_dispatch = Some(DurableRuntimeDispatch { identity });
        Ok(())
    }

    pub fn record_runtime_outcome(
        &mut self,
        identity: AgentRunForkOperationIdentity,
        outcome: RuntimeOperationOutcome,
    ) -> Result<(), AgentRunForkSagaError> {
        if self.failed.is_some()
            || self.lost.is_some()
            || self.phase == AgentRunForkSagaPhase::Succeeded
        {
            return Err(AgentRunForkSagaError::Terminal);
        }
        let Some(expected_operation) = self.runtime_operation_for_phase() else {
            return Err(AgentRunForkSagaError::InvalidOperation {
                phase: self.phase,
                actual: identity.operation,
            });
        };
        if identity.operation != expected_operation {
            return Err(AgentRunForkSagaError::InvalidOperation {
                phase: self.phase,
                actual: identity.operation,
            });
        }
        let expected = self
            .durable_runtime_dispatch
            .as_ref()
            .map(|dispatch| dispatch.identity.clone())
            .unwrap_or_else(|| self.expected_operation_identity());
        if expected != identity {
            return Err(AgentRunForkSagaError::OperationIdentityMismatch);
        }
        match outcome {
            RuntimeOperationOutcome::Unknown => {}
            RuntimeOperationOutcome::Lost {
                known_child,
                reason,
            } => {
                if let Some(child) = known_child {
                    self.pin_runtime_child(&child)?;
                }
                self.lost = Some(LostRuntimeOperation {
                    identity,
                    known_child: self.runtime_child.clone(),
                    known_host_binding: self.host_binding.clone(),
                    reason,
                });
                self.durable_runtime_dispatch = None;
            }
            RuntimeOperationOutcome::Applied(evidence) => {
                if identity.operation == AgentRunForkRuntimeOperation::Activate {
                    self.ensure_initial_context_evidence(evidence.context.as_ref())?;
                }
                if identity.operation == AgentRunForkRuntimeOperation::ApplyAgentFork
                    && evidence.child_history_digest.is_none()
                {
                    return Err(AgentRunForkSagaError::MissingChildHistoryDigest);
                }
                if let Some(child) = &evidence.child {
                    self.pin_runtime_child(child)?;
                }
                if let Some(binding) = &evidence.host_binding {
                    self.pin_host_binding(binding)?;
                }
                if let Some(digest) = &evidence.child_history_digest {
                    self.pin_child_history_digest(digest)?;
                }
                if let Some(context) = evidence.context {
                    self.initial_context_evidence = Some(context);
                }
                match identity.operation {
                    AgentRunForkRuntimeOperation::Admit => {
                        self.receipts.runtime_admission = Some(evidence.receipt);
                    }
                    AgentRunForkRuntimeOperation::ApplyAgentFork => {
                        self.receipts.agent_fork = Some(evidence.receipt);
                    }
                    AgentRunForkRuntimeOperation::Provision => {
                        self.receipts.runtime_provisioning = Some(evidence.receipt);
                    }
                    AgentRunForkRuntimeOperation::Activate => {
                        self.receipts.runtime_activation = Some(evidence.receipt);
                    }
                }
                self.phase = match identity.operation {
                    AgentRunForkRuntimeOperation::Admit => AgentRunForkSagaPhase::RuntimeAdmitted,
                    AgentRunForkRuntimeOperation::ApplyAgentFork => {
                        AgentRunForkSagaPhase::AgentForkApplied
                    }
                    AgentRunForkRuntimeOperation::Provision => {
                        AgentRunForkSagaPhase::RuntimeProvisioned
                    }
                    AgentRunForkRuntimeOperation::Activate => {
                        AgentRunForkSagaPhase::RuntimeActivated
                    }
                };
                self.durable_runtime_dispatch = None;
            }
        }
        Ok(())
    }

    pub fn record_product_graph_commit(
        &mut self,
        evidence: ProductGraphCommitEvidence,
    ) -> Result<(), AgentRunForkSagaError> {
        if self.phase != AgentRunForkSagaPhase::RuntimeProvisioned {
            return Err(AgentRunForkSagaError::ProductGraphOutOfOrder);
        }
        if evidence.agent_run_id != self.child.agent_run_id
            || evidence.child_run_id != self.child.run_id
            || evidence.child_agent_id != self.child.agent_id
            || evidence.child_frame_id != self.child.frame_id
            || evidence.presentation_thread_id != self.child.presentation_thread_id
            || Some(&evidence.runtime_child) != self.runtime_child.as_ref()
            || Some(evidence.host_binding.as_str()) != self.host_binding.as_deref()
            || Some(evidence.child_history_digest.as_str()) != self.child_history_digest.as_deref()
        {
            return Err(AgentRunForkSagaError::ProductGraphIdentityMismatch);
        }
        self.graph_commit = Some(evidence);
        self.phase = AgentRunForkSagaPhase::ProductGraphCommitted;
        Ok(())
    }

    pub fn mark_succeeded(&mut self) -> Result<(), AgentRunForkSagaError> {
        if self.phase != AgentRunForkSagaPhase::RuntimeActivated {
            return Err(AgentRunForkSagaError::SuccessOutOfOrder);
        }
        self.phase = AgentRunForkSagaPhase::Succeeded;
        Ok(())
    }

    pub fn mark_failed(&mut self, reason: String) -> Result<(), AgentRunForkSagaError> {
        if self.runtime_child.is_some() || self.child_history_digest.is_some() {
            return Err(AgentRunForkSagaError::KnownChildCannotFail);
        }
        if !matches!(
            self.phase,
            AgentRunForkSagaPhase::Requested
                | AgentRunForkSagaPhase::RuntimeAdmitted
                | AgentRunForkSagaPhase::AgentForkApplied
                | AgentRunForkSagaPhase::RuntimeProvisioned
        ) {
            return Err(AgentRunForkSagaError::FailureOutOfOrder);
        }
        self.failed = Some(AgentRunForkFailure {
            phase: self.phase,
            reason,
        });
        self.durable_runtime_dispatch = None;
        Ok(())
    }

    pub fn mark_known_child_lost(&mut self, reason: String) -> Result<(), AgentRunForkSagaError> {
        let Some(known_child) = self.runtime_child.clone() else {
            return Err(AgentRunForkSagaError::MissingKnownChild);
        };
        let identity = self
            .durable_runtime_dispatch
            .as_ref()
            .map(|dispatch| dispatch.identity.clone())
            .unwrap_or_else(|| self.operation_identity(AgentRunForkRuntimeOperation::Provision));
        self.lost = Some(LostRuntimeOperation {
            identity,
            known_child: Some(known_child),
            known_host_binding: self.host_binding.clone(),
            reason,
        });
        self.durable_runtime_dispatch = None;
        Ok(())
    }

    pub fn permits_new_fork(&self) -> bool {
        false
    }

    fn operation_identity(
        &self,
        operation: AgentRunForkRuntimeOperation,
    ) -> AgentRunForkOperationIdentity {
        AgentRunForkOperationIdentity {
            request_id: self.request_id.clone(),
            operation,
            child_run_id: self.child.run_id,
        }
    }

    fn expected_operation_identity(&self) -> AgentRunForkOperationIdentity {
        let operation = self
            .runtime_operation_for_phase()
            .expect("runtime outcome is only accepted during a Runtime phase");
        self.operation_identity(operation)
    }

    fn runtime_operation_for_phase(&self) -> Option<AgentRunForkRuntimeOperation> {
        match self.phase {
            AgentRunForkSagaPhase::Requested => Some(AgentRunForkRuntimeOperation::Admit),
            AgentRunForkSagaPhase::RuntimeAdmitted => {
                Some(AgentRunForkRuntimeOperation::ApplyAgentFork)
            }
            AgentRunForkSagaPhase::AgentForkApplied => {
                Some(AgentRunForkRuntimeOperation::Provision)
            }
            AgentRunForkSagaPhase::ProductGraphCommitted => {
                Some(AgentRunForkRuntimeOperation::Activate)
            }
            AgentRunForkSagaPhase::RuntimeProvisioned
            | AgentRunForkSagaPhase::RuntimeActivated
            | AgentRunForkSagaPhase::Succeeded => None,
        }
    }

    fn pin_runtime_child(
        &mut self,
        child: &RuntimeAgentChildIdentity,
    ) -> Result<(), AgentRunForkSagaError> {
        if self
            .runtime_child
            .as_ref()
            .is_some_and(|current| current != child)
        {
            return Err(AgentRunForkSagaError::RuntimeChildIdentityDrift);
        }
        self.runtime_child = Some(child.clone());
        Ok(())
    }

    fn pin_host_binding(&mut self, binding: &str) -> Result<(), AgentRunForkSagaError> {
        if self
            .host_binding
            .as_deref()
            .is_some_and(|current| current != binding)
        {
            return Err(AgentRunForkSagaError::HostBindingIdentityDrift);
        }
        self.host_binding = Some(binding.to_owned());
        Ok(())
    }

    fn pin_child_history_digest(&mut self, digest: &str) -> Result<(), AgentRunForkSagaError> {
        if self
            .child_history_digest
            .as_deref()
            .is_some_and(|current| current != digest)
        {
            return Err(AgentRunForkSagaError::ChildHistoryDigestDrift);
        }
        self.child_history_digest = Some(digest.to_owned());
        Ok(())
    }

    fn ensure_initial_context_evidence(
        &self,
        current: Option<&InitialContextApplicationEvidence>,
    ) -> Result<(), AgentRunForkSagaError> {
        let Some(required) = &self.required_initial_context else {
            return Ok(());
        };
        let applied = current.or(self.initial_context_evidence.as_ref());
        if applied.is_some_and(|applied| {
            applied.package_id == required.package_id
                && applied.package_digest == required.package_digest
                && applied.fidelity != InitialContextDeliveryFidelity::Unsupported
        }) {
            Ok(())
        } else {
            Err(AgentRunForkSagaError::InitialContextEvidenceRequired)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunForkSagaRepositoryError {
    #[error("fork request already exists")]
    AlreadyExists,
    #[error("fork saga was not found")]
    NotFound,
    #[error("fork saga revision conflict: expected {expected}, actual {actual}")]
    Conflict { expected: u64, actual: u64 },
}

#[async_trait]
pub trait AgentRunForkSagaRepository: Send + Sync {
    async fn create(
        &self,
        saga: AgentRunForkSaga,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError>;

    async fn load(
        &self,
        request_id: &AgentRunForkRequestId,
    ) -> Result<Option<AgentRunForkSaga>, AgentRunForkSagaRepositoryError>;

    async fn save(
        &self,
        expected_version: u64,
        saga: AgentRunForkSaga,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError>;
}

#[derive(Default)]
pub struct RecordingAgentRunForkSagaRepository {
    sagas: Arc<Mutex<HashMap<AgentRunForkRequestId, AgentRunForkSaga>>>,
}

#[async_trait]
impl AgentRunForkSagaRepository for RecordingAgentRunForkSagaRepository {
    async fn create(
        &self,
        mut saga: AgentRunForkSaga,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError> {
        let mut sagas = self.sagas.lock().await;
        if sagas.contains_key(&saga.request_id) {
            return Err(AgentRunForkSagaRepositoryError::AlreadyExists);
        }
        saga.version = 1;
        sagas.insert(saga.request_id.clone(), saga.clone());
        Ok(saga)
    }

    async fn load(
        &self,
        request_id: &AgentRunForkRequestId,
    ) -> Result<Option<AgentRunForkSaga>, AgentRunForkSagaRepositoryError> {
        Ok(self.sagas.lock().await.get(request_id).cloned())
    }

    async fn save(
        &self,
        expected_version: u64,
        mut saga: AgentRunForkSaga,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError> {
        let mut sagas = self.sagas.lock().await;
        let current = sagas
            .get(&saga.request_id)
            .ok_or(AgentRunForkSagaRepositoryError::NotFound)?;
        if current.version != expected_version {
            return Err(AgentRunForkSagaRepositoryError::Conflict {
                expected: expected_version,
                actual: current.version,
            });
        }
        saga.version = expected_version + 1;
        sagas.insert(saga.request_id.clone(), saga.clone());
        Ok(saga)
    }
}

#[async_trait]
pub trait AgentRunForkRuntimePort: Send + Sync {
    async fn execute(
        &self,
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
    ) -> Result<RuntimeOperationOutcome, String>;

    async fn inspect(
        &self,
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
    ) -> Result<RuntimeOperationOutcome, String>;
}

#[async_trait]
pub trait AgentRunForkProductGraphPort: Send + Sync {
    async fn commit_child_graph(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<ProductGraphCommitEvidence, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunForkSagaWorkerError {
    #[error(transparent)]
    Repository(#[from] AgentRunForkSagaRepositoryError),
    #[error(transparent)]
    Saga(#[from] AgentRunForkSagaError),
    #[error("Runtime fork operation failed: {0}")]
    Runtime(String),
    #[error("product graph commit failed: {0}")]
    ProductGraph(String),
}

/// Advances exactly one durable step. Every side effect is followed by a CAS
/// commit, so a new worker can continue from the persisted phase after restart.
pub struct AgentRunForkSagaWorker<'a> {
    repository: &'a dyn AgentRunForkSagaRepository,
    runtime: &'a dyn AgentRunForkRuntimePort,
    product_graph: &'a dyn AgentRunForkProductGraphPort,
}

impl<'a> AgentRunForkSagaWorker<'a> {
    pub fn new(
        repository: &'a dyn AgentRunForkSagaRepository,
        runtime: &'a dyn AgentRunForkRuntimePort,
        product_graph: &'a dyn AgentRunForkProductGraphPort,
    ) -> Self {
        Self {
            repository,
            runtime,
            product_graph,
        }
    }

    pub async fn advance(
        &self,
        request_id: &AgentRunForkRequestId,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaWorkerError> {
        let mut saga = self
            .repository
            .load(request_id)
            .await?
            .ok_or(AgentRunForkSagaRepositoryError::NotFound)?;
        match saga.next_step() {
            AgentRunForkSagaStep::DispatchRuntime(identity) => {
                let expected_version = saga.version;
                saga.mark_runtime_dispatched(identity.clone())?;
                let mut dispatched = self.repository.save(expected_version, saga).await?;
                let outcome = self
                    .runtime
                    .execute(&dispatched, &identity)
                    .await
                    .map_err(AgentRunForkSagaWorkerError::Runtime)?;
                let dispatched_version = dispatched.version;
                dispatched.record_runtime_outcome(identity, outcome)?;
                return Ok(self.repository.save(dispatched_version, dispatched).await?);
            }
            AgentRunForkSagaStep::InspectRuntime(identity) => {
                let outcome = self
                    .runtime
                    .inspect(&saga, &identity)
                    .await
                    .map_err(AgentRunForkSagaWorkerError::Runtime)?;
                saga.record_runtime_outcome(identity, outcome)?;
            }
            AgentRunForkSagaStep::CommitProductGraph => {
                let evidence = match self.product_graph.commit_child_graph(&saga).await {
                    Ok(evidence) => evidence,
                    Err(reason) if saga.runtime_child.is_some() => {
                        let expected_version = saga.version;
                        saga.mark_known_child_lost(reason.clone())?;
                        self.repository.save(expected_version, saga).await?;
                        return Err(AgentRunForkSagaWorkerError::ProductGraph(reason));
                    }
                    Err(reason) => {
                        return Err(AgentRunForkSagaWorkerError::ProductGraph(reason));
                    }
                };
                saga.record_product_graph_commit(evidence)?;
            }
            AgentRunForkSagaStep::MarkSucceeded => saga.mark_succeeded()?,
            AgentRunForkSagaStep::Terminal => return Ok(saga),
        }
        let expected_version = saga.version;
        Ok(self.repository.save(expected_version, saga).await?)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn saga() -> AgentRunForkSaga {
        AgentRunForkSaga::requested(
            AgentRunForkRequestId(Uuid::new_v4()),
            AgentRunForkParent {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                source_coordinate: "parent-source".to_owned(),
                through_turn_id: "turn-7".to_owned(),
            },
            PreallocatedAgentRunChild {
                agent_run_id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                frame_id: Uuid::new_v4(),
                presentation_thread_id: "thread-child".to_owned(),
            },
        )
    }

    fn evidence(saga: &AgentRunForkSaga, receipt: &str) -> RuntimeForkPhaseEvidence {
        let child = (saga.phase != AgentRunForkSagaPhase::Requested).then(|| {
            saga.runtime_child
                .clone()
                .unwrap_or(RuntimeAgentChildIdentity {
                    source_coordinate: "child-source".to_owned(),
                    runtime_agent_id: "runtime-child".to_owned(),
                })
        });
        RuntimeForkPhaseEvidence {
            child,
            host_binding: matches!(
                saga.phase,
                AgentRunForkSagaPhase::AgentForkApplied
                    | AgentRunForkSagaPhase::RuntimeProvisioned
                    | AgentRunForkSagaPhase::ProductGraphCommitted
            )
            .then(|| "host-binding-child".to_owned()),
            child_history_digest: (saga.phase != AgentRunForkSagaPhase::Requested)
                .then(|| "sha256:child-history".to_owned()),
            context: None,
            receipt: receipt.to_owned(),
        }
    }

    fn graph_evidence(saga: &AgentRunForkSaga) -> ProductGraphCommitEvidence {
        ProductGraphCommitEvidence {
            agent_run_id: saga.child.agent_run_id,
            child_run_id: saga.child.run_id,
            child_agent_id: saga.child.agent_id,
            child_frame_id: saga.child.frame_id,
            presentation_thread_id: saga.child.presentation_thread_id.clone(),
            runtime_child: saga.runtime_child.clone().expect("runtime child"),
            host_binding: saga.host_binding.clone().expect("host binding"),
            child_history_digest: saga.child_history_digest.clone().expect("history digest"),
            commit_revision: 1,
        }
    }

    fn apply_runtime(saga: &mut AgentRunForkSaga) {
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("runtime step");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch marker");
        let ev = evidence(saga, "receipt");
        saga.record_runtime_outcome(identity, RuntimeOperationOutcome::Applied(ev))
            .expect("advance");
    }

    #[test]
    fn restart_at_every_phase_preserves_the_next_exact_step() {
        let mut current = saga();
        let mut observed = Vec::new();
        loop {
            let encoded = serde_json::to_vec(&current).expect("serialize");
            let mut restarted: AgentRunForkSaga =
                serde_json::from_slice(&encoded).expect("deserialize");
            observed.push(restarted.phase);
            match restarted.next_step() {
                AgentRunForkSagaStep::DispatchRuntime(identity) => {
                    restarted
                        .mark_runtime_dispatched(identity)
                        .expect("dispatch marker");
                }
                AgentRunForkSagaStep::InspectRuntime(identity) => {
                    let ev = evidence(&restarted, "runtime");
                    restarted
                        .record_runtime_outcome(identity, RuntimeOperationOutcome::Applied(ev))
                        .expect("runtime");
                }
                AgentRunForkSagaStep::CommitProductGraph => restarted
                    .record_product_graph_commit(graph_evidence(&restarted))
                    .expect("graph"),
                AgentRunForkSagaStep::MarkSucceeded => restarted.mark_succeeded().expect("succeed"),
                AgentRunForkSagaStep::Terminal => break,
            }
            current = restarted;
        }
        observed.dedup();
        assert_eq!(
            observed,
            vec![
                AgentRunForkSagaPhase::Requested,
                AgentRunForkSagaPhase::RuntimeAdmitted,
                AgentRunForkSagaPhase::AgentForkApplied,
                AgentRunForkSagaPhase::RuntimeProvisioned,
                AgentRunForkSagaPhase::ProductGraphCommitted,
                AgentRunForkSagaPhase::RuntimeActivated,
                AgentRunForkSagaPhase::Succeeded,
            ]
        );
    }

    #[test]
    fn unknown_result_only_allows_inspection_of_the_same_identity() {
        let mut saga = saga();
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("execute");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch marker");
        saga.record_runtime_outcome(identity.clone(), RuntimeOperationOutcome::Unknown)
            .expect("unknown");
        assert_eq!(
            saga.next_step(),
            AgentRunForkSagaStep::InspectRuntime(identity.clone())
        );

        let mut wrong = identity.clone();
        wrong.child_run_id = Uuid::new_v4();
        assert_eq!(
            saga.record_runtime_outcome(wrong, RuntimeOperationOutcome::Unknown),
            Err(AgentRunForkSagaError::OperationIdentityMismatch)
        );
    }

    #[test]
    fn lost_result_retains_known_child_and_forbids_second_fork() {
        let mut saga = saga();
        apply_runtime(&mut saga);
        apply_runtime(&mut saga);
        apply_runtime(&mut saga);
        saga.record_product_graph_commit(graph_evidence(&saga))
            .expect("graph");
        let known_child = saga.runtime_child.clone().expect("known child");
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("execute");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch marker");
        saga.record_runtime_outcome(
            identity,
            RuntimeOperationOutcome::Lost {
                known_child: None,
                reason: "inspection horizon expired".to_owned(),
            },
        )
        .expect("lost");

        assert_eq!(
            saga.lost.as_ref().and_then(|lost| lost.known_child.clone()),
            Some(known_child)
        );
        assert_eq!(
            saga.lost
                .as_ref()
                .and_then(|lost| lost.known_host_binding.as_deref()),
            Some("host-binding-child")
        );
        assert_eq!(saga.next_step(), AgentRunForkSagaStep::Terminal);
        assert!(!saga.permits_new_fork());
    }

    #[test]
    fn runtime_activation_cannot_bypass_graph_or_required_context_evidence() {
        let mut saga = saga();
        let activate = saga.operation_identity(AgentRunForkRuntimeOperation::Activate);
        let ev = evidence(&saga, "activate");
        assert!(matches!(
            saga.record_runtime_outcome(activate, RuntimeOperationOutcome::Applied(ev)),
            Err(AgentRunForkSagaError::InvalidOperation { .. })
        ));

        apply_runtime(&mut saga);
        apply_runtime(&mut saga);
        apply_runtime(&mut saga);
        saga.record_product_graph_commit(graph_evidence(&saga))
            .expect("graph");
        let package_id = Uuid::new_v4();
        saga.required_initial_context = Some(RequiredInitialContextEvidence {
            package_id,
            package_digest: "sha256:package".to_owned(),
        });
        let AgentRunForkSagaStep::DispatchRuntime(activate) = saga.next_step() else {
            panic!("activate");
        };
        saga.mark_runtime_dispatched(activate.clone())
            .expect("dispatch marker");
        let ev = evidence(&saga, "activate");
        assert_eq!(
            saga.record_runtime_outcome(activate, RuntimeOperationOutcome::Applied(ev)),
            Err(AgentRunForkSagaError::InitialContextEvidenceRequired)
        );
    }

    #[tokio::test]
    async fn repository_claim_is_unique_and_updates_are_compare_and_swap() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let created = repository.create(saga()).await.expect("create");
        assert_eq!(
            repository.create(created.clone()).await,
            Err(AgentRunForkSagaRepositoryError::AlreadyExists)
        );
        let mut updated = created.clone();
        apply_runtime(&mut updated);
        let saved = repository
            .save(created.version, updated)
            .await
            .expect("save");
        assert_eq!(saved.version, created.version + 1);
        assert!(matches!(
            repository.save(created.version, saved).await,
            Err(AgentRunForkSagaRepositoryError::Conflict { .. })
        ));
    }

    #[test]
    fn clean_failure_is_terminal_without_delete_compensation() {
        let mut saga = saga();
        apply_runtime(&mut saga);
        saga.mark_failed("native fork rejected".to_owned())
            .expect("clean failure");

        assert_eq!(saga.next_step(), AgentRunForkSagaStep::Terminal);
        assert_eq!(
            saga.failed.as_ref().map(|failure| failure.phase),
            Some(AgentRunForkSagaPhase::RuntimeAdmitted)
        );
        assert!(saga.graph_commit.is_none());
    }

    #[derive(Default)]
    struct CompleteAgentTargetFixture {
        effects: Mutex<HashMap<AgentRunForkOperationIdentity, RuntimeOperationOutcome>>,
        lose_response_once: Mutex<HashSet<AgentRunForkRuntimeOperation>>,
        execute_calls: Mutex<Vec<AgentRunForkOperationIdentity>>,
        inspect_calls: Mutex<Vec<AgentRunForkOperationIdentity>>,
    }

    impl CompleteAgentTargetFixture {
        fn losing_responses(
            operations: impl IntoIterator<Item = AgentRunForkRuntimeOperation>,
        ) -> Self {
            Self {
                lose_response_once: Mutex::new(operations.into_iter().collect()),
                ..Self::default()
            }
        }
    }

    #[async_trait]
    impl AgentRunForkRuntimePort for CompleteAgentTargetFixture {
        async fn execute(
            &self,
            saga: &AgentRunForkSaga,
            identity: &AgentRunForkOperationIdentity,
        ) -> Result<RuntimeOperationOutcome, String> {
            self.execute_calls.lock().await.push(identity.clone());
            let outcome = RuntimeOperationOutcome::Applied(evidence(saga, "complete-agent-effect"));
            self.effects
                .lock()
                .await
                .insert(identity.clone(), outcome.clone());
            if self
                .lose_response_once
                .lock()
                .await
                .remove(&identity.operation)
            {
                return Err("effect applied but response was lost".to_owned());
            }
            Ok(outcome)
        }

        async fn inspect(
            &self,
            _saga: &AgentRunForkSaga,
            identity: &AgentRunForkOperationIdentity,
        ) -> Result<RuntimeOperationOutcome, String> {
            self.inspect_calls.lock().await.push(identity.clone());
            self.effects
                .lock()
                .await
                .get(identity)
                .cloned()
                .ok_or_else(|| "stable Complete Agent effect was not found".to_owned())
        }
    }

    struct MatchingProductGraph;

    #[async_trait]
    impl AgentRunForkProductGraphPort for MatchingProductGraph {
        async fn commit_child_graph(
            &self,
            saga: &AgentRunForkSaga,
        ) -> Result<ProductGraphCommitEvidence, String> {
            Ok(graph_evidence(saga))
        }
    }

    #[tokio::test]
    async fn a_new_worker_can_resume_each_persisted_step_to_success() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let created = repository.create(saga()).await.expect("create");
        let runtime = CompleteAgentTargetFixture::default();
        for _ in 0..6 {
            AgentRunForkSagaWorker::new(&repository, &runtime, &MatchingProductGraph)
                .advance(&created.request_id)
                .await
                .expect("advance");
        }
        let succeeded = repository
            .load(&created.request_id)
            .await
            .expect("load")
            .expect("saga");
        assert_eq!(succeeded.phase, AgentRunForkSagaPhase::Succeeded);
        assert_eq!(
            succeeded.child_history_digest.as_deref(),
            Some("sha256:child-history")
        );
        assert!(succeeded.receipts.runtime_admission.is_some());
        assert!(succeeded.receipts.agent_fork.is_some());
        assert!(succeeded.receipts.runtime_provisioning.is_some());
        assert!(succeeded.receipts.runtime_activation.is_some());
    }

    #[tokio::test]
    async fn side_effect_before_save_restart_only_inspects_the_same_identity() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let created = repository.create(saga()).await.expect("create");
        let runtime =
            CompleteAgentTargetFixture::losing_responses([AgentRunForkRuntimeOperation::Admit]);
        let worker = AgentRunForkSagaWorker::new(&repository, &runtime, &MatchingProductGraph);

        assert!(matches!(
            worker.advance(&created.request_id).await,
            Err(AgentRunForkSagaWorkerError::Runtime(_))
        ));
        let after_crash = repository
            .load(&created.request_id)
            .await
            .expect("load")
            .expect("saga");
        let dispatch = after_crash
            .durable_runtime_dispatch()
            .expect("durable dispatch")
            .identity
            .clone();
        assert_eq!(after_crash.phase(), AgentRunForkSagaPhase::Requested);

        let admitted = worker.advance(&created.request_id).await.expect("inspect");
        assert_eq!(admitted.phase(), AgentRunForkSagaPhase::RuntimeAdmitted);
        assert_eq!(
            runtime.execute_calls.lock().await.as_slice(),
            &[dispatch.clone()]
        );
        assert_eq!(runtime.inspect_calls.lock().await.as_slice(), &[dispatch]);
    }

    #[tokio::test]
    async fn every_runtime_crash_window_recovers_by_inspection() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let created = repository.create(saga()).await.expect("create");
        let runtime = CompleteAgentTargetFixture::losing_responses([
            AgentRunForkRuntimeOperation::Admit,
            AgentRunForkRuntimeOperation::ApplyAgentFork,
            AgentRunForkRuntimeOperation::Provision,
            AgentRunForkRuntimeOperation::Activate,
        ]);
        let worker = AgentRunForkSagaWorker::new(&repository, &runtime, &MatchingProductGraph);

        for _ in 0..10 {
            let _ = worker.advance(&created.request_id).await;
        }
        let succeeded = repository
            .load(&created.request_id)
            .await
            .expect("load")
            .expect("saga");
        assert_eq!(succeeded.phase(), AgentRunForkSagaPhase::Succeeded);
        assert_eq!(runtime.execute_calls.lock().await.len(), 4);
        assert_eq!(runtime.inspect_calls.lock().await.len(), 4);
    }

    #[test]
    fn pinned_runtime_child_binding_and_history_reject_drift() {
        let mut saga = saga();
        apply_runtime(&mut saga);
        apply_runtime(&mut saga);
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("provision");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch");
        let mut drifted = evidence(&saga, "provision");
        drifted.child = Some(RuntimeAgentChildIdentity {
            source_coordinate: "different-child".to_owned(),
            runtime_agent_id: "runtime-child".to_owned(),
        });
        assert_eq!(
            saga.record_runtime_outcome(identity, RuntimeOperationOutcome::Applied(drifted)),
            Err(AgentRunForkSagaError::RuntimeChildIdentityDrift)
        );
    }

    #[test]
    fn product_graph_receipt_covers_every_preallocated_and_runtime_identity() {
        let mut base = saga();
        apply_runtime(&mut base);
        apply_runtime(&mut base);
        apply_runtime(&mut base);
        let expected = graph_evidence(&base);
        let mut mismatches = Vec::new();
        let mut evidence = expected.clone();
        evidence.agent_run_id = Uuid::new_v4();
        mismatches.push(evidence);
        let mut evidence = expected.clone();
        evidence.child_run_id = Uuid::new_v4();
        mismatches.push(evidence);
        let mut evidence = expected.clone();
        evidence.child_agent_id = Uuid::new_v4();
        mismatches.push(evidence);
        let mut evidence = expected.clone();
        evidence.child_frame_id = Uuid::new_v4();
        mismatches.push(evidence);
        let mut evidence = expected.clone();
        evidence.presentation_thread_id = "different-thread".to_owned();
        mismatches.push(evidence);
        let mut evidence = expected.clone();
        evidence.runtime_child.source_coordinate = "different-child".to_owned();
        mismatches.push(evidence);
        let mut evidence = expected.clone();
        evidence.host_binding = "different-binding".to_owned();
        mismatches.push(evidence);
        let mut evidence = expected;
        evidence.child_history_digest = "sha256:different-history".to_owned();
        mismatches.push(evidence);

        for evidence in mismatches {
            let mut saga = base.clone();
            assert_eq!(
                saga.record_product_graph_commit(evidence),
                Err(AgentRunForkSagaError::ProductGraphIdentityMismatch)
            );
        }
    }

    struct FailingProductGraph;

    #[async_trait]
    impl AgentRunForkProductGraphPort for FailingProductGraph {
        async fn commit_child_graph(
            &self,
            _saga: &AgentRunForkSaga,
        ) -> Result<ProductGraphCommitEvidence, String> {
            Err("Runtime child mapping could not be committed".to_owned())
        }
    }

    #[tokio::test]
    async fn known_native_child_mapping_failure_is_lost_not_failed() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let created = repository.create(saga()).await.expect("create");
        let runtime = CompleteAgentTargetFixture::default();
        let worker = AgentRunForkSagaWorker::new(&repository, &runtime, &FailingProductGraph);
        for _ in 0..3 {
            worker
                .advance(&created.request_id)
                .await
                .expect("runtime phase");
        }
        assert!(matches!(
            worker.advance(&created.request_id).await,
            Err(AgentRunForkSagaWorkerError::ProductGraph(_))
        ));
        let mut lost = repository
            .load(&created.request_id)
            .await
            .expect("load")
            .expect("saga");
        assert!(lost.lost().is_some());
        assert!(lost.failure().is_none());
        assert_eq!(
            lost.mark_failed("clean failure".to_owned()),
            Err(AgentRunForkSagaError::KnownChildCannotFail)
        );
    }
}
