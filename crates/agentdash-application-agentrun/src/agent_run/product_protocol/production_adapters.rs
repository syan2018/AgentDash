use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedAgentRuntimeGateway, ManagedRuntimeAppliedInitialContextEvidence,
    ManagedRuntimeChangesRequest, ManagedRuntimeCommand, ManagedRuntimeCommandEnvelope,
    ManagedRuntimeContentBlock, ManagedRuntimeContextAuthority, ManagedRuntimeContextProvenance,
    ManagedRuntimeForkCutoff, ManagedRuntimeForkProgressEvidence,
    ManagedRuntimeInitialContextAppliedFidelity, ManagedRuntimeInitialContextContribution,
    ManagedRuntimeInitialContextContributionContent, ManagedRuntimeInitialContextContributionKind,
    ManagedRuntimeInitialContextMode, ManagedRuntimeInitialContextPackage,
    ManagedRuntimeOperationEvidence, ManagedRuntimeOperationStatus, ManagedRuntimeReadRequest,
    ManagedRuntimeSnapshot, RuntimeChangeSequence, RuntimeContextContributionId,
    RuntimeContextPackageId, RuntimeContextSourceRef, RuntimeContextSourceRevision,
    RuntimeIdempotencyKey, RuntimeOperationId, RuntimePayloadDigest, RuntimeProjectionRevision,
    RuntimeThreadId,
};
use agentdash_application_ports::agent_frame_materialization::{
    AgentRunFrameConstructionPort, FrameConstructionCommand,
};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunLineage, LifecycleAgent, LifecycleAgentRepository, LifecycleRun,
    LifecycleRunRepository,
};
use async_trait::async_trait;
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::agent_run::{
    AgentRunProductLaunchService, AgentRunProductRuntimeProvisioningRequest, ProductAgentFrameRef,
    ProductAgentSurfaceFacts,
};

use super::{
    AcceptedRuntimeOperation, AgentRunForkChildProductSelection, AgentRunForkGraph,
    AgentRunForkOperationIdentity, AgentRunForkProductGraphPort, AgentRunForkRuntimeOperation,
    AgentRunForkRuntimePort, AgentRunForkSaga, AgentRunRuntimeProjectionPort,
    CompanionFreshEffectEvidence, CompanionFreshEffectOutcome, CompanionFreshOperation,
    CompanionFreshOperationIdentity, CompanionFreshRuntimePort, CompanionFreshSaga,
    CompanionRuntimePreparation, CompiledContextApplication, CompiledContextAuthority,
    CompiledContextContributionApplication, CompiledContextDeliveryFidelity,
    CompiledFreshContextMode, CompiledInitialContextContribution, CompiledInitialContextPackage,
    PreparedAgentRunForkGraph, RuntimeForkChildProgress, RuntimeForkPhaseEvidence,
    RuntimeOperationOutcome,
};

const PRODUCT_RUNTIME_CHANGE_PAGE_LIMIT: u32 = 256;

#[derive(Debug, Clone)]
pub struct ProductManagedRuntimeOperationObservation {
    pub status: ManagedRuntimeOperationStatus,
    pub evidence: Option<ManagedRuntimeOperationEvidence>,
    pub accepted_revision: RuntimeProjectionRevision,
}

#[derive(Clone)]
pub struct ProductManagedRuntimeCommandAdapter {
    gateway: Arc<dyn ManagedAgentRuntimeGateway>,
}

impl ProductManagedRuntimeCommandAdapter {
    pub fn new(gateway: Arc<dyn ManagedAgentRuntimeGateway>) -> Self {
        Self { gateway }
    }

    pub async fn execute(
        &self,
        command: ManagedRuntimeCommandEnvelope,
    ) -> Result<ProductManagedRuntimeOperationObservation, String> {
        let expected_operation_id = command.operation_id.clone();
        let expected_thread_id = command.thread_id.clone();
        let receipt = self
            .gateway
            .execute(command)
            .await
            .map_err(|error| error.to_string())?;
        if receipt.operation_id != expected_operation_id || receipt.thread_id != expected_thread_id
        {
            return Err(
                "Runtime receipt identity drifted from the dispatched operation".to_owned(),
            );
        }
        Ok(ProductManagedRuntimeOperationObservation {
            status: receipt.status,
            evidence: receipt.evidence,
            accepted_revision: receipt.accepted_revision,
        })
    }

    pub async fn inspect(
        &self,
        thread_id: &RuntimeThreadId,
        operation_id: &RuntimeOperationId,
    ) -> Result<Option<ProductManagedRuntimeOperationObservation>, String> {
        let snapshot = match self
            .gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: thread_id.clone(),
            })
            .await
        {
            Ok(snapshot) => snapshot,
            Err(agentdash_agent_runtime_contract::ManagedRuntimeGatewayError::NotFound) => {
                return Ok(None);
            }
            Err(error) => return Err(error.to_string()),
        };
        if &snapshot.thread_id != thread_id {
            return Err("Runtime inspection returned a different thread".to_owned());
        }
        let Some(operation) = snapshot
            .operations
            .iter()
            .find(|operation| &operation.id == operation_id)
        else {
            return Ok(None);
        };
        Ok(Some(ProductManagedRuntimeOperationObservation {
            status: operation.status,
            evidence: operation.evidence.clone(),
            accepted_revision: snapshot.revision,
        }))
    }
}

enum ProductLaunchConvergence {
    Required(Arc<AgentRunProductLaunchService>),
    #[cfg(test)]
    Noop,
}

impl ProductLaunchConvergence {
    async fn prepare(
        &self,
        request: &crate::agent_run::AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<(), String> {
        match self {
            Self::Required(service) => service
                .prepare_runtime_target(request)
                .await
                .map_err(|error| error.to_string()),
            #[cfg(test)]
            Self::Noop => Ok(()),
        }
    }

    async fn created(
        &self,
        request: &crate::agent_run::AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<(), String> {
        match self {
            Self::Required(service) => service
                .converge_created_runtime(request)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string()),
            #[cfg(test)]
            Self::Noop => Ok(()),
        }
    }

    async fn activated(
        &self,
        request: &crate::agent_run::AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<(), String> {
        match self {
            Self::Required(service) => service
                .converge_activated_runtime(request)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string()),
            #[cfg(test)]
            Self::Noop => Ok(()),
        }
    }
}

/// Product's exact-fork adapter over the final managed Runtime Gateway.
pub struct ProductAgentRunForkRuntimeAdapter {
    gateway: Arc<dyn ManagedAgentRuntimeGateway>,
    convergence: ProductLaunchConvergence,
}

impl ProductAgentRunForkRuntimeAdapter {
    pub fn with_product_launch(
        gateway: Arc<dyn ManagedAgentRuntimeGateway>,
        product_launch: Arc<AgentRunProductLaunchService>,
    ) -> Self {
        Self {
            gateway,
            convergence: ProductLaunchConvergence::Required(product_launch),
        }
    }

    #[cfg(test)]
    pub fn new(gateway: Arc<dyn ManagedAgentRuntimeGateway>) -> Self {
        Self {
            gateway,
            convergence: ProductLaunchConvergence::Noop,
        }
    }

    fn command(
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
    ) -> Result<ManagedRuntimeCommandEnvelope, String> {
        let (thread_id, command) = match identity.operation {
            AgentRunForkRuntimeOperation::Fork => (
                saga.parent().runtime_thread_id.clone(),
                ManagedRuntimeCommand::Fork {
                    child_thread_id: saga.child().runtime_thread_id.clone(),
                    through_completed_turn_id: Some(saga.parent().through_turn_id.clone()),
                },
            ),
            AgentRunForkRuntimeOperation::Rebind => (
                saga.child().runtime_thread_id.clone(),
                ManagedRuntimeCommand::Rebind,
            ),
            AgentRunForkRuntimeOperation::Activate => (
                saga.child().runtime_thread_id.clone(),
                ManagedRuntimeCommand::Activate,
            ),
        };
        Ok(ManagedRuntimeCommandEnvelope {
            operation_id: identity.runtime_operation_id.clone(),
            idempotency_key: runtime_idempotency_key(&identity.runtime_operation_id)?,
            thread_id,
            expected_revision: None,
            command,
        })
    }

    fn operation_thread(
        saga: &AgentRunForkSaga,
        operation: AgentRunForkRuntimeOperation,
    ) -> &RuntimeThreadId {
        match operation {
            AgentRunForkRuntimeOperation::Fork => &saga.parent().runtime_thread_id,
            AgentRunForkRuntimeOperation::Rebind | AgentRunForkRuntimeOperation::Activate => {
                &saga.child().runtime_thread_id
            }
        }
    }

