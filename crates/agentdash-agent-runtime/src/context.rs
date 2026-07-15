use agentdash_agent_runtime_contract::{
    ActiveContextHeadView, ContextActivationId, ContextBlock, ContextCandidateId,
    ContextCheckpointId, ContextCheckpointView, ContextCompactionId, ContextCompactionTrigger,
    ContextDigest, ContextFidelity, ContextProvenance, ContextRecipe, ContextRevision,
    ContextSnapshotConsistencyCode, DriverContextRevision, MaterializedContext, RuntimeCommand,
    RuntimeContextView, RuntimeEvent, RuntimeJournalFact, RuntimeOperationId,
    RuntimeOperationTerminal, RuntimePresentationCoordinate, RuntimeThreadId, RuntimeThreadStatus,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ContextActivationOutboxEntry, ManagedAgentRuntime, RuntimeCommit, RuntimeRepository,
    RuntimeStoreError, RuntimeUnitOfWork,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextCheckpoint {
    pub checkpoint_id: ContextCheckpointId,
    pub thread_id: RuntimeThreadId,
    pub revision: ContextRevision,
    pub materialized: MaterializedContext,
}

impl ContextCheckpoint {
    fn view(&self) -> ContextCheckpointView {
        ContextCheckpointView {
            checkpoint_id: self.checkpoint_id.clone(),
            thread_id: self.thread_id.clone(),
            revision: self.revision,
            materialized: self.materialized.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveContextHead {
    pub thread_id: RuntimeThreadId,
    pub checkpoint_id: ContextCheckpointId,
    pub revision: ContextRevision,
    pub digest: ContextDigest,
    pub provenance: ContextProvenance,
    pub fidelity: ContextFidelity,
}

impl ActiveContextHead {
    fn view(&self) -> ActiveContextHeadView {
        ActiveContextHeadView {
            checkpoint_id: self.checkpoint_id.clone(),
            revision: self.revision,
            digest: self.digest.clone(),
            provenance: self.provenance.clone(),
            fidelity: self.fidelity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionTerminal {
    Succeeded,
    Failed { reason: String },
    Lost { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextCandidate {
    pub candidate_id: ContextCandidateId,
    pub compaction_id: ContextCompactionId,
    pub operation_id: RuntimeOperationId,
    pub activation_id: ContextActivationId,
    pub thread_id: RuntimeThreadId,
    pub trigger: ContextCompactionTrigger,
    pub expected_base_checkpoint_id: Option<ContextCheckpointId>,
    pub expected_base_revision: ContextRevision,
    pub checkpoint: ContextCheckpoint,
    pub presentation: crate::CompactionPresentationFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextActivationStatus {
    Prepared,
    Applied {
        digest: ContextDigest,
        driver_context_revision: DriverContextRevision,
    },
    Terminal {
        terminal: CompactionTerminal,
        applied: Option<ContextAppliedState>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAppliedState {
    pub digest: ContextDigest,
    pub driver_context_revision: DriverContextRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextActivation {
    pub activation_id: ContextActivationId,
    pub candidate_id: ContextCandidateId,
    pub compaction_id: ContextCompactionId,
    pub thread_id: RuntimeThreadId,
    pub status: ContextActivationStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextHeadWrite {
    pub expected_revision: Option<ContextRevision>,
    pub head: ActiveContextHead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextPreparationStatus {
    Pending,
    Prepared {
        candidate_id: ContextCandidateId,
        activation_id: ContextActivationId,
    },
    Terminal {
        terminal: RuntimeOperationTerminal,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextPreparationWorkItem {
    pub compaction_id: ContextCompactionId,
    pub operation_id: RuntimeOperationId,
    pub thread_id: RuntimeThreadId,
    pub trigger: ContextCompactionTrigger,
    pub expected_base_checkpoint_id: Option<ContextCheckpointId>,
    pub expected_base_revision: ContextRevision,
    pub status: ContextPreparationStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionPreparation {
    pub candidate_id: ContextCandidateId,
    pub compaction_id: ContextCompactionId,
    pub activation_id: ContextActivationId,
    pub operation_id: RuntimeOperationId,
    pub thread_id: RuntimeThreadId,
    pub trigger: ContextCompactionTrigger,
    pub expected_base_checkpoint_id: Option<ContextCheckpointId>,
    pub expected_base_revision: ContextRevision,
    pub checkpoint_id: ContextCheckpointId,
    pub materialized: MaterializedContext,
    pub presentation: Option<crate::CompactionPresentationFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivationObservation {
    Applied {
        digest: ContextDigest,
        driver_context_revision: DriverContextRevision,
    },
    NotApplied,
    Unverifiable {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ContextRuntimeError {
    #[error("thread was not found")]
    ThreadNotFound,
    #[error("compaction was not found")]
    CompactionNotFound,
    #[error("compaction operation was not accepted or is already terminal")]
    OperationNotActive,
    #[error("thread context base changed")]
    BaseChanged,
    #[error("candidate digest does not match the driver applied digest")]
    DigestMismatch,
    #[error("opaque context cannot become a platform context checkpoint")]
    OpaqueContext,
    #[error("managed compaction did not provide typed presentation facts")]
    MissingCompactionPresentation,
    #[error("context store failed: {0}")]
    Store(#[from] RuntimeStoreError),
    #[error("context transition failed: {0}")]
    Transition(#[from] crate::TransitionError),
    #[error("durable context records are inconsistent: {0:?}")]
    InconsistentStore(ContextSnapshotConsistencyCode),
}

impl<S> ManagedAgentRuntime<S>
where
    S: RuntimeRepository + RuntimeUnitOfWork + 'static,
{
    pub async fn complete_compaction_without_changes(
        &self,
        compaction_id: &ContextCompactionId,
    ) -> Result<(), ContextRuntimeError> {
        let mut work_item = self
            .store()
            .load_context_preparation(compaction_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        if matches!(work_item.status, ContextPreparationStatus::Terminal { .. }) {
            return Ok(());
        }
        if !matches!(work_item.status, ContextPreparationStatus::Pending) {
            return Err(ContextRuntimeError::OperationNotActive);
        }
        let operation = self
            .store()
            .find_operation(&work_item.operation_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::OperationNotActive)?;
        let RuntimeCommand::ContextCompact {
            thread_id,
            compaction_id: operation_compaction_id,
            ..
        } = &operation.command
        else {
            return Err(ContextRuntimeError::OperationNotActive);
        };
        if thread_id != &work_item.thread_id
            || operation_compaction_id != compaction_id
            || operation.thread_id != work_item.thread_id
            || operation.terminal.is_some()
        {
            return Err(ContextRuntimeError::OperationNotActive);
        }
        let mut thread = self
            .store()
            .load_thread(&work_item.thread_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::ThreadNotFound)?;
        let expected_projection_revision = thread.revision;
        let terminal = RuntimeOperationTerminal::Succeeded;
        work_item.status = ContextPreparationStatus::Terminal {
            terminal: terminal.clone(),
        };
        let events = thread
            .append_events([
                RuntimeEvent::ContextCompactionTerminal {
                    compaction_id: compaction_id.clone(),
                    operation_id: work_item.operation_id.clone(),
                    terminal: terminal.clone(),
                    context_revision: thread.context_revision,
                },
                RuntimeEvent::OperationTerminal {
                    operation_id: work_item.operation_id.clone(),
                    terminal: terminal.clone(),
                },
            ])
            .map_err(ContextRuntimeError::Transition)?;
        self.store()
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected_projection_revision),
                projection: thread,
                operation: None,
                operation_terminals: vec![(work_item.operation_id.clone(), terminal)],
                records: crate::internal_journal_records(events)?,
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: vec![work_item],
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await
            .map_err(store_error)
    }

    pub async fn prepare_compaction(
        &self,
        preparation: CompactionPreparation,
    ) -> Result<(), ContextRuntimeError> {
        let presentation = preparation
            .presentation
            .clone()
            .filter(|facts| !facts.summary.trim().is_empty())
            .ok_or(ContextRuntimeError::MissingCompactionPresentation)?;
        let operation = self
            .store()
            .find_operation(&preparation.operation_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::OperationNotActive)?;
        let RuntimeCommand::ContextCompact {
            thread_id,
            compaction_id,
            trigger,
            base_checkpoint_id,
            expected_context_revision,
        } = &operation.command
        else {
            return Err(ContextRuntimeError::OperationNotActive);
        };
        if thread_id != &preparation.thread_id
            || compaction_id != &preparation.compaction_id
            || trigger != &preparation.trigger
            || base_checkpoint_id != &preparation.expected_base_checkpoint_id
            || expected_context_revision != &preparation.expected_base_revision
            || operation.thread_id != preparation.thread_id
        {
            return Err(ContextRuntimeError::BaseChanged);
        }
        let work_item = self
            .store()
            .load_context_preparation(&preparation.compaction_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        if work_item.operation_id != preparation.operation_id
            || work_item.thread_id != preparation.thread_id
            || work_item.trigger != preparation.trigger
            || work_item.expected_base_checkpoint_id != preparation.expected_base_checkpoint_id
            || work_item.expected_base_revision != preparation.expected_base_revision
        {
            return Err(ContextRuntimeError::BaseChanged);
        }
        let checkpoint = ContextCheckpoint {
            checkpoint_id: preparation.checkpoint_id.clone(),
            thread_id: preparation.thread_id.clone(),
            revision: ContextRevision(preparation.expected_base_revision.0 + 1),
            materialized: preparation.materialized.clone(),
        };
        let candidate = ContextCandidate {
            candidate_id: preparation.candidate_id.clone(),
            compaction_id: preparation.compaction_id.clone(),
            operation_id: preparation.operation_id.clone(),
            activation_id: preparation.activation_id.clone(),
            thread_id: preparation.thread_id.clone(),
            trigger: preparation.trigger,
            expected_base_checkpoint_id: preparation.expected_base_checkpoint_id.clone(),
            expected_base_revision: preparation.expected_base_revision,
            checkpoint: checkpoint.clone(),
            presentation,
        };
        if let Some(existing) = self
            .store()
            .load_context_candidate(&preparation.compaction_id)
            .await
            .map_err(store_error)?
        {
            return if existing == candidate {
                Ok(())
            } else {
                Err(ContextRuntimeError::BaseChanged)
            };
        }
        if operation.terminal.is_some() {
            return Err(ContextRuntimeError::OperationNotActive);
        }
        let mut thread = self
            .store()
            .load_thread(&preparation.thread_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::ThreadNotFound)?;
        if thread.active_turn_id.is_some() || thread.status != RuntimeThreadStatus::Active {
            return Err(ContextRuntimeError::OperationNotActive);
        }
        if preparation.materialized.recipe.provenance.settings_revision != thread.settings_revision
            || preparation.materialized.recipe.provenance.tool_set_revision
                != thread.tool_set_revision
        {
            return Err(ContextRuntimeError::BaseChanged);
        }
        if preparation.materialized.fidelity == ContextFidelity::Opaque {
            return Err(ContextRuntimeError::OpaqueContext);
        }
        let head = self
            .store()
            .load_context_head(&preparation.thread_id)
            .await
            .map_err(store_error)?;
        if head.as_ref().map(|head| &head.checkpoint_id)
            != preparation.expected_base_checkpoint_id.as_ref()
            || head
                .as_ref()
                .map_or(ContextRevision(0), |head| head.revision)
                != preparation.expected_base_revision
        {
            return Err(ContextRuntimeError::BaseChanged);
        }
        let expected_projection_revision = thread.revision;
        let prepared_work_item = ContextPreparationWorkItem {
            status: ContextPreparationStatus::Prepared {
                candidate_id: candidate.candidate_id.clone(),
                activation_id: candidate.activation_id.clone(),
            },
            ..work_item
        };
        let activation = ContextActivation {
            activation_id: preparation.activation_id.clone(),
            candidate_id: preparation.candidate_id.clone(),
            compaction_id: preparation.compaction_id.clone(),
            thread_id: preparation.thread_id.clone(),
            status: ContextActivationStatus::Prepared,
        };
        let events = thread
            .append_events([RuntimeEvent::ContextCheckpointPrepared {
                checkpoint_id: checkpoint.checkpoint_id.clone(),
                candidate_id: candidate.candidate_id.clone(),
                compaction_id: candidate.compaction_id.clone(),
            }])
            .map_err(ContextRuntimeError::Transition)?;
        let outbox = ContextActivationOutboxEntry {
            activation_id: activation.activation_id.clone(),
            candidate_id: candidate.candidate_id.clone(),
            compaction_id: candidate.compaction_id.clone(),
            thread_id: candidate.thread_id.clone(),
            binding_id: thread.binding_id.clone(),
            generation: thread.driver_generation,
            checkpoint_id: checkpoint.checkpoint_id.clone(),
            digest: checkpoint.materialized.digest.clone(),
        };
        self.store()
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected_projection_revision),
                projection: thread,
                operation: None,
                operation_terminals: Vec::new(),
                records: crate::internal_journal_records(events)?,
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
                context_activation_outbox: vec![outbox],
                context_preparation_work_items: vec![prepared_work_item],
                context_checkpoints: vec![checkpoint],
                context_candidates: vec![candidate],
                context_activations: vec![activation],
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await
            .map_err(store_error)
    }

    pub async fn recover_compaction(
        &self,
        compaction_id: &ContextCompactionId,
        observation: ActivationObservation,
    ) -> Result<(), ContextRuntimeError> {
        let candidate = self
            .store()
            .load_context_candidate(compaction_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        let thread = self
            .store()
            .load_thread(&candidate.thread_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::ThreadNotFound)?;
        let expected_projection_revision = thread.revision;
        let activation = self
            .store()
            .load_context_activation(&candidate.activation_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        if matches!(activation.status, ContextActivationStatus::Terminal { .. }) {
            return Ok(());
        }

        match observation {
            ActivationObservation::NotApplied => {
                if matches!(activation.status, ContextActivationStatus::Applied { .. }) {
                    return self
                        .desynchronize_compaction(
                            thread,
                            candidate,
                            activation,
                            "driver reported not-applied after an applied acknowledgment"
                                .to_string(),
                        )
                        .await;
                }
                let outbox = activation_outbox(&thread, &candidate, &activation);
                return self
                    .store()
                    .commit(RuntimeCommit {
                        expected_projection_revision: Some(expected_projection_revision),
                        projection: thread,
                        operation: None,
                        operation_terminals: Vec::new(),
                        records: Vec::new(),
                        outbox: Vec::new(),
                        terminal_application_effects: Vec::new(),
                        context_activation_outbox: vec![outbox],
                        context_preparation_work_items: Vec::new(),
                        context_checkpoints: Vec::new(),
                        context_candidates: Vec::new(),
                        context_activations: Vec::new(),
                        context_head: None,
                        hook_plan_binding: None,
                        hook_runs: Vec::new(),
                        hook_effects: Vec::new(),
                        quarantine: Vec::new(),
                    })
                    .await
                    .map_err(store_error);
            }
            ActivationObservation::Unverifiable { reason } => {
                return self
                    .desynchronize_compaction(thread, candidate, activation, reason)
                    .await;
            }
            ActivationObservation::Applied {
                digest,
                driver_context_revision,
            } => {
                self.confirm_compaction_activation(compaction_id, digest, driver_context_revision)
                    .await?;
                self.finalize_compaction(compaction_id).await
            }
        }
    }

    pub async fn confirm_compaction_activation(
        &self,
        compaction_id: &ContextCompactionId,
        digest: ContextDigest,
        driver_context_revision: DriverContextRevision,
    ) -> Result<(), ContextRuntimeError> {
        let candidate = self
            .store()
            .load_context_candidate(compaction_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        let mut activation = self
            .store()
            .load_context_activation(&candidate.activation_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        if let ContextActivationStatus::Terminal { applied, .. } = &activation.status {
            return match applied {
                Some(applied)
                    if applied.digest == digest
                        && applied.driver_context_revision == driver_context_revision =>
                {
                    Ok(())
                }
                _ => Err(ContextRuntimeError::DigestMismatch),
            };
        }
        if digest != candidate.checkpoint.materialized.digest {
            let thread = self
                .store()
                .load_thread(&candidate.thread_id)
                .await
                .map_err(store_error)?
                .ok_or(ContextRuntimeError::ThreadNotFound)?;
            return self
                .desynchronize_compaction(
                    thread,
                    candidate,
                    activation,
                    "driver applied an unexpected context digest".to_string(),
                )
                .await;
        }
        match &activation.status {
            ContextActivationStatus::Applied {
                digest: applied,
                driver_context_revision: applied_revision,
            } if applied == &digest && applied_revision == &driver_context_revision => {
                return Ok(());
            }
            ContextActivationStatus::Terminal { .. } => unreachable!("handled above"),
            ContextActivationStatus::Applied { .. } => {
                let thread = self
                    .store()
                    .load_thread(&candidate.thread_id)
                    .await
                    .map_err(store_error)?
                    .ok_or(ContextRuntimeError::ThreadNotFound)?;
                return self
                    .desynchronize_compaction(
                        thread,
                        candidate,
                        activation,
                        "driver returned conflicting activation acknowledgments".to_string(),
                    )
                    .await;
            }
            ContextActivationStatus::Prepared => {}
        }
        let mut thread = self
            .store()
            .load_thread(&candidate.thread_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::ThreadNotFound)?;
        let expected = thread.revision;
        let applied_driver_context_revision = driver_context_revision.clone();
        activation.status = ContextActivationStatus::Applied {
            digest: digest.clone(),
            driver_context_revision,
        };
        let events = thread
            .append_events([RuntimeEvent::ContextActivationApplied {
                activation_id: activation.activation_id.clone(),
                candidate_id: candidate.candidate_id,
                digest,
                driver_context_revision: applied_driver_context_revision,
            }])
            .map_err(ContextRuntimeError::Transition)?;
        self.store()
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: thread,
                operation: None,
                operation_terminals: Vec::new(),
                records: crate::internal_journal_records(events)?,
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: vec![activation],
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await
            .map_err(store_error)
    }

    pub async fn finalize_compaction(
        &self,
        compaction_id: &ContextCompactionId,
    ) -> Result<(), ContextRuntimeError> {
        let candidate = self
            .store()
            .load_context_candidate(compaction_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        let mut work_item = self
            .store()
            .load_context_preparation(compaction_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        let mut activation = self
            .store()
            .load_context_activation(&candidate.activation_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        let applied = match &activation.status {
            ContextActivationStatus::Applied {
                digest,
                driver_context_revision,
            } => ContextAppliedState {
                digest: digest.clone(),
                driver_context_revision: driver_context_revision.clone(),
            },
            ContextActivationStatus::Prepared => {
                return Err(ContextRuntimeError::OperationNotActive);
            }
            ContextActivationStatus::Terminal { .. } => return Ok(()),
        };
        let mut thread = self
            .store()
            .load_thread(&candidate.thread_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::ThreadNotFound)?;
        let expected_projection_revision = thread.revision;
        let head = self
            .store()
            .load_context_head(&candidate.thread_id)
            .await
            .map_err(store_error)?;
        if head.as_ref().map(|head| &head.checkpoint_id)
            != candidate.expected_base_checkpoint_id.as_ref()
            || head
                .as_ref()
                .map_or(ContextRevision(0), |head| head.revision)
                != candidate.expected_base_revision
        {
            return self
                .desynchronize_compaction(
                    thread,
                    candidate,
                    activation,
                    "active context head changed after driver activation".to_string(),
                )
                .await;
        }
        let new_head = ActiveContextHead {
            thread_id: candidate.thread_id.clone(),
            checkpoint_id: candidate.checkpoint.checkpoint_id.clone(),
            revision: candidate.checkpoint.revision,
            digest: applied.digest.clone(),
            provenance: candidate.checkpoint.materialized.recipe.provenance.clone(),
            fidelity: candidate.checkpoint.materialized.fidelity,
        };
        activation.status = ContextActivationStatus::Terminal {
            terminal: CompactionTerminal::Succeeded,
            applied: Some(applied),
        };
        work_item.status = ContextPreparationStatus::Terminal {
            terminal: RuntimeOperationTerminal::Succeeded,
        };
        let events = thread
            .append_events([
                RuntimeEvent::ContextCheckpointActivated {
                    checkpoint_id: candidate.checkpoint.checkpoint_id.clone(),
                    candidate_id: candidate.candidate_id.clone(),
                    activation_id: activation.activation_id.clone(),
                    compaction_id: candidate.compaction_id.clone(),
                    context_revision: new_head.revision,
                    digest: new_head.digest.clone(),
                },
                RuntimeEvent::ContextCompactionTerminal {
                    compaction_id: candidate.compaction_id.clone(),
                    operation_id: candidate.operation_id.clone(),
                    terminal: RuntimeOperationTerminal::Succeeded,
                    context_revision: new_head.revision,
                },
                RuntimeEvent::OperationTerminal {
                    operation_id: candidate.operation_id.clone(),
                    terminal: RuntimeOperationTerminal::Succeeded,
                },
            ])
            .map_err(ContextRuntimeError::Transition)?;
        thread.active_checkpoint_id = Some(new_head.checkpoint_id.clone());
        thread.context_revision = new_head.revision;
        let mut records = crate::internal_journal_records(events)?;
        if let Some(frame) = compaction_summary_frame(&candidate) {
            records.push(
                thread
                    .append_durable_fact(
                        RuntimeJournalFact::Presentation(
                            agentdash_agent_runtime_contract::ImmutablePresentationEvent::new(
                                agentdash_agent_runtime_contract::PresentationDurability::Durable,
                                agentdash_agent_protocol::BackboneEvent::Platform(
                                    agentdash_agent_protocol::PlatformEvent::ContextFrameChanged(
                                        Box::new(agentdash_agent_protocol::ContextFrameChanged {
                                            frame,
                                        }),
                                    ),
                                ),
                            ),
                        ),
                        crate::model::current_time_ms(),
                        Some(thread.binding_id.clone()),
                        None,
                        RuntimePresentationCoordinate {
                            runtime_turn_id: thread.active_turn_id.clone(),
                            runtime_item_id: None,
                            interaction_id: None,
                            source_thread_id: Some(thread.presentation_thread_id.to_string()),
                            source_turn_id: None,
                            source_item_id: None,
                            source_request_id: Some(candidate.operation_id.to_string()),
                            source_entry_index: Some(0),
                        },
                    )
                    .map_err(ContextRuntimeError::Transition)?,
            );
        }
        self.store()
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected_projection_revision),
                projection: thread,
                operation: None,
                operation_terminals: vec![(
                    candidate.operation_id.clone(),
                    RuntimeOperationTerminal::Succeeded,
                )],
                records,
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: vec![work_item],
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: vec![activation],
                context_head: Some(ContextHeadWrite {
                    expected_revision: head.map(|head| head.revision),
                    head: new_head,
                }),
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await
            .map_err(store_error)
    }

    async fn desynchronize_compaction(
        &self,
        mut thread: crate::RuntimeThreadState,
        candidate: ContextCandidate,
        mut activation: ContextActivation,
        reason: String,
    ) -> Result<(), ContextRuntimeError> {
        let expected = thread.revision;
        let mut work_item = self
            .store()
            .load_context_preparation(&candidate.compaction_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::CompactionNotFound)?;
        thread.status = RuntimeThreadStatus::Desynchronized;
        let applied = match &activation.status {
            ContextActivationStatus::Applied {
                digest,
                driver_context_revision,
            } => Some(ContextAppliedState {
                digest: digest.clone(),
                driver_context_revision: driver_context_revision.clone(),
            }),
            ContextActivationStatus::Prepared => None,
            ContextActivationStatus::Terminal { applied, .. } => applied.clone(),
        };
        activation.status = ContextActivationStatus::Terminal {
            terminal: CompactionTerminal::Lost {
                reason: reason.clone(),
            },
            applied,
        };
        let terminal = RuntimeOperationTerminal::Lost {
            retryable: false,
            message: Some(reason.clone()),
        };
        work_item.status = ContextPreparationStatus::Terminal {
            terminal: terminal.clone(),
        };
        let events = thread
            .append_events([
                RuntimeEvent::ThreadStatusChanged {
                    status: RuntimeThreadStatus::Desynchronized,
                },
                RuntimeEvent::ContextCompactionTerminal {
                    compaction_id: candidate.compaction_id.clone(),
                    operation_id: candidate.operation_id.clone(),
                    terminal: terminal.clone(),
                    context_revision: thread.context_revision,
                },
                RuntimeEvent::OperationTerminal {
                    operation_id: candidate.operation_id.clone(),
                    terminal: terminal.clone(),
                },
            ])
            .map_err(ContextRuntimeError::Transition)?;
        self.store()
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: thread,
                operation: None,
                operation_terminals: vec![(candidate.operation_id.clone(), terminal)],
                records: crate::internal_journal_records(events)?,
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: vec![work_item],
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: vec![activation],
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await
            .map_err(store_error)
    }

    pub async fn observe_opaque_driver_compaction(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<(), ContextRuntimeError> {
        let mut thread = self
            .store()
            .load_thread(thread_id)
            .await
            .map_err(store_error)?
            .ok_or(ContextRuntimeError::ThreadNotFound)?;
        let expected = thread.revision;
        let events = thread
            .append_events([RuntimeEvent::DriverContextCompactedOpaque])
            .map_err(ContextRuntimeError::Transition)?;
        self.store()
            .commit(RuntimeCommit {
                expected_projection_revision: Some(expected),
                projection: thread,
                operation: None,
                operation_terminals: Vec::new(),
                records: crate::internal_journal_records(events)?,
                outbox: Vec::new(),
                terminal_application_effects: Vec::new(),
                context_activation_outbox: Vec::new(),
                context_preparation_work_items: Vec::new(),
                context_checkpoints: Vec::new(),
                context_candidates: Vec::new(),
                context_activations: Vec::new(),
                context_head: None,
                hook_plan_binding: None,
                hook_runs: Vec::new(),
                hook_effects: Vec::new(),
                quarantine: Vec::new(),
            })
            .await
            .map_err(store_error)
    }

    pub(crate) async fn context_view(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<RuntimeContextView, ContextRuntimeError> {
        let head = self
            .store()
            .load_context_head(thread_id)
            .await
            .map_err(store_error)?;
        let checkpoint = match &head {
            Some(head) => self
                .store()
                .load_context_checkpoint(&head.checkpoint_id)
                .await
                .map_err(store_error)?
                .ok_or(ContextRuntimeError::InconsistentStore(
                    ContextSnapshotConsistencyCode::HeadCheckpointMissing,
                ))
                .map(Some)?,
            None => None,
        };
        if let (Some(head), Some(checkpoint)) = (&head, &checkpoint)
            && (checkpoint.thread_id != *thread_id
                || checkpoint.checkpoint_id != head.checkpoint_id
                || checkpoint.revision != head.revision
                || checkpoint.materialized.digest != head.digest
                || checkpoint.materialized.recipe.provenance != head.provenance
                || checkpoint.materialized.fidelity != head.fidelity)
        {
            return Err(ContextRuntimeError::InconsistentStore(
                ContextSnapshotConsistencyCode::HeadCheckpointMismatch,
            ));
        }
        Ok(RuntimeContextView {
            thread_id: thread_id.clone(),
            head: head.as_ref().map(ActiveContextHead::view),
            blocks: checkpoint.as_ref().map_or_else(Vec::new, |checkpoint| {
                checkpoint.materialized.blocks.clone()
            }),
            checkpoint: checkpoint.as_ref().map(ContextCheckpoint::view),
            fidelity: checkpoint
                .as_ref()
                .map_or(ContextFidelity::Opaque, |checkpoint| {
                    checkpoint.materialized.fidelity
                }),
        })
    }
}

fn compaction_summary_frame(
    candidate: &ContextCandidate,
) -> Option<agentdash_agent_protocol::ContextFrame> {
    crate::project_compaction_summary(
        &crate::ContextProjectionIdentity {
            operation_id: candidate.operation_id.to_string(),
            source_frame_id: candidate.checkpoint.checkpoint_id.to_string(),
            source_frame_revision: candidate.checkpoint.revision.0,
            recorded_at_ms: i64::try_from(crate::model::current_time_ms()).unwrap_or(i64::MAX),
        },
        &candidate.presentation,
    )
}

fn activation_outbox(
    thread: &crate::RuntimeThreadState,
    candidate: &ContextCandidate,
    activation: &ContextActivation,
) -> ContextActivationOutboxEntry {
    ContextActivationOutboxEntry {
        activation_id: activation.activation_id.clone(),
        candidate_id: candidate.candidate_id.clone(),
        compaction_id: candidate.compaction_id.clone(),
        thread_id: candidate.thread_id.clone(),
        binding_id: thread.binding_id.clone(),
        generation: thread.driver_generation,
        checkpoint_id: candidate.checkpoint.checkpoint_id.clone(),
        digest: candidate.checkpoint.materialized.digest.clone(),
    }
}

fn store_error(error: RuntimeStoreError) -> ContextRuntimeError {
    ContextRuntimeError::Store(error)
}

pub fn materialized_context(
    recipe: ContextRecipe,
    blocks: Vec<ContextBlock>,
    digest: ContextDigest,
    fidelity: ContextFidelity,
) -> MaterializedContext {
    MaterializedContext {
        recipe,
        blocks,
        digest,
        fidelity,
    }
}
