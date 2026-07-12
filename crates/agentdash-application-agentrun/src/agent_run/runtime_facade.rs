use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    AgentRuntimeGateway, ContextCompactionId, ContextCompactionTrigger, EventSequence,
    IdempotencyKey, InteractionResponse, OperationMeta, OperationReceipt, RuntimeActor,
    RuntimeCommand, RuntimeCommandEnvelope, RuntimeEventStream, RuntimeExecuteError, RuntimeInput,
    RuntimeInteractionId, RuntimeOperationId, RuntimeSnapshot, RuntimeSnapshotError,
    RuntimeSnapshotQuery, RuntimeSnapshotResult, RuntimeSubscribeError, RuntimeThreadId,
    RuntimeThreadStatus, RuntimeTurnId,
};
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeBindingRepository,
    AgentRunRuntimeProvisionRequest, AgentRunRuntimeProvisioner, AgentRunRuntimeRecoveryState,
    AgentRunRuntimeTarget,
};
use agentdash_application_ports::launch::BackendSelectionInput;
use agentdash_spi::AuthIdentity;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunRuntimeView {
    pub target: AgentRunRuntimeTarget,
    pub binding: Option<AgentRunRuntimeBinding>,
    pub snapshot: Option<RuntimeSnapshot>,
    pub binding_epoch: Option<agentdash_agent_runtime_contract::BindingEpoch>,
    pub recovery: AgentRunRuntimeRecoverySummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunRuntimeRecoverySummary {
    Active,
    Lost,
    Recovering,
    RecoveryFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunCommandGuard {
    pub thread_id: RuntimeThreadId,
    pub expected_revision: agentdash_agent_runtime_contract::RuntimeRevision,
    pub expected_active_turn_id: Option<RuntimeTurnId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendAgentRunMessage {
    pub target: AgentRunRuntimeTarget,
    pub client_command_id: String,
    pub input: Vec<RuntimeInput>,
    pub actor: RuntimeActor,
    pub identity: Option<AuthIdentity>,
    pub backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuardedAgentRunCommand {
    pub target: AgentRunRuntimeTarget,
    pub client_command_id: String,
    pub guard: AgentRunCommandGuard,
    pub actor: RuntimeActor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SteerAgentRunTurn {
    pub command: GuardedAgentRunCommand,
    pub input: Vec<RuntimeInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolveAgentRunInteraction {
    pub command: GuardedAgentRunCommand,
    pub interaction_id: RuntimeInteractionId,
    pub response: InteractionResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadAgentRunEvents {
    pub target: AgentRunRuntimeTarget,
    pub after: Option<EventSequence>,
    pub include_transient: bool,
}

#[derive(Debug, Error)]
pub enum AgentRunRuntimeError {
    #[error("AgentRun has no runtime binding")]
    BindingNotFound,
    #[error("AgentRun runtime binding failed: {0}")]
    Binding(#[from] AgentRunRuntimeBindingError),
    #[error("AgentRun runtime command failed: {0}")]
    Execute(#[from] RuntimeExecuteError),
    #[error("AgentRun runtime snapshot failed: {0}")]
    Snapshot(#[from] RuntimeSnapshotError),
    #[error("AgentRun runtime event read failed: {0}")]
    Events(#[from] RuntimeSubscribeError),
    #[error("AgentRun runtime command guard targets another thread")]
    StaleThread,
    #[error("AgentRun active turn changed")]
    StaleActiveTurn,
    #[error("AgentRun runtime returned an unexpected snapshot result")]
    UnexpectedSnapshot,
    #[error("AgentRun client command id is empty")]
    EmptyClientCommandId,
    #[error("AgentRun client command id is already bound to a different command")]
    ClientCommandConflict,
}

#[async_trait]
pub trait AgentRunRuntime: Send + Sync {
    async fn inspect(
        &self,
        target: AgentRunRuntimeTarget,
    ) -> Result<AgentRunRuntimeView, AgentRunRuntimeError>;

    async fn send_message(
        &self,
        command: SendAgentRunMessage,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    async fn compact_context(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    async fn steer_active_turn(
        &self,
        command: SteerAgentRunTurn,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    async fn interrupt_active_turn(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    async fn resolve_interaction(
        &self,
        command: ResolveAgentRunInteraction,
    ) -> Result<OperationReceipt, AgentRunRuntimeError>;

    async fn read_context(
        &self,
        target: AgentRunRuntimeTarget,
    ) -> Result<agentdash_agent_runtime_contract::RuntimeContextView, AgentRunRuntimeError>;

    async fn read_events(
        &self,
        query: ReadAgentRunEvents,
    ) -> Result<Box<dyn RuntimeEventStream>, AgentRunRuntimeError>;
}

pub struct ManagedAgentRunRuntime {
    gateway: Arc<dyn AgentRuntimeGateway>,
    bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
    provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
}

impl ManagedAgentRunRuntime {
    pub fn new(
        gateway: Arc<dyn AgentRuntimeGateway>,
        bindings: Arc<dyn AgentRunRuntimeBindingRepository>,
        provisioner: Arc<dyn AgentRunRuntimeProvisioner>,
    ) -> Self {
        Self {
            gateway,
            bindings,
            provisioner,
        }
    }

    fn operation_identity(
        target: &AgentRunRuntimeTarget,
        client_command_id: &str,
    ) -> Result<String, AgentRunRuntimeError> {
        let client_command_id = client_command_id.trim();
        if client_command_id.is_empty() {
            return Err(AgentRunRuntimeError::EmptyClientCommandId);
        }
        Ok(format!(
            "agentrun-{}-{}-{client_command_id}",
            target.run_id, target.agent_id
        ))
    }

    async fn replay_existing<F>(
        &self,
        target: &AgentRunRuntimeTarget,
        client_command_id: &str,
        actor: &RuntimeActor,
        matches_command: F,
    ) -> Result<Option<OperationReceipt>, AgentRunRuntimeError>
    where
        F: FnOnce(&RuntimeCommand) -> bool,
    {
        let operation_id =
            RuntimeOperationId::new(Self::operation_identity(target, client_command_id)?)
                .expect("non-empty AgentRun operation identity");
        match self
            .gateway
            .snapshot(RuntimeSnapshotQuery::Operation { operation_id })
            .await
        {
            Ok(RuntimeSnapshotResult::Operation { operation }) => {
                if &operation.actor != actor || !matches_command(&operation.command) {
                    return Err(AgentRunRuntimeError::ClientCommandConflict);
                }
                Ok(Some(
                    self.gateway
                        .execute(RuntimeCommandEnvelope {
                            meta: OperationMeta {
                                operation_id: operation.operation_id,
                                idempotency_key: operation.idempotency_key,
                                expected_thread_revision: None,
                                actor: operation.actor,
                            },
                            command: operation.command,
                        })
                        .await?,
                ))
            }
            Err(RuntimeSnapshotError::NotFound) => Ok(None),
            Ok(_) => Err(AgentRunRuntimeError::UnexpectedSnapshot),
            Err(error) => Err(error.into()),
        }
    }

    async fn binding(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError> {
        self.bindings
            .load(target)
            .await?
            .ok_or(AgentRunRuntimeError::BindingNotFound)
    }

    async fn snapshot_for(
        &self,
        binding: &AgentRunRuntimeBinding,
    ) -> Result<Option<RuntimeSnapshot>, AgentRunRuntimeError> {
        match self
            .gateway
            .snapshot(RuntimeSnapshotQuery::Thread {
                thread_id: binding.thread_id.clone(),
                at_revision: None,
            })
            .await
        {
            Ok(RuntimeSnapshotResult::Thread { snapshot }) => Ok(Some(*snapshot)),
            Ok(_) => Err(AgentRunRuntimeError::UnexpectedSnapshot),
            Err(RuntimeSnapshotError::NotFound) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    async fn reconcile_committed_recovery(
        &self,
        target: &AgentRunRuntimeTarget,
        binding: &AgentRunRuntimeBinding,
    ) -> Result<(), AgentRunRuntimeError> {
        let Some(intent) = self.bindings.load_active_recovery(target).await? else {
            return Ok(());
        };
        if intent.state == AgentRunRuntimeRecoveryState::HostBound
            && intent.proposed_binding_id == binding.binding_id
        {
            self.bindings
                .advance_recovery(
                    &intent.id,
                    AgentRunRuntimeRecoveryState::HostBound,
                    AgentRunRuntimeRecoveryState::Committed,
                    None,
                )
                .await?;
        }
        Ok(())
    }

    fn envelope(
        target: &AgentRunRuntimeTarget,
        client_command_id: &str,
        expected_thread_revision: Option<agentdash_agent_runtime_contract::RuntimeRevision>,
        actor: RuntimeActor,
        command: RuntimeCommand,
    ) -> Result<RuntimeCommandEnvelope, AgentRunRuntimeError> {
        let identity = Self::operation_identity(target, client_command_id)?;
        Ok(RuntimeCommandEnvelope {
            meta: OperationMeta {
                operation_id: RuntimeOperationId::new(identity.clone())
                    .expect("non-empty AgentRun operation identity"),
                idempotency_key: IdempotencyKey::new(identity)
                    .expect("non-empty AgentRun idempotency identity"),
                expected_thread_revision,
                actor,
            },
            command,
        })
    }

    async fn guarded_binding(
        &self,
        command: &GuardedAgentRunCommand,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError> {
        let binding = self.coordinate_binding(command).await?;
        let snapshot = self
            .snapshot_for(&binding)
            .await?
            .ok_or(AgentRunRuntimeError::BindingNotFound)?;
        if snapshot.active_turn_id != command.guard.expected_active_turn_id {
            return Err(AgentRunRuntimeError::StaleActiveTurn);
        }
        Ok(binding)
    }

    async fn coordinate_binding(
        &self,
        command: &GuardedAgentRunCommand,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError> {
        let binding = self.binding(&command.target).await?;
        if binding.thread_id != command.guard.thread_id {
            return Err(AgentRunRuntimeError::StaleThread);
        }
        Ok(binding)
    }
}

#[async_trait]
impl AgentRunRuntime for ManagedAgentRunRuntime {
    async fn inspect(
        &self,
        target: AgentRunRuntimeTarget,
    ) -> Result<AgentRunRuntimeView, AgentRunRuntimeError> {
        let binding = self.bindings.load(&target).await?;
        let snapshot = match &binding {
            Some(binding) => self.snapshot_for(binding).await?,
            None => None,
        };
        let latest_recovery = self.bindings.load_latest_recovery(&target).await?;
        let recovery = match latest_recovery.as_ref().map(|intent| intent.state) {
            Some(
                AgentRunRuntimeRecoveryState::Prepared | AgentRunRuntimeRecoveryState::HostBound,
            ) => AgentRunRuntimeRecoverySummary::Recovering,
            Some(AgentRunRuntimeRecoveryState::Failed) => {
                AgentRunRuntimeRecoverySummary::RecoveryFailed
            }
            _ if snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.status == RuntimeThreadStatus::Lost) =>
            {
                AgentRunRuntimeRecoverySummary::Lost
            }
            _ => AgentRunRuntimeRecoverySummary::Active,
        };
        let binding_epoch = binding.as_ref().map(|binding| binding.binding_epoch);
        Ok(AgentRunRuntimeView {
            target,
            binding,
            snapshot,
            binding_epoch,
            recovery,
        })
    }

    async fn send_message(
        &self,
        command: SendAgentRunMessage,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        Self::operation_identity(&command.target, &command.client_command_id)?;
        let mut binding = match self.bindings.load(&command.target).await? {
            Some(binding) => binding,
            None => {
                self.provisioner
                    .provision(&AgentRunRuntimeProvisionRequest {
                        target: command.target.clone(),
                        identity: command.identity.clone(),
                        backend_selection: command.backend_selection.clone(),
                    })
                    .await?
            }
        };
        self.reconcile_committed_recovery(&command.target, &binding)
            .await?;
        if let Some(receipt) = self
            .replay_existing(
                &command.target,
                &command.client_command_id,
                &command.actor,
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::ThreadStart { thread_id, input, .. }
                            | RuntimeCommand::TurnStart { thread_id, input }
                            if thread_id == &binding.thread_id && input == &command.input
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let mut snapshot = self.snapshot_for(&binding).await?;
        if snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.status == RuntimeThreadStatus::Lost)
        {
            let revision = snapshot.as_ref().expect("lost snapshot exists").revision;
            binding = self.provisioner.recover(&binding, revision).await?;
            snapshot = self.snapshot_for(&binding).await?;
        }
        let expected = snapshot.as_ref().map(|snapshot| snapshot.revision);
        let runtime_command = match snapshot {
            None => RuntimeCommand::ThreadStart {
                thread_id: binding.thread_id.clone(),
                binding_id: binding.binding_id.clone(),
                driver_generation: binding.driver_generation,
                source_thread_id: binding.source_thread_id.clone(),
                profile_digest: binding.profile_digest.clone(),
                bound_profile: Box::new(binding.bound_profile.clone()),
                input: command.input,
                surface_digest: binding.surface_digest,
                settings_revision: binding.settings_revision,
                tool_set_revision: binding.tool_set_revision,
                hook_plan: binding.hook_plan,
            },
            Some(_) => RuntimeCommand::TurnStart {
                thread_id: binding.thread_id,
                input: command.input,
            },
        };
        Ok(self
            .gateway
            .execute(Self::envelope(
                &command.target,
                &command.client_command_id,
                expected,
                command.actor,
                runtime_command,
            )?)
            .await?)
    }

    async fn compact_context(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.coordinate_binding(&command).await?;
        let compaction_id = ContextCompactionId::new(format!(
            "compaction-{}-{}-{}",
            command.target.run_id, command.target.agent_id, command.client_command_id
        ))
        .expect("non-empty compaction identity");
        if let Some(receipt) = self
            .replay_existing(
                &command.target,
                &command.client_command_id,
                &command.actor,
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::ContextCompact {
                            thread_id,
                            compaction_id: existing_id,
                            ..
                        } if thread_id == &command.guard.thread_id && existing_id == &compaction_id
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let binding = self.guarded_binding(&command).await?;
        let snapshot = self
            .snapshot_for(&binding)
            .await?
            .ok_or(AgentRunRuntimeError::BindingNotFound)?;
        Ok(self
            .gateway
            .execute(Self::envelope(
                &command.target,
                &command.client_command_id,
                Some(command.guard.expected_revision),
                command.actor,
                RuntimeCommand::ContextCompact {
                    thread_id: binding.thread_id,
                    compaction_id,
                    trigger: ContextCompactionTrigger::Manual,
                    base_checkpoint_id: snapshot.active_checkpoint_id,
                    expected_context_revision: snapshot.context_revision,
                },
            )?)
            .await?)
    }

    async fn steer_active_turn(
        &self,
        command: SteerAgentRunTurn,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.coordinate_binding(&command.command).await?;
        if let Some(receipt) = self
            .replay_existing(
                &command.command.target,
                &command.command.client_command_id,
                &command.command.actor,
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::TurnSteer {
                            thread_id,
                            expected_turn_id,
                            input,
                        } if thread_id == &command.command.guard.thread_id
                            && Some(expected_turn_id) == command.command.guard.expected_active_turn_id.as_ref()
                            && input == &command.input
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let binding = self.guarded_binding(&command.command).await?;
        let turn_id = command
            .command
            .guard
            .expected_active_turn_id
            .clone()
            .ok_or(AgentRunRuntimeError::StaleActiveTurn)?;
        Ok(self
            .gateway
            .execute(Self::envelope(
                &command.command.target,
                &command.command.client_command_id,
                Some(command.command.guard.expected_revision),
                command.command.actor,
                RuntimeCommand::TurnSteer {
                    thread_id: binding.thread_id,
                    expected_turn_id: turn_id,
                    input: command.input,
                },
            )?)
            .await?)
    }

    async fn interrupt_active_turn(
        &self,
        command: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.coordinate_binding(&command).await?;
        if let Some(receipt) = self
            .replay_existing(
                &command.target,
                &command.client_command_id,
                &command.actor,
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::TurnInterrupt {
                            thread_id,
                            expected_turn_id,
                        } if thread_id == &command.guard.thread_id
                            && Some(expected_turn_id) == command.guard.expected_active_turn_id.as_ref()
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let binding = self.guarded_binding(&command).await?;
        let turn_id = command
            .guard
            .expected_active_turn_id
            .clone()
            .ok_or(AgentRunRuntimeError::StaleActiveTurn)?;
        Ok(self
            .gateway
            .execute(Self::envelope(
                &command.target,
                &command.client_command_id,
                Some(command.guard.expected_revision),
                command.actor,
                RuntimeCommand::TurnInterrupt {
                    thread_id: binding.thread_id,
                    expected_turn_id: turn_id,
                },
            )?)
            .await?)
    }

    async fn resolve_interaction(
        &self,
        command: ResolveAgentRunInteraction,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        self.coordinate_binding(&command.command).await?;
        if let Some(receipt) = self
            .replay_existing(
                &command.command.target,
                &command.command.client_command_id,
                &command.command.actor,
                |existing| {
                    matches!(
                        existing,
                        RuntimeCommand::InteractionRespond {
                            thread_id,
                            interaction_id,
                            response,
                        } if thread_id == &command.command.guard.thread_id
                            && interaction_id == &command.interaction_id
                            && response == &command.response
                    )
                },
            )
            .await?
        {
            return Ok(receipt);
        }
        let binding = self.guarded_binding(&command.command).await?;
        Ok(self
            .gateway
            .execute(Self::envelope(
                &command.command.target,
                &command.command.client_command_id,
                Some(command.command.guard.expected_revision),
                command.command.actor,
                RuntimeCommand::InteractionRespond {
                    thread_id: binding.thread_id,
                    interaction_id: command.interaction_id,
                    response: command.response,
                },
            )?)
            .await?)
    }

    async fn read_context(
        &self,
        target: AgentRunRuntimeTarget,
    ) -> Result<agentdash_agent_runtime_contract::RuntimeContextView, AgentRunRuntimeError> {
        let binding = self.binding(&target).await?;
        match self
            .gateway
            .snapshot(RuntimeSnapshotQuery::Context {
                thread_id: binding.thread_id,
                at_context_revision: None,
            })
            .await?
        {
            RuntimeSnapshotResult::Context { context } => Ok(*context),
            _ => Err(AgentRunRuntimeError::UnexpectedSnapshot),
        }
    }

    async fn read_events(
        &self,
        query: ReadAgentRunEvents,
    ) -> Result<Box<dyn RuntimeEventStream>, AgentRunRuntimeError> {
        let binding = self.binding(&query.target).await?;
        Ok(self
            .gateway
            .events(agentdash_agent_runtime_contract::RuntimeEventSubscription {
                thread_id: binding.thread_id,
                after: query.after,
                include_transient: query.include_transient,
            })
            .await?)
    }
}
