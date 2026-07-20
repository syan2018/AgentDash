use std::{
    fmt::Write,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use agentdash_agent_runtime_contract::{
    ManagedAgentRuntimeGateway, ManagedRuntimeAppliedContextProvenance,
    ManagedRuntimeAppliedInitialContextEvidence, ManagedRuntimeChangeGap, ManagedRuntimeChangePage,
    ManagedRuntimeChangesRequest, ManagedRuntimeCommand, ManagedRuntimeCommandEnvelope,
    ManagedRuntimeContentBlock, ManagedRuntimeForkCutoff, ManagedRuntimeForkProgressEvidence,
    ManagedRuntimeGatewayError, ManagedRuntimeInitialContextAppliedFidelity,
    ManagedRuntimeInitialContextContributionEvidence, ManagedRuntimeInitialContextContributionKind,
    ManagedRuntimeLifecycleStatus, ManagedRuntimeOperationEvidence, ManagedRuntimeOperationReceipt,
    ManagedRuntimeOperationStatus, ManagedRuntimeReadRequest, RuntimeChangeSequence,
    RuntimePayloadDigest, RuntimeProjectionRevision, RuntimeSourceRef, RuntimeThreadId,
};
use agentdash_agent_service_api::{
    AgentCommand, AgentCommandEnvelope, AgentCommandId, AgentCommandMeta, AgentEffectIdentity,
    AgentForkPoint, AgentIdempotencyKey, AgentInput, AgentInputContent, AgentInteractionResponse,
    AgentPayloadDigest, AgentReceiptState, AgentTerminalOutcome, InitialContextContributionKind,
    InitialContextDeliveryFidelity,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::{
    ManagedRuntimeAgentBinding, ManagedRuntimeBindingFact, ManagedRuntimeCoordinator,
    ManagedRuntimeCreateOutcome, ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError,
    ManagedRuntimeLifecycleInspection, ManagedRuntimeLifecyclePort,
    ManagedRuntimePendingCommandState, ManagedRuntimeRebindOutcome, ManagedRuntimeResumeOutcome,
    ManagedRuntimeSettlement, ManagedRuntimeStateRepository, ManagedRuntimeStateStoreError,
    context_contribution_kind, map_initial_context_package,
};

#[async_trait]
pub trait ManagedRuntimeClock: Send + Sync {
    async fn now_ms(&self) -> u64;
}

pub struct SystemManagedRuntimeClock;

#[async_trait]
impl ManagedRuntimeClock for SystemManagedRuntimeClock {
    async fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
}

pub struct ProductionManagedAgentRuntimeGateway {
    repository: Arc<dyn ManagedRuntimeStateRepository>,
    lifecycle: Arc<dyn ManagedRuntimeLifecyclePort>,
    clock: Arc<dyn ManagedRuntimeClock>,
    dispatch_owner: String,
    lease_duration_ms: u64,
}

impl ProductionManagedAgentRuntimeGateway {
    pub fn new(
        repository: Arc<dyn ManagedRuntimeStateRepository>,
        lifecycle: Arc<dyn ManagedRuntimeLifecyclePort>,
        clock: Arc<dyn ManagedRuntimeClock>,
        dispatch_owner: impl Into<String>,
        lease_duration_ms: u64,
    ) -> Result<Self, ManagedRuntimeGatewayError> {
        let dispatch_owner = dispatch_owner.into();
        if dispatch_owner.trim().is_empty() || lease_duration_ms == 0 {
            return Err(ManagedRuntimeGatewayError::Invalid {
                reason: "Runtime gateway requires a dispatch owner and positive lease duration"
                    .to_owned(),
            });
        }
        Ok(Self {
            repository,
            lifecycle,
            clock,
            dispatch_owner,
            lease_duration_ms,
        })
    }

    pub fn system(
        repository: Arc<dyn ManagedRuntimeStateRepository>,
        lifecycle: Arc<dyn ManagedRuntimeLifecyclePort>,
        dispatch_owner: impl Into<String>,
        lease_duration_ms: u64,
    ) -> Result<Self, ManagedRuntimeGatewayError> {
        Self::new(
            repository,
            lifecycle,
            Arc::new(SystemManagedRuntimeClock),
            dispatch_owner,
            lease_duration_ms,
        )
    }

    async fn dispatch(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        accepted: ManagedRuntimeOperationReceipt,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        if is_terminal(accepted.status) {
            return Ok(accepted);
        }
        let now_ms = self.clock.now_ms().await;
        let effect_id = effect_identity(&command)?;
        let state = self
            .repository
            .load(&command.thread_id)
            .await
            .map_err(map_store_error)?;
        let pending = state
            .facts
            .pending_commands
            .get(&command.operation_id)
            .ok_or(ManagedRuntimeGatewayError::NotFound)?;
        if pending.state == ManagedRuntimePendingCommandState::InspectionRequired
            || pending.state == ManagedRuntimePendingCommandState::Claimed
        {
            return self
                .inspect_pending(command, effect_id, now_ms, state.facts.binding)
                .await;
        }
        let coordinator = ManagedRuntimeCoordinator::new(self.repository.clone());
        let running = coordinator
            .mark_running(
                &command.thread_id,
                &command.operation_id,
                self.dispatch_owner.clone(),
                now_ms,
            )
            .await
            .map_err(map_store_error)?;
        if is_terminal(running.status) {
            return Ok(running);
        }
        let context = crate::ManagedRuntimeDispatchContext {
            runtime_thread_id: command.thread_id.clone(),
            effect_id,
            dispatch_owner: self.dispatch_owner.clone(),
            now_ms,
            lease_duration_ms: self.lease_duration_ms,
        };
        match &command.command {
            ManagedRuntimeCommand::Create { initial_context } => {
                let mapped = initial_context
                    .clone()
                    .map(map_initial_context_package)
                    .transpose()
                    .map_err(map_lifecycle_error)?;
                match self.lifecycle.create(context, mapped).await {
                    Ok(outcome) => self.settle_create(command, outcome, now_ms).await,
                    Err(error) => self.settle_lifecycle_error(command, error, now_ms).await,
                }
            }
            ManagedRuntimeCommand::Resume => {
                let binding = required_binding(&state.facts.binding)?;
                match self.lifecycle.resume(context, binding).await {
                    Ok(outcome) => self.settle_resume(command, outcome, now_ms).await,
                    Err(error) => self.settle_lifecycle_error(command, error, now_ms).await,
                }
            }
            ManagedRuntimeCommand::Rebind => {
                let binding = required_binding(&state.facts.binding)?;
                match self.lifecycle.rebind(context, binding).await {
                    Ok(outcome) => self.settle_rebind(command, outcome, now_ms).await,
                    Err(error) => self.settle_lifecycle_error(command, error, now_ms).await,
                }
            }
            ManagedRuntimeCommand::Activate => {
                let binding = required_binding(&state.facts.binding)?;
                match self
                    .lifecycle
                    .is_ready(command.thread_id.clone(), binding)
                    .await
                {
                    Ok(true) => self.settle_activate(command, now_ms).await,
                    Ok(false) => {
                        self.settle_lifecycle_error(
                            command,
                            ManagedRuntimeLifecycleError::Unavailable {
                                reason: "Host binding has no matching applied surface".to_owned(),
                            },
                            now_ms,
                        )
                        .await
                    }
                    Err(error) => self.settle_lifecycle_error(command, error, now_ms).await,
                }
            }
            ManagedRuntimeCommand::Fork {
                child_thread_id,
                through_completed_turn_id,
            } => {
                let binding = required_binding(&state.facts.binding)?;
                let cutoff = match through_completed_turn_id {
                    Some(turn_id) => {
                        let identities =
                            state.facts.source_identities.as_ref().ok_or_else(|| {
                                ManagedRuntimeGatewayError::Unavailable {
                                    reason: "Runtime source identity map is not committed"
                                        .to_owned(),
                                }
                            })?;
                        AgentForkPoint::CompletedTurn {
                            turn_id: identities.source_turn_id(turn_id).map_err(|error| {
                                ManagedRuntimeGatewayError::Invalid {
                                    reason: error.to_string(),
                                }
                            })?,
                        }
                    }
                    None => AgentForkPoint::Head,
                };
                match self
                    .lifecycle
                    .fork(context, binding, child_thread_id.clone(), cutoff)
                    .await
                {
                    Ok(outcome) => self.settle_fork(command, outcome, now_ms).await,
                    Err(error) => self.settle_lifecycle_error(command, error, now_ms).await,
                }
            }
            _ => {
                let binding = required_binding(&state.facts.binding)?;
                let agent_command = map_agent_command(&command, &state.facts, &binding)?;
                match self
                    .lifecycle
                    .execute(context, binding, agent_command)
                    .await
                {
                    Ok(receipt) => {
                        self.settle_agent_receipt(command, receipt.state, now_ms)
                            .await
                    }
                    Err(error) => self.settle_lifecycle_error(command, error, now_ms).await,
                }
            }
        }
    }

    async fn inspect_pending(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        effect_id: AgentEffectIdentity,
        now_ms: u64,
        binding: Option<ManagedRuntimeBindingFact>,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        let context = crate::ManagedRuntimeDispatchContext {
            runtime_thread_id: command.thread_id.clone(),
            effect_id,
            dispatch_owner: self.dispatch_owner.clone(),
            now_ms,
            lease_duration_ms: self.lease_duration_ms,
        };
        match self
            .lifecycle
            .inspect(context, binding.map(|binding| binding.binding))
            .await
        {
            Ok(ManagedRuntimeLifecycleInspection::CreateApplied(outcome)) => {
                self.settle_create(command, outcome, now_ms).await
            }
            Ok(ManagedRuntimeLifecycleInspection::ResumeApplied(outcome)) => {
                self.settle_resume(command, outcome, now_ms).await
            }
            Ok(ManagedRuntimeLifecycleInspection::RebindApplied(outcome)) => {
                self.settle_rebind(command, outcome, now_ms).await
            }
            Ok(ManagedRuntimeLifecycleInspection::ForkApplied(outcome)) => {
                self.settle_fork(command, outcome, now_ms).await
            }
            Ok(ManagedRuntimeLifecycleInspection::CommandApplied(receipt)) => {
                self.settle_agent_receipt(command, receipt.state, now_ms)
                    .await
            }
            Ok(ManagedRuntimeLifecycleInspection::NotApplied) => {
                self.reset_pending(command, now_ms).await
            }
            Ok(
                ManagedRuntimeLifecycleInspection::Accepted
                | ManagedRuntimeLifecycleInspection::Unknown,
            ) => self.keep_inspection_required(command, now_ms).await,
            Err(error) => self.settle_lifecycle_error(command, error, now_ms).await,
        }
    }

    async fn settle_create(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        outcome: ManagedRuntimeCreateOutcome,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        if !receipt_succeeded(&outcome.receipt.state) {
            return self
                .settle_agent_receipt(command, outcome.receipt.state, now_ms)
                .await;
        }
        let initial_context = map_initial_context_evidence(&command, &outcome)?;
        let revision = self.next_revision(&command.thread_id).await?;
        let binding = binding_fact(&command.thread_id, outcome.binding, revision, None)?;
        let evidence = ManagedRuntimeOperationEvidence::Create {
            binding: binding.evidence(),
            initial_context,
        };
        self.finish(
            command,
            ManagedRuntimeOperationStatus::Succeeded,
            Some(evidence),
            Some(binding),
            Some(ManagedRuntimeLifecycleStatus::Provisioning),
            now_ms,
        )
        .await
    }

    async fn settle_resume(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        outcome: ManagedRuntimeResumeOutcome,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        if !receipt_succeeded(&outcome.receipt.state) {
            return self
                .settle_agent_receipt(command, outcome.receipt.state, now_ms)
                .await;
        }
        let current = self
            .repository
            .load(&command.thread_id)
            .await
            .map_err(map_store_error)?
            .facts
            .binding
            .ok_or(ManagedRuntimeGatewayError::NotFound)?;
        if current.binding.source != outcome.binding.source
            || current.binding.generation != outcome.binding.generation
        {
            return Err(ManagedRuntimeGatewayError::Conflict {
                actual: current.committed_at_revision,
            });
        }
        let binding = ManagedRuntimeBindingFact {
            binding: outcome.binding,
            activated_at_revision: None,
            ..current
        };
        let evidence = ManagedRuntimeOperationEvidence::Resume {
            binding: binding.evidence(),
        };
        self.finish(
            command,
            ManagedRuntimeOperationStatus::Succeeded,
            Some(evidence),
            Some(binding),
            Some(ManagedRuntimeLifecycleStatus::Provisioning),
            now_ms,
        )
        .await
    }

    async fn settle_rebind(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        outcome: ManagedRuntimeRebindOutcome,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        if !receipt_succeeded(&outcome.receipt.state) {
            return self
                .settle_agent_receipt(command, outcome.receipt.state, now_ms)
                .await;
        }
        let current = self
            .repository
            .load(&command.thread_id)
            .await
            .map_err(map_store_error)?
            .facts
            .binding
            .ok_or(ManagedRuntimeGatewayError::NotFound)?;
        if current.binding != outcome.previous_binding
            || outcome.binding.source != current.binding.source
            || outcome.binding.generation.0 != current.binding.generation.0.saturating_add(1)
        {
            return Err(ManagedRuntimeGatewayError::Conflict {
                actual: current.committed_at_revision,
            });
        }
        let revision = self.next_revision(&command.thread_id).await?;
        let binding = binding_fact(&command.thread_id, outcome.binding, revision, None)?;
        let evidence = ManagedRuntimeOperationEvidence::Rebind {
            previous_binding: current.evidence(),
            binding: binding.evidence(),
        };
        self.finish(
            command,
            ManagedRuntimeOperationStatus::Succeeded,
            Some(evidence),
            Some(binding),
            Some(ManagedRuntimeLifecycleStatus::Provisioning),
            now_ms,
        )
        .await
    }

    async fn settle_activate(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        let revision = self.next_revision(&command.thread_id).await?;
        let mut binding = self
            .repository
            .load(&command.thread_id)
            .await
            .map_err(map_store_error)?
            .facts
            .binding
            .ok_or(ManagedRuntimeGatewayError::NotFound)?;
        binding.activated_at_revision = Some(revision);
        let evidence = ManagedRuntimeOperationEvidence::Activate {
            binding: binding.evidence(),
        };
        self.finish(
            command,
            ManagedRuntimeOperationStatus::Succeeded,
            Some(evidence),
            Some(binding),
            Some(ManagedRuntimeLifecycleStatus::Active),
            now_ms,
        )
        .await
    }

    async fn settle_fork(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        outcome: ManagedRuntimeForkOutcome,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        if !receipt_succeeded(&outcome.receipt.state) {
            return self
                .settle_agent_receipt(command, outcome.receipt.state, now_ms)
                .await;
        }
        let ManagedRuntimeCommand::Fork {
            child_thread_id,
            through_completed_turn_id,
        } = &command.command
        else {
            return Err(ManagedRuntimeGatewayError::Invalid {
                reason: "fork outcome was attached to another command".to_owned(),
            });
        };
        let parent = self
            .repository
            .load(&command.thread_id)
            .await
            .map_err(map_store_error)?
            .facts
            .binding
            .ok_or(ManagedRuntimeGatewayError::NotFound)?;
        let child_binding = binding_fact(
            child_thread_id,
            outcome.child_binding,
            RuntimeProjectionRevision(1),
            None,
        )?;
        ManagedRuntimeCoordinator::new(self.repository.clone())
            .provision_child(child_thread_id.clone(), child_binding.clone(), now_ms)
            .await
            .map_err(map_store_error)?;
        let history_digest = RuntimePayloadDigest::new(outcome.child_history_digest.into_inner())
            .map_err(|error| ManagedRuntimeGatewayError::Invalid {
            reason: error.to_string(),
        })?;
        let cutoff =
            through_completed_turn_id
                .as_ref()
                .map_or(ManagedRuntimeForkCutoff::Head, |turn_id| {
                    ManagedRuntimeForkCutoff::CompletedTurn {
                        turn_id: turn_id.clone(),
                    }
                });
        let evidence = ManagedRuntimeOperationEvidence::Fork {
            parent_binding: parent.evidence(),
            progress: ManagedRuntimeForkProgressEvidence::Provisioned {
                child_thread_id: child_thread_id.clone(),
                child_binding: child_binding.evidence(),
                cutoff,
                child_history_digest: history_digest,
            },
        };
        self.finish(
            command,
            ManagedRuntimeOperationStatus::Succeeded,
            Some(evidence),
            None,
            None,
            now_ms,
        )
        .await
    }

    async fn settle_agent_receipt(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        state: AgentReceiptState,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        match state {
            AgentReceiptState::Accepted | AgentReceiptState::Unknown => {
                self.keep_inspection_required(command, now_ms).await
            }
            AgentReceiptState::Rejected { .. } => {
                self.finish(
                    command,
                    ManagedRuntimeOperationStatus::Failed,
                    None,
                    None,
                    None,
                    now_ms,
                )
                .await
            }
            AgentReceiptState::AlreadyApplied { terminal } => {
                let status =
                    terminal.map_or(ManagedRuntimeOperationStatus::Succeeded, terminal_status);
                self.finish(command, status, None, None, None, now_ms).await
            }
            AgentReceiptState::Terminal { outcome } => {
                self.finish(command, terminal_status(outcome), None, None, None, now_ms)
                    .await
            }
        }
    }

    async fn settle_lifecycle_error(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        error: ManagedRuntimeLifecycleError,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        match error {
            ManagedRuntimeLifecycleError::ForkChildKnown {
                child_source,
                child_history_digest,
                ..
            } => {
                self.settle_fork_child_known(
                    command,
                    child_source,
                    child_history_digest,
                    ManagedRuntimeOperationStatus::Lost,
                    ManagedRuntimePendingCommandState::Lost,
                    now_ms,
                )
                .await
            }
            ManagedRuntimeLifecycleError::ForkInspectionRequired {
                child_source,
                child_history_digest,
                ..
            } => {
                self.settle_fork_child_known(
                    command,
                    child_source,
                    child_history_digest,
                    ManagedRuntimeOperationStatus::Running,
                    ManagedRuntimePendingCommandState::InspectionRequired,
                    now_ms,
                )
                .await
            }
            ManagedRuntimeLifecycleError::InspectionRequired { .. } => {
                self.keep_inspection_required(command, now_ms).await
            }
            ManagedRuntimeLifecycleError::StaleGeneration
            | ManagedRuntimeLifecycleError::NotFound => {
                self.finish(
                    command,
                    ManagedRuntimeOperationStatus::Lost,
                    None,
                    None,
                    Some(ManagedRuntimeLifecycleStatus::Lost),
                    now_ms,
                )
                .await
            }
            ManagedRuntimeLifecycleError::Unavailable { reason }
            | ManagedRuntimeLifecycleError::Invalid { reason }
            | ManagedRuntimeLifecycleError::Persistence { reason } => {
                let _ = reason;
                self.finish(
                    command,
                    ManagedRuntimeOperationStatus::Failed,
                    None,
                    None,
                    None,
                    now_ms,
                )
                .await
            }
        }
    }

    async fn settle_fork_child_known(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        child_source: agentdash_agent_service_api::AgentSourceCoordinate,
        child_history_digest: Option<AgentPayloadDigest>,
        status: ManagedRuntimeOperationStatus,
        pending_state: ManagedRuntimePendingCommandState,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        let ManagedRuntimeCommand::Fork {
            child_thread_id,
            through_completed_turn_id,
        } = &command.command
        else {
            return Err(ManagedRuntimeGatewayError::Invalid {
                reason: "Fork partial evidence was attached to another command".to_owned(),
            });
        };
        let child_thread_id = child_thread_id.clone();
        let through_completed_turn_id = through_completed_turn_id.clone();
        let parent = self
            .repository
            .load(&command.thread_id)
            .await
            .map_err(map_store_error)?
            .facts
            .binding
            .ok_or(ManagedRuntimeGatewayError::NotFound)?;
        let child_source_ref = source_ref(&child_thread_id, &child_source)?;
        let cutoff =
            through_completed_turn_id
                .as_ref()
                .map_or(ManagedRuntimeForkCutoff::Head, |turn_id| {
                    ManagedRuntimeForkCutoff::CompletedTurn {
                        turn_id: turn_id.clone(),
                    }
                });
        let child_history_digest = child_history_digest
            .map(|digest| RuntimePayloadDigest::new(digest.into_inner()))
            .transpose()
            .map_err(|error| ManagedRuntimeGatewayError::Invalid {
                reason: error.to_string(),
            })?;
        ManagedRuntimeCoordinator::new(self.repository.clone())
            .settle(
                &command.thread_id,
                &command.operation_id,
                ManagedRuntimeSettlement {
                    status,
                    evidence: Some(ManagedRuntimeOperationEvidence::Fork {
                        parent_binding: parent.evidence(),
                        progress: ManagedRuntimeForkProgressEvidence::ChildKnown {
                            child_thread_id,
                            child_source_ref,
                            cutoff,
                            child_history_digest,
                        },
                    }),
                    binding: None,
                    lifecycle: None,
                    pending_state,
                    captured_at_ms: now_ms,
                },
            )
            .await
            .map_err(map_store_error)
    }

    async fn keep_inspection_required(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        let evidence = self
            .repository
            .load(&command.thread_id)
            .await
            .map_err(map_store_error)?
            .facts
            .operations
            .get(&command.operation_id)
            .ok_or(ManagedRuntimeGatewayError::NotFound)?
            .operation
            .evidence
            .clone();
        ManagedRuntimeCoordinator::new(self.repository.clone())
            .settle(
                &command.thread_id,
                &command.operation_id,
                ManagedRuntimeSettlement {
                    status: ManagedRuntimeOperationStatus::Running,
                    evidence,
                    binding: None,
                    lifecycle: None,
                    pending_state: ManagedRuntimePendingCommandState::InspectionRequired,
                    captured_at_ms: now_ms,
                },
            )
            .await
            .map_err(map_store_error)
    }

    async fn reset_pending(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        _now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        ManagedRuntimeCoordinator::new(self.repository.clone())
            .reset_for_redispatch(&command.thread_id, &command.operation_id)
            .await
            .map_err(map_store_error)?;
        Err(ManagedRuntimeGatewayError::Unavailable {
            reason: format!(
                "effect {} was confirmed not applied; retry execute to redispatch",
                command.operation_id
            ),
        })
    }

    async fn finish(
        &self,
        command: ManagedRuntimeCommandEnvelope,
        status: ManagedRuntimeOperationStatus,
        evidence: Option<ManagedRuntimeOperationEvidence>,
        binding: Option<ManagedRuntimeBindingFact>,
        lifecycle: Option<ManagedRuntimeLifecycleStatus>,
        now_ms: u64,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        let pending_state = if status == ManagedRuntimeOperationStatus::Lost {
            ManagedRuntimePendingCommandState::Lost
        } else {
            ManagedRuntimePendingCommandState::Settled
        };
        ManagedRuntimeCoordinator::new(self.repository.clone())
            .settle(
                &command.thread_id,
                &command.operation_id,
                ManagedRuntimeSettlement {
                    status,
                    evidence,
                    binding,
                    lifecycle,
                    pending_state,
                    captured_at_ms: now_ms,
                },
            )
            .await
            .map_err(map_store_error)
    }

    async fn next_revision(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<RuntimeProjectionRevision, ManagedRuntimeGatewayError> {
        let current = self
            .repository
            .load(thread_id)
            .await
            .map_err(map_store_error)?
            .facts
            .projection
            .ok_or(ManagedRuntimeGatewayError::NotFound)?
            .revision;
        current
            .0
            .checked_add(1)
            .map(RuntimeProjectionRevision)
            .ok_or_else(|| ManagedRuntimeGatewayError::Persistence {
                reason: "Runtime projection revision is exhausted".to_owned(),
            })
    }
}

pub fn production_managed_runtime_gateway(
    repository: Arc<dyn ManagedRuntimeStateRepository>,
    lifecycle: Arc<dyn ManagedRuntimeLifecyclePort>,
    dispatch_owner: impl Into<String>,
    lease_duration_ms: u64,
) -> Result<Arc<dyn ManagedAgentRuntimeGateway>, ManagedRuntimeGatewayError> {
    Ok(Arc::new(ProductionManagedAgentRuntimeGateway::system(
        repository,
        lifecycle,
        dispatch_owner,
        lease_duration_ms,
    )?))
}

#[async_trait]
impl ManagedAgentRuntimeGateway for ProductionManagedAgentRuntimeGateway {
    async fn execute(
        &self,
        command: ManagedRuntimeCommandEnvelope,
    ) -> Result<ManagedRuntimeOperationReceipt, ManagedRuntimeGatewayError> {
        if matches!(
            command.command,
            ManagedRuntimeCommand::Resume | ManagedRuntimeCommand::Rebind
        ) {
            let state = self
                .repository
                .load(&command.thread_id)
                .await
                .map_err(map_store_error)?;
            if state.facts.projection.is_none() {
                return Err(ManagedRuntimeGatewayError::NotFound);
            }
            if state.facts.binding.is_none() {
                return Err(ManagedRuntimeGatewayError::Unavailable {
                    reason: "Resume or Rebind requires a committed Runtime source binding"
                        .to_owned(),
                });
            }
        }
        let effect_id = effect_identity(&command)?;
        let now_ms = self.clock.now_ms().await;
        let accepted = ManagedRuntimeCoordinator::new(self.repository.clone())
            .accept(command.clone(), effect_id, now_ms)
            .await
            .map_err(map_store_error)?;
        self.dispatch(command, accepted).await
    }

    async fn read(
        &self,
        request: ManagedRuntimeReadRequest,
    ) -> Result<agentdash_agent_runtime_contract::ManagedRuntimeSnapshot, ManagedRuntimeGatewayError>
    {
        self.repository
            .load(&request.thread_id)
            .await
            .map_err(map_store_error)?
            .facts
            .projection
            .ok_or(ManagedRuntimeGatewayError::NotFound)
    }

    async fn changes(
        &self,
        request: ManagedRuntimeChangesRequest,
    ) -> Result<ManagedRuntimeChangePage, ManagedRuntimeGatewayError> {
        if request.limit == 0 {
            return Err(ManagedRuntimeGatewayError::Invalid {
                reason: "Runtime change page limit must be positive".to_owned(),
            });
        }
        let state = self
            .repository
            .load(&request.thread_id)
            .await
            .map_err(map_store_error)?;
        let projection = state
            .facts
            .projection
            .ok_or(ManagedRuntimeGatewayError::NotFound)?;
        let latest = projection.latest_change_sequence;
        if request.after.is_some_and(|after| after > latest) {
            return Err(ManagedRuntimeGatewayError::Invalid {
                reason: "Runtime change cursor is ahead of the committed head".to_owned(),
            });
        }
        let earliest = state
            .facts
            .changes
            .first()
            .map(|change| change.sequence)
            .unwrap_or(latest);
        let gap = request.after.and_then(|after| {
            (after.0.saturating_add(1) < earliest.0).then_some(ManagedRuntimeChangeGap {
                requested_after: Some(after),
                earliest_available: earliest,
                latest_available: latest,
                snapshot_revision: projection.revision,
            })
        });
        let after = gap
            .as_ref()
            .map(|gap| gap.earliest_available.0.saturating_sub(1))
            .or_else(|| request.after.map(|after| after.0))
            .unwrap_or(0);
        let changes = state
            .facts
            .changes
            .into_iter()
            .filter(|change| change.sequence.0 > after)
            .take(request.limit as usize)
            .collect::<Vec<_>>();
        let next = changes
            .last()
            .map(|change| change.sequence)
            .unwrap_or(RuntimeChangeSequence(after));
        Ok(ManagedRuntimeChangePage {
            thread_id: request.thread_id,
            changes,
            next,
            gap,
        })
    }
}

fn required_binding(
    binding: &Option<ManagedRuntimeBindingFact>,
) -> Result<ManagedRuntimeAgentBinding, ManagedRuntimeGatewayError> {
    binding
        .as_ref()
        .map(|binding| binding.binding.clone())
        .ok_or_else(|| ManagedRuntimeGatewayError::Unavailable {
            reason: "Runtime thread has no committed source binding".to_owned(),
        })
}

fn effect_identity(
    command: &ManagedRuntimeCommandEnvelope,
) -> Result<AgentEffectIdentity, ManagedRuntimeGatewayError> {
    AgentEffectIdentity::new(format!(
        "runtime:{}:{}",
        command.thread_id, command.operation_id
    ))
    .map_err(|error| ManagedRuntimeGatewayError::Invalid {
        reason: error.to_string(),
    })
}

fn binding_fact(
    thread_id: &RuntimeThreadId,
    binding: ManagedRuntimeAgentBinding,
    committed_at_revision: RuntimeProjectionRevision,
    activated_at_revision: Option<RuntimeProjectionRevision>,
) -> Result<ManagedRuntimeBindingFact, ManagedRuntimeGatewayError> {
    let source_ref = source_ref(thread_id, &binding.source)?;
    Ok(ManagedRuntimeBindingFact {
        source_ref,
        binding,
        committed_at_revision,
        activated_at_revision,
    })
}

fn source_ref(
    thread_id: &RuntimeThreadId,
    source: &agentdash_agent_service_api::AgentSourceCoordinate,
) -> Result<RuntimeSourceRef, ManagedRuntimeGatewayError> {
    let mut hasher = Sha256::new();
    hasher.update(b"agentdash-runtime-source-ref-v1\0");
    hasher.update(thread_id.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(source.as_str().as_bytes());
    let mut source_ref_value = String::from("rsrc:");
    for byte in hasher.finalize() {
        write!(&mut source_ref_value, "{byte:02x}").expect("writing to String cannot fail");
    }
    let source_ref = RuntimeSourceRef::new(source_ref_value).map_err(|error| {
        ManagedRuntimeGatewayError::Invalid {
            reason: error.to_string(),
        }
    })?;
    Ok(source_ref)
}

fn map_initial_context_evidence(
    command: &ManagedRuntimeCommandEnvelope,
    outcome: &ManagedRuntimeCreateOutcome,
) -> Result<Option<ManagedRuntimeAppliedInitialContextEvidence>, ManagedRuntimeGatewayError> {
    let ManagedRuntimeCommand::Create { initial_context } = &command.command else {
        return Err(ManagedRuntimeGatewayError::Invalid {
            reason: "Create evidence was attached to another command".to_owned(),
        });
    };
    match (initial_context, &outcome.initial_context) {
        (None, None) => Ok(None),
        (Some(package), Some(applied)) => {
            if package.package_id.as_str() != applied.package_id.as_str()
                || package.digest.as_str() != applied.package_digest.as_str()
            {
                return Err(ManagedRuntimeGatewayError::Invalid {
                    reason: "initial context evidence does not match the admitted package"
                        .to_owned(),
                });
            }
            let contributions = package
                .contributions
                .iter()
                .map(|contribution| {
                    let kind = context_contribution_kind(contribution);
                    let service_kind = match kind {
                        ManagedRuntimeInitialContextContributionKind::CompactSummary => {
                            InitialContextContributionKind::CompactSummary
                        }
                        ManagedRuntimeInitialContextContributionKind::WorkflowContext => {
                            InitialContextContributionKind::WorkflowContext
                        }
                        ManagedRuntimeInitialContextContributionKind::ConstraintSet => {
                            InitialContextContributionKind::ConstraintSet
                        }
                    };
                    let offered_fidelity = outcome
                        .contribution_fidelity
                        .get(&service_kind)
                        .copied()
                        .ok_or_else(|| ManagedRuntimeGatewayError::Invalid {
                            reason: format!(
                                "initial context evidence is missing {service_kind:?} fidelity"
                            ),
                        })?;
                    if !offered_fidelity.satisfies(applied.fidelity) {
                        return Err(ManagedRuntimeGatewayError::Invalid {
                            reason: format!(
                                "initial context {service_kind:?} applied fidelity exceeds the Host capability"
                            ),
                        });
                    }
                    let materialized_digest = applied
                        .materialized_digest
                        .as_ref()
                        .ok_or_else(|| ManagedRuntimeGatewayError::Invalid {
                            reason: format!(
                                "initial context {service_kind:?} has no applied digest"
                            ),
                        })
                        .and_then(|digest| {
                            RuntimePayloadDigest::new(digest.as_str().to_owned()).map_err(|error| {
                                ManagedRuntimeGatewayError::Invalid {
                                    reason: error.to_string(),
                                }
                            })
                        })?;
                    let provenance = match &contribution.content {
                        agentdash_agent_runtime_contract::ManagedRuntimeInitialContextContributionContent::CompactSummary {
                            provenance,
                            ..
                        }
                        | agentdash_agent_runtime_contract::ManagedRuntimeInitialContextContributionContent::WorkflowContext {
                            provenance,
                            ..
                        }
                        | agentdash_agent_runtime_contract::ManagedRuntimeInitialContextContributionContent::ConstraintSet {
                            provenance,
                            ..
                        } => provenance,
                    };
                    Ok(ManagedRuntimeInitialContextContributionEvidence {
                        contribution_id: contribution.contribution_id.clone(),
                        kind,
                        contribution_digest: contribution.digest.clone(),
                        provenance: ManagedRuntimeAppliedContextProvenance {
                            authority: provenance.authority,
                            source: provenance.source.clone(),
                            revision: provenance.revision.clone(),
                            digest: provenance.digest.clone(),
                        },
                        fidelity: map_context_fidelity(
                            applied.fidelity,
                            applied.renderer_version.as_deref(),
                            materialized_digest,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, ManagedRuntimeGatewayError>>()?;
            Ok(Some(ManagedRuntimeAppliedInitialContextEvidence {
                package_id: package.package_id.clone(),
                package_digest: package.digest.clone(),
                contributions,
            }))
        }
        _ => Err(ManagedRuntimeGatewayError::Invalid {
            reason: "initial context applied evidence is incomplete".to_owned(),
        }),
    }
}

fn map_context_fidelity(
    fidelity: InitialContextDeliveryFidelity,
    renderer_version: Option<&str>,
    applied_digest: RuntimePayloadDigest,
) -> Result<ManagedRuntimeInitialContextAppliedFidelity, ManagedRuntimeGatewayError> {
    match fidelity {
        InitialContextDeliveryFidelity::Unsupported => Err(ManagedRuntimeGatewayError::Invalid {
            reason: "Unsupported initial context fidelity cannot be successful evidence".to_owned(),
        }),
        InitialContextDeliveryFidelity::CanonicalRendered => {
            let renderer_version = renderer_version
                .filter(|version| !version.trim().is_empty())
                .ok_or_else(|| ManagedRuntimeGatewayError::Invalid {
                    reason: "canonical rendered context has no renderer version".to_owned(),
                })?;
            Ok(
                ManagedRuntimeInitialContextAppliedFidelity::CanonicalRendered {
                    renderer_version: renderer_version.to_owned(),
                    rendered_digest: applied_digest,
                },
            )
        }
        InitialContextDeliveryFidelity::TypedNative => {
            Ok(ManagedRuntimeInitialContextAppliedFidelity::TypedNative { applied_digest })
        }
    }
}

fn map_agent_command(
    command: &ManagedRuntimeCommandEnvelope,
    facts: &crate::ManagedRuntimeFacts,
    binding: &ManagedRuntimeAgentBinding,
) -> Result<AgentCommandEnvelope, ManagedRuntimeGatewayError> {
    let identities = facts.source_identities.as_ref();
    let map_turn = |turn_id| {
        identities
            .ok_or_else(|| ManagedRuntimeGatewayError::Unavailable {
                reason: "Runtime source identity map is not committed".to_owned(),
            })?
            .source_turn_id(turn_id)
            .map_err(|error| ManagedRuntimeGatewayError::Invalid {
                reason: error.to_string(),
            })
    };
    let map_interaction = |interaction_id| {
        identities
            .ok_or_else(|| ManagedRuntimeGatewayError::Unavailable {
                reason: "Runtime source identity map is not committed".to_owned(),
            })?
            .source_interaction_id(interaction_id)
            .map_err(|error| ManagedRuntimeGatewayError::Invalid {
                reason: error.to_string(),
            })
    };
    let mapped = match &command.command {
        ManagedRuntimeCommand::SubmitInput { content } => AgentCommand::SubmitInput {
            input: AgentInput {
                content: map_input(content)?,
            },
        },
        ManagedRuntimeCommand::Steer {
            expected_turn_id,
            content,
        } => AgentCommand::Steer {
            expected_turn_id: map_turn(expected_turn_id)?,
            input: AgentInput {
                content: map_input(content)?,
            },
        },
        ManagedRuntimeCommand::Interrupt { expected_turn_id } => AgentCommand::Interrupt {
            expected_turn_id: map_turn(expected_turn_id)?,
        },
        ManagedRuntimeCommand::RequestCompaction => AgentCommand::RequestCompaction,
        ManagedRuntimeCommand::ResolveInteraction {
            interaction_id,
            response,
        } => AgentCommand::ResolveInteraction {
            interaction_id: map_interaction(interaction_id)?,
            response: match response {
                agentdash_agent_runtime_contract::ManagedRuntimeInteractionResponse::Approved => {
                    AgentInteractionResponse::Approved
                }
                agentdash_agent_runtime_contract::ManagedRuntimeInteractionResponse::Denied {
                    reason,
                } => AgentInteractionResponse::Denied {
                    reason: reason.clone(),
                },
                agentdash_agent_runtime_contract::ManagedRuntimeInteractionResponse::UserInput {
                    content,
                } => AgentInteractionResponse::UserInput {
                    input: AgentInput {
                        content: map_input(content)?,
                    },
                },
                agentdash_agent_runtime_contract::ManagedRuntimeInteractionResponse::Structured {
                    value,
                    ..
                } => AgentInteractionResponse::McpElicitation {
                    response: value.clone(),
                },
            },
        },
        ManagedRuntimeCommand::Close => AgentCommand::Close,
        ManagedRuntimeCommand::Create { .. }
        | ManagedRuntimeCommand::Resume
        | ManagedRuntimeCommand::Rebind
        | ManagedRuntimeCommand::Activate
        | ManagedRuntimeCommand::Fork { .. } => {
            return Err(ManagedRuntimeGatewayError::Invalid {
                reason: "lifecycle command cannot use the Agent command envelope".to_owned(),
            });
        }
    };
    Ok(AgentCommandEnvelope {
        meta: AgentCommandMeta {
            command_id: AgentCommandId::new(format!("runtime-command:{}", command.operation_id))
                .map_err(|error| ManagedRuntimeGatewayError::Invalid {
                    reason: error.to_string(),
                })?,
            effect_id: effect_identity(command)?,
            idempotency_key: AgentIdempotencyKey::new(command.idempotency_key.as_str().to_owned())
                .map_err(|error| ManagedRuntimeGatewayError::Invalid {
                    reason: error.to_string(),
                })?,
            binding_generation: binding.generation,
            expected_snapshot_revision: facts
                .source_projection
                .as_ref()
                .map(|projection| projection.snapshot_revision),
        },
        source: binding.source.clone(),
        command: mapped,
    })
}

fn map_input(
    content: &[ManagedRuntimeContentBlock],
) -> Result<Vec<AgentInputContent>, ManagedRuntimeGatewayError> {
    content
        .iter()
        .map(|block| {
            Ok(match block {
                ManagedRuntimeContentBlock::Text { text } => {
                    AgentInputContent::Text { text: text.clone() }
                }
                ManagedRuntimeContentBlock::Image {
                    media_type,
                    source,
                    digest,
                } => AgentInputContent::Image {
                    media_type: media_type.clone(),
                    source: source.clone(),
                    digest: AgentPayloadDigest::new(digest.as_str().to_owned()).map_err(
                        |error| ManagedRuntimeGatewayError::Invalid {
                            reason: error.to_string(),
                        },
                    )?,
                },
                ManagedRuntimeContentBlock::Resource {
                    uri,
                    media_type,
                    digest,
                } => AgentInputContent::Resource {
                    uri: uri.clone(),
                    media_type: media_type.clone(),
                    digest: digest
                        .as_ref()
                        .map(|digest| AgentPayloadDigest::new(digest.as_str().to_owned()))
                        .transpose()
                        .map_err(|error| ManagedRuntimeGatewayError::Invalid {
                            reason: error.to_string(),
                        })?,
                },
                ManagedRuntimeContentBlock::Structured { schema, value } => {
                    AgentInputContent::Structured {
                        schema: schema.clone(),
                        value: value.clone(),
                    }
                }
            })
        })
        .collect()
}

fn terminal_status(outcome: AgentTerminalOutcome) -> ManagedRuntimeOperationStatus {
    match outcome {
        AgentTerminalOutcome::Succeeded | AgentTerminalOutcome::Closed => {
            ManagedRuntimeOperationStatus::Succeeded
        }
        AgentTerminalOutcome::Failed => ManagedRuntimeOperationStatus::Failed,
        AgentTerminalOutcome::Interrupted => ManagedRuntimeOperationStatus::Interrupted,
        AgentTerminalOutcome::Lost => ManagedRuntimeOperationStatus::Lost,
    }
}

fn receipt_succeeded(state: &AgentReceiptState) -> bool {
    matches!(
        state,
        AgentReceiptState::AlreadyApplied { .. } | AgentReceiptState::Terminal { .. }
    )
}

fn is_terminal(status: ManagedRuntimeOperationStatus) -> bool {
    matches!(
        status,
        ManagedRuntimeOperationStatus::Succeeded
            | ManagedRuntimeOperationStatus::Failed
            | ManagedRuntimeOperationStatus::Interrupted
            | ManagedRuntimeOperationStatus::Lost
    )
}

fn map_store_error(error: ManagedRuntimeStateStoreError) -> ManagedRuntimeGatewayError {
    match error {
        ManagedRuntimeStateStoreError::Conflict => ManagedRuntimeGatewayError::Conflict {
            actual: RuntimeProjectionRevision(0),
        },
        ManagedRuntimeStateStoreError::Invariant { reason } => {
            ManagedRuntimeGatewayError::Invalid { reason }
        }
        ManagedRuntimeStateStoreError::Persistence { reason } => {
            ManagedRuntimeGatewayError::Persistence { reason }
        }
    }
}

fn map_lifecycle_error(error: ManagedRuntimeLifecycleError) -> ManagedRuntimeGatewayError {
    match error {
        ManagedRuntimeLifecycleError::NotFound => ManagedRuntimeGatewayError::NotFound,
        ManagedRuntimeLifecycleError::StaleGeneration => ManagedRuntimeGatewayError::Unavailable {
            reason: "Host binding generation is stale".to_owned(),
        },
        ManagedRuntimeLifecycleError::Unavailable { reason }
        | ManagedRuntimeLifecycleError::InspectionRequired { reason } => {
            ManagedRuntimeGatewayError::Unavailable { reason }
        }
        ManagedRuntimeLifecycleError::ForkChildKnown { reason, .. }
        | ManagedRuntimeLifecycleError::ForkInspectionRequired { reason, .. } => {
            ManagedRuntimeGatewayError::Unavailable { reason }
        }
        ManagedRuntimeLifecycleError::Invalid { reason } => {
            ManagedRuntimeGatewayError::Invalid { reason }
        }
        ManagedRuntimeLifecycleError::Persistence { reason } => {
            ManagedRuntimeGatewayError::Persistence { reason }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };

    use agentdash_agent_runtime_contract::{
        ManagedRuntimeCommand, ManagedRuntimeCommandEnvelope, ManagedRuntimeForkProgressEvidence,
        RuntimeIdempotencyKey, RuntimeOperationId,
    };
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentCommandReceipt, AgentForkPoint, AgentPayloadDigest,
        AgentReceiptState, AgentSourceCoordinate, AgentSurfaceDigest, AgentSurfaceRevision,
        AgentTerminalOutcome, AppliedAgentSurface, ForkAgentReceipt,
    };
    use tokio::sync::Mutex;

    use super::*;
    use crate::{
        ManagedRuntimeDispatchContext, ManagedRuntimeStateCommit, ManagedRuntimeStateSnapshot,
        apply_managed_runtime_state_commit,
    };

    #[derive(Default)]
    struct FixtureRepository {
        states: Mutex<BTreeMap<RuntimeThreadId, ManagedRuntimeStateSnapshot>>,
    }

    #[async_trait]
    impl ManagedRuntimeStateRepository for FixtureRepository {
        async fn load(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
            Ok(self
                .states
                .lock()
                .await
                .get(thread_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn commit(
            &self,
            commit: ManagedRuntimeStateCommit,
        ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
            let mut states = self.states.lock().await;
            let current = states.entry(commit.thread_id.clone()).or_default();
            apply_managed_runtime_state_commit(current, commit)
        }
    }

    struct FixedClock(u64);

    #[async_trait]
    impl ManagedRuntimeClock for FixedClock {
        async fn now_ms(&self) -> u64 {
            self.0
        }
    }

    struct FixtureLifecycle {
        repository: Arc<FixtureRepository>,
        create_calls: AtomicU64,
        fork_calls: AtomicU64,
        recover_create_from_inspect: std::sync::atomic::AtomicBool,
        leave_fork_child_unprovisioned: std::sync::atomic::AtomicBool,
        require_fork_inspection: std::sync::atomic::AtomicBool,
        recover_fork_from_inspect: std::sync::atomic::AtomicBool,
    }

    impl FixtureLifecycle {
        fn new(repository: Arc<FixtureRepository>) -> Self {
            Self {
                repository,
                create_calls: AtomicU64::new(0),
                fork_calls: AtomicU64::new(0),
                recover_create_from_inspect: std::sync::atomic::AtomicBool::new(false),
                leave_fork_child_unprovisioned: std::sync::atomic::AtomicBool::new(false),
                require_fork_inspection: std::sync::atomic::AtomicBool::new(false),
                recover_fork_from_inspect: std::sync::atomic::AtomicBool::new(false),
            }
        }

        async fn assert_durable_intent(&self, context: &ManagedRuntimeDispatchContext) {
            let state = self
                .repository
                .load(&context.runtime_thread_id)
                .await
                .expect("load durable Runtime intent");
            assert!(
                state
                    .facts
                    .pending_commands
                    .values()
                    .any(|pending| pending.effect_id == context.effect_id)
            );
        }
    }

    #[async_trait]
    impl ManagedRuntimeLifecyclePort for FixtureLifecycle {
        async fn create(
            &self,
            context: ManagedRuntimeDispatchContext,
            _initial_context: Option<agentdash_agent_service_api::InitialAgentContextPackage>,
        ) -> Result<ManagedRuntimeCreateOutcome, ManagedRuntimeLifecycleError> {
            self.assert_durable_intent(&context).await;
            self.create_calls.fetch_add(1, Ordering::SeqCst);
            if self.recover_create_from_inspect.load(Ordering::SeqCst) {
                return Err(ManagedRuntimeLifecycleError::InspectionRequired {
                    reason: "simulated lost Create receipt".to_owned(),
                });
            }
            let binding = binding("source-parent");
            Ok(ManagedRuntimeCreateOutcome {
                receipt: receipt(&context, &binding.source),
                binding,
                initial_context: None,
                contribution_fidelity: BTreeMap::new(),
            })
        }

        async fn resume(
            &self,
            context: ManagedRuntimeDispatchContext,
            binding: ManagedRuntimeAgentBinding,
        ) -> Result<ManagedRuntimeResumeOutcome, ManagedRuntimeLifecycleError> {
            Ok(ManagedRuntimeResumeOutcome {
                receipt: receipt(&context, &binding.source),
                binding,
            })
        }

        async fn rebind(
            &self,
            context: ManagedRuntimeDispatchContext,
            previous_binding: ManagedRuntimeAgentBinding,
        ) -> Result<ManagedRuntimeRebindOutcome, ManagedRuntimeLifecycleError> {
            let binding = ManagedRuntimeAgentBinding {
                source: previous_binding.source.clone(),
                generation: AgentBindingGeneration(previous_binding.generation.0 + 1),
                applied_surface: previous_binding.applied_surface.clone(),
            };
            Ok(ManagedRuntimeRebindOutcome {
                receipt: receipt(&context, &binding.source),
                previous_binding,
                binding,
            })
        }

        async fn fork(
            &self,
            context: ManagedRuntimeDispatchContext,
            parent: ManagedRuntimeAgentBinding,
            _child_thread_id: RuntimeThreadId,
            cutoff: AgentForkPoint,
        ) -> Result<ManagedRuntimeForkOutcome, ManagedRuntimeLifecycleError> {
            self.assert_durable_intent(&context).await;
            self.fork_calls.fetch_add(1, Ordering::SeqCst);
            let child_binding = binding("source-child");
            if self.leave_fork_child_unprovisioned.load(Ordering::SeqCst) {
                return Err(ManagedRuntimeLifecycleError::ForkChildKnown {
                    child_source: child_binding.source,
                    child_history_digest: Some(
                        AgentPayloadDigest::new("sha256:child-history").expect("digest"),
                    ),
                    reason: "simulated child provisioning failure".to_owned(),
                });
            }
            if self.require_fork_inspection.load(Ordering::SeqCst) {
                return Err(ManagedRuntimeLifecycleError::ForkInspectionRequired {
                    child_source: child_binding.source,
                    child_history_digest: Some(
                        AgentPayloadDigest::new("sha256:child-history").expect("digest"),
                    ),
                    reason: "simulated Host settlement failure".to_owned(),
                });
            }
            Ok(ManagedRuntimeForkOutcome {
                receipt: ForkAgentReceipt {
                    command_id: AgentCommandId::new(format!(
                        "runtime-command:{}",
                        context.effect_id
                    ))
                    .expect("command"),
                    effect_id: context.effect_id,
                    parent_source: parent.source,
                    child_source: Some(child_binding.source.clone()),
                    cutoff,
                    child_history_digest: Some(
                        AgentPayloadDigest::new("sha256:child-history").expect("digest"),
                    ),
                    state: AgentReceiptState::Terminal {
                        outcome: AgentTerminalOutcome::Succeeded,
                    },
                },
                child_binding,
                child_history_digest: AgentPayloadDigest::new("sha256:child-history")
                    .expect("digest"),
            })
        }

        async fn execute(
            &self,
            context: ManagedRuntimeDispatchContext,
            _binding: ManagedRuntimeAgentBinding,
            command: AgentCommandEnvelope,
        ) -> Result<AgentCommandReceipt, ManagedRuntimeLifecycleError> {
            self.assert_durable_intent(&context).await;
            Ok(AgentCommandReceipt {
                command_id: command.meta.command_id,
                effect_id: command.meta.effect_id,
                source: command.source,
                state: AgentReceiptState::Terminal {
                    outcome: AgentTerminalOutcome::Succeeded,
                },
                snapshot_revision: None,
                initial_context: None,
            })
        }

        async fn inspect(
            &self,
            context: ManagedRuntimeDispatchContext,
            _binding: Option<ManagedRuntimeAgentBinding>,
        ) -> Result<ManagedRuntimeLifecycleInspection, ManagedRuntimeLifecycleError> {
            if self.recover_create_from_inspect.load(Ordering::SeqCst) {
                let binding = binding("source-parent");
                return Ok(ManagedRuntimeLifecycleInspection::CreateApplied(
                    ManagedRuntimeCreateOutcome {
                        receipt: receipt(&context, &binding.source),
                        binding,
                        initial_context: None,
                        contribution_fidelity: BTreeMap::new(),
                    },
                ));
            }
            if self.recover_fork_from_inspect.load(Ordering::SeqCst) {
                let child_binding = binding("source-child");
                return Ok(ManagedRuntimeLifecycleInspection::ForkApplied(
                    ManagedRuntimeForkOutcome {
                        receipt: ForkAgentReceipt {
                            command_id: AgentCommandId::new(format!(
                                "runtime-command:{}",
                                context.effect_id
                            ))
                            .expect("command"),
                            effect_id: context.effect_id,
                            parent_source: AgentSourceCoordinate::new("source-parent")
                                .expect("source"),
                            child_source: Some(child_binding.source.clone()),
                            cutoff: AgentForkPoint::Head,
                            child_history_digest: Some(
                                AgentPayloadDigest::new("sha256:child-history").expect("digest"),
                            ),
                            state: AgentReceiptState::Terminal {
                                outcome: AgentTerminalOutcome::Succeeded,
                            },
                        },
                        child_binding,
                        child_history_digest: AgentPayloadDigest::new("sha256:child-history")
                            .expect("digest"),
                    },
                ));
            }
            Ok(ManagedRuntimeLifecycleInspection::Unknown)
        }

        async fn read(
            &self,
            _runtime_thread_id: RuntimeThreadId,
            _binding: ManagedRuntimeAgentBinding,
            _query: agentdash_agent_service_api::AgentReadQuery,
        ) -> Result<agentdash_agent_service_api::AgentSnapshot, ManagedRuntimeLifecycleError>
        {
            Err(ManagedRuntimeLifecycleError::Unavailable {
                reason: "unused fixture read".to_owned(),
            })
        }

        async fn changes(
            &self,
            _runtime_thread_id: RuntimeThreadId,
            _binding: ManagedRuntimeAgentBinding,
            _query: agentdash_agent_service_api::AgentChangesQuery,
        ) -> Result<agentdash_agent_service_api::AgentChangePage, ManagedRuntimeLifecycleError>
        {
            Err(ManagedRuntimeLifecycleError::Unavailable {
                reason: "unused fixture changes".to_owned(),
            })
        }

        async fn is_ready(
            &self,
            _runtime_thread_id: RuntimeThreadId,
            _binding: ManagedRuntimeAgentBinding,
        ) -> Result<bool, ManagedRuntimeLifecycleError> {
            Ok(true)
        }
    }

    fn binding(source: &str) -> ManagedRuntimeAgentBinding {
        ManagedRuntimeAgentBinding {
            source: AgentSourceCoordinate::new(source).expect("source"),
            generation: AgentBindingGeneration(1),
            applied_surface: AppliedAgentSurface {
                revision: AgentSurfaceRevision(1),
                digest: AgentSurfaceDigest::new("sha256:surface").expect("surface digest"),
                contributions: Vec::new(),
            },
        }
    }

    fn receipt(
        context: &ManagedRuntimeDispatchContext,
        source: &AgentSourceCoordinate,
    ) -> AgentCommandReceipt {
        AgentCommandReceipt {
            command_id: AgentCommandId::new(format!("command:{}", context.effect_id))
                .expect("command"),
            effect_id: context.effect_id.clone(),
            source: source.clone(),
            state: AgentReceiptState::Terminal {
                outcome: AgentTerminalOutcome::Succeeded,
            },
            snapshot_revision: None,
            initial_context: None,
        }
    }

    fn command(
        thread_id: RuntimeThreadId,
        operation: &str,
        command: ManagedRuntimeCommand,
    ) -> ManagedRuntimeCommandEnvelope {
        ManagedRuntimeCommandEnvelope {
            operation_id: RuntimeOperationId::new(operation).expect("operation"),
            idempotency_key: RuntimeIdempotencyKey::new(format!("idem:{operation}"))
                .expect("idempotency"),
            thread_id,
            command,
        }
    }

    fn gateway(
        repository: Arc<FixtureRepository>,
        lifecycle: Arc<FixtureLifecycle>,
    ) -> ProductionManagedAgentRuntimeGateway {
        ProductionManagedAgentRuntimeGateway::new(
            repository,
            lifecycle,
            Arc::new(FixedClock(100)),
            "fixture-dispatcher",
            1_000,
        )
        .expect("gateway")
    }

    #[tokio::test]
    async fn create_activate_duplicate_and_fork_are_one_durable_runtime_fact_graph() {
        let repository = Arc::new(FixtureRepository::default());
        let lifecycle = Arc::new(FixtureLifecycle::new(repository.clone()));
        let gateway = gateway(repository.clone(), lifecycle.clone());
        let parent = RuntimeThreadId::new("parent").expect("thread");
        let create = command(
            parent.clone(),
            "create",
            ManagedRuntimeCommand::Create {
                initial_context: None,
            },
        );
        let created = gateway.execute(create.clone()).await.expect("create");
        assert_eq!(created.status, ManagedRuntimeOperationStatus::Succeeded);
        assert!(matches!(
            created.evidence,
            Some(ManagedRuntimeOperationEvidence::Create { .. })
        ));
        let duplicate = gateway.execute(create).await.expect("duplicate");
        assert!(duplicate.duplicate);
        assert_eq!(lifecycle.create_calls.load(Ordering::SeqCst), 1);

        gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: parent.clone(),
            })
            .await
            .expect("parent snapshot");
        gateway
            .execute(command(
                parent.clone(),
                "activate-parent",
                ManagedRuntimeCommand::Activate,
            ))
            .await
            .expect("activate parent");
        let active = gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: parent.clone(),
            })
            .await
            .expect("active parent");
        assert_eq!(active.lifecycle, ManagedRuntimeLifecycleStatus::Active);

        let child = RuntimeThreadId::new("child").expect("thread");
        let forked = gateway
            .execute(command(
                parent,
                "fork",
                ManagedRuntimeCommand::Fork {
                    child_thread_id: child.clone(),
                    through_completed_turn_id: None,
                },
            ))
            .await
            .expect("fork");
        assert!(matches!(
            forked.evidence,
            Some(ManagedRuntimeOperationEvidence::Fork {
                progress: ManagedRuntimeForkProgressEvidence::Provisioned { .. },
                ..
            })
        ));
        assert_eq!(lifecycle.fork_calls.load(Ordering::SeqCst), 1);
        let child_snapshot = gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: child.clone(),
            })
            .await
            .expect("child snapshot");
        assert_eq!(
            child_snapshot.lifecycle,
            ManagedRuntimeLifecycleStatus::Provisioning
        );
        let activated_child = gateway
            .execute(command(
                child.clone(),
                "activate-child",
                ManagedRuntimeCommand::Activate,
            ))
            .await
            .expect("activate child");
        assert!(matches!(
            activated_child.evidence,
            Some(ManagedRuntimeOperationEvidence::Activate { .. })
        ));
        assert_eq!(
            gateway
                .read(ManagedRuntimeReadRequest { thread_id: child })
                .await
                .expect("read child")
                .lifecycle,
            ManagedRuntimeLifecycleStatus::Active
        );
    }

    #[tokio::test]
    async fn rebind_is_one_idempotent_runtime_operation_with_source_preserving_evidence() {
        let repository = Arc::new(FixtureRepository::default());
        let lifecycle = Arc::new(FixtureLifecycle::new(repository.clone()));
        let gateway = gateway(repository, lifecycle);
        let thread_id = RuntimeThreadId::new("rebind-thread").expect("thread");
        gateway
            .execute(command(
                thread_id.clone(),
                "rebind-create",
                ManagedRuntimeCommand::Create {
                    initial_context: None,
                },
            ))
            .await
            .expect("create");
        gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: thread_id.clone(),
            })
            .await
            .expect("created");
        gateway
            .execute(command(
                thread_id.clone(),
                "rebind-activate",
                ManagedRuntimeCommand::Activate,
            ))
            .await
            .expect("activate");
        gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: thread_id.clone(),
            })
            .await
            .expect("active");
        let command = command(thread_id.clone(), "rebind", ManagedRuntimeCommand::Rebind);

        let rebound = gateway.execute(command.clone()).await.expect("rebind");
        let replay = gateway.execute(command).await.expect("replay rebind");

        assert_eq!(rebound.status, ManagedRuntimeOperationStatus::Succeeded);
        assert!(replay.duplicate);
        assert!(matches!(
            &rebound.evidence,
            Some(ManagedRuntimeOperationEvidence::Rebind {
                previous_binding,
                binding,
            }) if previous_binding.source_ref == binding.source_ref
                && binding.committed_at_revision > previous_binding.committed_at_revision
        ));
        let snapshot = gateway
            .read(ManagedRuntimeReadRequest { thread_id })
            .await
            .expect("rebound snapshot");
        assert_eq!(
            snapshot.lifecycle,
            ManagedRuntimeLifecycleStatus::Provisioning
        );
        assert_eq!(
            snapshot.source_binding,
            rebound.evidence.and_then(|evidence| match evidence {
                ManagedRuntimeOperationEvidence::Rebind { binding, .. } => Some(binding),
                _ => None,
            })
        );
    }

    #[tokio::test]
    async fn resume_without_mapping_is_typed_and_changes_cursor_is_strict() {
        let repository = Arc::new(FixtureRepository::default());
        let lifecycle = Arc::new(FixtureLifecycle::new(repository.clone()));
        let gateway = gateway(repository, lifecycle);
        let thread_id = RuntimeThreadId::new("missing").expect("thread");
        let error = gateway
            .execute(command(
                thread_id.clone(),
                "resume",
                ManagedRuntimeCommand::Resume,
            ))
            .await
            .expect_err("missing Resume mapping");
        assert_eq!(error, ManagedRuntimeGatewayError::NotFound);
        let error = gateway
            .changes(ManagedRuntimeChangesRequest {
                thread_id,
                after: None,
                limit: 0,
            })
            .await
            .expect_err("zero page limit");
        assert!(matches!(error, ManagedRuntimeGatewayError::Invalid { .. }));
    }

    #[tokio::test]
    async fn lost_create_receipt_recovers_by_same_effect_inspection_after_restart() {
        let repository = Arc::new(FixtureRepository::default());
        let lifecycle = Arc::new(FixtureLifecycle::new(repository.clone()));
        lifecycle
            .recover_create_from_inspect
            .store(true, Ordering::SeqCst);
        let initial_gateway = gateway(repository.clone(), lifecycle.clone());
        let thread_id = RuntimeThreadId::new("recover-create").expect("thread");
        let create = command(
            thread_id.clone(),
            "create-lost-receipt",
            ManagedRuntimeCommand::Create {
                initial_context: None,
            },
        );
        let running = initial_gateway
            .execute(create.clone())
            .await
            .expect("accepted Create");
        assert_eq!(running.status, ManagedRuntimeOperationStatus::Running);

        let restarted = gateway(repository, lifecycle.clone());
        let recovered = restarted
            .execute(create)
            .await
            .expect("inspect durable Create intent");
        assert_eq!(recovered.status, ManagedRuntimeOperationStatus::Succeeded);
        assert!(matches!(
            recovered.evidence,
            Some(ManagedRuntimeOperationEvidence::Create { .. })
        ));
        assert_eq!(lifecycle.create_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            restarted
                .read(ManagedRuntimeReadRequest { thread_id })
                .await
                .expect("recovered snapshot")
                .operations
                .last()
                .expect("Create operation")
                .status,
            ManagedRuntimeOperationStatus::Succeeded
        );
    }

    #[tokio::test]
    async fn fork_child_known_without_provisioning_is_terminal_lost_evidence() {
        let repository = Arc::new(FixtureRepository::default());
        let lifecycle = Arc::new(FixtureLifecycle::new(repository.clone()));
        let gateway = gateway(repository, lifecycle.clone());
        let parent = RuntimeThreadId::new("partial-parent").expect("thread");
        gateway
            .execute(command(
                parent.clone(),
                "partial-create",
                ManagedRuntimeCommand::Create {
                    initial_context: None,
                },
            ))
            .await
            .expect("create parent");
        gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: parent.clone(),
            })
            .await
            .expect("read parent");
        gateway
            .execute(command(
                parent.clone(),
                "partial-activate",
                ManagedRuntimeCommand::Activate,
            ))
            .await
            .expect("activate parent");
        gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: parent.clone(),
            })
            .await
            .expect("read active parent");
        lifecycle
            .leave_fork_child_unprovisioned
            .store(true, Ordering::SeqCst);
        let child = RuntimeThreadId::new("partial-child").expect("thread");
        let forked = gateway
            .execute(command(
                parent,
                "partial-fork",
                ManagedRuntimeCommand::Fork {
                    child_thread_id: child.clone(),
                    through_completed_turn_id: None,
                },
            ))
            .await
            .expect("settle partial Fork");
        assert_eq!(forked.status, ManagedRuntimeOperationStatus::Lost);
        assert!(matches!(
            forked.evidence,
            Some(ManagedRuntimeOperationEvidence::Fork {
                progress: ManagedRuntimeForkProgressEvidence::ChildKnown {
                    child_thread_id,
                    ..
                },
                ..
            }) if child_thread_id == child
        ));
        assert_eq!(lifecycle.fork_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            gateway
                .read(ManagedRuntimeReadRequest { thread_id: child })
                .await
                .expect_err("partial child was not provisioned"),
            ManagedRuntimeGatewayError::NotFound
        );
    }

    #[tokio::test]
    async fn fork_child_known_settlement_failure_recovers_to_provisioned_by_same_effect() {
        let repository = Arc::new(FixtureRepository::default());
        let lifecycle = Arc::new(FixtureLifecycle::new(repository.clone()));
        let gateway = gateway(repository, lifecycle.clone());
        let parent = RuntimeThreadId::new("inspection-parent").expect("thread");
        gateway
            .execute(command(
                parent.clone(),
                "inspection-create",
                ManagedRuntimeCommand::Create {
                    initial_context: None,
                },
            ))
            .await
            .expect("create parent");
        gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: parent.clone(),
            })
            .await
            .expect("read parent");
        gateway
            .execute(command(
                parent.clone(),
                "inspection-activate",
                ManagedRuntimeCommand::Activate,
            ))
            .await
            .expect("activate parent");
        gateway
            .read(ManagedRuntimeReadRequest {
                thread_id: parent.clone(),
            })
            .await
            .expect("read active parent");
        lifecycle
            .require_fork_inspection
            .store(true, Ordering::SeqCst);
        let child = RuntimeThreadId::new("inspection-child").expect("thread");
        let fork = command(
            parent,
            "inspection-fork",
            ManagedRuntimeCommand::Fork {
                child_thread_id: child.clone(),
                through_completed_turn_id: None,
            },
        );
        let uncertain = gateway.execute(fork.clone()).await.expect("uncertain Fork");
        assert_eq!(uncertain.status, ManagedRuntimeOperationStatus::Running);
        assert!(matches!(
            uncertain.evidence,
            Some(ManagedRuntimeOperationEvidence::Fork {
                progress: ManagedRuntimeForkProgressEvidence::ChildKnown { .. },
                ..
            })
        ));
        let still_unknown = gateway
            .execute(fork.clone())
            .await
            .expect("keep inspecting Fork");
        assert_eq!(still_unknown.status, ManagedRuntimeOperationStatus::Running);
        assert!(matches!(
            still_unknown.evidence,
            Some(ManagedRuntimeOperationEvidence::Fork {
                progress: ManagedRuntimeForkProgressEvidence::ChildKnown { .. },
                ..
            })
        ));

        lifecycle
            .require_fork_inspection
            .store(false, Ordering::SeqCst);
        lifecycle
            .recover_fork_from_inspect
            .store(true, Ordering::SeqCst);
        let recovered = gateway.execute(fork).await.expect("recover Fork");
        assert_eq!(recovered.status, ManagedRuntimeOperationStatus::Succeeded);
        assert!(matches!(
            recovered.evidence,
            Some(ManagedRuntimeOperationEvidence::Fork {
                progress: ManagedRuntimeForkProgressEvidence::Provisioned { .. },
                ..
            })
        ));
        assert_eq!(
            gateway
                .read(ManagedRuntimeReadRequest { thread_id: child })
                .await
                .expect("read provisioned child")
                .lifecycle,
            ManagedRuntimeLifecycleStatus::Provisioning
        );
        assert_eq!(lifecycle.fork_calls.load(Ordering::SeqCst), 1);
    }
}
