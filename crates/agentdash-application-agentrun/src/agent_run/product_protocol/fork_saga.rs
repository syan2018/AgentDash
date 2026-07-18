#[cfg(test)]
use std::{collections::HashMap, sync::Arc};

use agentdash_agent_runtime_contract::{
    RuntimeOperationId, RuntimePayloadDigest, RuntimeProjectionRevision, RuntimeThreadId,
};
use agentdash_application_ports::agent_run_fork::AgentRunForkGraph;
#[cfg(test)]
use agentdash_domain::workflow::AgentSource;
use agentdash_domain::workflow::{AgentFrame, AgentRunLineage, LifecycleAgent, LifecycleRun};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
#[cfg(test)]
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentRunForkRequestId(pub Uuid);

/// Product coordinates for the exact source Runtime fork.
///
/// The Gateway route is a typed Runtime thread coordinate. An opaque Complete-Agent source
/// coordinate cannot be substituted:
///
/// ```compile_fail
/// use agentdash_application_agentrun::agent_run::product_protocol::AgentRunForkParent;
/// use uuid::Uuid;
///
/// let opaque_agent_source = String::from("agent-source");
/// let _ = AgentRunForkParent {
///     run_id: Uuid::nil(),
///     agent_id: Uuid::nil(),
///     runtime_thread_id: opaque_agent_source,
///     through_turn_id: "turn-1".to_owned(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunForkParent {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_thread_id: RuntimeThreadId,
    pub through_turn_id: String,
}

