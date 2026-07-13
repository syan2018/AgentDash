use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    CommandAvailability, OperationReceipt, PresentationThreadId, RuntimeActor, RuntimeCommandKind,
    RuntimeInput,
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

use super::runtime_facade::AgentRunPresentationDraft;
use super::{
    AgentRunCommandGuard, AgentRunPresentationInput, AgentRunRuntime, AgentRunRuntimeError,
    GuardedAgentRunCommand, SendAgentRunMessage, SteerAgentRunTurn,
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
    pub executor_config: Option<serde_json::Value>,
    pub backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredRuntimeMailboxCommand {
    target: AgentRunRuntimeTarget,
    presentation_thread_id: PresentationThreadId,
    presentation_input: AgentRunPresentationInput,
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
}

#[derive(Clone)]
pub struct RuntimeAgentRunMailbox {
    repository: Arc<dyn AgentRunMailboxRepository>,
    runtime: Arc<dyn AgentRunRuntime>,
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
        let steer_active_turn = command.delivery_intent.as_deref() == Some("steer")
            && command.presentation.emits_user_submission()
            && runtime_can_steer_turn(&view);
        let can_dispatch = runtime_can_start_turn(&view) || steer_active_turn;
        let message_id = Uuid::new_v4();
        let presentation_input = if steer_active_turn {
            steer_presentation_input(&view, message_id, command.presentation.clone())?
        } else {
            launch_presentation_input(command.presentation.clone())?
        };
        let payload = serde_json::to_value(StoredRuntimeMailboxCommand {
            target: command.target.clone(),
            presentation_thread_id: command.presentation_thread_id.clone(),
            presentation_input,
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
                id: Some(message_id),
                run_id: command.target.run_id,
                agent_id: command.target.agent_id,
                origin: command.origin,
                source: command.source,
                delivery: if steer_active_turn {
                    MailboxDelivery::SteerActiveTurn {
                        stop_effect: agentdash_domain::agent_run_mailbox::SteeringStopEffect::None,
                    }
                } else {
                    MailboxDelivery::LaunchOrContinueTurn
                },
                barrier: if steer_active_turn {
                    ConsumptionBarrier::AgentLoopTurnBoundary
                } else if can_dispatch {
                    ConsumptionBarrier::ImmediateIfIdle
                } else {
                    ConsumptionBarrier::AgentRunTurnBoundary
                },
                drain_mode: if steer_active_turn {
                    MailboxDrainMode::All
                } else {
                    MailboxDrainMode::One
                },
                priority: 0,
                source_dedup_key: Some(format!(
                    "runtime-command:{}:{}:{}",
                    command.target.run_id, command.target.agent_id, command.client_command_id
                )),
                payload_json: Some(visible_input),
                executor_config_json: command.executor_config,
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
            Some((message, receipt, steered)) => Ok(RuntimeMailboxSubmitOutcome::Dispatched {
                message,
                receipt,
                steered,
            }),
            None => Ok(RuntimeMailboxSubmitOutcome::Queued { message }),
        }
    }

    pub async fn recover_and_drain_once(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<(AgentRunMailboxMessage, OperationReceipt, bool)>, RuntimeMailboxError> {
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
    ) -> Result<Option<(AgentRunMailboxMessage, OperationReceipt, bool)>, RuntimeMailboxError> {
        let view = self.runtime.inspect(target.clone()).await?;
        let can_start = runtime_can_start_turn(&view);
        let can_steer = runtime_can_steer_turn(&view);
        if !can_start && !can_steer {
            return Ok(None);
        }
        let claim_token = Uuid::new_v4();
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
                    ]
                },
                drain_mode: Some(if can_steer {
                    MailboxDrainMode::All
                } else {
                    MailboxDrainMode::One
                }),
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
        let steered = matches!(message.delivery, MailboxDelivery::SteerActiveTurn { .. });
        let delivery = if steered {
            let snapshot = view.snapshot.as_ref().ok_or_else(|| {
                RuntimeMailboxError::InvalidPayload("active Runtime snapshot is missing".into())
            })?;
            self.runtime
                .steer_active_turn(SteerAgentRunTurn {
                    command: GuardedAgentRunCommand {
                        target: target.clone(),
                        client_command_id: format!("mailbox-{}", message.id),
                        guard: AgentRunCommandGuard {
                            thread_id: snapshot.thread_id.clone(),
                            expected_revision: snapshot.revision,
                            expected_active_turn_id: snapshot.active_turn_id.clone(),
                        },
                        actor: command.actor,
                    },
                    presentation_input: command.presentation_input,
                    input: command.input,
                })
                .await
        } else {
            self.runtime
                .send_message(SendAgentRunMessage {
                    target: target.clone(),
                    presentation_thread_id: command.presentation_thread_id,
                    presentation_input: command.presentation_input,
                    client_command_id: format!("mailbox-{}", message.id),
                    input: command.input,
                    actor: command.actor,
                    identity: command.identity,
                    backend_selection: command.backend_selection,
                })
                .await
        };
        let receipt = match delivery {
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
                Some(format!("turn-{}", receipt.operation_id)),
                Some(format!("turn-{}", receipt.operation_id)),
            )
            .await?;
        Ok(Some((dispatched, receipt, steered)))
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
                    started_at_seconds: Utc::now().timestamp(),
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

fn launch_presentation_input(
    draft: AgentRunPresentationDraft,
) -> Result<AgentRunPresentationInput, RuntimeMailboxError> {
    let turn_id = agentdash_agent_runtime_contract::PresentationTurnId::new(format!(
        "t{}",
        Utc::now().timestamp_millis()
    ))
    .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
    if draft.emits_user_submission() {
        let item_id = agentdash_agent_runtime_contract::PresentationItemId::new(format!(
            "{turn_id}:user-input:0"
        ))
        .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
        return Ok(AgentRunPresentationInput::UserSubmission {
            turn_id,
            item_id,
            content: draft.content,
            source: draft.source,
            submission_kind: draft.submission_kind,
            started_at_seconds: draft.started_at_seconds,
        });
    }
    let message = presentation_input_text(&draft.content);
    Ok(AgentRunPresentationInput::SystemDelivery {
        launch_source: draft.launch_source,
        message,
        turn_id,
        started_at_seconds: draft.started_at_seconds,
    })
}

fn steer_presentation_input(
    view: &super::AgentRunRuntimeView,
    mailbox_message_id: Uuid,
    draft: AgentRunPresentationDraft,
) -> Result<AgentRunPresentationInput, RuntimeMailboxError> {
    let turn_id = view
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.active_presentation_turn_id.clone())
        .ok_or_else(|| {
            RuntimeMailboxError::InvalidPayload(
                "active presentation turn is required for mailbox steer".into(),
            )
        })?;
    let item_id = agentdash_agent_runtime_contract::PresentationItemId::new(format!(
        "{turn_id}:mailbox_steering:scheduler:{mailbox_message_id}:{}",
        Uuid::new_v4()
    ))
    .map_err(|error| RuntimeMailboxError::InvalidPayload(error.to_string()))?;
    Ok(AgentRunPresentationInput::UserSubmission {
        turn_id,
        item_id,
        content: draft.content,
        source: draft.source,
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Steer,
        started_at_seconds: draft.started_at_seconds,
    })
}

fn presentation_input_text(input: &[agentdash_agent_protocol::UserInputBlock]) -> String {
    input
        .iter()
        .filter_map(|block| match block {
            agentdash_agent_protocol::UserInputBlock::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
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
