use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use agentdash_agent_runtime_contract::{
    AgentInputContent, AgentRuntimeOperationReceipt, AgentRuntimeOperationStatus, AgentTurnId,
};
use agentdash_domain::{
    agent_run_mailbox::{
        AgentRunMailboxClaimRequest, AgentRunMailboxMessage, AgentRunMailboxRepository,
        ConsumptionBarrier, MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin,
        MailboxMessageStatus, MailboxSourceIdentity, NewAgentRunMailboxMessage, SteeringStopEffect,
    },
    agent_run_target::AgentRunTarget,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use super::{
    AgentRunProductCommand, AgentRunProductCommandError, AgentRunProductCommandFacade,
    AgentRunProductCommandRequest, AgentRunProductProjectionQueryPort,
};

const DISPATCHER_LEASE_SECONDS: i64 = 30;
const RECOVERY_INTERVAL_SECONDS: u64 = 2;
static DISPATCHER_OWNER_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub struct AgentRunMailboxWorkerHandle {
    task: tokio::task::JoinHandle<()>,
}

impl Drop for AgentRunMailboxWorkerHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn spawn_agent_run_mailbox_worker(
    service: Arc<AgentRunMailboxService>,
) -> AgentRunMailboxWorkerHandle {
    let task = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(RECOVERY_INTERVAL_SECONDS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = service.recover_and_dispatch_pending().await {
                tracing::warn!(
                    subsystem = "agent_run",
                    operation = "mailbox.recover_and_dispatch",
                    error = %error,
                    "AgentRun mailbox recovery pass failed"
                );
            }
        }
    });
    AgentRunMailboxWorkerHandle { task }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRunMailboxDeliveryIntent {
    Queue,
    Steer { expected_turn_id: AgentTurnId },
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxIntakeCommand {
    pub target: AgentRunTarget,
    pub content: Vec<AgentInputContent>,
    pub source: MailboxSourceIdentity,
    pub origin: MailboxMessageOrigin,
    pub client_command_id: String,
    pub delivery_intent: AgentRunMailboxDeliveryIntent,
    pub retain_payload: bool,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxIntakeOutcome {
    pub message: AgentRunMailboxMessage,
    pub operation_receipt: Option<AgentRuntimeOperationReceipt>,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunMailboxDispatchTrigger {
    Intake,
    AgentLoopTurnBoundary,
    AgentRunTurnBoundary,
    Recovery,
}

impl AgentRunMailboxDispatchTrigger {
    fn barriers(self) -> Vec<ConsumptionBarrier> {
        match self {
            Self::Intake => vec![ConsumptionBarrier::ImmediateIfIdle],
            Self::AgentLoopTurnBoundary => vec![ConsumptionBarrier::AgentLoopTurnBoundary],
            Self::AgentRunTurnBoundary => vec![ConsumptionBarrier::AgentRunTurnBoundary],
            // Recovery is the durable wake path when a terminal live notification was missed.
            // dispatch_one rechecks the authoritative Agent view and requeues a boundary message
            // while a turn is still active, then submits it once the owner becomes idle.
            Self::Recovery => vec![
                ConsumptionBarrier::ImmediateIfIdle,
                ConsumptionBarrier::AgentRunTurnBoundary,
            ],
        }
    }
}

#[derive(Debug, Error)]
pub enum AgentRunMailboxError {
    #[error("mailbox input is empty")]
    EmptyInput,
    #[error("mailbox client command id is invalid")]
    InvalidClientCommandId,
    #[error("mailbox explicit Steer requires the current active turn")]
    StaleExpectedTurn,
    #[error("mailbox client command id was reused with different input")]
    DuplicateConflict,
    #[error("mailbox repository failed: {0}")]
    Repository(String),
    #[error("mailbox Product projection failed: {0}")]
    Projection(String),
    #[error("mailbox Complete Agent delivery failed: {0}")]
    Delivery(String),
}

pub struct AgentRunMailboxService {
    repository: Arc<dyn AgentRunMailboxRepository>,
    commands: Arc<AgentRunProductCommandFacade>,
    projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    dispatcher_owner_id: Arc<str>,
}

impl AgentRunMailboxService {
    pub fn new(
        repository: Arc<dyn AgentRunMailboxRepository>,
        commands: Arc<AgentRunProductCommandFacade>,
        projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    ) -> Self {
        Self {
            repository,
            commands,
            projection,
            dispatcher_owner_id: Arc::from(format!(
                "agent-run-mailbox:{}:{}",
                std::process::id(),
                DISPATCHER_OWNER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            )),
        }
    }

    pub async fn accept(
        &self,
        command: AgentRunMailboxIntakeCommand,
    ) -> Result<AgentRunMailboxIntakeOutcome, AgentRunMailboxError> {
        validate_client_command_id(&command.client_command_id)?;
        validate_content(&command.content)?;

        let (delivery_evidence, active_turn_id) = match &command.delivery_intent {
            AgentRunMailboxDeliveryIntent::Queue => (None, None),
            AgentRunMailboxDeliveryIntent::Steer { expected_turn_id } => {
                let evidence = self
                    .commands
                    .delivery_evidence(&command.target)
                    .await
                    .map_err(|error| AgentRunMailboxError::Projection(error.to_string()))?;
                let view = self
                    .projection
                    .runtime_view(&command.target)
                    .await
                    .map_err(|error| AgentRunMailboxError::Projection(error.to_string()))?;
                let active_turn_id = view.active_turn_id().map(str::to_owned);
                if active_turn_id.as_deref() != Some(expected_turn_id.as_str()) {
                    return Err(AgentRunMailboxError::StaleExpectedTurn);
                }
                (Some(evidence), active_turn_id)
            }
        };

        let (delivery, barrier, drain_mode, priority, expected_active_agent_run_turn_id) =
            match &command.delivery_intent {
                AgentRunMailboxDeliveryIntent::Queue => (
                    MailboxDelivery::LaunchOrContinueTurn,
                    ConsumptionBarrier::ImmediateIfIdle,
                    MailboxDrainMode::One,
                    0,
                    None,
                ),
                AgentRunMailboxDeliveryIntent::Steer { expected_turn_id } => (
                    MailboxDelivery::SteerActiveTurn {
                        stop_effect: SteeringStopEffect::None,
                    },
                    ConsumptionBarrier::AgentLoopTurnBoundary,
                    MailboxDrainMode::All,
                    10_000,
                    Some(expected_turn_id.as_str().to_owned()),
                ),
            };

        let preview = content_preview(&command.content);
        let payload_json = serde_json::to_value(&command.content)
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
        let source_dedup_key =
            source_dedup_key(&command.target, &command.client_command_id, &command.source);
        let new_message = NewAgentRunMailboxMessage {
            run_id: command.target.run_id,
            agent_id: command.target.agent_id,
            delivery_runtime_thread_id: delivery_evidence
                .as_ref()
                .map(|evidence| evidence.runtime_thread_id.as_str().to_owned()),
            delivery_source_coordinate: delivery_evidence
                .as_ref()
                .map(|evidence| evidence.source.as_str().to_owned()),
            delivery_binding_generation: delivery_evidence
                .as_ref()
                .map(|evidence| evidence.binding_generation.0 as i64),
            delivery_snapshot_revision: delivery_evidence
                .as_ref()
                .map(|evidence| evidence.snapshot_revision.0 as i64),
            origin: command.origin,
            source: command.source,
            delivery,
            barrier,
            drain_mode,
            priority,
            source_dedup_key: Some(source_dedup_key),
            queued_agent_run_turn_id: active_turn_id.clone(),
            expected_active_agent_run_turn_id,
            command_receipt_id: None,
            payload_json: Some(payload_json),
            executor_config_json: None,
            launch_planning_input: None,
            preview,
            has_images: command
                .content
                .iter()
                .any(|item| matches!(item, AgentInputContent::Image { .. })),
            retain_payload: command.retain_payload,
        };
        let put = self
            .repository
            .create_message_idempotent(new_message.clone())
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
        let message = put.message;
        if !same_intake(&message, &new_message) {
            return Err(AgentRunMailboxError::DuplicateConflict);
        }

        if message.status.is_terminal_delivery() {
            return Ok(AgentRunMailboxIntakeOutcome {
                message,
                operation_receipt: None,
                duplicate: put.duplicate,
            });
        }

        let barriers = vec![message.barrier];
        let dispatched = match self.dispatch_one(&command.target, barriers).await {
            Ok(dispatched) => dispatched,
            Err(error)
                if matches!(
                    command.delivery_intent,
                    AgentRunMailboxDeliveryIntent::Queue
                ) =>
            {
                tracing::warn!(
                    subsystem = "agent_run",
                    operation = "mailbox.dispatch_after_intake",
                    run_id = %command.target.run_id,
                    agent_id = %command.target.agent_id,
                    mailbox_message_id = %message.id,
                    error = %error,
                    "Mailbox accepted input durably; immediate delivery remains pending"
                );
                None
            }
            Err(error) => return Err(error),
        };
        let refreshed = self
            .repository
            .get_message(message.id)
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?
            .unwrap_or(message);
        let refreshed_id = refreshed.id;
        Ok(AgentRunMailboxIntakeOutcome {
            message: refreshed,
            operation_receipt: dispatched
                .filter(|(message_id, _)| *message_id == refreshed_id)
                .map(|(_, receipt)| receipt),
            duplicate: put.duplicate,
        })
    }

    pub async fn dispatch_pending(
        &self,
        target: &AgentRunTarget,
        trigger: AgentRunMailboxDispatchTrigger,
    ) -> Result<Option<(Uuid, AgentRuntimeOperationReceipt)>, AgentRunMailboxError> {
        self.repository
            .recover_expired_consuming(Utc::now())
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
        self.dispatch_one(target, trigger.barriers()).await
    }

    pub async fn recover_and_dispatch_pending(&self) -> Result<usize, AgentRunMailboxError> {
        self.repository
            .recover_expired_consuming(Utc::now())
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
        let targets = self
            .repository
            .list_pending_targets(128)
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
        let mut dispatched = 0;
        for target in targets {
            if self
                .dispatch_one(&target, AgentRunMailboxDispatchTrigger::Recovery.barriers())
                .await?
                .is_some()
            {
                dispatched += 1;
            }
        }
        Ok(dispatched)
    }

    pub async fn list(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Vec<AgentRunMailboxMessage>, AgentRunMailboxError> {
        self.repository
            .list_messages(target.run_id, target.agent_id)
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))
    }

    pub async fn state(
        &self,
        target: &AgentRunTarget,
    ) -> Result<
        Option<agentdash_domain::agent_run_mailbox::AgentRunMailboxState>,
        AgentRunMailboxError,
    > {
        self.repository
            .get_state(target.run_id, target.agent_id)
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))
    }

    pub async fn content(
        &self,
        target: &AgentRunTarget,
        message_id: Uuid,
    ) -> Result<Option<serde_json::Value>, AgentRunMailboxError> {
        let message = self.owned_message(target, message_id).await?;
        Ok(message.payload_json)
    }

    pub async fn delete(
        &self,
        target: &AgentRunTarget,
        message_id: Uuid,
    ) -> Result<Option<AgentRunMailboxMessage>, AgentRunMailboxError> {
        self.owned_message(target, message_id).await?;
        self.repository
            .delete_message(message_id)
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))
    }

    pub async fn move_after(
        &self,
        target: &AgentRunTarget,
        message_id: Uuid,
        after_id: Option<Uuid>,
    ) -> Result<AgentRunMailboxMessage, AgentRunMailboxError> {
        self.owned_message(target, message_id).await?;
        self.repository
            .move_message_after(message_id, after_id, target.run_id, target.agent_id)
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))
    }

    pub async fn resume(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<(Uuid, AgentRuntimeOperationReceipt)>, AgentRunMailboxError> {
        let binding = self
            .projection
            .runtime_product_binding(target)
            .await
            .map_err(|error| AgentRunMailboxError::Projection(error.to_string()))?
            .ok_or_else(|| {
                AgentRunMailboxError::Projection("AgentRun Product binding is missing".to_owned())
            })?;
        self.repository
            .resume_state(
                target.run_id,
                target.agent_id,
                Some(binding.runtime_thread_id.as_str().to_owned()),
            )
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
        self.dispatch_one(
            target,
            vec![
                ConsumptionBarrier::ManualResume,
                ConsumptionBarrier::ImmediateIfIdle,
            ],
        )
        .await
    }

    pub async fn promote(
        &self,
        target: &AgentRunTarget,
        message_id: Uuid,
        expected_turn_id: AgentTurnId,
    ) -> Result<AgentRunMailboxIntakeOutcome, AgentRunMailboxError> {
        let view = self
            .projection
            .runtime_view(target)
            .await
            .map_err(|error| AgentRunMailboxError::Projection(error.to_string()))?;
        if view.active_turn_id() != Some(expected_turn_id.as_str()) {
            return Err(AgentRunMailboxError::StaleExpectedTurn);
        }
        let message = self.owned_message(target, message_id).await?;
        let message = self
            .repository
            .update_message_policy(
                message.id,
                MailboxDelivery::SteerActiveTurn {
                    stop_effect: SteeringStopEffect::None,
                },
                ConsumptionBarrier::AgentLoopTurnBoundary,
                MailboxDrainMode::All,
                10_000,
                Some(expected_turn_id.as_str().to_owned()),
            )
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
        let dispatched = self
            .dispatch_one(target, vec![ConsumptionBarrier::AgentLoopTurnBoundary])
            .await?;
        let refreshed = self
            .repository
            .get_message(message.id)
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?
            .unwrap_or(message);
        let refreshed_id = refreshed.id;
        Ok(AgentRunMailboxIntakeOutcome {
            message: refreshed,
            operation_receipt: dispatched
                .filter(|(claimed_id, _)| *claimed_id == refreshed_id)
                .map(|(_, receipt)| receipt),
            duplicate: false,
        })
    }

    async fn owned_message(
        &self,
        target: &AgentRunTarget,
        message_id: Uuid,
    ) -> Result<AgentRunMailboxMessage, AgentRunMailboxError> {
        let message = self
            .repository
            .get_message(message_id)
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?
            .ok_or_else(|| {
                AgentRunMailboxError::Repository("mailbox message not found".to_owned())
            })?;
        if message.run_id != target.run_id || message.agent_id != target.agent_id {
            return Err(AgentRunMailboxError::Repository(
                "mailbox message belongs to another AgentRun owner".to_owned(),
            ));
        }
        Ok(message)
    }

    async fn dispatch_one(
        &self,
        target: &AgentRunTarget,
        barriers: Vec<ConsumptionBarrier>,
    ) -> Result<Option<(Uuid, AgentRuntimeOperationReceipt)>, AgentRunMailboxError> {
        let binding = self
            .projection
            .runtime_product_binding(target)
            .await
            .map_err(|error| AgentRunMailboxError::Projection(error.to_string()))?
            .ok_or_else(|| {
                AgentRunMailboxError::Projection("AgentRun Product binding is missing".to_owned())
            })?;
        let lease = self
            .repository
            .claim_dispatcher(
                agentdash_domain::agent_run_mailbox::AgentRunMailboxDispatcherLeaseRequest {
                    run_id: target.run_id,
                    agent_id: target.agent_id,
                    owner_id: self.dispatcher_owner_id.to_string(),
                    lease_token: Uuid::new_v4(),
                    expires_at: Utc::now() + Duration::seconds(DISPATCHER_LEASE_SECONDS),
                },
            )
            .await
            .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
        let Some(lease) = lease else {
            return Ok(None);
        };

        let result = async {
            let claim_token = Uuid::new_v4();
            let (drain_mode, claim_limit) = claim_policy(&barriers);
            let claimed = self
                .repository
                .claim_next(AgentRunMailboxClaimRequest {
                    run_id: target.run_id,
                    agent_id: target.agent_id,
                    delivery_runtime_thread_id: Some(binding.runtime_thread_id.as_str().to_owned()),
                    barriers,
                    drain_mode,
                    limit: claim_limit,
                    claim_token,
                    claim_expires_at: Utc::now() + Duration::seconds(DISPATCHER_LEASE_SECONDS),
                })
                .await
                .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
            let mut last_receipt = None;
            for message in claimed {
                if let Some(receipt) = self
                    .dispatch_claimed_message(target, message, claim_token)
                    .await?
                {
                    last_receipt = Some(receipt);
                }
            }
            Ok(last_receipt)
        }
        .await;

        let _ = self.repository.release_dispatcher(&lease).await;
        result
    }

    async fn dispatch_claimed_message(
        &self,
        target: &AgentRunTarget,
        message: AgentRunMailboxMessage,
        claim_token: Uuid,
    ) -> Result<Option<(Uuid, AgentRuntimeOperationReceipt)>, AgentRunMailboxError> {
        let content: Vec<AgentInputContent> =
            serde_json::from_value(message.payload_json.clone().ok_or_else(|| {
                AgentRunMailboxError::Delivery("mailbox payload is absent".to_owned())
            })?)
            .map_err(|error| AgentRunMailboxError::Delivery(error.to_string()))?;
        let product_command = match &message.delivery {
            MailboxDelivery::LaunchOrContinueTurn => {
                let view = self
                    .projection
                    .runtime_view(target)
                    .await
                    .map_err(|error| AgentRunMailboxError::Projection(error.to_string()))?;
                if view.active_turn_id().is_some() {
                    self.repository
                        .mark_message_status(
                            message.id,
                            Some(claim_token),
                            MailboxMessageStatus::Queued,
                            None,
                            None,
                            None,
                        )
                        .await
                        .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
                    return Ok(None);
                }
                AgentRunProductCommand::SubmitInput { content }
            }
            MailboxDelivery::SteerActiveTurn { .. } => {
                let expected_turn_id = message
                    .expected_active_agent_run_turn_id
                    .as_deref()
                    .ok_or(AgentRunMailboxError::StaleExpectedTurn)?;
                AgentRunProductCommand::Steer {
                    content,
                    expected_turn_id: AgentTurnId::new(expected_turn_id.to_owned())
                        .map_err(|_| AgentRunMailboxError::StaleExpectedTurn)?,
                }
            }
            MailboxDelivery::ResumeLaunchSource { .. } => {
                AgentRunProductCommand::SubmitInput { content }
            }
        };
        let receipt = self
            .commands
            .execute(AgentRunProductCommandRequest {
                target: target.clone(),
                client_command_id: format!("mailbox:{}", message.id),
                command: product_command,
            })
            .await;
        self.settle_delivery(message, claim_token, receipt).await
    }

    async fn settle_delivery(
        &self,
        message: AgentRunMailboxMessage,
        claim_token: Uuid,
        receipt: Result<AgentRuntimeOperationReceipt, AgentRunProductCommandError>,
    ) -> Result<Option<(Uuid, AgentRuntimeOperationReceipt)>, AgentRunMailboxError> {
        match receipt {
            Ok(receipt) => {
                let status = match receipt.status {
                    AgentRuntimeOperationStatus::Accepted
                    | AgentRuntimeOperationStatus::Running
                    | AgentRuntimeOperationStatus::Succeeded => match message.delivery {
                        MailboxDelivery::SteerActiveTurn { .. } => MailboxMessageStatus::Steered,
                        _ => MailboxMessageStatus::Dispatched,
                    },
                    AgentRuntimeOperationStatus::Failed
                    | AgentRuntimeOperationStatus::Interrupted => MailboxMessageStatus::Failed,
                    AgentRuntimeOperationStatus::Lost => MailboxMessageStatus::Consuming,
                };
                let last_error = matches!(receipt.status, AgentRuntimeOperationStatus::Lost)
                    .then(|| "delivery_result_unknown".to_owned());
                self.repository
                    .mark_message_status(
                        message.id,
                        Some(claim_token),
                        status,
                        message.expected_active_agent_run_turn_id.clone(),
                        None,
                        last_error,
                    )
                    .await
                    .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
                if status.is_terminal_delivery() && !message.retain_payload {
                    self.repository
                        .cleanup_user_payload(message.id)
                        .await
                        .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
                }
                Ok(Some((message.id, receipt)))
            }
            Err(AgentRunProductCommandError::InspectionPending) => {
                self.repository
                    .mark_message_status(
                        message.id,
                        Some(claim_token),
                        MailboxMessageStatus::Consuming,
                        None,
                        None,
                        Some("delivery_result_unknown".to_owned()),
                    )
                    .await
                    .map_err(|error| AgentRunMailboxError::Repository(error.to_string()))?;
                Ok(None)
            }
            Err(error) if retryable_delivery_error(&error) => {
                self.repository
                    .mark_message_status(
                        message.id,
                        Some(claim_token),
                        MailboxMessageStatus::Queued,
                        None,
                        None,
                        Some(error.to_string()),
                    )
                    .await
                    .map_err(|repository_error| {
                        AgentRunMailboxError::Repository(repository_error.to_string())
                    })?;
                Ok(None)
            }
            Err(error) => {
                self.repository
                    .mark_message_status(
                        message.id,
                        Some(claim_token),
                        MailboxMessageStatus::Failed,
                        None,
                        None,
                        Some(error.to_string()),
                    )
                    .await
                    .map_err(|repository_error| {
                        AgentRunMailboxError::Repository(repository_error.to_string())
                    })?;
                Err(AgentRunMailboxError::Delivery(error.to_string()))
            }
        }
    }
}