/// Stable Product and Runtime coordinates allocated before dispatch.
///
/// ```compile_fail
/// use agentdash_application_agentrun::agent_run::product_protocol::PreallocatedAgentRunChild;
/// use uuid::Uuid;
///
/// let _ = PreallocatedAgentRunChild {
///     agent_run_id: Uuid::nil(),
///     run_id: Uuid::nil(),
///     agent_id: Uuid::nil(),
///     frame_id: Uuid::nil(),
///     runtime_thread_id: String::from("runtime-thread"),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreallocatedAgentRunChild {
    pub agent_run_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub runtime_thread_id: RuntimeThreadId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunForkSagaPhase {
    Requested,
    RuntimeAdmitted,
    RuntimeProvisioned,
    ProductGraphCommitted,
    RuntimeActivated,
    Succeeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunForkRuntimeOperation {
    Fork,
    Activate,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentRunForkOperationIdentity {
    pub request_id: AgentRunForkRequestId,
    pub operation: AgentRunForkRuntimeOperation,
    pub child_run_id: Uuid,
    pub runtime_operation_id: RuntimeOperationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompiledContextDeliveryFidelity {
    Unsupported,
    CanonicalRendered,
    TypedNative,
}

impl CompiledContextDeliveryFidelity {
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
pub struct CompiledContextContributionApplication {
    pub kind: String,
    pub fidelity: CompiledContextDeliveryFidelity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledContextApplication {
    pub package_id: Uuid,
    pub package_digest: String,
    pub fidelity: CompiledContextDeliveryFidelity,
    pub contribution_fidelity: Vec<CompiledContextContributionApplication>,
    pub renderer_version: Option<String>,
    pub materialized_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredInitialContextEvidence {
    pub package_id: Uuid,
    pub package_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedRuntimeOperation {
    pub operation_id: RuntimeOperationId,
    pub accepted_revision: RuntimeProjectionRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeForkPhaseEvidence {
    ForkProvisioned {
        child_thread_id: RuntimeThreadId,
        child_history_digest: RuntimePayloadDigest,
        context: Option<CompiledContextApplication>,
        receipt: AcceptedRuntimeOperation,
    },
    Activated {
        child_thread_id: RuntimeThreadId,
        context: Option<CompiledContextApplication>,
        receipt: AcceptedRuntimeOperation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductGraphCommitEvidence {
    pub agent_run_id: Uuid,
    pub child_run_id: Uuid,
    pub child_agent_id: Uuid,
    pub child_frame_id: Uuid,
    pub runtime_thread_id: RuntimeThreadId,
    pub child_history_digest: RuntimePayloadDigest,
    pub payload_digest: String,
    pub commit_revision: u64,
}

/// Product graph 的 immutable transaction payload。
///
/// W8 PostgreSQL adapter 直接从这里写入 LifecycleRun、LifecycleAgent、AgentFrame 与
/// AgentRunLineage，并与 transitioned saga revision 在同一个 transaction 中提交。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedAgentRunForkGraph {
    request_id: AgentRunForkRequestId,
    agent_run_id: Uuid,
    child_run: LifecycleRun,
    child_agent: LifecycleAgent,
    child_frame: AgentFrame,
    lineage: AgentRunLineage,
    runtime_thread_id: RuntimeThreadId,
    child_history_digest: RuntimePayloadDigest,
    payload_digest: String,
}

impl PreparedAgentRunForkGraph {
    pub fn prepare(
        saga: &AgentRunForkSaga,
        graph: AgentRunForkGraph,
    ) -> Result<Self, AgentRunForkSagaError> {
        if saga.phase != AgentRunForkSagaPhase::RuntimeProvisioned {
            return Err(AgentRunForkSagaError::ProductGraphOutOfOrder);
        }
        let child_history_digest = saga
            .child_history_digest
            .clone()
            .ok_or(AgentRunForkSagaError::ProductGraphOutOfOrder)?;
        let mut prepared = Self {
            request_id: saga.request_id.clone(),
            agent_run_id: saga.child.agent_run_id,
            child_run: graph.child_run,
            child_agent: graph.child_agent,
            child_frame: graph.child_frame,
            lineage: graph.lineage,
            runtime_thread_id: saga.child.runtime_thread_id.clone(),
            child_history_digest,
            payload_digest: String::new(),
        };
        prepared.validate_identities(saga)?;
        prepared.payload_digest = prepared.calculate_digest();
        Ok(prepared)
    }

    pub fn validate_for_saga(&self, saga: &AgentRunForkSaga) -> Result<(), AgentRunForkSagaError> {
        self.validate_identities(saga)?;
        if self.payload_digest != self.calculate_digest() {
            return Err(AgentRunForkSagaError::PreparedGraphDigestMismatch);
        }
        Ok(())
    }

    pub fn validate_for_saga_transition(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<(), AgentRunForkSagaError> {
        self.validate_identities(saga)?;
        if saga.phase != AgentRunForkSagaPhase::ProductGraphCommitted
            || saga
                .graph_commit
                .as_ref()
                .is_none_or(|evidence| evidence.payload_digest != self.payload_digest)
            || self.payload_digest != self.calculate_digest()
        {
            return Err(AgentRunForkSagaError::PreparedGraphDigestMismatch);
        }
        Ok(())
    }

    pub fn commit_evidence(&self, commit_revision: u64) -> ProductGraphCommitEvidence {
        ProductGraphCommitEvidence {
            agent_run_id: self.agent_run_id,
            child_run_id: self.child_run.id,
            child_agent_id: self.child_agent.id,
            child_frame_id: self.child_frame.id,
            runtime_thread_id: self.runtime_thread_id.clone(),
            child_history_digest: self.child_history_digest.clone(),
            payload_digest: self.payload_digest.clone(),
            commit_revision,
        }
    }

    pub fn graph(&self) -> AgentRunForkGraph {
        AgentRunForkGraph {
            child_run: self.child_run.clone(),
            child_agent: self.child_agent.clone(),
            child_frame: self.child_frame.clone(),
            lineage: self.lineage.clone(),
        }
    }

    pub fn request_id(&self) -> &AgentRunForkRequestId {
        &self.request_id
    }

    pub fn agent_run_id(&self) -> Uuid {
        self.agent_run_id
    }

    pub fn runtime_thread_id(&self) -> &RuntimeThreadId {
        &self.runtime_thread_id
    }

    pub fn child_history_digest(&self) -> &RuntimePayloadDigest {
        &self.child_history_digest
    }

    pub fn payload_digest(&self) -> &str {
        &self.payload_digest
    }

    fn validate_identities(&self, saga: &AgentRunForkSaga) -> Result<(), AgentRunForkSagaError> {
        if !matches!(
            saga.phase,
            AgentRunForkSagaPhase::RuntimeProvisioned
                | AgentRunForkSagaPhase::ProductGraphCommitted
        ) || self.request_id != saga.request_id
            || self.agent_run_id != saga.child.agent_run_id
            || self.child_run.id != saga.child.run_id
            || self.child_agent.id != saga.child.agent_id
            || self.child_agent.run_id != saga.child.run_id
            || self.child_agent.project_id != self.child_run.project_id
            || self.child_frame.id != saga.child.frame_id
            || self.child_frame.agent_id != saga.child.agent_id
            || self.lineage.parent_run_id != saga.parent.run_id
            || self.lineage.parent_agent_id != saga.parent.agent_id
            || self.lineage.child_run_id != saga.child.run_id
            || self.lineage.child_agent_id != saga.child.agent_id
            || self.lineage.child_frame_id != Some(saga.child.frame_id)
            || self.runtime_thread_id != saga.child.runtime_thread_id
            || Some(&self.child_history_digest) != saga.child_history_digest.as_ref()
        {
            return Err(AgentRunForkSagaError::PreparedGraphIdentityMismatch);
        }
        Ok(())
    }

    fn calculate_digest(&self) -> String {
        let canonical = serde_json::to_vec(&(
            &self.request_id,
            self.agent_run_id,
            &self.child_run,
            &self.child_agent,
            &self.child_frame,
            &self.lineage,
            &self.runtime_thread_id,
            &self.child_history_digest,
        ))
        .expect("prepared AgentRun fork graph must serialize");
        format!("sha256:{:x}", Sha256::digest(canonical))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableRuntimeDispatch {
    pub identity: AgentRunForkOperationIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LostRuntimeOperation {
    pub identity: AgentRunForkOperationIdentity,
    pub known_child_thread_id: RuntimeThreadId,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentRunForkSagaReceipts {
    pub runtime_admission: Option<AcceptedRuntimeOperation>,
    pub runtime_provisioning: Option<AcceptedRuntimeOperation>,
    pub runtime_activation: Option<AcceptedRuntimeOperation>,
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
    child_history_digest: Option<RuntimePayloadDigest>,
    required_initial_context: Option<RequiredInitialContextEvidence>,
    initial_context_evidence: Option<CompiledContextApplication>,
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
    Accepted(AcceptedRuntimeOperation),
    Applied(RuntimeForkPhaseEvidence),
    Unknown,
    Lost { reason: String },
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
    #[error("Runtime child thread identity does not match the preallocated coordinate")]
    RuntimeChildIdentityDrift,
    #[error("Runtime operation receipt does not match the pending operation")]
    RuntimeReceiptIdentityMismatch,
    #[error("Runtime operation evidence does not match the pending operation kind")]
    InvalidRuntimeEvidence,
    #[error("native child history digest drifted after it was pinned")]
    ChildHistoryDigestDrift,
    #[error("product graph commit does not match the preallocated child")]
    ProductGraphIdentityMismatch,
    #[error("prepared product graph does not match the saga identities")]
    PreparedGraphIdentityMismatch,
    #[error("prepared product graph payload digest does not match its immutable rows")]
    PreparedGraphDigestMismatch,
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
            AgentRunForkSagaPhase::Requested | AgentRunForkSagaPhase::RuntimeAdmitted => {
                AgentRunForkSagaStep::DispatchRuntime(
                    self.operation_identity(AgentRunForkRuntimeOperation::Fork),
                )
            }
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

    pub fn child_history_digest(&self) -> Option<&RuntimePayloadDigest> {
        self.child_history_digest.as_ref()
    }

    pub fn durable_runtime_dispatch(&self) -> Option<&DurableRuntimeDispatch> {
        self.durable_runtime_dispatch.as_ref()
    }

    pub fn initial_context_evidence(&self) -> Option<&CompiledContextApplication> {
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
            RuntimeOperationOutcome::Lost { reason } => {
                self.lost = Some(LostRuntimeOperation {
                    identity,
                    known_child_thread_id: self.child.runtime_thread_id.clone(),
                    reason,
                });
                self.durable_runtime_dispatch = None;
            }
            RuntimeOperationOutcome::Accepted(receipt) => {
                self.ensure_receipt_identity(&identity, &receipt)?;
                match identity.operation {
                    AgentRunForkRuntimeOperation::Fork => {
                        self.receipts.runtime_admission = Some(receipt);
                        self.phase = AgentRunForkSagaPhase::RuntimeAdmitted;
                    }
                    AgentRunForkRuntimeOperation::Activate => {
                        self.receipts.runtime_activation = Some(receipt);
                    }
                }
            }
            RuntimeOperationOutcome::Applied(evidence) => {
                match (identity.operation, evidence) {
                    (
                        AgentRunForkRuntimeOperation::Fork,
                        RuntimeForkPhaseEvidence::ForkProvisioned {
                            child_thread_id,
                            child_history_digest,
                            context,
                            receipt,
                        },
                    ) => {
                        self.ensure_child_thread(&child_thread_id)?;
                        self.ensure_receipt_identity(&identity, &receipt)?;
                        self.pin_child_history_digest(&child_history_digest)?;
                        if let Some(context) = context {
                            self.initial_context_evidence = Some(context);
                        }
                        self.receipts
                            .runtime_admission
                            .get_or_insert(receipt.clone());
                        self.receipts.runtime_provisioning = Some(receipt);
                        self.phase = AgentRunForkSagaPhase::RuntimeProvisioned;
                    }
                    (
                        AgentRunForkRuntimeOperation::Activate,
                        RuntimeForkPhaseEvidence::Activated {
                            child_thread_id,
                            context,
                            receipt,
                        },
                    ) => {
                        self.ensure_child_thread(&child_thread_id)?;
                        self.ensure_receipt_identity(&identity, &receipt)?;
                        self.ensure_initial_context_evidence(context.as_ref())?;
                        if let Some(context) = context {
                            self.initial_context_evidence = Some(context);
                        }
                        self.receipts.runtime_activation = Some(receipt);
                        self.phase = AgentRunForkSagaPhase::RuntimeActivated;
                    }
                    _ => return Err(AgentRunForkSagaError::InvalidRuntimeEvidence),
                }
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
            || evidence.runtime_thread_id != self.child.runtime_thread_id
            || Some(&evidence.child_history_digest) != self.child_history_digest.as_ref()
            || evidence.payload_digest.trim().is_empty()
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
        if self.child_history_digest.is_some() {
            return Err(AgentRunForkSagaError::KnownChildCannotFail);
        }
        if !matches!(
            self.phase,
            AgentRunForkSagaPhase::Requested
                | AgentRunForkSagaPhase::RuntimeAdmitted
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
        if self.child_history_digest.is_none() {
            return Err(AgentRunForkSagaError::MissingKnownChild);
        }
        let identity = self
            .durable_runtime_dispatch
            .as_ref()
            .map(|dispatch| dispatch.identity.clone())
            .unwrap_or_else(|| self.operation_identity(AgentRunForkRuntimeOperation::Activate));
        self.lost = Some(LostRuntimeOperation {
            identity,
            known_child_thread_id: self.child.runtime_thread_id.clone(),
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
            runtime_operation_id: RuntimeOperationId::new(format!(
                "agent-run-fork:{}:{}",
                self.request_id.0,
                match operation {
                    AgentRunForkRuntimeOperation::Fork => "fork",
                    AgentRunForkRuntimeOperation::Activate => "activate",
                }
            ))
            .expect("stable Product fork operation identity is non-empty"),
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
            AgentRunForkSagaPhase::Requested | AgentRunForkSagaPhase::RuntimeAdmitted => {
                Some(AgentRunForkRuntimeOperation::Fork)
            }
            AgentRunForkSagaPhase::ProductGraphCommitted => {
                Some(AgentRunForkRuntimeOperation::Activate)
            }
            AgentRunForkSagaPhase::RuntimeProvisioned
            | AgentRunForkSagaPhase::RuntimeActivated
            | AgentRunForkSagaPhase::Succeeded => None,
        }
    }

    fn ensure_child_thread(
        &self,
        child_thread_id: &RuntimeThreadId,
    ) -> Result<(), AgentRunForkSagaError> {
        if child_thread_id != &self.child.runtime_thread_id {
            return Err(AgentRunForkSagaError::RuntimeChildIdentityDrift);
        }
        Ok(())
    }

    fn ensure_receipt_identity(
        &self,
        identity: &AgentRunForkOperationIdentity,
        receipt: &AcceptedRuntimeOperation,
    ) -> Result<(), AgentRunForkSagaError> {
        if receipt.operation_id != identity.runtime_operation_id {
            return Err(AgentRunForkSagaError::RuntimeReceiptIdentityMismatch);
        }
        Ok(())
    }

    fn pin_child_history_digest(
        &mut self,
        digest: &RuntimePayloadDigest,
    ) -> Result<(), AgentRunForkSagaError> {
        if self
            .child_history_digest
            .as_ref()
            .is_some_and(|current| current != digest)
        {
            return Err(AgentRunForkSagaError::ChildHistoryDigestDrift);
        }
        self.child_history_digest = Some(digest.clone());
        Ok(())
    }

    fn ensure_initial_context_evidence(
        &self,
        current: Option<&CompiledContextApplication>,
    ) -> Result<(), AgentRunForkSagaError> {
        let Some(required) = &self.required_initial_context else {
            return Ok(());
        };
        let applied = current.or(self.initial_context_evidence.as_ref());
        if applied.is_some_and(|applied| {
            applied.package_id == required.package_id
                && applied.package_digest == required.package_digest
                && applied.fidelity != CompiledContextDeliveryFidelity::Unsupported
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
    #[error("fork saga repository unavailable: {0}")]
    Unavailable(String),
    #[error("prepared product graph payload conflicts with the committed request")]
    GraphPayloadConflict,
    #[error("prepared product graph payload is invalid: {0}")]
    InvalidGraphPayload(String),
}

#[async_trait]
pub trait AgentRunForkSagaRepository: Send + Sync {
    /// 在一个 durable transaction 中物化请求并保留完整 child identity。
    /// 重复请求返回 `AlreadyExists`，caller 随后比较已持久化的 immutable request。
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

    /// 在一个 transaction 中提交 Product graph 与 saga phase。
    ///
    /// transaction 与 schema 由 W8 PostgreSQL adapter 持有。匹配的
    /// `ProductGraphCommitted` saga revision 可见前，graph row 不能对外可见。
    async fn commit_product_graph(
        &self,
        expected_version: u64,
        saga: AgentRunForkSaga,
        graph: PreparedAgentRunForkGraph,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError>;
}

/// Product owner 冻结的持久化 shape；W8 是唯一 migration 与 PostgreSQL adapter owner。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRunForkSagaSchemaContract {
    pub table: &'static str,
    pub request_key: &'static str,
    pub optimistic_revision: &'static str,
    pub durable_dispatch_identity: &'static str,
    pub known_child_coordinate: &'static str,
    pub graph_commit_revision: &'static str,
}

pub const AGENT_RUN_FORK_SAGA_SCHEMA_CONTRACT: AgentRunForkSagaSchemaContract =
    AgentRunForkSagaSchemaContract {
        table: "agent_run_fork_saga",
        request_key: "request_id",
        optimistic_revision: "version",
        durable_dispatch_identity: "durable_runtime_dispatch",
        known_child_coordinate: "runtime_thread_id",
        graph_commit_revision: "graph_commit_revision",
    };

#[cfg(test)]
#[derive(Default)]
struct RecordingAgentRunForkSagaState {
    sagas: HashMap<AgentRunForkRequestId, AgentRunForkSaga>,
    graphs: HashMap<AgentRunForkRequestId, PreparedAgentRunForkGraph>,
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct RecordingAgentRunForkSagaRepository {
    state: Arc<Mutex<RecordingAgentRunForkSagaState>>,
}

#[cfg(test)]
impl RecordingAgentRunForkSagaRepository {
    async fn load_graph(
        &self,
        request_id: &AgentRunForkRequestId,
    ) -> Option<PreparedAgentRunForkGraph> {
        self.state.lock().await.graphs.get(request_id).cloned()
    }
}

#[cfg(test)]
#[async_trait]
impl AgentRunForkSagaRepository for RecordingAgentRunForkSagaRepository {
    async fn create(
        &self,
        mut saga: AgentRunForkSaga,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError> {
        let mut state = self.state.lock().await;
        if state.sagas.contains_key(&saga.request_id) {
            return Err(AgentRunForkSagaRepositoryError::AlreadyExists);
        }
        saga.version = 1;
        state.sagas.insert(saga.request_id.clone(), saga.clone());
        Ok(saga)
    }

    async fn load(
        &self,
        request_id: &AgentRunForkRequestId,
    ) -> Result<Option<AgentRunForkSaga>, AgentRunForkSagaRepositoryError> {
        Ok(self.state.lock().await.sagas.get(request_id).cloned())
    }

    async fn save(
        &self,
        expected_version: u64,
        mut saga: AgentRunForkSaga,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError> {
        let mut state = self.state.lock().await;
        let current = state
            .sagas
            .get(&saga.request_id)
            .ok_or(AgentRunForkSagaRepositoryError::NotFound)?;
        if current.version != expected_version {
            return Err(AgentRunForkSagaRepositoryError::Conflict {
                expected: expected_version,
                actual: current.version,
            });
        }
        saga.version = expected_version + 1;
        state.sagas.insert(saga.request_id.clone(), saga.clone());
        Ok(saga)
    }

    async fn commit_product_graph(
        &self,
        expected_version: u64,
        mut saga: AgentRunForkSaga,
        graph: PreparedAgentRunForkGraph,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError> {
        let mut state = self.state.lock().await;
        let current = state
            .sagas
            .get(&saga.request_id)
            .ok_or(AgentRunForkSagaRepositoryError::NotFound)?;
        if let Some(committed) = state.graphs.get(&saga.request_id) {
            if committed.payload_digest != graph.payload_digest {
                return Err(AgentRunForkSagaRepositoryError::GraphPayloadConflict);
            }
            graph.validate_for_saga_transition(&saga).map_err(|error| {
                AgentRunForkSagaRepositoryError::InvalidGraphPayload(error.to_string())
            })?;
            return Ok(current.clone());
        }
        graph.validate_for_saga_transition(&saga).map_err(|error| {
            AgentRunForkSagaRepositoryError::InvalidGraphPayload(error.to_string())
        })?;
        if current.version != expected_version {
            return Err(AgentRunForkSagaRepositoryError::Conflict {
                expected: expected_version,
                actual: current.version,
            });
        }
        saga.version = expected_version + 1;
        state.graphs.insert(saga.request_id.clone(), graph);
        state.sagas.insert(saga.request_id.clone(), saga.clone());
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
    /// 构造并校验 graph commit evidence，但不发布 graph。
    /// `AgentRunForkSagaRepository` 将持久化与 saga phase transition 放进同一 transaction。
    async fn prepare_child_graph_commit(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<PreparedAgentRunForkGraph, String>;
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
                let graph = match self.product_graph.prepare_child_graph_commit(&saga).await {
                    Ok(graph) => graph,
                    Err(reason) if saga.child_history_digest.is_some() => {
                        let expected_version = saga.version;
                        saga.mark_known_child_lost(reason.clone())?;
                        self.repository.save(expected_version, saga).await?;
                        return Err(AgentRunForkSagaWorkerError::ProductGraph(reason));
                    }
                    Err(reason) => {
                        return Err(AgentRunForkSagaWorkerError::ProductGraph(reason));
                    }
                };
                graph.validate_for_saga(&saga)?;
                let evidence = graph.commit_evidence(saga.version + 1);
                let expected_version = saga.version;
                saga.record_product_graph_commit(evidence)?;
                return Ok(self
                    .repository
                    .commit_product_graph(expected_version, saga, graph)
                    .await?);
            }
            AgentRunForkSagaStep::MarkSucceeded => saga.mark_succeeded()?,
            AgentRunForkSagaStep::Terminal => return Ok(saga),
        }
        let expected_version = saga.version;
        Ok(self.repository.save(expected_version, saga).await?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializeAgentRunFork {
    pub request_id: AgentRunForkRequestId,
    pub parent: AgentRunForkParent,
    pub child: PreallocatedAgentRunChild,
    pub required_initial_context: Option<RequiredInitialContextEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentRunForkFacadeError {
    #[error(transparent)]
    Repository(#[from] AgentRunForkSagaRepositoryError),
    #[error(transparent)]
    Worker(#[from] AgentRunForkSagaWorkerError),
    #[error("existing fork request drifted from its immutable parent or child identity")]
    ExistingRequestDrift,
}

/// durable AgentRun fork 的 Product 生产入口。
///
/// constructor 必须显式注入三个最终 port，不提供 default、recording repository、
/// legacy runtime facade 或 graph-first 分支。
pub struct AgentRunForkFacade<'a> {
    repository: &'a dyn AgentRunForkSagaRepository,
    worker: AgentRunForkSagaWorker<'a>,
}

impl<'a> AgentRunForkFacade<'a> {
    pub fn new(
        repository: &'a dyn AgentRunForkSagaRepository,
        runtime: &'a dyn AgentRunForkRuntimePort,
        product_graph: &'a dyn AgentRunForkProductGraphPort,
    ) -> Self {
        Self {
            repository,
            worker: AgentRunForkSagaWorker::new(repository, runtime, product_graph),
        }
    }

    pub async fn materialize(
        &self,
        command: MaterializeAgentRunFork,
    ) -> Result<AgentRunForkSaga, AgentRunForkFacadeError> {
        let requested = AgentRunForkSaga::requested_with_initial_context(
            command.request_id.clone(),
            command.parent,
            command.child,
            command.required_initial_context,
        );
        match self.repository.create(requested.clone()).await {
            Ok(saga) => Ok(saga),
            Err(AgentRunForkSagaRepositoryError::AlreadyExists) => {
                let existing = self
                    .repository
                    .load(&command.request_id)
                    .await?
                    .ok_or(AgentRunForkSagaRepositoryError::NotFound)?;
                if existing.parent() != requested.parent()
                    || existing.child() != requested.child()
                    || existing.required_initial_context != requested.required_initial_context
                {
                    return Err(AgentRunForkFacadeError::ExistingRequestDrift);
                }
                Ok(existing)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn advance(
        &self,
        request_id: &AgentRunForkRequestId,
    ) -> Result<AgentRunForkSaga, AgentRunForkFacadeError> {
        Ok(self.worker.advance(request_id).await?)
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
                runtime_thread_id: RuntimeThreadId::new("parent-thread").expect("thread id"),
                through_turn_id: "turn-7".to_owned(),
            },
            PreallocatedAgentRunChild {
                agent_run_id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                frame_id: Uuid::new_v4(),
                runtime_thread_id: RuntimeThreadId::new("thread-child").expect("thread id"),
            },
        )
    }

    fn accepted(identity: &AgentRunForkOperationIdentity) -> AcceptedRuntimeOperation {
        AcceptedRuntimeOperation {
            operation_id: identity.runtime_operation_id.clone(),
            accepted_revision: RuntimeProjectionRevision(1),
        }
    }

    fn evidence(saga: &AgentRunForkSaga) -> RuntimeForkPhaseEvidence {
        let identity = saga
            .durable_runtime_dispatch
            .as_ref()
            .expect("durable dispatch")
            .identity
            .clone();
        match identity.operation {
            AgentRunForkRuntimeOperation::Fork => RuntimeForkPhaseEvidence::ForkProvisioned {
                child_thread_id: saga.child.runtime_thread_id.clone(),
                child_history_digest: RuntimePayloadDigest::new("sha256:child-history")
                    .expect("history digest"),
                context: None,
                receipt: accepted(&identity),
            },
            AgentRunForkRuntimeOperation::Activate => RuntimeForkPhaseEvidence::Activated {
                child_thread_id: saga.child.runtime_thread_id.clone(),
                context: None,
                receipt: accepted(&identity),
            },
        }
    }

    fn graph_evidence(saga: &AgentRunForkSaga) -> ProductGraphCommitEvidence {
        ProductGraphCommitEvidence {
            agent_run_id: saga.child.agent_run_id,
            child_run_id: saga.child.run_id,
            child_agent_id: saga.child.agent_id,
            child_frame_id: saga.child.frame_id,
            runtime_thread_id: saga.child.runtime_thread_id.clone(),
            child_history_digest: saga.child_history_digest.clone().expect("history digest"),
            payload_digest: "sha256:prepared-graph".to_owned(),
            commit_revision: 1,
        }
    }

    fn prepared_graph(saga: &AgentRunForkSaga) -> PreparedAgentRunForkGraph {
        let project_id = Uuid::new_v4();
        let mut child_run = LifecycleRun::new_plain(project_id);
        child_run.id = saga.child.run_id;
        let mut child_agent =
            LifecycleAgent::new_root(saga.child.run_id, project_id, AgentSource::Subagent);
        child_agent.id = saga.child.agent_id;
        let mut child_frame = AgentFrame::new_revision(saga.child.agent_id, 1, "agent_run_fork");
        child_frame.id = saga.child.frame_id;
        let lineage = AgentRunLineage::new_fork(
            saga.parent.run_id,
            saga.parent.agent_id,
            saga.child.run_id,
            saga.child.agent_id,
            None,
            Some(serde_json::json!({
                "through_turn_id": saga.parent.through_turn_id,
            })),
            "tester",
            None,
        )
        .with_frame_baseline(Uuid::new_v4(), 1, saga.child.frame_id, child_frame.revision);
        PreparedAgentRunForkGraph::prepare(
            saga,
            AgentRunForkGraph {
                child_run,
                child_agent,
                child_frame,
                lineage,
            },
        )
        .expect("prepared graph")
    }

    fn apply_runtime(saga: &mut AgentRunForkSaga) {
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("runtime step");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch marker");
        let ev = evidence(saga);
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
                    let ev = evidence(&restarted);
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
        saga.record_product_graph_commit(graph_evidence(&saga))
            .expect("graph");
        let known_child = saga.child.runtime_thread_id.clone();
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("execute");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch marker");
        saga.record_runtime_outcome(
            identity,
            RuntimeOperationOutcome::Lost {
                reason: "inspection horizon expired".to_owned(),
            },
        )
        .expect("lost");

        assert_eq!(
            saga.lost
                .as_ref()
                .map(|lost| lost.known_child_thread_id.clone()),
            Some(known_child)
        );
        assert_eq!(saga.next_step(), AgentRunForkSagaStep::Terminal);
        assert!(!saga.permits_new_fork());
    }

    #[test]
    fn runtime_activation_cannot_bypass_graph_or_required_context_evidence() {
        let mut saga = saga();
        let activate = saga.operation_identity(AgentRunForkRuntimeOperation::Activate);
        let ev = RuntimeForkPhaseEvidence::Activated {
            child_thread_id: saga.child.runtime_thread_id.clone(),
            context: None,
            receipt: accepted(&activate),
        };
        assert!(matches!(
            saga.record_runtime_outcome(activate, RuntimeOperationOutcome::Applied(ev)),
            Err(AgentRunForkSagaError::InvalidOperation { .. })
        ));

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
        let ev = evidence(&saga);
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
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("fork");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch marker");
        saga.record_runtime_outcome(
            identity.clone(),
            RuntimeOperationOutcome::Accepted(accepted(&identity)),
        )
        .expect("accepted");
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
            let outcome = RuntimeOperationOutcome::Applied(evidence(saga));
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
            Ok(RuntimeOperationOutcome::Accepted(accepted(identity)))
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
        async fn prepare_child_graph_commit(
            &self,
            saga: &AgentRunForkSaga,
        ) -> Result<PreparedAgentRunForkGraph, String> {
            Ok(prepared_graph(saga))
        }
    }

    async fn persisted_runtime_provisioned(
        repository: &RecordingAgentRunForkSagaRepository,
    ) -> AgentRunForkSaga {
        let created = repository.create(saga()).await.expect("create");
        let mut provisioned = created.clone();
        apply_runtime(&mut provisioned);
        repository
            .save(created.version, provisioned)
            .await
            .expect("persist provisioned")
    }

    fn transitioned_graph_commit(
        provisioned: &AgentRunForkSaga,
        graph: &PreparedAgentRunForkGraph,
    ) -> AgentRunForkSaga {
        let mut transitioned = provisioned.clone();
        transitioned
            .record_product_graph_commit(graph.commit_evidence(provisioned.version + 1))
            .expect("transition graph commit");
        transitioned
    }

    #[tokio::test]
    async fn graph_and_saga_commit_are_atomic_cas_and_idempotent() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let provisioned = persisted_runtime_provisioned(&repository).await;
        let graph = prepared_graph(&provisioned);
        let transitioned = transitioned_graph_commit(&provisioned, &graph);

        assert!(
            repository
                .load_graph(provisioned.request_id())
                .await
                .is_none()
        );

        assert!(matches!(
            repository
                .commit_product_graph(provisioned.version - 1, transitioned.clone(), graph.clone(),)
                .await,
            Err(AgentRunForkSagaRepositoryError::Conflict { .. })
        ));
        assert!(
            repository
                .load_graph(provisioned.request_id())
                .await
                .is_none()
        );
        assert_eq!(
            repository
                .load(provisioned.request_id())
                .await
                .expect("load")
                .expect("saga")
                .phase(),
            AgentRunForkSagaPhase::RuntimeProvisioned
        );

        let committed = repository
            .commit_product_graph(provisioned.version, transitioned.clone(), graph.clone())
            .await
            .expect("commit graph and saga");
        assert_eq!(
            committed.phase(),
            AgentRunForkSagaPhase::ProductGraphCommitted
        );
        assert_eq!(
            repository
                .load_graph(provisioned.request_id())
                .await
                .expect("graph")
                .payload_digest,
            graph.payload_digest
        );

        let replayed = repository
            .commit_product_graph(provisioned.version, transitioned.clone(), graph.clone())
            .await
            .expect("idempotent replay");
        assert_eq!(replayed, committed);

        let mut conflicting = graph;
        conflicting.child_run.created_by_user_id = "different-owner".to_owned();
        conflicting.payload_digest = conflicting.calculate_digest();
        assert_eq!(
            repository
                .commit_product_graph(provisioned.version, transitioned, conflicting)
                .await,
            Err(AgentRunForkSagaRepositoryError::GraphPayloadConflict)
        );
    }

    #[tokio::test]
    async fn production_facade_materializes_one_preallocated_identity_set() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let runtime = CompleteAgentTargetFixture::default();
        let facade = AgentRunForkFacade::new(&repository, &runtime, &MatchingProductGraph);
        let requested = saga();
        let command = MaterializeAgentRunFork {
            request_id: requested.request_id.clone(),
            parent: requested.parent.clone(),
            child: requested.child.clone(),
            required_initial_context: None,
        };

        let created = facade
            .materialize(command.clone())
            .await
            .expect("materialize");
        let replayed = facade.materialize(command).await.expect("replay");
        assert_eq!(created, replayed);

        let mut drifted_child = requested.child;
        drifted_child.frame_id = Uuid::new_v4();
        assert_eq!(
            facade
                .materialize(MaterializeAgentRunFork {
                    request_id: requested.request_id,
                    parent: requested.parent,
                    child: drifted_child,
                    required_initial_context: None,
                })
                .await,
            Err(AgentRunForkFacadeError::ExistingRequestDrift)
        );
    }

    #[test]
    fn persistence_contract_freezes_the_w8_transaction_coordinates() {
        assert_eq!(
            AGENT_RUN_FORK_SAGA_SCHEMA_CONTRACT.table,
            "agent_run_fork_saga"
        );
        assert_eq!(
            AGENT_RUN_FORK_SAGA_SCHEMA_CONTRACT.durable_dispatch_identity,
            "durable_runtime_dispatch"
        );
        assert_eq!(
            AGENT_RUN_FORK_SAGA_SCHEMA_CONTRACT.graph_commit_revision,
            "graph_commit_revision"
        );
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
            succeeded
                .child_history_digest
                .as_ref()
                .map(RuntimePayloadDigest::as_str),
            Some("sha256:child-history")
        );
        assert!(succeeded.receipts.runtime_admission.is_some());
        assert!(succeeded.receipts.runtime_provisioning.is_some());
        assert!(succeeded.receipts.runtime_activation.is_some());
    }

    #[tokio::test]
    async fn side_effect_before_save_restart_only_inspects_the_same_identity() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let created = repository.create(saga()).await.expect("create");
        let runtime =
            CompleteAgentTargetFixture::losing_responses([AgentRunForkRuntimeOperation::Fork]);
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

        let provisioned = worker.advance(&created.request_id).await.expect("inspect");
        assert_eq!(
            provisioned.phase(),
            AgentRunForkSagaPhase::RuntimeProvisioned
        );
        assert_eq!(
            runtime.execute_calls.lock().await.as_slice(),
            std::slice::from_ref(&dispatch)
        );
        assert_eq!(runtime.inspect_calls.lock().await.as_slice(), &[dispatch]);
    }

    #[tokio::test]
    async fn every_runtime_crash_window_recovers_by_inspection() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let created = repository.create(saga()).await.expect("create");
        let runtime = CompleteAgentTargetFixture::losing_responses([
            AgentRunForkRuntimeOperation::Fork,
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
        assert_eq!(runtime.execute_calls.lock().await.len(), 2);
        assert_eq!(runtime.inspect_calls.lock().await.len(), 2);
    }

    #[test]
    fn pinned_runtime_child_binding_and_history_reject_drift() {
        let mut saga = saga();
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("fork");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch");
        let mut drifted = evidence(&saga);
        let RuntimeForkPhaseEvidence::ForkProvisioned {
            child_thread_id, ..
        } = &mut drifted
        else {
            panic!("fork evidence");
        };
        *child_thread_id = RuntimeThreadId::new("different-child").expect("thread id");
        assert_eq!(
            saga.record_runtime_outcome(identity, RuntimeOperationOutcome::Applied(drifted)),
            Err(AgentRunForkSagaError::RuntimeChildIdentityDrift)
        );
    }

    #[test]
    fn product_graph_receipt_covers_every_preallocated_and_runtime_identity() {
        let mut base = saga();
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
        evidence.runtime_thread_id =
            RuntimeThreadId::new("different-thread").expect("different thread id");
        mismatches.push(evidence);
        let mut evidence = expected;
        evidence.child_history_digest =
            RuntimePayloadDigest::new("sha256:different-history").expect("history digest");
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
        async fn prepare_child_graph_commit(
            &self,
            _saga: &AgentRunForkSaga,
        ) -> Result<PreparedAgentRunForkGraph, String> {
            Err("Runtime child mapping could not be committed".to_owned())
        }
    }

    #[tokio::test]
    async fn known_native_child_mapping_failure_is_lost_not_failed() {
        let repository = RecordingAgentRunForkSagaRepository::default();
        let created = repository.create(saga()).await.expect("create");
        let runtime = CompleteAgentTargetFixture::default();
        let worker = AgentRunForkSagaWorker::new(&repository, &runtime, &FailingProductGraph);
        for _ in 0..2 {
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