    fn map_outcome(
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
        status: ManagedRuntimeOperationStatus,
        evidence: Option<&ManagedRuntimeOperationEvidence>,
        accepted_revision: RuntimeProjectionRevision,
    ) -> Result<RuntimeOperationOutcome, String> {
        let receipt = accepted_receipt(&identity.runtime_operation_id, accepted_revision);
        match status {
            ManagedRuntimeOperationStatus::Accepted | ManagedRuntimeOperationStatus::Running => {
                if identity.operation == AgentRunForkRuntimeOperation::Fork {
                    if let Some(ManagedRuntimeOperationEvidence::Fork { progress, .. }) = evidence {
                        Self::validate_fork_progress(saga, progress)?;
                        if let ManagedRuntimeForkProgressEvidence::ChildKnown { .. } = progress {
                            return Ok(RuntimeOperationOutcome::ChildKnown(
                                Self::map_child_progress(identity, progress, receipt)?,
                            ));
                        }
                    }
                }
                Ok(RuntimeOperationOutcome::Accepted(receipt))
            }
            ManagedRuntimeOperationStatus::Lost => {
                let known_child =
                    Self::known_child_from_evidence(saga, identity, evidence, receipt)?;
                Ok(RuntimeOperationOutcome::Lost {
                    reason: "managed Runtime operation ended with Lost".to_owned(),
                    known_child,
                })
            }
            ManagedRuntimeOperationStatus::Failed | ManagedRuntimeOperationStatus::Interrupted => {
                let known_child =
                    Self::known_child_from_evidence(saga, identity, evidence, receipt)?;
                if known_child.is_some() || saga.child_progress().is_some() {
                    Ok(RuntimeOperationOutcome::Lost {
                        reason: format!(
                            "managed Runtime operation ended with {status:?} after child creation"
                        ),
                        known_child,
                    })
                } else {
                    Ok(RuntimeOperationOutcome::Failed {
                        reason: format!("managed Runtime operation ended with {status:?}"),
                    })
                }
            }
            ManagedRuntimeOperationStatus::Succeeded => match identity.operation {
                AgentRunForkRuntimeOperation::Fork => {
                    let Some(ManagedRuntimeOperationEvidence::Fork { progress, .. }) = evidence
                    else {
                        return Err(
                            "succeeded Runtime fork is missing typed Fork evidence".to_owned()
                        );
                    };
                    Self::validate_fork_progress(saga, progress)?;
                    match progress {
                        ManagedRuntimeForkProgressEvidence::ChildKnown { .. } => Err(
                            "succeeded Runtime fork only reports a known, unprovisioned child"
                                .to_owned(),
                        ),
                        ManagedRuntimeForkProgressEvidence::Provisioned {
                            child_thread_id,
                            child_binding,
                            cutoff,
                            child_history_digest,
                        } => {
                            if child_thread_id != &saga.child().runtime_thread_id {
                                return Err(
                                    "Runtime fork provisioned a different child thread".to_owned()
                                );
                            }
                            if cutoff
                                != &(ManagedRuntimeForkCutoff::CompletedTurn {
                                    turn_id: saga.parent().through_turn_id.clone(),
                                })
                            {
                                return Err(
                                    "Runtime fork evidence does not match the exact turn cutoff"
                                        .to_owned(),
                                );
                            }
                            if child_binding.activated_at_revision.is_some() {
                                return Err(
                                    "Runtime fork child was activated before Product graph commit"
                                        .to_owned(),
                                );
                            }
                            Ok(RuntimeOperationOutcome::Applied(
                                RuntimeForkPhaseEvidence::ForkProvisioned {
                                    child_thread_id: child_thread_id.clone(),
                                    child_binding: child_binding.clone(),
                                    child_history_digest: child_history_digest.clone(),
                                    context: None,
                                    receipt,
                                },
                            ))
                        }
                    }
                }
                AgentRunForkRuntimeOperation::Rebind => {
                    let Some(ManagedRuntimeOperationEvidence::Rebind { binding, .. }) = evidence
                    else {
                        return Err(
                            "succeeded Runtime rebind is missing typed Rebind evidence".to_owned()
                        );
                    };
                    let Some(provisioning) = saga.materialized_child_product_selection() else {
                        return Err(
                            "Runtime Rebind has no durable child Product selection".to_owned()
                        );
                    };
                    if binding.activated_at_revision.is_some()
                        || binding.applied_surface_revision.0
                            != provisioning.surface_facts.surface_revision
                    {
                        return Err(
                            "Runtime Rebind evidence does not match selected child surface"
                                .to_owned(),
                        );
                    }
                    Ok(RuntimeOperationOutcome::Applied(
                        RuntimeForkPhaseEvidence::Rebound {
                            child_thread_id: saga.child().runtime_thread_id.clone(),
                            child_binding: binding.clone(),
                            receipt,
                        },
                    ))
                }
                AgentRunForkRuntimeOperation::Activate => {
                    let Some(ManagedRuntimeOperationEvidence::Activate { binding }) = evidence
                    else {
                        return Err(
                            "succeeded Runtime activation is missing typed Activate evidence"
                                .to_owned(),
                        );
                    };
                    if binding.activated_at_revision.is_none() {
                        return Err(
                            "Runtime Activate evidence does not prove child activation".to_owned()
                        );
                    }
                    Self::validate_activation_binding(saga.child_binding(), binding)?;
                    Ok(RuntimeOperationOutcome::Applied(
                        RuntimeForkPhaseEvidence::Activated {
                            child_thread_id: saga.child().runtime_thread_id.clone(),
                            child_binding: binding.clone(),
                            context: saga.initial_context_evidence().cloned(),
                            receipt,
                        },
                    ))
                }
            },
        }
    }

    fn validate_fork_progress(
        saga: &AgentRunForkSaga,
        progress: &ManagedRuntimeForkProgressEvidence,
    ) -> Result<(), String> {
        let (child_thread_id, cutoff) = match progress {
            ManagedRuntimeForkProgressEvidence::ChildKnown {
                child_thread_id,
                cutoff,
                ..
            }
            | ManagedRuntimeForkProgressEvidence::Provisioned {
                child_thread_id,
                cutoff,
                ..
            } => (child_thread_id, cutoff),
        };
        if child_thread_id != &saga.child().runtime_thread_id {
            return Err("Runtime fork progress reports a different child thread".to_owned());
        }
        if cutoff
            != &(ManagedRuntimeForkCutoff::CompletedTurn {
                turn_id: saga.parent().through_turn_id.clone(),
            })
        {
            return Err("Runtime fork progress does not match the exact turn cutoff".to_owned());
        }
        Ok(())
    }

    fn map_child_progress(
        identity: &AgentRunForkOperationIdentity,
        progress: &ManagedRuntimeForkProgressEvidence,
        receipt: AcceptedRuntimeOperation,
    ) -> Result<RuntimeForkChildProgress, String> {
        let ManagedRuntimeForkProgressEvidence::ChildKnown {
            child_thread_id,
            child_source_ref,
            child_history_digest,
            ..
        } = progress
        else {
            return Err("Runtime fork progress is not ChildKnown".to_owned());
        };
        Ok(RuntimeForkChildProgress {
            identity: identity.clone(),
            child_thread_id: child_thread_id.clone(),
            child_source_ref: child_source_ref.clone(),
            child_history_digest: child_history_digest.clone(),
            receipt,
        })
    }

    fn known_child_from_evidence(
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
        evidence: Option<&ManagedRuntimeOperationEvidence>,
        receipt: AcceptedRuntimeOperation,
    ) -> Result<Option<RuntimeForkChildProgress>, String> {
        if identity.operation != AgentRunForkRuntimeOperation::Fork {
            return Ok(saga.child_progress().cloned());
        }
        match evidence {
            Some(ManagedRuntimeOperationEvidence::Fork { progress, .. }) => {
                Self::validate_fork_progress(saga, progress)?;
                match progress {
                    ManagedRuntimeForkProgressEvidence::ChildKnown { .. } => {
                        Self::map_child_progress(identity, progress, receipt).map(Some)
                    }
                    ManagedRuntimeForkProgressEvidence::Provisioned {
                        child_thread_id,
                        child_binding,
                        child_history_digest,
                        ..
                    } => Ok(Some(RuntimeForkChildProgress {
                        identity: identity.clone(),
                        child_thread_id: child_thread_id.clone(),
                        child_source_ref: child_binding.source_ref.clone(),
                        child_history_digest: Some(child_history_digest.clone()),
                        receipt,
                    })),
                }
            }
            Some(_) => Err("Runtime fork outcome contains another command's evidence".to_owned()),
            None => Ok(saga.child_progress().cloned()),
        }
    }

    fn validate_activation_binding(
        expected: Option<&agentdash_agent_runtime_contract::ManagedRuntimeSourceBindingEvidence>,
        actual: &agentdash_agent_runtime_contract::ManagedRuntimeSourceBindingEvidence,
    ) -> Result<(), String> {
        let Some(expected) = expected else {
            return Err("Runtime activation has no pinned child binding".to_owned());
        };
        if actual.activated_at_revision.is_none()
            || actual.source_ref != expected.source_ref
            || actual.committed_at_revision != expected.committed_at_revision
            || actual.applied_surface_revision != expected.applied_surface_revision
        {
            return Err("Runtime activation rebound the pinned child source".to_owned());
        }
        Ok(())
    }

    async fn converge_outcome(
        &self,
        saga: &AgentRunForkSaga,
        outcome: &RuntimeOperationOutcome,
    ) -> Result<(), String> {
        let Some(provisioning) = saga.materialized_child_product_selection() else {
            return Ok(());
        };
        match outcome {
            RuntimeOperationOutcome::Applied(RuntimeForkPhaseEvidence::Rebound { .. }) => {
                self.convergence.created(provisioning).await
            }
            RuntimeOperationOutcome::Applied(RuntimeForkPhaseEvidence::Activated { .. }) => {
                self.convergence.activated(provisioning).await
            }
            _ => Ok(()),
        }
    }
}

#[async_trait]
impl AgentRunForkRuntimePort for ProductAgentRunForkRuntimeAdapter {
    async fn execute(
        &self,
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
    ) -> Result<RuntimeOperationOutcome, String> {
        if identity.operation == AgentRunForkRuntimeOperation::Rebind {
            let provisioning = saga.materialized_child_product_selection().ok_or_else(|| {
                "Runtime Rebind has no durable child Product selection".to_owned()
            })?;
            self.convergence.prepare(provisioning).await?;
        }
        let command = Self::command(saga, identity)?;
        let expected_thread = command.thread_id.clone();
        let receipt = self
            .gateway
            .execute(command)
            .await
            .map_err(|error| error.to_string())?;
        if receipt.operation_id != identity.runtime_operation_id
            || receipt.thread_id != expected_thread
        {
            return Err(
                "Runtime receipt identity drifted from the dispatched operation".to_owned(),
            );
        }
        let outcome = Self::map_outcome(
            saga,
            identity,
            receipt.status,
            receipt.evidence.as_ref(),
            receipt.accepted_revision,
        )?;
        self.converge_outcome(saga, &outcome).await?;
        Ok(outcome)
    }

    async fn inspect(
        &self,
        saga: &AgentRunForkSaga,
        identity: &AgentRunForkOperationIdentity,
    ) -> Result<RuntimeOperationOutcome, String> {
        let thread_id = Self::operation_thread(saga, identity.operation).clone();
        let snapshot = match self
            .gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: thread_id.clone(),
            })
            .await
        {
            Ok(snapshot) => snapshot,
            Err(agentdash_agent_runtime_contract::ManagedRuntimeGatewayError::NotFound) => {
                return Ok(RuntimeOperationOutcome::Unknown);
            }
            Err(error) => return Err(error.to_string()),
        };
        if snapshot.thread_id != thread_id {
            return Err("Runtime inspection returned a different thread".to_owned());
        }
        let Some(operation) = snapshot
            .operations
            .iter()
            .find(|operation| operation.id == identity.runtime_operation_id)
        else {
            return Ok(RuntimeOperationOutcome::Unknown);
        };
        let outcome = Self::map_outcome(
            saga,
            identity,
            operation.status,
            operation.evidence.as_ref(),
            snapshot.revision,
        )?;
        self.converge_outcome(saga, &outcome).await?;
        Ok(outcome)
    }
}