fn retryable_delivery_error(error: &AgentRunProductCommandError) -> bool {
    match error {
        AgentRunProductCommandError::TargetNotBound
        | AgentRunProductCommandError::Binding(_)
        | AgentRunProductCommandError::Unavailable(_) => true,
        AgentRunProductCommandError::Agent(error) => error.retryable,
        AgentRunProductCommandError::TargetMismatch
        | AgentRunProductCommandError::InvalidClientCommandId
        | AgentRunProductCommandError::InvalidCommand(_)
        | AgentRunProductCommandError::ActiveTurnMissing
        | AgentRunProductCommandError::InspectionPending => false,
    }
}

fn claim_policy(barriers: &[ConsumptionBarrier]) -> (Option<MailboxDrainMode>, i64) {
    if barriers.contains(&ConsumptionBarrier::AgentLoopTurnBoundary) {
        (Some(MailboxDrainMode::All), 128)
    } else {
        (None, 1)
    }
}

trait MailboxMessageStatusExt {
    fn is_terminal_delivery(self) -> bool;
}

impl MailboxMessageStatusExt for MailboxMessageStatus {
    fn is_terminal_delivery(self) -> bool {
        matches!(
            self,
            MailboxMessageStatus::Dispatched
                | MailboxMessageStatus::Steered
                | MailboxMessageStatus::Deleted
        )
    }
}

