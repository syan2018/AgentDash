use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    CommandAvailability, OperationReceipt, PresentationThreadId, RuntimeActor, RuntimeCommandKind,
    RuntimeInput,
};
use agentdash_application_ports::agent_run_message_submission::{
    AgentRunAcceptedDeliveryKind, AgentRunMailboxAcceptedSettlement,
    AgentRunMailboxDeliverySettlementPort, AgentRunMailboxFailedSettlement,
};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_application_ports::launch::BackendSelectionInput;
use agentdash_application_ports::request_digest::canonical_request_digest;
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxClaimRequest, AgentRunMailboxCreateOutcome, AgentRunMailboxMessage,
    AgentRunMailboxRepository, ConsumptionBarrier, MailboxDelivery, MailboxDrainMode,
    MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity, NewAgentRunMailboxMessage,
};
use agentdash_domain::workflow::AgentRunAcceptedRefs;
use agentdash_platform_spi::{AgentConfig, AuthIdentity};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::runtime_facade::{
    AcceptAgentRunMessage, AgentRunMessageAcceptedDelivery, AgentRunMessageAdmission,
    AgentRunMessageDeliveryPreference, AgentRunPresentationDraft,
};
use super::{
    AgentRunMessageDeliveryAttempt, AgentRunMessageDeliveryCoordinator, AgentRunRuntime,
    AgentRunRuntimeError,
};