/// Product's fresh Companion adapter over the final managed Runtime Gateway.
pub struct ProductCompanionFreshRuntimeAdapter {
    runtime: ProductManagedRuntimeCommandAdapter,
    convergence: ProductLaunchConvergence,
}

impl ProductCompanionFreshRuntimeAdapter {
    pub fn with_product_launch(
        gateway: Arc<dyn ManagedAgentRuntimeGateway>,
        product_launch: Arc<AgentRunProductLaunchService>,
    ) -> Self {
        Self {
            runtime: ProductManagedRuntimeCommandAdapter::new(gateway),
            convergence: ProductLaunchConvergence::Required(product_launch),
        }
    }

    #[cfg(test)]
    pub fn new(gateway: Arc<dyn ManagedAgentRuntimeGateway>) -> Self {
        Self {
            runtime: ProductManagedRuntimeCommandAdapter::new(gateway),
            convergence: ProductLaunchConvergence::Noop,
        }
    }

    fn initial_context(
        saga: &CompanionFreshSaga,
    ) -> Result<&CompiledInitialContextPackage, String> {
        match &saga.plan().preparation {
            CompanionRuntimePreparation::FreshCreate { initial_context } => Ok(initial_context),
            CompanionRuntimePreparation::ForkParentHistory { .. } => {
                Err("fresh Companion saga does not contain a fresh context package".to_owned())
            }
        }
    }

    fn command(
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
    ) -> Result<ManagedRuntimeCommandEnvelope, String> {
        let command = match identity.operation {
            CompanionFreshOperation::CreateWithContextPackage => ManagedRuntimeCommand::Create {
                initial_context: Some(compile_runtime_initial_context(Self::initial_context(
                    saga,
                )?)?),
            },
            CompanionFreshOperation::Activate => ManagedRuntimeCommand::Activate,
            CompanionFreshOperation::SubmitFirstInput => ManagedRuntimeCommand::SubmitInput {
                content: vec![ManagedRuntimeContentBlock::Text {
                    text: saga.plan().first_submit_input.text.clone(),
                }],
            },
        };
        Ok(ManagedRuntimeCommandEnvelope {
            operation_id: identity.runtime_operation_id.clone(),
            idempotency_key: runtime_idempotency_key(&identity.runtime_operation_id)?,
            thread_id: saga.runtime_thread_id().clone(),
            expected_revision: None,
            command,
        })
    }

    fn map_outcome(
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
        status: ManagedRuntimeOperationStatus,
        evidence: Option<&ManagedRuntimeOperationEvidence>,
        accepted_revision: RuntimeProjectionRevision,
    ) -> Result<CompanionFreshEffectOutcome, String> {
        let receipt = accepted_receipt(&identity.runtime_operation_id, accepted_revision);
        match status {
            ManagedRuntimeOperationStatus::Accepted | ManagedRuntimeOperationStatus::Running => {
                Ok(CompanionFreshEffectOutcome::Accepted(receipt))
            }
            ManagedRuntimeOperationStatus::Lost => Ok(CompanionFreshEffectOutcome::Lost {
                reason: "managed Runtime operation ended with Lost".to_owned(),
            }),
            ManagedRuntimeOperationStatus::Failed | ManagedRuntimeOperationStatus::Interrupted => {
                if saga.child_binding().is_some() {
                    Ok(CompanionFreshEffectOutcome::Lost {
                        reason: format!(
                            "managed Runtime operation ended with {status:?} after child creation"
                        ),
                    })
                } else {
                    Ok(CompanionFreshEffectOutcome::Failed {
                        reason: format!("managed Runtime operation ended with {status:?}"),
                    })
                }
            }
            ManagedRuntimeOperationStatus::Succeeded => match identity.operation {
                CompanionFreshOperation::CreateWithContextPackage => {
                    let Some(ManagedRuntimeOperationEvidence::Create {
                        binding,
                        initial_context: Some(evidence),
                    }) = evidence
                    else {
                        return Err(
                            "succeeded Runtime create is missing typed initial-context evidence"
                                .to_owned(),
                        );
                    };
                    if binding.activated_at_revision.is_some() {
                        return Err(
                            "fresh Runtime child was activated before the Activate operation"
                                .to_owned(),
                        );
                    }
                    let context =
                        map_initial_context_evidence(Self::initial_context(saga)?, evidence)?;
                    Ok(CompanionFreshEffectOutcome::Applied(
                        CompanionFreshEffectEvidence::Created {
                            child_runtime_thread_id: saga.runtime_thread_id().clone(),
                            child_binding: binding.clone(),
                            context,
                            receipt,
                        },
                    ))
                }
                CompanionFreshOperation::Activate => {
                    let Some(ManagedRuntimeOperationEvidence::Activate { binding }) = evidence
                    else {
                        return Err(
                            "succeeded Runtime activation is missing typed Activate evidence"
                                .to_owned(),
                        );
                    };
                    if binding.activated_at_revision.is_none() {
                        return Err(
                            "Runtime Activate evidence does not prove child activation".to_owned()
                        );
                    }
                    ProductAgentRunForkRuntimeAdapter::validate_activation_binding(
                        saga.child_binding(),
                        binding,
                    )?;
                    Ok(CompanionFreshEffectOutcome::Applied(
                        CompanionFreshEffectEvidence::Activated {
                            child_runtime_thread_id: saga.runtime_thread_id().clone(),
                            child_binding: binding.clone(),
                            receipt,
                        },
                    ))
                }
                CompanionFreshOperation::SubmitFirstInput => {
                    if evidence.is_some() {
                        return Err(
                            "SubmitInput operation returned evidence for another command family"
                                .to_owned(),
                        );
                    }
                    Ok(CompanionFreshEffectOutcome::Applied(
                        CompanionFreshEffectEvidence::FirstInputSubmitted {
                            child_runtime_thread_id: saga.runtime_thread_id().clone(),
                            receipt,
                        },
                    ))
                }
            },
        }
    }
}

#[async_trait]
impl CompanionFreshRuntimePort for ProductCompanionFreshRuntimeAdapter {
    async fn execute(
        &self,
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
    ) -> Result<CompanionFreshEffectOutcome, String> {
        if identity.operation == CompanionFreshOperation::CreateWithContextPackage {
            self.convergence.prepare(saga.provisioning()).await?;
        }
        let command = Self::command(saga, identity)?;
        let observation = self.runtime.execute(command).await?;
        let outcome = Self::map_outcome(
            saga,
            identity,
            observation.status,
            observation.evidence.as_ref(),
            observation.accepted_revision,
        )?;
        self.converge_product_state(saga, identity, outcome).await
    }

    async fn inspect(
        &self,
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
    ) -> Result<CompanionFreshEffectOutcome, String> {
        let Some(observation) = self
            .runtime
            .inspect(saga.runtime_thread_id(), &identity.runtime_operation_id)
            .await?
        else {
            return Ok(CompanionFreshEffectOutcome::Unknown);
        };
        let outcome = Self::map_outcome(
            saga,
            identity,
            observation.status,
            observation.evidence.as_ref(),
            observation.accepted_revision,
        )?;
        self.converge_product_state(saga, identity, outcome).await
    }
}

impl ProductCompanionFreshRuntimeAdapter {
    async fn converge_product_state(
        &self,
        saga: &CompanionFreshSaga,
        identity: &CompanionFreshOperationIdentity,
        outcome: CompanionFreshEffectOutcome,
    ) -> Result<CompanionFreshEffectOutcome, String> {
        if !matches!(outcome, CompanionFreshEffectOutcome::Applied(_)) {
            return Ok(outcome);
        }
        match identity.operation {
            CompanionFreshOperation::CreateWithContextPackage => {
                self.convergence.created(saga.provisioning()).await?;
            }
            CompanionFreshOperation::Activate => {
                self.convergence.activated(saga.provisioning()).await?;
            }
            CompanionFreshOperation::SubmitFirstInput => {}
        }
        Ok(outcome)
    }
}

/// Lossless Product read adapter for the canonical Runtime snapshot/change protocol.
pub struct ProductAgentRunRuntimeProjectionAdapter {
    gateway: Arc<dyn ManagedAgentRuntimeGateway>,
}