fn same_intake(existing: &AgentRunMailboxMessage, requested: &NewAgentRunMailboxMessage) -> bool {
    existing.run_id == requested.run_id
        && existing.agent_id == requested.agent_id
        && existing.origin == requested.origin
        && existing.source == requested.source
        && existing.delivery == requested.delivery
        && existing.barrier == requested.barrier
        && existing.drain_mode == requested.drain_mode
        && existing.priority == requested.priority
        && existing.source_dedup_key == requested.source_dedup_key
        && existing.payload_json == requested.payload_json
        && existing.retain_payload == requested.retain_payload
}

fn validate_client_command_id(value: &str) -> Result<(), AgentRunMailboxError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        return Err(AgentRunMailboxError::InvalidClientCommandId);
    }
    Ok(())
}

fn validate_content(content: &[AgentInputContent]) -> Result<(), AgentRunMailboxError> {
    if content.is_empty()
        || !content.iter().any(|item| match item {
            AgentInputContent::Text { text } => !text.trim().is_empty(),
            AgentInputContent::Image { source, .. } => !source.trim().is_empty(),
            AgentInputContent::Resource { uri, .. } => !uri.trim().is_empty(),
            AgentInputContent::Structured { schema, .. } => !schema.trim().is_empty(),
        })
    {
        return Err(AgentRunMailboxError::EmptyInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_scans_normal_input_waiting_for_the_previous_turn_boundary() {
        assert_eq!(
            AgentRunMailboxDispatchTrigger::Recovery.barriers(),
            vec![
                ConsumptionBarrier::ImmediateIfIdle,
                ConsumptionBarrier::AgentRunTurnBoundary,
            ]
        );
    }

    #[test]
    fn agent_loop_boundary_claims_all_matching_messages() {
        assert_eq!(
            claim_policy(&[ConsumptionBarrier::AgentLoopTurnBoundary]),
            (Some(MailboxDrainMode::All), 128)
        );
        assert_eq!(
            claim_policy(&[ConsumptionBarrier::ImmediateIfIdle]),
            (None, 1)
        );
    }

    #[test]
    fn transient_complete_agent_errors_keep_mailbox_messages_recoverable() {
        assert!(retryable_delivery_error(
            &AgentRunProductCommandError::TargetNotBound
        ));
        assert!(retryable_delivery_error(
            &AgentRunProductCommandError::Unavailable("offline".to_owned())
        ));
        assert!(retryable_delivery_error(
            &AgentRunProductCommandError::Agent(
                agentdash_agent_runtime_contract::AgentServiceError::new(
                    agentdash_agent_runtime_contract::AgentServiceErrorCode::Unavailable,
                    "offline",
                    true,
                )
            )
        ));
        assert!(!retryable_delivery_error(
            &AgentRunProductCommandError::InvalidCommand("bad input".to_owned())
        ));
    }
}

fn content_preview(content: &[AgentInputContent]) -> String {
    content
        .iter()
        .find_map(|item| match item {
            AgentInputContent::Text { text } => Some(text.trim()),
            _ => None,
        })
        .unwrap_or("[attachment]")
        .chars()
        .take(240)
        .collect()
}

fn source_dedup_key(
    target: &AgentRunTarget,
    client_command_id: &str,
    source: &MailboxSourceIdentity,
) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "agentdash.mailbox-source/v3:{}:{}:{}:{}:{}",
                target.run_id, target.agent_id, client_command_id, source.namespace, source.kind
            )
            .as_bytes()
        )
    )
}