const CLAIM_LEASE_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct EnqueueRuntimeMailboxMessage {
    pub target: AgentRunRuntimeTarget,
    pub presentation_thread_id: PresentationThreadId,
    pub presentation: AgentRunPresentationDraft,
    pub client_command_id: String,
    pub input: Vec<RuntimeInput>,
    pub actor: RuntimeActor,
    pub identity: Option<AuthIdentity>,
    pub origin: MailboxMessageOrigin,
    pub source: MailboxSourceIdentity,
    pub delivery_intent: Option<String>,
    pub executor_config: Option<AgentConfig>,
    pub backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredRuntimeMailboxCommand {
    target: AgentRunRuntimeTarget,
    presentation_thread_id: PresentationThreadId,
    presentation: AgentRunPresentationDraft,
    input: Vec<RuntimeInput>,
    actor: RuntimeActor,
    identity: Option<AuthIdentity>,
    execution_profile_override: Option<AgentConfig>,
    backend_selection: Option<BackendSelectionInput>,
}

impl StoredRuntimeMailboxCommand {
    fn into_accept_message(
        self,
        message_id: Uuid,
        delivery: &MailboxDelivery,
    ) -> AcceptAgentRunMessage {
        AcceptAgentRunMessage {
            target: self.target,
            presentation_thread_id: self.presentation_thread_id,
            presentation: self.presentation,
            client_command_id: format!("mailbox-{message_id}"),
            input: self.input,
            actor: self.actor,
            identity: self.identity,
            execution_profile_override: self.execution_profile_override,
            backend_selection: self.backend_selection,
            delivery_preference: if matches!(delivery, MailboxDelivery::SteerActiveTurn { .. }) {
                AgentRunMessageDeliveryPreference::PreferSteer
            } else {
                AgentRunMessageDeliveryPreference::StartWhenIdle
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeMailboxSubmitOutcome {
    Queued {
        message: AgentRunMailboxMessage,
    },
    Dispatched {
        message: AgentRunMailboxMessage,
        receipt: OperationReceipt,
        steered: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliverAgentRunProductInput {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub presentation_thread_id: PresentationThreadId,
    pub presentation: AgentRunPresentationDraft,
    pub input: Vec<RuntimeInput>,
    pub actor: RuntimeActor,
    pub client_command_id: String,
    pub backend_selection: Option<BackendSelectionInput>,
    pub identity: Option<AuthIdentity>,
    pub origin: MailboxMessageOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunProductDelivery {
    pub mailbox_message_id: Uuid,
    pub operation_receipt: Option<OperationReceipt>,
    pub queued: bool,
}

#[async_trait::async_trait]
pub trait AgentRunProductDeliveryPort: Send + Sync {
    async fn deliver(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductDelivery, RuntimeMailboxError>;
}

#[derive(Debug, Error)]
pub enum RuntimeMailboxError {
    #[error("runtime mailbox persistence failed: {0}")]
    Persistence(#[from] agentdash_domain::DomainError),
    #[error("runtime mailbox command payload is invalid: {0}")]
    InvalidPayload(String),
    #[error("AgentRun runtime failed: {0}")]
    Runtime(#[from] AgentRunRuntimeError),
    #[error("runtime mailbox delivery failed after durable settlement: {0}")]
    DeliveryFailed(String),
}

#[derive(Clone)]
pub struct RuntimeAgentRunMailbox {
    repository: Arc<dyn AgentRunMailboxRepository>,
    runtime: Arc<dyn AgentRunRuntime>,
    settlements: Arc<dyn AgentRunMailboxDeliverySettlementPort>,
}

enum RuntimeMailboxDrainResult {
    Deferred,
    Accepted {
        message: AgentRunMailboxMessage,
        receipt: OperationReceipt,
        delivery: AgentRunMessageAcceptedDelivery,
    },
    Failed {
        message: AgentRunMailboxMessage,
        error: String,
    },
}

#[derive(Clone)]
pub struct RuntimeMailboxTerminalConvergence {
    bindings:
        Arc<dyn agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository>,
    mailbox: RuntimeAgentRunMailbox,
}

impl RuntimeMailboxTerminalConvergence {
    pub fn new(
        bindings: Arc<
            dyn agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository,
        >,
        mailbox: RuntimeAgentRunMailbox,
    ) -> Self {
        Self { bindings, mailbox }
    }
}

#[async_trait::async_trait]
impl agentdash_application_ports::agent_run_control_effect::AgentRunDeliveryTerminalConvergencePort
    for RuntimeMailboxTerminalConvergence
{
    async fn converge_delivery_terminal(
        &self,
        input: &agentdash_application_ports::agent_run_control_effect::AgentRunTerminalControlInput,
    ) -> Result<(), String> {
        let binding = self
            .bindings
            .load_by_thread_id(&input.runtime_thread_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                format!(
                    "terminal effect targets an unbound Runtime thread {}",
                    input.runtime_thread_id
                )
            })?;
        if binding.presentation_thread_id != input.presentation_thread_id {
            return Err(format!(
                "terminal effect presentation thread {} does not match binding {}",
                input.presentation_thread_id, binding.presentation_thread_id
            ));
        }
        if binding.binding_id != input.binding_id
            || binding.driver_generation != input.driver_generation
            || binding.source_thread_id.as_str() != input.source_thread_id
        {
            return Err(format!(
                "terminal effect coordinates no longer match Runtime binding {} generation {} source {}",
                input.binding_id, input.driver_generation.0, input.source_thread_id
            ));
        }
        self.mailbox
            .recover_and_drain_once(&binding.target)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

impl RuntimeAgentRunMailbox {
    pub fn new(
        repository: Arc<dyn AgentRunMailboxRepository>,
        runtime: Arc<dyn AgentRunRuntime>,
        settlements: Arc<dyn AgentRunMailboxDeliverySettlementPort>,
    ) -> Self {
        Self {
            repository,
            runtime,
            settlements,
        }
    }

    pub async fn promote(
        &self,
        target: &AgentRunRuntimeTarget,
        message_id: Uuid,
    ) -> Result<AgentRunMailboxMessage, RuntimeMailboxError> {
        Ok(self
            .repository
            .promote_message(target.run_id, target.agent_id, message_id, 100)
            .await?)
    }

    pub fn prepare_message(
        &self,
        command: EnqueueRuntimeMailboxMessage,
    ) -> Result<NewAgentRunMailboxMessage, RuntimeMailboxError> {
        let prefer_steer = command.delivery_intent.as_deref() == Some("steer")
            && command.presentation.emits_user_submission();
        let message_id = Uuid::new_v4();
        let visible_content = command.presentation.content.clone();
        let delivery = if prefer_steer {
            MailboxDelivery::SteerActiveTurn {
                stop_effect: agentdash_domain::agent_run_mailbox::SteeringStopEffect::None,
            }
        } else {
            MailboxDelivery::LaunchOrContinueTurn
        };
        let barrier = if prefer_steer {
            ConsumptionBarrier::AgentLoopTurnBoundary
        } else {
            ConsumptionBarrier::ImmediateIfIdle
        };
        let drain_mode = if prefer_steer {
            MailboxDrainMode::All
        } else {
            MailboxDrainMode::One
        };
        let retain_payload = command.origin != MailboxMessageOrigin::User;
        let stored_command = StoredRuntimeMailboxCommand {
            target: command.target.clone(),
            presentation_thread_id: command.presentation_thread_id.clone(),
            presentation: command.presentation.clone(),
            input: command.input.clone(),
            actor: command.actor.clone(),
            identity: command.identity.clone(),
            execution_profile_override: command.executor_config.clone(),
            backend_selection: command.backend_selection.clone(),
        };
        let delivery_request_digest = canonical_request_digest(&serde_json::json!({
            // Producer idempotency covers the stable delivery draft only.
            // The mailbox id is allocated after this semantic identity and is
            // used solely to derive the canonical Runtime operation key. The
            // authentication snapshot is persisted for first admission but is
            // not product semantics: profile/avatar/group refreshes cannot
            // turn a network retry into a different delivery request.
            "runtime_delivery_draft": {
                "target": &stored_command.target,
                "presentation_thread_id": &stored_command.presentation_thread_id,
                "presentation": &stored_command.presentation,
                "input": &stored_command.input,
                "actor": &stored_command.actor,
                "execution_profile_override": &stored_command.execution_profile_override,
                "backend_selection": &stored_command.backend_selection,
            },
            "origin": command.origin.as_str(),
            "source": {
                "namespace": &command.source.namespace,
                "kind": &command.source.kind,
                "source_ref": &command.source.source_ref,
                "correlation_ref": &command.source.correlation_ref,
                "actor": &command.source.actor,
                "route": &command.source.route,
                "display_label_key": &command.source.display_label_key,
                "metadata": &command.source.metadata,
            },
            "delivery": delivery.to_json(),
            "barrier": barrier.as_str(),
            "drain_mode": drain_mode.as_str(),
            "retain_payload": retain_payload,
        }))
        .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
        let payload = serde_json::to_value(stored_command)
            .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
        let visible_input = serde_json::to_value(&visible_content)
            .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
        Ok(NewAgentRunMailboxMessage {
            id: Some(message_id),
            run_id: command.target.run_id,
            agent_id: command.target.agent_id,
            origin: command.origin,
            source: command.source,
            delivery,
            barrier,
            drain_mode,
            priority: 0,
            source_dedup_key: Some(format!(
                "runtime-command:{}:{}:{}",
                command.target.run_id, command.target.agent_id, command.client_command_id
            )),
            delivery_request_digest,
            payload_json: Some(visible_input),
            launch_planning_input: Some(payload),
            preview: presentation_preview(&visible_content),
            has_images: presentation_has_images(&visible_content),
            retain_payload,
        })
    }

    pub async fn submit(
        &self,
        command: EnqueueRuntimeMailboxMessage,
    ) -> Result<RuntimeMailboxSubmitOutcome, RuntimeMailboxError> {
        let target = command.target.clone();
        let prepared = self.prepare_message(command)?;
        let creation = self
            .repository
            .create_message_idempotent(prepared.clone())
            .await?;
        let (message, existing) = match creation {
            AgentRunMailboxCreateOutcome::Created(message) => (message, false),
            AgentRunMailboxCreateOutcome::Existing(message) => (message, true),
        };
        if existing {
            if matches!(
                message.status,
                MailboxMessageStatus::Dispatched | MailboxMessageStatus::Steered
            ) {
                return self.replay_terminal_delivery(message, prepared).await;
            }
            if matches!(
                message.status,
                MailboxMessageStatus::Failed | MailboxMessageStatus::Deleted
            ) {
                return Err(RuntimeMailboxError::DeliveryFailed(
                    message
                        .last_error
                        .clone()
                        .unwrap_or_else(|| format!("mailbox message {} is terminal", message.id)),
                ));
            }
        }

        match self.drain_once_with_settlement(&target).await? {
            RuntimeMailboxDrainResult::Accepted {
                message: delivered,
                receipt,
                delivery,
                ..
            } if delivered.id == message.id => Ok(RuntimeMailboxSubmitOutcome::Dispatched {
                message: delivered,
                receipt,
                steered: delivery == AgentRunMessageAcceptedDelivery::Steered,
            }),
            RuntimeMailboxDrainResult::Failed {
                message: failed,
                error,
                ..
            } if failed.id == message.id => Err(RuntimeMailboxError::DeliveryFailed(error)),
            RuntimeMailboxDrainResult::Deferred
            | RuntimeMailboxDrainResult::Accepted { .. }
            | RuntimeMailboxDrainResult::Failed { .. } => {
                Ok(RuntimeMailboxSubmitOutcome::Queued { message })
            }
        }
    }

    async fn replay_terminal_delivery(
        &self,
        message: AgentRunMailboxMessage,
        prepared: NewAgentRunMailboxMessage,
    ) -> Result<RuntimeMailboxSubmitOutcome, RuntimeMailboxError> {
        let command: StoredRuntimeMailboxCommand =
            serde_json::from_value(prepared.launch_planning_input.ok_or_else(|| {
                RuntimeMailboxError::InvalidPayload("runtime delivery command is missing".into())
            })?)
            .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
        let admission = self
            .runtime
            .accept_message(command.into_accept_message(message.id, &message.delivery))
            .await?;
        let AgentRunMessageAdmission::Accepted { receipt, delivery } = admission else {
            return Err(RuntimeMailboxError::InvalidPayload(format!(
                "terminal mailbox message {} has no canonical Runtime operation",
                message.id
            )));
        };
        let expected_status = match delivery {
            AgentRunMessageAcceptedDelivery::Started => MailboxMessageStatus::Dispatched,
            AgentRunMessageAcceptedDelivery::Steered => MailboxMessageStatus::Steered,
        };
        if message.status != expected_status {
            return Err(RuntimeMailboxError::InvalidPayload(format!(
                "terminal mailbox message {} delivery disagrees with canonical Runtime operation",
                message.id
            )));
        }
        if message.accepted_runtime_operation_id.as_deref() != Some(receipt.operation_id.as_str()) {
            return Err(RuntimeMailboxError::InvalidPayload(format!(
                "terminal mailbox message {} operation reference disagrees with canonical Runtime operation",
                message.id
            )));
        }
        Ok(RuntimeMailboxSubmitOutcome::Dispatched {
            message,
            receipt,
            steered: delivery == AgentRunMessageAcceptedDelivery::Steered,
        })
    }

    pub async fn recover_and_drain_once(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<(AgentRunMailboxMessage, OperationReceipt, bool)>, RuntimeMailboxError> {
        self.repository
            .recover_expired_consuming(Utc::now())
            .await?;
        match self.drain_once_with_settlement(target).await? {
            RuntimeMailboxDrainResult::Accepted {
                message,
                receipt,
                delivery,
                ..
            } => Ok(Some((
                message,
                receipt,
                delivery == AgentRunMessageAcceptedDelivery::Steered,
            ))),
            RuntimeMailboxDrainResult::Deferred | RuntimeMailboxDrainResult::Failed { .. } => {
                Ok(None)
            }
        }
    }

    pub async fn recover_pending_once(&self) -> Result<usize, RuntimeMailboxError> {
        self.repository
            .recover_expired_consuming(Utc::now())
            .await?;
        let targets = self.repository.list_pending_targets().await?;
        let mut settled = 0;
        for (run_id, agent_id) in targets {
            if !matches!(
                self.drain_once_with_settlement(&AgentRunRuntimeTarget { run_id, agent_id })
                    .await?,
                RuntimeMailboxDrainResult::Deferred
            ) {
                settled += 1;
            }
        }
        Ok(settled)
    }

    pub async fn drain_once(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<(AgentRunMailboxMessage, OperationReceipt, bool)>, RuntimeMailboxError> {
        match self.drain_once_with_settlement(target).await? {
            RuntimeMailboxDrainResult::Deferred => Ok(None),
            RuntimeMailboxDrainResult::Accepted {
                message,
                receipt,
                delivery,
                ..
            } => Ok(Some((
                message,
                receipt,
                delivery == AgentRunMessageAcceptedDelivery::Steered,
            ))),
            RuntimeMailboxDrainResult::Failed { error, .. } => {
                Err(RuntimeMailboxError::DeliveryFailed(error))
            }
        }
    }

    async fn drain_once_with_settlement(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<RuntimeMailboxDrainResult, RuntimeMailboxError> {
        let claim_token = Uuid::new_v4();
        if let Some(message) = self
            .repository
            .claim_reconciliation(
                target.run_id,
                target.agent_id,
                claim_token,
                Utc::now() + Duration::seconds(CLAIM_LEASE_SECONDS),
            )
            .await?
        {
            return self.deliver_claimed(target, message, claim_token).await;
        }

        let view = self.runtime.inspect(target.clone()).await?;
        let can_start = runtime_can_start_turn(&view);
        let can_steer = runtime_can_steer_turn(&view);
        if !can_start && !can_steer {
            return Ok(RuntimeMailboxDrainResult::Deferred);
        }
        let Some(message) = self
            .repository
            .claim_next(AgentRunMailboxClaimRequest {
                run_id: target.run_id,
                agent_id: target.agent_id,
                barriers: if can_steer {
                    vec![ConsumptionBarrier::AgentLoopTurnBoundary]
                } else {
                    vec![
                        ConsumptionBarrier::ImmediateIfIdle,
                        ConsumptionBarrier::AgentRunTurnBoundary,
                        ConsumptionBarrier::AgentLoopTurnBoundary,
                    ]
                },
                drain_mode: None,
                limit: 1,
                claim_token,
                claim_expires_at: Utc::now() + Duration::seconds(CLAIM_LEASE_SECONDS),
            })
            .await?
            .into_iter()
            .next()
        else {
            return Ok(RuntimeMailboxDrainResult::Deferred);
        };
        self.deliver_claimed(target, message, claim_token).await
    }

    async fn deliver_claimed(
        &self,
        target: &AgentRunRuntimeTarget,
        message: AgentRunMailboxMessage,
        claim_token: Uuid,
    ) -> Result<RuntimeMailboxDrainResult, RuntimeMailboxError> {
        let command_value = match message.launch_planning_input.clone() {
            Some(command) => command,
            None => {
                return self
                    .settle_failed_claim(
                        message,
                        claim_token,
                        "runtime delivery command is missing",
                    )
                    .await;
            }
        };
        let command: StoredRuntimeMailboxCommand = match serde_json::from_value(command_value) {
            Ok(command) => command,
            Err(error) => {
                return self
                    .settle_failed_claim(message, claim_token, error.to_string())
                    .await;
            }
        };
        if command.target != *target {
            return self
                .settle_failed_claim(
                    message,
                    claim_token,
                    "payload target does not match mailbox ownership",
                )
                .await;
        }
        let admission = self
            .runtime
            .accept_message(command.into_accept_message(message.id, &message.delivery))
            .await;
        let (receipt, delivery) = match admission {
            Ok(AgentRunMessageAdmission::Accepted { receipt, delivery }) => (receipt, delivery),
            Ok(AgentRunMessageAdmission::Deferred) => {
                self.repository
                    .mark_message_status(
                        message.id,
                        Some(claim_token),
                        MailboxMessageStatus::Queued,
                        None,
                    )
                    .await?;
                return Ok(RuntimeMailboxDrainResult::Deferred);
            }
            Err(error) if retryable_delivery_error(&error) => {
                if message.reconcile_required {
                    self.repository
                        .release_reconciliation_claim(message.id, claim_token, error.to_string())
                        .await?;
                } else {
                    self.repository
                        .mark_message_status(
                            message.id,
                            Some(claim_token),
                            MailboxMessageStatus::Queued,
                            Some(error.to_string()),
                        )
                        .await?;
                }
                return Ok(RuntimeMailboxDrainResult::Deferred);
            }
            Err(error) => {
                return self
                    .settle_failed_claim(message, claim_token, error.to_string())
                    .await;
            }
        };
        let settlement = self
            .settlements
            .settle_delivery_accepted(AgentRunMailboxAcceptedSettlement {
                mailbox_message_id: message.id,
                claim_token,
                delivery_kind: match delivery {
                    AgentRunMessageAcceptedDelivery::Started => {
                        AgentRunAcceptedDeliveryKind::Started
                    }
                    AgentRunMessageAcceptedDelivery::Steered => {
                        AgentRunAcceptedDeliveryKind::Steered
                    }
                },
                accepted_refs: AgentRunAcceptedRefs {
                    run_id: message.run_id,
                    agent_id: message.agent_id,
                    frame_id: None,
                    frame_revision: None,
                    runtime_thread_id: receipt.thread_id.as_ref().map(ToString::to_string),
                    runtime_operation_id: Some(receipt.operation_id.to_string()),
                },
            })
            .await?;
        Ok(RuntimeMailboxDrainResult::Accepted {
            message: settlement.message,
            receipt,
            delivery,
        })
    }

    async fn settle_failed_claim(
        &self,
        message: AgentRunMailboxMessage,
        claim_token: Uuid,
        error: impl Into<String>,
    ) -> Result<RuntimeMailboxDrainResult, RuntimeMailboxError> {
        let error = error.into();
        let settlement = self
            .settlements
            .settle_delivery_failed(AgentRunMailboxFailedSettlement {
                mailbox_message_id: message.id,
                claim_token,
                error_message: error.clone(),
            })
            .await?;
        Ok(RuntimeMailboxDrainResult::Failed {
            message: settlement.message,
            error,
        })
    }
}

#[async_trait::async_trait]
impl AgentRunMessageDeliveryCoordinator for RuntimeAgentRunMailbox {
    async fn try_deliver(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<AgentRunMessageDeliveryAttempt, crate::WorkflowApplicationError> {
        self.repository
            .recover_expired_consuming(Utc::now())
            .await
            .map_err(|error| crate::WorkflowApplicationError::Internal(error.to_string()))?;
        let outcome = self
            .drain_once_with_settlement(target)
            .await
            .map_err(|error| crate::WorkflowApplicationError::Internal(error.to_string()))?;
        Ok(match outcome {
            RuntimeMailboxDrainResult::Deferred => AgentRunMessageDeliveryAttempt::Deferred,
            RuntimeMailboxDrainResult::Accepted { message, .. } => {
                AgentRunMessageDeliveryAttempt::Accepted {
                    mailbox_message_id: message.id,
                }
            }
            RuntimeMailboxDrainResult::Failed { message, .. } => {
                AgentRunMessageDeliveryAttempt::Failed {
                    mailbox_message_id: message.id,
                }
            }
        })
    }
}

#[async_trait::async_trait]
impl AgentRunProductDeliveryPort for RuntimeAgentRunMailbox {
    async fn deliver(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductDelivery, RuntimeMailboxError> {
        let source = MailboxSourceIdentity {
            namespace: command.presentation.source.namespace.clone(),
            kind: command.presentation.source.kind.clone(),
            source_ref: command.presentation.source.source_ref.clone(),
            correlation_ref: command.presentation.source.correlation_ref.clone(),
            actor: command.presentation.source.actor.clone(),
            route: command.presentation.source.route.clone(),
            display_label_key: command.presentation.source.display_label_key.clone(),
            metadata: command.presentation.source.metadata.clone(),
        };
        let outcome = self
            .submit(EnqueueRuntimeMailboxMessage {
                target: AgentRunRuntimeTarget {
                    run_id: command.run_id,
                    agent_id: command.agent_id,
                },
                presentation_thread_id: command.presentation_thread_id,
                presentation: command.presentation,
                client_command_id: command.client_command_id,
                input: command.input,
                actor: command.actor,
                identity: command.identity,
                origin: command.origin,
                source,
                delivery_intent: None,
                executor_config: None,
                backend_selection: command.backend_selection,
            })
            .await?;
        Ok(match outcome {
            RuntimeMailboxSubmitOutcome::Queued { message } => AgentRunProductDelivery {
                mailbox_message_id: message.id,
                operation_receipt: None,
                queued: true,
            },
            RuntimeMailboxSubmitOutcome::Dispatched {
                message, receipt, ..
            } => AgentRunProductDelivery {
                mailbox_message_id: message.id,
                operation_receipt: Some(receipt),
                queued: false,
            },
        })
    }
}

#[async_trait::async_trait]
impl agentdash_application_ports::workflow_agent_run_delivery::WorkflowAgentRunDeliveryPort
    for RuntimeAgentRunMailbox
{
    async fn deliver(
        &self,
        command: agentdash_application_ports::workflow_agent_run_delivery::WorkflowAgentRunDeliveryCommand,
    ) -> Result<
        agentdash_application_ports::workflow_agent_run_delivery::WorkflowAgentRunDeliveryReceipt,
        agentdash_application_ports::workflow_agent_run_delivery::WorkflowAgentRunDeliveryError,
    > {
        use agentdash_application_ports::workflow_agent_run_delivery::{
            WorkflowAgentRunDeliveryError, WorkflowAgentRunDeliveryReceipt,
        };

        if command.presentation_content.is_empty() {
            return Err(WorkflowAgentRunDeliveryError::Failed(
                "workflow delivery requires non-empty owner-resolved presentation content".into(),
            ));
        }
        let source_ref = format!(
            "{}:{}#{}",
            command.orchestration_id, command.node_path, command.attempt
        );
        let outcome = self
            .submit(EnqueueRuntimeMailboxMessage {
                target: command.target,
                presentation_thread_id: command.presentation_thread_id,
                presentation: AgentRunPresentationDraft {
                    content: command.presentation_content,
                    source: agentdash_agent_protocol::UserInputSource::new(
                        "workflow",
                        "orchestrator",
                        "system",
                    ),
                    launch_source:
                        super::runtime_facade::LaunchPresentationSource::WorkflowOrchestrator,
                    submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                },
                client_command_id: command.client_command_id,
                input: command.input,
                actor: command.actor,
                identity: None,
                origin: MailboxMessageOrigin::Workflow,
                source: MailboxSourceIdentity::workflow_orchestrator().with_source_ref(source_ref),
                delivery_intent: None,
                executor_config: None,
                backend_selection: None,
            })
            .await
            .map_err(|error| WorkflowAgentRunDeliveryError::Failed(error.to_string()))?;
        let (message, runtime_operation_id) = match outcome {
            RuntimeMailboxSubmitOutcome::Queued { message } => (message, None),
            RuntimeMailboxSubmitOutcome::Dispatched {
                message, receipt, ..
            } => (message, Some(receipt.operation_id.to_string())),
        };
        Ok(WorkflowAgentRunDeliveryReceipt {
            mailbox_message_id: message.id,
            runtime_operation_id,
        })
    }
}

fn runtime_can_start_turn(view: &super::AgentRunRuntimeView) -> bool {
    match &view.snapshot {
        None => true,
        Some(snapshot) if snapshot.active_turn_id.is_some() => false,
        Some(snapshot)
            if snapshot.status == agentdash_agent_runtime_contract::RuntimeThreadStatus::Lost =>
        {
            true
        }
        Some(snapshot) => matches!(
            snapshot
                .command_availability
                .get(&RuntimeCommandKind::TurnStart),
            Some(CommandAvailability::Available)
        ),
    }
}

fn runtime_can_steer_turn(view: &super::AgentRunRuntimeView) -> bool {
    view.snapshot.as_ref().is_some_and(|snapshot| {
        snapshot.active_turn_id.is_some()
            && snapshot.active_presentation_turn_id.is_some()
            && matches!(
                snapshot
                    .command_availability
                    .get(&RuntimeCommandKind::TurnSteer),
                Some(CommandAvailability::Available)
            )
    })
}

fn retryable_delivery_error(error: &AgentRunRuntimeError) -> bool {
    matches!(
        error,
        AgentRunRuntimeError::Binding(
            agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingError::Unavailable {
                retryable: true,
                ..
            }
        ) |
        AgentRunRuntimeError::Execute(
            agentdash_agent_runtime_contract::RuntimeExecuteError::RevisionConflict { .. }
                | agentdash_agent_runtime_contract::RuntimeExecuteError::Unavailable {
                    retryable: true,
                    ..
                }
                | agentdash_agent_runtime_contract::RuntimeExecuteError::Persistence {
                    retryable: true,
                    ..
                }
        ) | AgentRunRuntimeError::StaleActiveTurn
            | AgentRunRuntimeError::Snapshot(
                agentdash_agent_runtime_contract::RuntimeSnapshotError::Unavailable { .. }
            )
    )
}

fn presentation_preview(input: &[agentdash_agent_protocol::UserInputBlock]) -> String {
    input
        .iter()
        .filter_map(agentdash_agent_protocol::user_input_text)
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(200)
        .collect()
}

fn presentation_has_images(input: &[agentdash_agent_protocol::UserInputBlock]) -> bool {
    input.iter().any(|input| {
        matches!(
            input,
            agentdash_agent_protocol::codex_app_server_protocol::UserInput::Image { .. }
                | agentdash_agent_protocol::codex_app_server_protocol::UserInput::LocalImage { .. }
        )
    })
}