impl ProductAgentRunRuntimeProjectionAdapter {
    pub fn new(gateway: Arc<dyn ManagedAgentRuntimeGateway>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl AgentRunRuntimeProjectionPort for ProductAgentRunRuntimeProjectionAdapter {
    async fn load_snapshot(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<ManagedRuntimeSnapshot, String> {
        let snapshot = self
            .gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: thread_id.clone(),
            })
            .await
            .map_err(|error| error.to_string())?;
        if &snapshot.thread_id != thread_id {
            return Err("Runtime Gateway returned a snapshot for another thread".to_owned());
        }
        Ok(snapshot)
    }

    async fn load_changes(
        &self,
        thread_id: &RuntimeThreadId,
        after: Option<RuntimeChangeSequence>,
    ) -> Result<agentdash_agent_runtime_contract::ManagedRuntimeChangePage, String> {
        let page = self
            .gateway
            .changes(ManagedRuntimeChangesRequest {
                thread_id: thread_id.clone(),
                after,
                limit: PRODUCT_RUNTIME_CHANGE_PAGE_LIMIT,
            })
            .await
            .map_err(|error| error.to_string())?;
        if &page.thread_id != thread_id
            || page
                .changes
                .iter()
                .any(|change| &change.thread_id != thread_id)
        {
            return Err("Runtime Gateway returned changes for another thread".to_owned());
        }
        Ok(page)
    }
}

fn runtime_idempotency_key(
    operation_id: &RuntimeOperationId,
) -> Result<RuntimeIdempotencyKey, String> {
    RuntimeIdempotencyKey::new(format!("product:{}", operation_id.as_str()))
        .map_err(|error| error.to_string())
}

fn accepted_receipt(
    operation_id: &RuntimeOperationId,
    accepted_revision: RuntimeProjectionRevision,
) -> AcceptedRuntimeOperation {
    AcceptedRuntimeOperation {
        operation_id: operation_id.clone(),
        accepted_revision,
    }
}

fn compile_runtime_initial_context(
    package: &CompiledInitialContextPackage,
) -> Result<ManagedRuntimeInitialContextPackage, String> {
    if !package.digest_matches() {
        return Err("Product initial-context package digest is invalid".to_owned());
    }
    let schema_version = u32::try_from(package.schema_version)
        .map_err(|_| "initial-context schema version exceeds Runtime contract".to_owned())?;
    let package_id = RuntimeContextPackageId::new(package.package_id.to_string())
        .map_err(|error| error.to_string())?;
    let mut contributions = Vec::with_capacity(package.contributions.len());
    for (index, contribution) in package.contributions.iter().enumerate() {
        let contribution_id = RuntimeContextContributionId::new(format!(
            "{}:{}:{index}",
            package.package_id,
            contribution.kind_name()
        ))
        .map_err(|error| error.to_string())?;
        let content = compile_runtime_contribution(contribution)?;
        let mut contribution = ManagedRuntimeInitialContextContribution {
            contribution_id,
            digest: RuntimePayloadDigest::new("pending").map_err(|error| error.to_string())?,
            content,
        };
        contribution.digest = contribution.calculated_digest();
        contributions.push(contribution);
    }
    let mut runtime_package = ManagedRuntimeInitialContextPackage {
        package_id,
        schema_version,
        mode: match package.mode {
            CompiledFreshContextMode::Compact => ManagedRuntimeInitialContextMode::Compact,
            CompiledFreshContextMode::WorkflowOnly => {
                ManagedRuntimeInitialContextMode::WorkflowOnly
            }
            CompiledFreshContextMode::ConstraintsOnly => {
                ManagedRuntimeInitialContextMode::ConstraintsOnly
            }
        },
        contributions,
        digest: RuntimePayloadDigest::new("pending").map_err(|error| error.to_string())?,
    };
    runtime_package.digest = runtime_package.calculated_digest();
    if !runtime_package.validate() {
        return Err("compiled Runtime initial-context package is invalid".to_owned());
    }
    Ok(runtime_package)
}

fn compile_runtime_contribution(
    contribution: &CompiledInitialContextContribution,
) -> Result<ManagedRuntimeInitialContextContributionContent, String> {
    Ok(match contribution {
        CompiledInitialContextContribution::CompactSummary {
            summary,
            provenance,
        } => ManagedRuntimeInitialContextContributionContent::CompactSummary {
            summary: summary.clone(),
            provenance: compile_runtime_provenance(provenance)?,
        },
        CompiledInitialContextContribution::WorkflowContext {
            payload,
            provenance,
        } => ManagedRuntimeInitialContextContributionContent::WorkflowContext {
            schema: payload.schema.clone(),
            value: payload.value.clone(),
            provenance: compile_runtime_provenance(provenance)?,
        },
        CompiledInitialContextContribution::ConstraintSet {
            payload,
            provenance,
        } => ManagedRuntimeInitialContextContributionContent::ConstraintSet {
            schema: payload.schema.clone(),
            value: payload.value.clone(),
            provenance: compile_runtime_provenance(provenance)?,
        },
    })
}

fn compile_runtime_provenance(
    provenance: &super::CompiledContextProvenance,
) -> Result<ManagedRuntimeContextProvenance, String> {
    Ok(ManagedRuntimeContextProvenance {
        authority: match provenance.authority {
            CompiledContextAuthority::AgentHistory => ManagedRuntimeContextAuthority::AgentHistory,
            CompiledContextAuthority::AgentSnapshot => {
                ManagedRuntimeContextAuthority::AgentSnapshot
            }
            CompiledContextAuthority::Workflow => ManagedRuntimeContextAuthority::Workflow,
            CompiledContextAuthority::Constraint => ManagedRuntimeContextAuthority::Constraint,
        },
        source: RuntimeContextSourceRef::new(provenance.source.clone())
            .map_err(|error| error.to_string())?,
        revision: RuntimeContextSourceRevision::new(provenance.revision.clone())
            .map_err(|error| error.to_string())?,
        digest: RuntimePayloadDigest::new(provenance.digest.clone())
            .map_err(|error| error.to_string())?,
    })
}

fn map_initial_context_evidence(
    product_package: &CompiledInitialContextPackage,
    evidence: &ManagedRuntimeAppliedInitialContextEvidence,
) -> Result<CompiledContextApplication, String> {
    let runtime_package = compile_runtime_initial_context(product_package)?;
    if evidence.package_id != runtime_package.package_id
        || evidence.package_digest != runtime_package.digest
        || evidence.contributions.len() != runtime_package.contributions.len()
    {
        return Err("Runtime initial-context package evidence drifted".to_owned());
    }

    let mut contribution_fidelity = Vec::with_capacity(evidence.contributions.len());
    let mut all_typed_native = true;
    let mut renderer_version: Option<String> = None;
    let mut materialized_digests = Vec::with_capacity(evidence.contributions.len());
    for (expected, actual) in runtime_package
        .contributions
        .iter()
        .zip(&evidence.contributions)
    {
        let (kind, provenance) = contribution_contract(&expected.content);
        if actual.contribution_id != expected.contribution_id
            || actual.kind != kind
            || actual.contribution_digest != expected.digest
            || actual.provenance.authority != provenance.authority
            || actual.provenance.source != provenance.source
            || actual.provenance.revision != provenance.revision
            || actual.provenance.digest != provenance.digest
        {
            return Err("Runtime contribution application evidence drifted".to_owned());
        }
        let fidelity = match &actual.fidelity {
            ManagedRuntimeInitialContextAppliedFidelity::TypedNative { applied_digest } => {
                materialized_digests.push(applied_digest.as_str().to_owned());
                CompiledContextDeliveryFidelity::TypedNative
            }
            ManagedRuntimeInitialContextAppliedFidelity::CanonicalRendered {
                renderer_version: actual_renderer,
                rendered_digest,
            } => {
                all_typed_native = false;
                if let Some(expected_renderer) = &renderer_version
                    && expected_renderer != actual_renderer
                {
                    return Err(
                        "Runtime used different renderers within one context package".to_owned(),
                    );
                }
                renderer_version = Some(actual_renderer.clone());
                materialized_digests.push(rendered_digest.as_str().to_owned());
                CompiledContextDeliveryFidelity::CanonicalRendered
            }
        };
        contribution_fidelity.push(CompiledContextContributionApplication {
            kind: contribution_kind_name(kind).to_owned(),
            fidelity,
        });
    }
    let materialized_digest = if materialized_digests.len() == 1 {
        materialized_digests.pop()
    } else {
        let canonical = serde_json::to_vec(&materialized_digests)
            .map_err(|error| format!("failed to digest context evidence: {error}"))?;
        Some(format!("sha256:{:x}", Sha256::digest(canonical)))
    };
    Ok(CompiledContextApplication {
        package_id: product_package.package_id,
        package_digest: product_package.digest.clone(),
        fidelity: if all_typed_native {
            CompiledContextDeliveryFidelity::TypedNative
        } else {
            CompiledContextDeliveryFidelity::CanonicalRendered
        },
        contribution_fidelity,
        renderer_version,
        materialized_digest,
    })
}

fn contribution_contract(
    content: &ManagedRuntimeInitialContextContributionContent,
) -> (
    ManagedRuntimeInitialContextContributionKind,
    &ManagedRuntimeContextProvenance,
) {
    match content {
        ManagedRuntimeInitialContextContributionContent::CompactSummary { provenance, .. } => (
            ManagedRuntimeInitialContextContributionKind::CompactSummary,
            provenance,
        ),
        ManagedRuntimeInitialContextContributionContent::WorkflowContext { provenance, .. } => (
            ManagedRuntimeInitialContextContributionKind::WorkflowContext,
            provenance,
        ),
        ManagedRuntimeInitialContextContributionContent::ConstraintSet { provenance, .. } => (
            ManagedRuntimeInitialContextContributionKind::ConstraintSet,
            provenance,
        ),
    }
}

fn contribution_kind_name(kind: ManagedRuntimeInitialContextContributionKind) -> &'static str {
    match kind {
        ManagedRuntimeInitialContextContributionKind::CompactSummary => "compact_summary",
        ManagedRuntimeInitialContextContributionKind::WorkflowContext => "workflow_context",
        ManagedRuntimeInitialContextContributionKind::ConstraintSet => "constraint_set",
    }
}

#[derive(Debug, Error)]
enum ProductAgentRunForkGraphError {
    #[error("parent LifecycleRun {0} was not found")]
    ParentRunNotFound(uuid::Uuid),
    #[error("parent LifecycleAgent {0} was not found")]
    ParentAgentNotFound(uuid::Uuid),
    #[error("parent AgentFrame for agent {0} was not found")]
    ParentFrameNotFound(uuid::Uuid),
    #[error("parent Product graph is inconsistent: {0}")]
    InconsistentParent(&'static str),
    #[error("Product repository failed while reading {entity}: {reason}")]
    Repository {
        entity: &'static str,
        reason: String,
    },
    #[error("prepared Product graph is invalid: {0}")]
    InvalidPreparedGraph(String),
}

/// Product-owned production adapter that prepares the immutable child graph for a fork.
///
/// This adapter is deliberately read-only. The prepared graph is published only by
/// `AgentRunForkSagaRepository::commit_product_graph`, which owns the atomic graph + saga
/// transition required by the fork protocol.
pub struct ProductAgentRunForkGraphAdapter {
    runs: Arc<dyn LifecycleRunRepository>,
    agents: Arc<dyn LifecycleAgentRepository>,
    frames: Arc<dyn AgentFrameRepository>,
    frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
}

impl ProductAgentRunForkGraphAdapter {
    pub fn new(
        runs: Arc<dyn LifecycleRunRepository>,
        agents: Arc<dyn LifecycleAgentRepository>,
        frames: Arc<dyn AgentFrameRepository>,
        frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    ) -> Self {
        Self {
            runs,
            agents,
            frames,
            frame_construction,
        }
    }

