use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    CommandAvailability, OperationReceipt, RuntimeActor, RuntimeCommandKind, RuntimeInput,
};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_application_ports::launch::BackendSelectionInput;
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxClaimRequest, AgentRunMailboxMessage, AgentRunMailboxRepository,
    ConsumptionBarrier, MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin,
    MailboxMessageStatus, MailboxSourceIdentity, NewAgentRunMailboxMessage,
};
use agentdash_spi::AuthIdentity;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::{AgentRunRuntime, AgentRunRuntimeError, SendAgentRunMessage};

const CLAIM_LEASE_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct EnqueueRuntimeMailboxMessage {
    pub target: AgentRunRuntimeTarget,
    pub client_command_id: String,
    pub input: Vec<RuntimeInput>,
    pub actor: RuntimeActor,
    pub identity: Option<AuthIdentity>,
    pub source: MailboxSourceIdentity,
    pub backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredRuntimeMailboxCommand {
    target: AgentRunRuntimeTarget,
    client_command_id: String,
    input: Vec<RuntimeInput>,
    actor: RuntimeActor,
    identity: Option<AuthIdentity>,
    backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeMailboxSubmitOutcome {
    Queued {
        message: AgentRunMailboxMessage,
    },
    Dispatched {
        message: AgentRunMailboxMessage,
        receipt: OperationReceipt,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliverAgentRunProductInput {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub input: Vec<RuntimeInput>,
    pub actor: RuntimeActor,
    pub client_command_id: String,
    pub backend_selection: Option<BackendSelectionInput>,
    pub identity: Option<AuthIdentity>,
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
}

pub struct RuntimeAgentRunMailbox {
    repository: Arc<dyn AgentRunMailboxRepository>,
    runtime: Arc<dyn AgentRunRuntime>,
}

impl RuntimeAgentRunMailbox {
    pub fn new(
        repository: Arc<dyn AgentRunMailboxRepository>,
        runtime: Arc<dyn AgentRunRuntime>,
    ) -> Self {
        Self {
            repository,
            runtime,
        }
    }

    pub async fn submit(
        &self,
        command: EnqueueRuntimeMailboxMessage,
    ) -> Result<RuntimeMailboxSubmitOutcome, RuntimeMailboxError> {
        let view = self.runtime.inspect(command.target.clone()).await?;
        let can_dispatch = runtime_can_start_turn(&view);
        let payload = serde_json::to_value(StoredRuntimeMailboxCommand {
            target: command.target.clone(),
            client_command_id: command.client_command_id.clone(),
            input: command.input.clone(),
            actor: command.actor.clone(),
            identity: command.identity.clone(),
            backend_selection: command.backend_selection.clone(),
        })
        .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
        let visible_input = serde_json::to_value(&command.input)
            .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
        let message = self
            .repository
            .create_message_idempotent(NewAgentRunMailboxMessage {
                run_id: command.target.run_id,
                agent_id: command.target.agent_id,
                origin: MailboxMessageOrigin::User,
                source: command.source,
                delivery: MailboxDelivery::LaunchOrContinueTurn,
                barrier: if can_dispatch {
                    ConsumptionBarrier::ImmediateIfIdle
                } else {
                    ConsumptionBarrier::AgentRunTurnBoundary
                },
                drain_mode: MailboxDrainMode::One,
                priority: 0,
                source_dedup_key: Some(format!(
                    "runtime-command:{}:{}:{}",
                    command.target.run_id, command.target.agent_id, command.client_command_id
                )),
                payload_json: Some(visible_input),
                executor_config_json: None,
                launch_planning_input: Some(payload),
                preview: runtime_input_preview(&command.input),
                has_images: command
                    .input
                    .iter()
                    .any(|input| matches!(input, RuntimeInput::Image { .. })),
                retain_payload: true,
            })
            .await?;

        if !can_dispatch {
            return Ok(RuntimeMailboxSubmitOutcome::Queued { message });
        }
        match self.drain_once(&command.target).await? {
            Some((message, receipt)) => {
                Ok(RuntimeMailboxSubmitOutcome::Dispatched { message, receipt })
            }
            None => Ok(RuntimeMailboxSubmitOutcome::Queued { message }),
        }
    }

    pub async fn recover_and_drain_once(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<(AgentRunMailboxMessage, OperationReceipt)>, RuntimeMailboxError> {
        self.repository
            .recover_expired_consuming(Utc::now())
            .await?;
        self.drain_once(target).await
    }

    pub async fn recover_pending_once(&self) -> Result<usize, RuntimeMailboxError> {
        self.repository
            .recover_expired_consuming(Utc::now())
            .await?;
        let targets = self.repository.list_pending_targets().await?;
        let mut dispatched = 0;
        for (run_id, agent_id) in targets {
            if self
                .drain_once(&AgentRunRuntimeTarget { run_id, agent_id })
                .await?
                .is_some()
            {
                dispatched += 1;
            }
        }
        Ok(dispatched)
    }

    pub async fn drain_once(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<(AgentRunMailboxMessage, OperationReceipt)>, RuntimeMailboxError> {
        let view = self.runtime.inspect(target.clone()).await?;
        if !runtime_can_start_turn(&view) {
            return Ok(None);
        }
        let claim_token = Uuid::new_v4();
        let Some(message) = self
            .repository
            .claim_next(AgentRunMailboxClaimRequest {
                run_id: target.run_id,
                agent_id: target.agent_id,
                barriers: vec![
                    ConsumptionBarrier::ImmediateIfIdle,
                    ConsumptionBarrier::AgentRunTurnBoundary,
                ],
                drain_mode: Some(MailboxDrainMode::One),
                limit: 1,
                claim_token,
                claim_expires_at: Utc::now() + Duration::seconds(CLAIM_LEASE_SECONDS),
            })
            .await?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let command: StoredRuntimeMailboxCommand =
            serde_json::from_value(message.launch_planning_input.clone().ok_or_else(|| {
                RuntimeMailboxError::InvalidPayload("runtime delivery command is missing".into())
            })?)
            .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
        if command.target != *target {
            return Err(RuntimeMailboxError::InvalidPayload(
                "payload target does not match mailbox ownership".into(),
            ));
        }
        let receipt = match self
            .runtime
            .send_message(SendAgentRunMessage {
                target: target.clone(),
                client_command_id: format!("mailbox-{}", message.id),
                input: command.input,
                actor: command.actor,
                identity: command.identity,
                backend_selection: command.backend_selection,
            })
            .await
        {
            Ok(receipt) => receipt,
            Err(error) if retryable_delivery_error(&error) => {
                self.repository
                    .mark_message_status(
                        message.id,
                        Some(claim_token),
                        MailboxMessageStatus::Queued,
                        Some(error.to_string()),
                    )
                    .await?;
                return Ok(None);
            }
            Err(error) => {
                self.repository
                    .mark_message_status(
                        message.id,
                        Some(claim_token),
                        MailboxMessageStatus::Failed,
                        Some(error.to_string()),
                    )
                    .await?;
                return Err(error.into());
            }
        };
        let dispatched = self
            .repository
            .mark_runtime_operation_accepted(
                message.id,
                claim_token,
                receipt.operation_id.to_string(),
            )
            .await?;
        Ok(Some((dispatched, receipt)))
    }
}

#[async_trait::async_trait]
impl AgentRunProductDeliveryPort for RuntimeAgentRunMailbox {
    async fn deliver(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductDelivery, RuntimeMailboxError> {
        let source = MailboxSourceIdentity::new("product", "runtime_delivery", "platform")
            .with_source_ref(command.client_command_id.clone());
        let outcome = self
            .submit(EnqueueRuntimeMailboxMessage {
                target: AgentRunRuntimeTarget {
                    run_id: command.run_id,
                    agent_id: command.agent_id,
                },
                client_command_id: command.client_command_id,
                input: command.input,
                actor: command.actor,
                identity: command.identity,
                source,
                backend_selection: command.backend_selection,
            })
            .await?;
        Ok(match outcome {
            RuntimeMailboxSubmitOutcome::Queued { message } => AgentRunProductDelivery {
                mailbox_message_id: message.id,
                operation_receipt: None,
                queued: true,
            },
            RuntimeMailboxSubmitOutcome::Dispatched { message, receipt } => {
                AgentRunProductDelivery {
                    mailbox_message_id: message.id,
                    operation_receipt: Some(receipt),
                    queued: false,
                }
            }
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
        ) | AgentRunRuntimeError::StaleActiveTurn
    )
}

fn runtime_input_preview(input: &[RuntimeInput]) -> String {
    input
        .iter()
        .filter_map(|input| match input {
            RuntimeInput::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(200)
        .collect()
}