    async fn prepare(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<PreparedAgentRunForkGraph, ProductAgentRunForkGraphError> {
        let parent = saga.parent();
        let parent_run = self
            .runs
            .get_by_id(parent.run_id)
            .await
            .map_err(|error| ProductAgentRunForkGraphError::Repository {
                entity: "LifecycleRun",
                reason: error.to_string(),
            })?
            .ok_or(ProductAgentRunForkGraphError::ParentRunNotFound(
                parent.run_id,
            ))?;
        let parent_agent = self
            .agents
            .get(parent.agent_id)
            .await
            .map_err(|error| ProductAgentRunForkGraphError::Repository {
                entity: "LifecycleAgent",
                reason: error.to_string(),
            })?
            .ok_or(ProductAgentRunForkGraphError::ParentAgentNotFound(
                parent.agent_id,
            ))?;
        let parent_frame = self
            .frames
            .get_latest(parent.agent_id)
            .await
            .map_err(|error| ProductAgentRunForkGraphError::Repository {
                entity: "AgentFrame",
                reason: error.to_string(),
            })?
            .ok_or(ProductAgentRunForkGraphError::ParentFrameNotFound(
                parent.agent_id,
            ))?;

        if parent_agent.run_id != parent_run.id {
            return Err(ProductAgentRunForkGraphError::InconsistentParent(
                "LifecycleAgent does not belong to LifecycleRun",
            ));
        }
        if parent_agent.project_id != parent_run.project_id {
            return Err(ProductAgentRunForkGraphError::InconsistentParent(
                "LifecycleAgent and LifecycleRun have different projects",
            ));
        }
        if parent_frame.agent_id != parent_agent.id {
            return Err(ProductAgentRunForkGraphError::InconsistentParent(
                "AgentFrame does not belong to LifecycleAgent",
            ));
        }

        let child = saga.child();
        let mut child_run = LifecycleRun::new_plain_for_user(
            parent_run.project_id,
            parent_run.created_by_user_id.clone(),
        );
        child_run.id = child.run_id;
        // The saga currently owns stable identities but not a separate requested-at timestamp.
        // Reusing the immutable parent graph timestamp keeps retry preparation byte-identical.
        child_run.created_at = parent_run.last_activity_at;
        child_run.updated_at = parent_run.last_activity_at;
        child_run.last_activity_at = parent_run.last_activity_at;

        let mut child_agent = LifecycleAgent::new_root_for_user(
            child.run_id,
            parent_run.project_id,
            parent_agent.source,
            parent_agent.created_by_user_id.clone(),
        );
        child_agent.id = child.agent_id;
        child_agent.project_agent_id = saga
            .child_product_selection()
            .map(|selection| selection.project_agent_id)
            .or(parent_agent.project_agent_id);
        child_agent.bootstrap_status = parent_agent.bootstrap_status.clone();
        child_agent.workspace_title = parent_agent.workspace_title.clone();
        child_agent.workspace_title_source = parent_agent
            .workspace_title
            .as_ref()
            .map(|_| "source".to_owned());
        child_agent.created_at = parent_agent.updated_at;
        child_agent.updated_at = parent_agent.updated_at;

        let mut child_frame = if saga.child_product_selection().is_some() {
            agentdash_domain::workflow::AgentFrame::new_revision(
                child.agent_id,
                1,
                "agent_run_fork_selection_pending",
            )
        } else {
            let mut inherited = parent_frame.clone();
            inherited.agent_id = child.agent_id;
            inherited.revision = 1;
            inherited.created_by_kind = "agent_run_fork_product_protocol".to_owned();
            inherited
        };
        child_frame.id = child.frame_id;
        child_frame.created_by_id = Some(parent_agent.created_by_user_id.clone());
        child_frame.created_at = parent_frame.created_at;

        let mut lineage = AgentRunLineage::new_fork(
            parent.run_id,
            parent.agent_id,
            child.run_id,
            child.agent_id,
            None,
            Some(json!({
                "kind": "completed_turn",
                "runtime_thread_id": parent.runtime_thread_id,
                "turn_id": parent.through_turn_id,
            })),
            parent_agent.created_by_user_id.clone(),
            Some(json!({
                "agent_run_id": child.agent_run_id,
                "runtime_thread_id": child.runtime_thread_id,
            })),
        )
        .with_frame_baseline(
            parent_frame.id,
            parent_frame.revision,
            child.frame_id,
            child_frame.revision,
        );
        lineage.id = saga.request_id().0;
        lineage.created_at = parent_frame.created_at;

        PreparedAgentRunForkGraph::prepare(
            saga,
            AgentRunForkGraph {
                child_run,
                child_agent,
                child_frame,
                lineage,
            },
        )
        .map_err(|error| ProductAgentRunForkGraphError::InvalidPreparedGraph(error.to_string()))
    }

    fn selected_provisioning(
        saga: &AgentRunForkSaga,
        selection: &AgentRunForkChildProductSelection,
        frame: &agentdash_domain::workflow::AgentFrame,
    ) -> Result<AgentRunProductRuntimeProvisioningRequest, String> {
        let child = saga.child();
        if frame.agent_id != child.agent_id
            || frame.created_by_kind == "agent_run_fork_selection_pending"
            || frame.execution_profile_json.as_ref()
                != Some(&selection.execution_profile.configuration)
        {
            return Err("materialized selected child frame evidence drifted".to_owned());
        }
        let revision = u64::try_from(frame.revision)
            .map_err(|_| "materialized child frame revision is invalid".to_owned())?;
        Ok(AgentRunProductRuntimeProvisioningRequest {
            target: agentdash_domain::agent_run_target::AgentRunTarget {
                run_id: child.run_id,
                agent_id: child.agent_id,
            },
            runtime_thread_id: child.runtime_thread_id.clone(),
            idempotency_key: selection.idempotency_key.clone(),
            frame: ProductAgentFrameRef {
                frame_id: frame.id,
                agent_id: frame.agent_id,
                revision,
            },
            execution_profile: selection.execution_profile.clone(),
            surface_facts: ProductAgentSurfaceFacts::from_frame(frame),
        })
    }
}

#[async_trait]
impl AgentRunForkProductGraphPort for ProductAgentRunForkGraphAdapter {
    async fn prepare_child_graph_commit(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<PreparedAgentRunForkGraph, String> {
        self.prepare(saga).await.map_err(|error| error.to_string())
    }

    async fn materialize_child_product_selection(
        &self,
        saga: &AgentRunForkSaga,
    ) -> Result<AgentRunProductRuntimeProvisioningRequest, String> {
        let selection = saga
            .child_product_selection()
            .ok_or_else(|| "fork saga has no selected child Product intent".to_owned())?;
        let child = saga.child();
        let child_agent = self
            .agents
            .get(child.agent_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "fork child LifecycleAgent is not committed".to_owned())?;
        if child_agent.project_agent_id != Some(selection.project_agent_id) {
            return Err("fork child selected ProjectAgent drifted".to_owned());
        }
        if let Some(existing) = self
            .frames
            .get_latest(child.agent_id)
            .await
            .map_err(|error| error.to_string())?
            .filter(|frame| frame.created_by_kind != "agent_run_fork_selection_pending")
        {
            return Self::selected_provisioning(saga, selection, &existing);
        }
        let outcome = self
            .frame_construction
            .execute_frame_construction_command(FrameConstructionCommand::DispatchLaunchAnchor {
                run_id: child.run_id,
                agent_id: child.agent_id,
                subject_ref: None,
                runtime_thread_id: Some(child.runtime_thread_id.to_string()),
                created_by_id: Some(child_agent.created_by_user_id.clone()),
                execution_profile: Some(selection.execution_profile.configuration.clone()),
            })
            .await
            .map_err(|error| error.to_string())?;
        let frame_id = outcome.frame_id.ok_or_else(|| {
            "selected child frame construction returned no frame identity".to_owned()
        })?;
        let frame = self
            .frames
            .get(frame_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "selected child frame was not durably materialized".to_owned())?;
        Self::selected_provisioning(saga, selection, &frame)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque};

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeAppliedContextProvenance, ManagedRuntimeChangeDelta, ManagedRuntimeChangeGap,
        ManagedRuntimeChangePage, ManagedRuntimeGatewayError,
        ManagedRuntimeInitialContextContributionEvidence, ManagedRuntimeLifecycleStatus,
        ManagedRuntimeOperation, ManagedRuntimeOperationEvidence, ManagedRuntimeOperationReceipt,
        ManagedRuntimePlatformChange, ManagedRuntimeProjectionAuthority,
        ManagedRuntimeProjectionFidelity, ManagedRuntimeSourceBindingEvidence,
        RuntimeChangeSequence, RuntimeSourceRef, SurfaceRevision,
    };
    use agentdash_domain::workflow::{AgentFrame, AgentSource, LifecycleAgent, LifecycleRun};
    use agentdash_test_support::workflow::{
        MemoryAgentFrameRepository, MemoryLifecycleAgentRepository, MemoryLifecycleRunRepository,
    };

    struct UnusedFrameConstruction;

    #[async_trait]
    impl AgentRunFrameConstructionPort for UnusedFrameConstruction {
        async fn execute_frame_construction_command(
            &self,
            _command: FrameConstructionCommand,
        ) -> Result<
            agentdash_application_ports::agent_frame_materialization::AgentRunFrameSurfaceCommandOutcome,
            agentdash_application_ports::agent_frame_materialization::AgentRunFrameSurfaceError,
        >{
            Err(
                agentdash_application_ports::agent_frame_materialization::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: "unused in graph preparation test".to_owned(),
                },
            )
        }
    }
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::*;
    use crate::agent_run::product_protocol::{
        AcceptedRuntimeOperation, AgentRunForkParent, AgentRunForkRequestId, AgentRunForkSagaStep,
        CompanionAdoptionMode, CompanionContextMode, CompanionContextSourceDraft,
        CompanionContextSources, CompanionFreshRequestId, CompanionFreshStableIdentities,
        CompanionFreshStep, PreallocatedAgentRunChild, RuntimeForkPhaseEvidence,
        RuntimeOperationOutcome, SubmitInput, compile_companion_dispatch_target,
    };

    #[derive(Default)]
    struct RecordingManagedRuntimeGateway {
        commands: Mutex<Vec<ManagedRuntimeCommandEnvelope>>,
        receipts: Mutex<VecDeque<ManagedRuntimeOperationReceipt>>,
        snapshots: Mutex<VecDeque<ManagedRuntimeSnapshot>>,
        change_pages: Mutex<VecDeque<ManagedRuntimeChangePage>>,
        change_requests: Mutex<Vec<ManagedRuntimeChangesRequest>>,
    }

    impl RecordingManagedRuntimeGateway {
        async fn commands(&self) -> Vec<ManagedRuntimeCommandEnvelope> {
            self.commands.lock().await.clone()
        }

        async fn change_requests(&self) -> Vec<ManagedRuntimeChangesRequest> {
            self.change_requests.lock().await.clone()
        }
    }

    #[async_trait]
    impl ManagedAgentRuntimeGateway for RecordingManagedRuntimeGateway {
        async fn execute(
            &self,
            command: ManagedRuntimeCommandEnvelope,
        ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
            self.commands.lock().await.push(command);
            self.receipts.lock().await.pop_front().ok_or_else(|| {
                ManagedRuntimeGatewayError::Invalid {
                    reason: "missing recorded receipt".to_owned(),
                }
            })
        }

        async fn read(
            &self,
            _request: ManagedRuntimeReadRequest,
        ) -> Result<ManagedRuntimeSnapshot, ManagedRuntimeGatewayError> {
            self.snapshots
                .lock()
                .await
                .pop_front()
                .ok_or(ManagedRuntimeGatewayError::NotFound)
        }

        async fn changes(
            &self,
            request: ManagedRuntimeChangesRequest,
        ) -> Result<ManagedRuntimeChangePage, ManagedRuntimeGatewayError> {
            self.change_requests.lock().await.push(request);
            self.change_pages.lock().await.pop_front().ok_or_else(|| {
                ManagedRuntimeGatewayError::Invalid {
                    reason: "missing recorded change page".to_owned(),
                }
            })
        }
    }

    fn binding(activated_at_revision: Option<u64>) -> ManagedRuntimeSourceBindingEvidence {
        ManagedRuntimeSourceBindingEvidence {
            source_ref: RuntimeSourceRef::new("source:native-child").expect("source"),
            committed_at_revision: RuntimeProjectionRevision(2),
            applied_surface_revision: SurfaceRevision(3),
            activated_at_revision: activated_at_revision.map(RuntimeProjectionRevision),
        }
    }

    fn snapshot_with_operation(
        thread_id: RuntimeThreadId,
        revision: u64,
        operation: ManagedRuntimeOperation,
    ) -> ManagedRuntimeSnapshot {
        ManagedRuntimeSnapshot {
            thread_id,
            revision: RuntimeProjectionRevision(revision),
            latest_change_sequence: RuntimeChangeSequence(revision),
            captured_at_ms: 1,
            lifecycle:
                agentdash_agent_runtime_contract::ManagedRuntimeLifecycleStatus::Provisioning,
            active_turn_id: None,
            turns: Vec::new(),
            items: Vec::new(),
            interactions: Vec::new(),
            conversation_history: Vec::new(),
            thread_name: None,
            thread_name_source: None,
            operations: vec![operation],
            source_binding: None,
            authority: ManagedRuntimeProjectionAuthority::SourceAuthoritative,
            fidelity: ManagedRuntimeProjectionFidelity::Exact,
            command_availability: BTreeMap::new(),
        }
    }

    fn requested_fork_saga() -> AgentRunForkSaga {
        AgentRunForkSaga::requested(
            AgentRunForkRequestId(Uuid::new_v4()),
            AgentRunForkParent {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                runtime_thread_id: RuntimeThreadId::new("runtime-parent").expect("parent"),
                through_turn_id: agentdash_agent_runtime_contract::RuntimeTurnId::new("turn-9")
                    .expect("turn"),
            },
            PreallocatedAgentRunChild {
                agent_run_id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                frame_id: Uuid::new_v4(),
                runtime_thread_id: RuntimeThreadId::new("runtime-child").expect("child"),
            },
        )
    }

    fn fresh_saga() -> CompanionFreshSaga {
        let plan = compile_companion_dispatch_target(
            CompanionContextMode::Compact,
            CompanionAdoptionMode::Suggestion,
            SubmitInput {
                text: "first exact task".to_owned(),
            },
            CompanionContextSources {
                parent_runtime_thread_id: RuntimeThreadId::new("runtime-parent").expect("parent"),
                through_turn_id: None,
                package_id: Uuid::new_v4(),
                compact_summary: Some((
                    "typed compact history".to_owned(),
                    CompanionContextSourceDraft {
                        authority: CompiledContextAuthority::AgentHistory,
                        source_coordinate: "history:parent".to_owned(),
                        source_revision: "turn:9".to_owned(),
                        source_digest: "sha256:source-history".to_owned(),
                    },
                )),
                workflow: None,
                constraints: None,
                surface_facts: json!({"surface": "separate"}),
            },
        )
        .expect("fresh plan");
        CompanionFreshSaga::requested(
            CompanionFreshStableIdentities {
                request_id: CompanionFreshRequestId(Uuid::new_v4()),
                runtime_thread_id: RuntimeThreadId::new("runtime-fresh-child").expect("child"),
                create_effect_id: Uuid::new_v4(),
                activation_effect_id: Uuid::new_v4(),
                first_input_effect_id: Uuid::new_v4(),
            },
            plan,
        )
        .expect("fresh saga")
    }

    fn runtime_context_evidence(
        product_package: &CompiledInitialContextPackage,
    ) -> ManagedRuntimeAppliedInitialContextEvidence {
        let runtime_package =
            compile_runtime_initial_context(product_package).expect("compile Runtime package");
        ManagedRuntimeAppliedInitialContextEvidence {
            package_id: runtime_package.package_id,
            package_digest: runtime_package.digest,
            contributions: runtime_package
                .contributions
                .into_iter()
                .map(|contribution| {
                    let (kind, provenance) = contribution_contract(&contribution.content);
                    ManagedRuntimeInitialContextContributionEvidence {
                        contribution_id: contribution.contribution_id,
                        kind,
                        contribution_digest: contribution.digest.clone(),
                        provenance: ManagedRuntimeAppliedContextProvenance {
                            authority: provenance.authority,
                            source: provenance.source.clone(),
                            revision: provenance.revision.clone(),
                            digest: provenance.digest.clone(),
                        },
                        fidelity: ManagedRuntimeInitialContextAppliedFidelity::TypedNative {
                            applied_digest: contribution.digest,
                        },
                    }
                })
                .collect(),
        }
    }

    async fn fixture() -> (
        ProductAgentRunForkGraphAdapter,
        AgentRunForkSaga,
        Arc<MemoryLifecycleRunRepository>,
        Arc<MemoryLifecycleAgentRepository>,
        Arc<MemoryAgentFrameRepository>,
    ) {
        let runs = Arc::new(MemoryLifecycleRunRepository::default());
        let agents = Arc::new(MemoryLifecycleAgentRepository::default());
        let frames = Arc::new(MemoryAgentFrameRepository::default());
        let parent_run = LifecycleRun::new_plain_for_user(Uuid::new_v4(), "user-1");
        let parent_agent = LifecycleAgent::new_root_for_user(
            parent_run.id,
            parent_run.project_id,
            AgentSource::ProjectAgent,
            "user-1",
        );
        let mut parent_frame = AgentFrame::new_initial(parent_agent.id);
        parent_frame.execution_profile_json = Some(json!({"model": "test"}));
        runs.create(&parent_run).await.expect("store parent run");
        agents
            .create(&parent_agent)
            .await
            .expect("store parent agent");
        frames
            .create(&parent_frame)
            .await
            .expect("store parent frame");

        let mut saga = AgentRunForkSaga::requested(
            AgentRunForkRequestId(Uuid::new_v4()),
            AgentRunForkParent {
                run_id: parent_run.id,
                agent_id: parent_agent.id,
                runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new(
                    "runtime-parent",
                )
                .expect("parent thread"),
                through_turn_id: agentdash_agent_runtime_contract::RuntimeTurnId::new("turn-7")
                    .expect("turn id"),
            },
            PreallocatedAgentRunChild {
                agent_run_id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                frame_id: Uuid::new_v4(),
                runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new(
                    "runtime-child",
                )
                .expect("child thread"),
            },
        );
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("fork dispatch");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("mark dispatch");
        let receipt = AcceptedRuntimeOperation {
            operation_id: identity.runtime_operation_id.clone(),
            accepted_revision: agentdash_agent_runtime_contract::RuntimeProjectionRevision(2),
        };
        saga.record_runtime_outcome(
            identity.clone(),
            RuntimeOperationOutcome::Accepted(receipt.clone()),
        )
        .expect("record admission");
        let child_thread_id = saga.child().runtime_thread_id.clone();
        saga.record_runtime_outcome(
            identity,
            RuntimeOperationOutcome::Applied(RuntimeForkPhaseEvidence::ForkProvisioned {
                child_thread_id,
                child_binding: binding(None),
                child_history_digest: agentdash_agent_runtime_contract::RuntimePayloadDigest::new(
                    "sha256:history",
                )
                .expect("history digest"),
                context: None,
                receipt,
            }),
        )
        .expect("record fork provisioning");

        (
            ProductAgentRunForkGraphAdapter::new(
                runs.clone(),
                agents.clone(),
                frames.clone(),
                Arc::new(UnusedFrameConstruction),
            ),
            saga,
            runs,
            agents,
            frames,
        )
    }

    #[tokio::test]
    async fn prepares_complete_stable_graph_without_publishing_rows() {
        let (adapter, saga, runs, agents, frames) = fixture().await;

        let first = adapter
            .prepare_child_graph_commit(&saga)
            .await
            .expect("prepare first graph");
        let second = adapter
            .prepare_child_graph_commit(&saga)
            .await
            .expect("prepare retry graph");

        assert_eq!(first.payload_digest(), second.payload_digest());
        assert_eq!(first.agent_run_id(), saga.child().agent_run_id);
        assert_eq!(first.runtime_thread_id().as_str(), "runtime-child");
        assert_eq!(
            first.child_binding(),
            saga.child_binding().expect("binding")
        );
        let graph = first.graph();
        assert_eq!(graph.child_run.id, saga.child().run_id);
        assert_eq!(graph.child_agent.id, saga.child().agent_id);
        assert_eq!(graph.child_frame.id, saga.child().frame_id);
        assert_eq!(
            graph.child_frame.execution_profile_json,
            Some(json!({"model": "test"}))
        );
        assert_eq!(graph.lineage.parent_run_id, saga.parent().run_id);
        assert_eq!(graph.lineage.child_frame_id, Some(saga.child().frame_id));

        assert!(runs.get_by_id(saga.child().run_id).await.unwrap().is_none());
        assert!(agents.get(saga.child().agent_id).await.unwrap().is_none());
        assert!(frames.get(saga.child().frame_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fork_retries_send_one_byte_identical_typed_envelope_and_keep_operation_identity() {
        let mut saga = requested_fork_saga();
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("fork dispatch");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch");
        let gateway = Arc::new(RecordingManagedRuntimeGateway::default());
        {
            let mut receipts = gateway.receipts.lock().await;
            for duplicate in [false, true] {
                receipts.push_back(ManagedRuntimeOperationReceipt {
                    operation_id: identity.runtime_operation_id.clone(),
                    thread_id: saga.parent().runtime_thread_id.clone(),
                    accepted_revision: RuntimeProjectionRevision(4),
                    status: ManagedRuntimeOperationStatus::Accepted,
                    evidence: None,
                    duplicate,
                });
            }
        }
        let adapter = ProductAgentRunForkRuntimeAdapter::new(gateway.clone());

        let first = adapter.execute(&saga, &identity).await.expect("first");
        let duplicate = adapter.execute(&saga, &identity).await.expect("duplicate");

        assert_eq!(first, duplicate);
        let commands = gateway.commands().await;
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0], commands[1]);
        assert_eq!(commands[0].operation_id, identity.runtime_operation_id);
        assert_eq!(commands[0].thread_id, saga.parent().runtime_thread_id);
        assert!(matches!(
            &commands[0].command,
            ManagedRuntimeCommand::Fork {
                child_thread_id,
                through_completed_turn_id: Some(turn_id),
            } if child_thread_id == &saga.child().runtime_thread_id
                && turn_id == &saga.parent().through_turn_id
        ));
    }

    #[tokio::test]
    async fn fork_restart_inspects_known_child_then_exact_provisioned_child_by_same_operation_id() {
        let mut saga = requested_fork_saga();
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("fork dispatch");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch");
        let cutoff = ManagedRuntimeForkCutoff::CompletedTurn {
            turn_id: saga.parent().through_turn_id.clone(),
        };
        let gateway = Arc::new(RecordingManagedRuntimeGateway::default());
        {
            let mut snapshots = gateway.snapshots.lock().await;
            snapshots.push_back(snapshot_with_operation(
                saga.parent().runtime_thread_id.clone(),
                5,
                ManagedRuntimeOperation {
                    id: identity.runtime_operation_id.clone(),
                    turn_id: None,
                    status: ManagedRuntimeOperationStatus::Running,
                    evidence: Some(ManagedRuntimeOperationEvidence::Fork {
                        parent_binding: binding(None),
                        progress: ManagedRuntimeForkProgressEvidence::ChildKnown {
                            child_thread_id: saga.child().runtime_thread_id.clone(),
                            child_source_ref: RuntimeSourceRef::new("source:native-child")
                                .expect("source"),
                            cutoff: cutoff.clone(),
                            child_history_digest: None,
                        },
                    }),
                },
            ));
            snapshots.push_back(snapshot_with_operation(
                saga.parent().runtime_thread_id.clone(),
                6,
                ManagedRuntimeOperation {
                    id: identity.runtime_operation_id.clone(),
                    turn_id: None,
                    status: ManagedRuntimeOperationStatus::Succeeded,
                    evidence: Some(ManagedRuntimeOperationEvidence::Fork {
                        parent_binding: binding(None),
                        progress: ManagedRuntimeForkProgressEvidence::Provisioned {
                            child_thread_id: saga.child().runtime_thread_id.clone(),
                            child_binding: binding(None),
                            cutoff,
                            child_history_digest: RuntimePayloadDigest::new(
                                "sha256:exact-native-history",
                            )
                            .expect("digest"),
                        },
                    }),
                },
            ));
        }
        let adapter = ProductAgentRunForkRuntimeAdapter::new(gateway.clone());

        let known = adapter
            .inspect(&saga, &identity)
            .await
            .expect("known child");
        assert!(matches!(
            &known,
            RuntimeOperationOutcome::ChildKnown(RuntimeForkChildProgress {
                identity: progress_identity,
                child_thread_id,
                child_source_ref,
                child_history_digest: None,
                receipt: AcceptedRuntimeOperation {
                    operation_id,
                    accepted_revision: RuntimeProjectionRevision(5),
                },
            }) if progress_identity == &identity
                && child_thread_id == &saga.child().runtime_thread_id
                && child_source_ref.as_str() == "source:native-child"
                && operation_id == &identity.runtime_operation_id
        ));
        saga.record_runtime_outcome(identity.clone(), known)
            .expect("persist child known");
        assert_eq!(
            saga.next_step(),
            AgentRunForkSagaStep::InspectRuntime(identity.clone())
        );
        assert!(matches!(
            adapter.inspect(&saga, &identity).await.expect("provisioned"),
            RuntimeOperationOutcome::Applied(RuntimeForkPhaseEvidence::ForkProvisioned {
                child_thread_id,
                child_history_digest,
                receipt: AcceptedRuntimeOperation {
                    operation_id,
                    accepted_revision: RuntimeProjectionRevision(6),
                },
                ..
            }) if child_thread_id == saga.child().runtime_thread_id
                && child_history_digest.as_str() == "sha256:exact-native-history"
                && operation_id == identity.runtime_operation_id
        ));
        assert!(gateway.commands().await.is_empty());
    }

    #[tokio::test]
    async fn lost_fork_with_child_known_retains_typed_child_progress() {
        let mut saga = requested_fork_saga();
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("fork dispatch");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch");
        let gateway = Arc::new(RecordingManagedRuntimeGateway::default());
        gateway
            .snapshots
            .lock()
            .await
            .push_back(snapshot_with_operation(
                saga.parent().runtime_thread_id.clone(),
                5,
                ManagedRuntimeOperation {
                    id: identity.runtime_operation_id.clone(),
                    turn_id: None,
                    status: ManagedRuntimeOperationStatus::Lost,
                    evidence: Some(ManagedRuntimeOperationEvidence::Fork {
                        parent_binding: binding(None),
                        progress: ManagedRuntimeForkProgressEvidence::ChildKnown {
                            child_thread_id: saga.child().runtime_thread_id.clone(),
                            child_source_ref: RuntimeSourceRef::new("source:native-child")
                                .expect("source"),
                            cutoff: ManagedRuntimeForkCutoff::CompletedTurn {
                                turn_id: saga.parent().through_turn_id.clone(),
                            },
                            child_history_digest: Some(
                                RuntimePayloadDigest::new("sha256:known-history").expect("digest"),
                            ),
                        },
                    }),
                },
            ));
        let adapter = ProductAgentRunForkRuntimeAdapter::new(gateway);

        let outcome = adapter.inspect(&saga, &identity).await.expect("lost");
        assert!(matches!(
            &outcome,
            RuntimeOperationOutcome::Lost {
                known_child: Some(RuntimeForkChildProgress {
                    identity: progress_identity,
                    child_thread_id,
                    child_source_ref,
                    child_history_digest: Some(history),
                    ..
                }),
                ..
            } if progress_identity == &identity
                && child_thread_id == &saga.child().runtime_thread_id
                && child_source_ref.as_str() == "source:native-child"
                && history.as_str() == "sha256:known-history"
        ));
        saga.record_runtime_outcome(identity, outcome)
            .expect("record lost");
        assert!(
            saga.lost()
                .and_then(|lost| lost.known_child.as_ref())
                .is_some()
        );
    }

    #[tokio::test]
    async fn clean_failed_fork_without_known_child_terminalizes_without_lost_upgrade() {
        let mut saga = requested_fork_saga();
        let AgentRunForkSagaStep::DispatchRuntime(identity) = saga.next_step() else {
            panic!("fork dispatch");
        };
        saga.mark_runtime_dispatched(identity.clone())
            .expect("dispatch");
        let gateway = Arc::new(RecordingManagedRuntimeGateway::default());
        gateway
            .receipts
            .lock()
            .await
            .push_back(ManagedRuntimeOperationReceipt {
                operation_id: identity.runtime_operation_id.clone(),
                thread_id: saga.parent().runtime_thread_id.clone(),
                accepted_revision: RuntimeProjectionRevision(2),
                status: ManagedRuntimeOperationStatus::Failed,
                evidence: None,
                duplicate: false,
            });
        let adapter = ProductAgentRunForkRuntimeAdapter::new(gateway);

        let outcome = adapter.execute(&saga, &identity).await.expect("failed");
        assert!(matches!(&outcome, RuntimeOperationOutcome::Failed { .. }));
        saga.record_runtime_outcome(identity, outcome)
            .expect("record clean failure");
        assert!(saga.failure().is_some());
        assert!(saga.lost().is_none());
        assert_eq!(saga.next_step(), AgentRunForkSagaStep::Terminal);
    }

    #[tokio::test]
    async fn fresh_create_sends_typed_package_and_translates_per_contribution_evidence() {
        let saga = fresh_saga();
        let CompanionFreshStep::Dispatch(identity) = saga.next_step() else {
            panic!("fresh create dispatch");
        };
        let product_package =
            ProductCompanionFreshRuntimeAdapter::initial_context(&saga).expect("package");
        let evidence = runtime_context_evidence(product_package);
        let gateway = Arc::new(RecordingManagedRuntimeGateway::default());
        gateway
            .receipts
            .lock()
            .await
            .push_back(ManagedRuntimeOperationReceipt {
                operation_id: identity.runtime_operation_id.clone(),
                thread_id: saga.runtime_thread_id().clone(),
                accepted_revision: RuntimeProjectionRevision(3),
                status: ManagedRuntimeOperationStatus::Succeeded,
                evidence: Some(ManagedRuntimeOperationEvidence::Create {
                    binding: binding(None),
                    initial_context: Some(evidence),
                }),
                duplicate: false,
            });
        let adapter = ProductCompanionFreshRuntimeAdapter::new(gateway.clone());

        let outcome = adapter
            .execute(&saga, &identity)
            .await
            .expect("fresh create");

        assert!(matches!(
            outcome,
            CompanionFreshEffectOutcome::Applied(CompanionFreshEffectEvidence::Created {
                child_runtime_thread_id,
                context: CompiledContextApplication {
                    fidelity: CompiledContextDeliveryFidelity::TypedNative,
                    ..
                },
                receipt: AcceptedRuntimeOperation { operation_id, .. },
                ..
            }) if child_runtime_thread_id == *saga.runtime_thread_id()
                && operation_id == identity.runtime_operation_id
        ));
        let commands = gateway.commands().await;
        let ManagedRuntimeCommand::Create {
            initial_context: Some(runtime_package),
        } = &commands[0].command
        else {
            panic!("typed create package");
        };
        assert!(runtime_package.validate());
        assert_eq!(runtime_package.contributions.len(), 1);
        assert_eq!(
            runtime_package.contributions[0].contribution_id.as_str(),
            format!("{}:compact_summary:0", product_package.package_id)
        );
    }

    #[tokio::test]
    async fn clean_failed_fresh_create_terminalizes_without_lost_upgrade() {
        let mut saga = fresh_saga();
        let CompanionFreshStep::Dispatch(identity) = saga.next_step() else {
            panic!("fresh create");
        };
        saga.mark_dispatched(identity.clone()).expect("dispatch");
        let gateway = Arc::new(RecordingManagedRuntimeGateway::default());
        gateway
            .receipts
            .lock()
            .await
            .push_back(ManagedRuntimeOperationReceipt {
                operation_id: identity.runtime_operation_id.clone(),
                thread_id: saga.runtime_thread_id().clone(),
                accepted_revision: RuntimeProjectionRevision(2),
                status: ManagedRuntimeOperationStatus::Failed,
                evidence: None,
                duplicate: false,
            });
        let adapter = ProductCompanionFreshRuntimeAdapter::new(gateway);

        let outcome = adapter.execute(&saga, &identity).await.expect("failed");
        assert!(matches!(
            &outcome,
            CompanionFreshEffectOutcome::Failed { .. }
        ));
        saga.record_outcome(identity, outcome)
            .expect("record clean failure");
        assert!(saga.failure().is_some());
        assert_eq!(saga.next_step(), CompanionFreshStep::Terminal);
    }

    #[test]
    fn fresh_create_activate_and_first_input_keep_separate_stable_identities_and_exact_text() {
        let mut saga = fresh_saga();
        let product_package =
            ProductCompanionFreshRuntimeAdapter::initial_context(&saga).expect("package");
        let context = map_initial_context_evidence(
            product_package,
            &runtime_context_evidence(product_package),
        )
        .expect("context evidence");

        let CompanionFreshStep::Dispatch(create) = saga.next_step() else {
            panic!("create");
        };
        saga.mark_dispatched(create.clone())
            .expect("dispatch create");
        saga.record_outcome(
            create.clone(),
            CompanionFreshEffectOutcome::Applied(CompanionFreshEffectEvidence::Created {
                child_runtime_thread_id: saga.runtime_thread_id().clone(),
                child_binding: binding(None),
                context,
                receipt: accepted_receipt(
                    &create.runtime_operation_id,
                    RuntimeProjectionRevision(1),
                ),
            }),
        )
        .expect("create applied");

        let CompanionFreshStep::Dispatch(activate) = saga.next_step() else {
            panic!("activate");
        };
        saga.mark_dispatched(activate.clone())
            .expect("dispatch activate");
        saga.record_outcome(
            activate.clone(),
            CompanionFreshEffectOutcome::Applied(CompanionFreshEffectEvidence::Activated {
                child_runtime_thread_id: saga.runtime_thread_id().clone(),
                child_binding: binding(Some(4)),
                receipt: accepted_receipt(
                    &activate.runtime_operation_id,
                    RuntimeProjectionRevision(2),
                ),
            }),
        )
        .expect("activate applied");
        assert_eq!(saga.child_binding().cloned(), Some(binding(Some(4))));

        let CompanionFreshStep::Dispatch(submit) = saga.next_step() else {
            panic!("submit");
        };
        let envelope =
            ProductCompanionFreshRuntimeAdapter::command(&saga, &submit).expect("submit envelope");

        assert_ne!(create.runtime_operation_id, activate.runtime_operation_id);
        assert_ne!(activate.runtime_operation_id, submit.runtime_operation_id);
        assert_ne!(create.effect_id, activate.effect_id);
        assert_ne!(activate.effect_id, submit.effect_id);
        assert_eq!(envelope.operation_id, submit.runtime_operation_id);
        assert_eq!(
            envelope.command,
            ManagedRuntimeCommand::SubmitInput {
                content: vec![ManagedRuntimeContentBlock::Text {
                    text: "first exact task".to_owned(),
                }],
            }
        );
    }

    #[test]
    fn dual_digest_bijection_rejects_missing_duplicate_provenance_content_and_either_digest_drift()
    {
        let saga = fresh_saga();
        let product = ProductCompanionFreshRuntimeAdapter::initial_context(&saga).expect("package");
        let baseline = runtime_context_evidence(product);
        assert!(map_initial_context_evidence(product, &baseline).is_ok());

        let mut missing = baseline.clone();
        missing.contributions.clear();
        assert!(map_initial_context_evidence(product, &missing).is_err());

        let mut duplicate = baseline.clone();
        duplicate
            .contributions
            .push(duplicate.contributions[0].clone());
        assert!(map_initial_context_evidence(product, &duplicate).is_err());

        let mut provenance = baseline.clone();
        provenance.contributions[0].provenance.source =
            RuntimeContextSourceRef::new("history:other").expect("source");
        assert!(map_initial_context_evidence(product, &provenance).is_err());

        let mut contribution_id = baseline.clone();
        contribution_id.contributions[0].contribution_id =
            RuntimeContextContributionId::new("different-stable-id").expect("id");
        assert!(map_initial_context_evidence(product, &contribution_id).is_err());

        let mut contribution_digest = baseline.clone();
        contribution_digest.contributions[0].contribution_digest =
            RuntimePayloadDigest::new("sha256:tampered-contribution").expect("digest");
        assert!(map_initial_context_evidence(product, &contribution_digest).is_err());

        let mut content = product.clone();
        let CompiledInitialContextContribution::CompactSummary { summary, .. } =
            &mut content.contributions[0]
        else {
            panic!("compact summary");
        };
        *summary = "different typed content".to_owned();
        let canonical = serde_json::to_vec(&(
            content.package_id,
            content.schema_version,
            content.mode,
            &content.contributions,
        ))
        .expect("canonical Product package");
        content.digest = format!("sha256:{:x}", Sha256::digest(canonical));
        assert!(content.digest_matches());
        assert!(map_initial_context_evidence(&content, &baseline).is_err());

        let mut product_digest = product.clone();
        product_digest.digest = "sha256:tampered-product".to_owned();
        assert!(map_initial_context_evidence(&product_digest, &baseline).is_err());

        let mut runtime_digest = baseline.clone();
        runtime_digest.package_digest =
            RuntimePayloadDigest::new("sha256:tampered-runtime").expect("digest");
        assert!(map_initial_context_evidence(product, &runtime_digest).is_err());
    }

    #[tokio::test]
    async fn projection_adapter_preserves_typed_gap_and_exact_cursor_request() {
        let thread_id = RuntimeThreadId::new("runtime-projection").expect("thread");
        let gap = ManagedRuntimeChangeGap {
            requested_after: Some(RuntimeChangeSequence(7)),
            earliest_available: RuntimeChangeSequence(11),
            latest_available: RuntimeChangeSequence(15),
            snapshot_revision: RuntimeProjectionRevision(9),
        };
        let page = ManagedRuntimeChangePage {
            thread_id: thread_id.clone(),
            changes: vec![ManagedRuntimePlatformChange {
                thread_id: thread_id.clone(),
                sequence: RuntimeChangeSequence(15),
                revision: RuntimeProjectionRevision(9),
                delta: ManagedRuntimeChangeDelta::RuntimeLifecycleChanged {
                    lifecycle: ManagedRuntimeLifecycleStatus::Active,
                },
            }],
            next: RuntimeChangeSequence(15),
            gap: Some(gap),
        };
        let gateway = Arc::new(RecordingManagedRuntimeGateway::default());
        gateway.change_pages.lock().await.push_back(page.clone());
        let adapter = ProductAgentRunRuntimeProjectionAdapter::new(gateway.clone());

        assert_eq!(
            adapter
                .load_changes(&thread_id, Some(RuntimeChangeSequence(7)))
                .await
                .expect("page"),
            page
        );
        assert_eq!(
            gateway.change_requests().await,
            vec![ManagedRuntimeChangesRequest {
                thread_id,
                after: Some(RuntimeChangeSequence(7)),
                limit: PRODUCT_RUNTIME_CHANGE_PAGE_LIMIT,
            }]
        );
    }

    #[tokio::test]
    async fn projection_adapter_rejects_cross_thread_snapshot_page_and_delta() {
        let requested = RuntimeThreadId::new("runtime-requested").expect("thread");
        let other = RuntimeThreadId::new("runtime-other").expect("thread");
        let gateway = Arc::new(RecordingManagedRuntimeGateway::default());
        gateway
            .snapshots
            .lock()
            .await
            .push_back(snapshot_with_operation(
                other.clone(),
                2,
                ManagedRuntimeOperation {
                    id: RuntimeOperationId::new("operation:other").expect("operation"),
                    turn_id: None,
                    status: ManagedRuntimeOperationStatus::Accepted,
                    evidence: None,
                },
            ));
        {
            let mut pages = gateway.change_pages.lock().await;
            pages.push_back(ManagedRuntimeChangePage {
                thread_id: other.clone(),
                changes: Vec::new(),
                next: RuntimeChangeSequence(0),
                gap: None,
            });
            pages.push_back(ManagedRuntimeChangePage {
                thread_id: requested.clone(),
                changes: vec![ManagedRuntimePlatformChange {
                    thread_id: other,
                    sequence: RuntimeChangeSequence(1),
                    revision: RuntimeProjectionRevision(1),
                    delta: ManagedRuntimeChangeDelta::RuntimeLifecycleChanged {
                        lifecycle: ManagedRuntimeLifecycleStatus::Active,
                    },
                }],
                next: RuntimeChangeSequence(1),
                gap: None,
            });
        }
        let adapter = ProductAgentRunRuntimeProjectionAdapter::new(gateway);

        assert!(adapter.load_snapshot(&requested).await.is_err());
        assert!(adapter.load_changes(&requested, None).await.is_err());
        assert!(adapter.load_changes(&requested, None).await.is_err());
    }
}
