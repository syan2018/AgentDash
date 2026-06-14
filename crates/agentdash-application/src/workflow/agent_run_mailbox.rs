use chrono::{Duration, Utc};
use uuid::Uuid;

use agentdash_agent_protocol::{
    UserInputBlock, UserInputSubmissionKind, user_input_blocks_to_content_parts,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentRunAcceptedRefs, AgentRunCommandKind,
    AgentRunCommandReceiptRepository, AgentRunMailboxClaimRequest, AgentRunMailboxMessage,
    AgentRunMailboxRepository, AgentRunMailboxState, ConsumptionBarrier, LifecycleAgent,
    LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    MAILBOX_DELIVERY_RESULT_UNKNOWN, MailboxDelivery, MailboxDrainMode, MailboxMessageOrigin,
    MailboxMessageSource, MailboxMessageStatus, NewAgentRunMailboxMessage,
    RuntimeSessionExecutionAnchorRepository, SteeringStopEffect,
};
use agentdash_spi::platform::auth::AuthIdentity;
use agentdash_spi::{AgentConfig, AgentMessage, ContentPart};

use crate::session::{
    LaunchCommand, SessionControlService, SessionCoreService, SessionEventingService,
    SessionExecutionState, SessionLaunchService, SessionTurnSteerCommand, UserPromptInput,
};
use crate::workflow::{
    AgentRunCommandReceiptView, AgentRunMessageDelivery, AgentRunMessageDeliveryPort,
    AgentRunMessageLaunchDeliveryPort, WorkflowApplicationError,
    command_receipt::{
        claim_agent_run_command_receipt, digest_command_request, mark_command_terminal_failed,
    },
};

const CLAIM_LEASE_SECONDS: i64 = 300;
const AGENT_LOOP_DRAIN_LIMIT: i64 = 100;
const PROMOTE_PRIORITY: i32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunMailboxCommandOutcome {
    Launched,
    Queued,
    Steered,
    Deleted,
    Resumed,
    Blocked,
    Failed,
}

impl AgentRunMailboxCommandOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Launched => "launched",
            Self::Queued => "queued",
            Self::Steered => "steered",
            Self::Deleted => "deleted",
            Self::Resumed => "resumed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunMailboxScheduleTrigger {
    UserMessageSubmitted,
    AgentLoopTurnBoundary,
    AgentRunTurnBoundary,
    ManualResume,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxUserMessageCommand {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: String,
    pub input: Vec<UserInputBlock>,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub identity: Option<AuthIdentity>,
    /// `Some("steer")` = 明确注入 active turn；其余情况排队（pending）。
    pub delivery_intent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxControlCommand {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: String,
    pub message_id: Option<Uuid>,
    pub client_command_id: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxCommandResult {
    pub command_receipt: AgentRunCommandReceiptView,
    pub outcome: AgentRunMailboxCommandOutcome,
    pub mailbox_message: Option<AgentRunMailboxMessage>,
    pub accepted_refs: Option<AgentRunAcceptedRefs>,
    pub runtime_state: Option<SessionExecutionState>,
}

#[derive(Debug, Clone)]
pub struct AgentRunMailboxScheduleOutcome {
    pub outcome: AgentRunMailboxCommandOutcome,
    pub mailbox_message: AgentRunMailboxMessage,
    pub accepted_refs: Option<AgentRunAcceptedRefs>,
}

pub struct AgentRunMailboxService<'a> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    agent_frame_repo: &'a dyn AgentFrameRepository,
    execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
    mailbox_repo: &'a dyn AgentRunMailboxRepository,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    session_eventing: SessionEventingService,
    session_launch: SessionLaunchService,
}

impl<'a> AgentRunMailboxService<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        agent_frame_repo: &'a dyn AgentFrameRepository,
        execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
        command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
        mailbox_repo: &'a dyn AgentRunMailboxRepository,
        session_core: SessionCoreService,
        session_control: SessionControlService,
        session_eventing: SessionEventingService,
        session_launch: SessionLaunchService,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            execution_anchor_repo,
            command_receipt_repo,
            mailbox_repo,
            session_core,
            session_control,
            session_eventing,
            session_launch,
        }
    }

    pub async fn accept_user_message(
        &self,
        command: AgentRunMailboxUserMessageCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        if command.input.is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "input 不能为空".to_string(),
            ));
        }
        if command.client_command_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "client_command_id 不能为空".to_string(),
            ));
        }

        let (run, agent, frame) = self
            .resolve_control_plane(&command.runtime_session_id)
            .await?;
        ensure_command_target(&run, &agent, command.run_id, command.agent_id)?;
        let execution_state = self
            .session_core
            .inspect_session_execution_state(&command.runtime_session_id)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        let supports_steering = match execution_state {
            SessionExecutionState::Running { turn_id: Some(_) } => {
                self.session_control
                    .supports_session_steering(&command.runtime_session_id)
                    .await
            }
            _ => false,
        };

        let request_digest = digest_command_request(&serde_json::json!({
            "kind": "agent_run_mailbox_message_submit",
            "run_id": command.run_id,
            "agent_id": command.agent_id,
            "runtime_session_id": command.runtime_session_id,
            "input": command.input,
            "executor_config": command.executor_config,
        }))?;
        let claim = claim_agent_run_command_receipt(
            self.command_receipt_repo,
            "agent_run_message",
            format!("{}:{}", command.run_id, command.agent_id),
            AgentRunCommandKind::MessageSubmit,
            command.client_command_id,
            request_digest,
        )
        .await?;
        if claim.duplicate {
            return self
                .replay_duplicate_command(claim.record, Some(frame))
                .await;
        }

        let policy = user_message_policy(
            &execution_state,
            supports_steering,
            command.delivery_intent.as_deref(),
        );
        let payload_json =
            serde_json::to_value(&command.input).map_err(serialization_error("mailbox input"))?;
        let executor_config_json = command
            .executor_config
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(serialization_error("mailbox executor_config"))?;
        let message = self
            .mailbox_repo
            .create_message_idempotent(NewAgentRunMailboxMessage {
                run_id: command.run_id,
                agent_id: command.agent_id,
                runtime_session_id: command.runtime_session_id.clone(),
                origin: MailboxMessageOrigin::User,
                source: MailboxMessageSource::Composer,
                delivery: policy.delivery,
                barrier: policy.barrier,
                drain_mode: policy.drain_mode,
                priority: 0,
                source_dedup_key: Some(format!("command_receipt:{}", claim.record.id)),
                queued_agent_run_turn_id: policy.queued_agent_run_turn_id,
                expected_active_agent_run_turn_id: policy.expected_active_agent_run_turn_id,
                command_receipt_id: Some(claim.record.id),
                payload_json: Some(payload_json),
                executor_config_json,
                preview: build_input_preview(&command.input),
                has_images: input_has_images(&command.input),
                retain_payload: false,
            })
            .await?;
        let _ = self
            .command_receipt_repo
            .attach_mailbox_message(claim.record.id, message.id)
            .await?;

        let outcomes = self
            .schedule(
                command.run_id,
                command.agent_id,
                &command.runtime_session_id,
                AgentRunMailboxScheduleTrigger::UserMessageSubmitted,
                command.identity,
            )
            .await?;
        if let Some(outcome) = outcomes
            .into_iter()
            .find(|outcome| outcome.mailbox_message.id == message.id)
        {
            let receipt = self
                .accepted_command_receipt(
                    claim.record.id,
                    outcome.outcome,
                    &outcome.mailbox_message,
                    outcome.accepted_refs.clone(),
                )
                .await?;
            let runtime_state = self
                .inspect_state_optional(&command.runtime_session_id)
                .await;
            return Ok(AgentRunMailboxCommandResult {
                command_receipt: receipt,
                outcome: outcome.outcome,
                mailbox_message: Some(outcome.mailbox_message),
                accepted_refs: outcome.accepted_refs,
                runtime_state,
            });
        }

        let accepted_refs = Some(base_refs(
            &run,
            &agent,
            Some(&frame),
            &command.runtime_session_id,
        ));
        let receipt = self
            .accepted_command_receipt(
                claim.record.id,
                AgentRunMailboxCommandOutcome::Queued,
                &message,
                accepted_refs.clone(),
            )
            .await?;
        Ok(AgentRunMailboxCommandResult {
            command_receipt: receipt,
            outcome: AgentRunMailboxCommandOutcome::Queued,
            mailbox_message: Some(message),
            accepted_refs,
            runtime_state: Some(execution_state),
        })
    }

    pub async fn accept_hook_auto_resume_effect(
        &self,
        runtime_session_id: &str,
        _effect_id: Uuid,
        source_turn_id: String,
        terminal_event_seq: u64,
        input: Vec<UserInputBlock>,
    ) -> Result<Vec<AgentRunMailboxScheduleOutcome>, WorkflowApplicationError> {
        let (run, agent, _) = self.resolve_control_plane(runtime_session_id).await?;
        let payload_json =
            serde_json::to_value(&input).map_err(serialization_error("hook auto-resume input"))?;
        let _message = self
            .mailbox_repo
            .create_message_idempotent(NewAgentRunMailboxMessage {
                run_id: run.id,
                agent_id: agent.id,
                runtime_session_id: runtime_session_id.to_string(),
                origin: MailboxMessageOrigin::Hook,
                source: MailboxMessageSource::HookAutoResume,
                delivery: MailboxDelivery::ResumeLaunchSource {
                    launch_source: "hook_auto_resume".to_string(),
                },
                barrier: ConsumptionBarrier::ImmediateIfIdle,
                drain_mode: MailboxDrainMode::One,
                priority: 0,
                source_dedup_key: Some(format!(
                    "hook_auto_resume:{runtime_session_id}:{source_turn_id}:{terminal_event_seq}"
                )),
                queued_agent_run_turn_id: Some(source_turn_id),
                expected_active_agent_run_turn_id: None,
                command_receipt_id: None,
                payload_json: Some(payload_json),
                executor_config_json: None,
                preview: format!("Hook auto-resume after terminal event #{terminal_event_seq}"),
                has_images: input_has_images(&input),
                retain_payload: true,
            })
            .await?;
        self.schedule(
            run.id,
            agent.id,
            runtime_session_id,
            AgentRunMailboxScheduleTrigger::ManualResume,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn accept_hook_steering_messages(
        &self,
        runtime_session_id: &str,
        source: MailboxMessageSource,
        barrier: ConsumptionBarrier,
        stop_effect: SteeringStopEffect,
        drain_mode: MailboxDrainMode,
        source_event_key: &str,
        messages: Vec<AgentMessage>,
    ) -> Result<Vec<AgentRunMailboxMessage>, WorkflowApplicationError> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }
        let (run, agent, _) = self.resolve_control_plane(runtime_session_id).await?;
        let active_turn_id = match self
            .session_core
            .inspect_session_execution_state(runtime_session_id)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?
        {
            SessionExecutionState::Running {
                turn_id: Some(turn_id),
            } => Some(turn_id),
            _ => None,
        };

        let mut created = Vec::new();
        for (index, message) in messages.into_iter().enumerate() {
            let input = agent_message_to_user_input_blocks(&message);
            if input.is_empty() {
                continue;
            }
            let payload_json =
                serde_json::to_value(&input).map_err(serialization_error("hook steering input"))?;
            let turn_key = active_turn_id.as_deref().unwrap_or("no_active_turn");
            let mailbox_message = self
                .mailbox_repo
                .create_message_idempotent(NewAgentRunMailboxMessage {
                    run_id: run.id,
                    agent_id: agent.id,
                    runtime_session_id: runtime_session_id.to_string(),
                    origin: MailboxMessageOrigin::Hook,
                    source,
                    delivery: MailboxDelivery::SteerActiveTurn { stop_effect },
                    barrier,
                    drain_mode,
                    priority: 100,
                    source_dedup_key: Some(format!(
                        "hook_delivery:{}:{runtime_session_id}:{turn_key}:{source_event_key}:{index}",
                        source.as_str(),
                    )),
                    queued_agent_run_turn_id: active_turn_id.clone(),
                    expected_active_agent_run_turn_id: active_turn_id.clone(),
                    command_receipt_id: None,
                    payload_json: Some(payload_json),
                    executor_config_json: None,
                    preview: build_input_preview(&input),
                    has_images: input_has_images(&input),
                    retain_payload: true,
                })
                .await?;
            created.push(mailbox_message);
        }
        Ok(created)
    }

    pub async fn delete_message(
        &self,
        command: AgentRunMailboxControlCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let message_id = command.message_id.ok_or_else(|| {
            WorkflowApplicationError::BadRequest("message_id 不能为空".to_string())
        })?;
        let claim = self
            .claim_control_receipt(
                &command,
                AgentRunCommandKind::MailboxDelete,
                "mailbox_delete",
            )
            .await?;
        if claim.duplicate {
            return self.replay_duplicate_command(claim.record, None).await;
        }
        let message = self
            .mailbox_repo
            .get_message(message_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("mailbox message 不存在: {message_id}"))
            })?;
        ensure_message_owner(&message, command.run_id, command.agent_id)?;
        let deleted = match self.mailbox_repo.delete_message(message_id).await? {
            Some(message) => message,
            None => self
                .mailbox_repo
                .get_message(message_id)
                .await?
                .ok_or_else(|| {
                    WorkflowApplicationError::NotFound(format!(
                        "mailbox message 不存在: {message_id}"
                    ))
                })?,
        };
        let refs = self
            .base_refs_for_runtime(
                command.run_id,
                command.agent_id,
                &command.runtime_session_id,
            )
            .await
            .ok();
        let receipt = self
            .accepted_command_receipt(
                claim.record.id,
                AgentRunMailboxCommandOutcome::Deleted,
                &deleted,
                refs.clone(),
            )
            .await?;
        Ok(AgentRunMailboxCommandResult {
            command_receipt: receipt,
            outcome: AgentRunMailboxCommandOutcome::Deleted,
            mailbox_message: Some(deleted),
            accepted_refs: refs,
            runtime_state: self
                .inspect_state_optional(&command.runtime_session_id)
                .await,
        })
    }

    pub async fn promote_message(
        &self,
        command: AgentRunMailboxControlCommand,
        _identity: Option<AuthIdentity>,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let message_id = command.message_id.ok_or_else(|| {
            WorkflowApplicationError::BadRequest("message_id 不能为空".to_string())
        })?;
        let claim = self
            .claim_control_receipt(
                &command,
                AgentRunCommandKind::MailboxPromote,
                "mailbox_promote",
            )
            .await?;
        if claim.duplicate {
            return self.replay_duplicate_command(claim.record, None).await;
        }
        let message = self
            .mailbox_repo
            .get_message(message_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("mailbox message 不存在: {message_id}"))
            })?;
        ensure_message_owner(&message, command.run_id, command.agent_id)?;
        if message.last_error.as_deref() == Some(MAILBOX_DELIVERY_RESULT_UNKNOWN) {
            let error = WorkflowApplicationError::Conflict(
                "mailbox message delivery result is unknown and cannot be promoted".to_string(),
            );
            mark_command_terminal_failed(self.command_receipt_repo, claim.record.id, &error).await;
            return Err(error);
        }
        let promoted = self
            .mailbox_repo
            .update_message_policy(
                message_id,
                MailboxDelivery::SteerActiveTurn {
                    stop_effect: SteeringStopEffect::None,
                },
                ConsumptionBarrier::AgentLoopTurnBoundary,
                MailboxDrainMode::All,
                PROMOTE_PRIORITY,
            )
            .await?;
        let accepted_refs = self
            .base_refs_for_runtime(
                command.run_id,
                command.agent_id,
                &command.runtime_session_id,
            )
            .await
            .ok();
        let receipt = self
            .accepted_command_receipt(
                claim.record.id,
                AgentRunMailboxCommandOutcome::Queued,
                &promoted,
                accepted_refs.clone(),
            )
            .await?;
        Ok(AgentRunMailboxCommandResult {
            command_receipt: receipt,
            outcome: AgentRunMailboxCommandOutcome::Queued,
            mailbox_message: Some(promoted),
            accepted_refs,
            runtime_state: self
                .inspect_state_optional(&command.runtime_session_id)
                .await,
        })
    }

    pub async fn resume_mailbox(
        &self,
        command: AgentRunMailboxControlCommand,
        identity: Option<AuthIdentity>,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let claim = self
            .claim_control_receipt(
                &command,
                AgentRunCommandKind::MailboxResume,
                "mailbox_resume",
            )
            .await?;
        if claim.duplicate {
            return self.replay_duplicate_command(claim.record, None).await;
        }
        let _state = self
            .mailbox_repo
            .resume_state(
                command.run_id,
                command.agent_id,
                command.runtime_session_id.clone(),
            )
            .await?;
        let outcomes = self
            .schedule(
                command.run_id,
                command.agent_id,
                &command.runtime_session_id,
                AgentRunMailboxScheduleTrigger::ManualResume,
                identity,
            )
            .await?;
        let selected = outcomes.into_iter().next();
        let refs = match selected
            .as_ref()
            .and_then(|outcome| outcome.accepted_refs.clone())
        {
            Some(refs) => Some(refs),
            None => self
                .base_refs_for_runtime(
                    command.run_id,
                    command.agent_id,
                    &command.runtime_session_id,
                )
                .await
                .ok(),
        };
        let outcome = selected
            .as_ref()
            .map(|selected| selected.outcome)
            .unwrap_or(AgentRunMailboxCommandOutcome::Resumed);
        let mailbox_message = selected.map(|selected| selected.mailbox_message);
        let receipt = self
            .accepted_control_receipt(
                claim.record.id,
                outcome,
                mailbox_message.as_ref(),
                refs.clone(),
            )
            .await?;
        Ok(AgentRunMailboxCommandResult {
            command_receipt: receipt,
            outcome,
            mailbox_message,
            accepted_refs: refs,
            runtime_state: self
                .inspect_state_optional(&command.runtime_session_id)
                .await,
        })
    }

    pub async fn list_messages(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Vec<AgentRunMailboxMessage>, WorkflowApplicationError> {
        Ok(self.mailbox_repo.list_messages(run_id, agent_id).await?)
    }

    pub async fn get_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<AgentRunMailboxState>, WorkflowApplicationError> {
        Ok(self.mailbox_repo.get_state(run_id, agent_id).await?)
    }

    pub async fn move_message(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        message_id: Uuid,
        after_message_id: Option<Uuid>,
    ) -> Result<AgentRunMailboxMessage, WorkflowApplicationError> {
        let target = self
            .mailbox_repo
            .get_message(message_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("mailbox message 不存在: {message_id}"))
            })?;
        ensure_message_owner(&target, run_id, agent_id)?;
        if target.origin != MailboxMessageOrigin::User {
            return Err(WorkflowApplicationError::BadRequest(
                "只能对 User 来源的消息重排序".to_string(),
            ));
        }
        if !matches!(target.delivery, MailboxDelivery::LaunchOrContinueTurn) {
            return Err(WorkflowApplicationError::BadRequest(
                "只能对 Pending 层消息重排序".to_string(),
            ));
        }
        if is_terminal_message_status(&target.status) {
            return Err(WorkflowApplicationError::Conflict(
                "终态消息不可重排序".to_string(),
            ));
        }

        if let Some(anchor_id) = after_message_id {
            let anchor = self
                .mailbox_repo
                .get_message(anchor_id)
                .await?
                .ok_or_else(|| {
                    WorkflowApplicationError::NotFound(format!(
                        "anchor message 不存在: {anchor_id}"
                    ))
                })?;
            ensure_message_owner(&anchor, run_id, agent_id)?;
            if is_terminal_message_status(&anchor.status) {
                return Err(WorkflowApplicationError::Conflict(
                    "anchor 消息已被消费，请刷新列表".to_string(),
                ));
            }
        }

        Ok(self
            .mailbox_repo
            .move_message_after(message_id, after_message_id, run_id, agent_id)
            .await?)
    }

    pub async fn get_message_content(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        message_id: Uuid,
    ) -> Result<serde_json::Value, WorkflowApplicationError> {
        let message = self
            .mailbox_repo
            .get_message(message_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("mailbox message 不存在: {message_id}"))
            })?;
        ensure_message_owner(&message, run_id, agent_id)?;
        if message.origin != MailboxMessageOrigin::User {
            return Err(WorkflowApplicationError::BadRequest(
                "只能召回 User 来源的消息内容".to_string(),
            ));
        }
        if is_terminal_message_status(&message.status) {
            return Err(WorkflowApplicationError::Conflict(
                "终态消息不可召回".to_string(),
            ));
        }
        message.payload_json.ok_or_else(|| {
            WorkflowApplicationError::Conflict(format!(
                "mailbox message {} payload 已被清理",
                message_id
            ))
        })
    }

    pub async fn pause_for_terminal(
        &self,
        runtime_session_id: &str,
        reason: impl Into<String>,
        message: Option<String>,
    ) -> Result<Option<AgentRunMailboxState>, WorkflowApplicationError> {
        let Some(anchor) = self
            .execution_anchor_repo
            .find_by_session(runtime_session_id)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(
            self.mailbox_repo
                .pause_state(
                    anchor.run_id,
                    anchor.agent_id,
                    runtime_session_id.to_string(),
                    reason.into(),
                    message,
                )
                .await?,
        ))
    }

    pub async fn schedule(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
        trigger: AgentRunMailboxScheduleTrigger,
        identity: Option<AuthIdentity>,
    ) -> Result<Vec<AgentRunMailboxScheduleOutcome>, WorkflowApplicationError> {
        let now = Utc::now();
        let _ = self.mailbox_repo.recover_expired_consuming(now).await?;
        let execution_state = self
            .session_core
            .inspect_session_execution_state(runtime_session_id)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        match trigger {
            AgentRunMailboxScheduleTrigger::UserMessageSubmitted => {
                if runtime_can_launch(&execution_state) {
                    self.claim_and_consume(
                        run_id,
                        agent_id,
                        runtime_session_id,
                        vec![ConsumptionBarrier::ImmediateIfIdle],
                        Some(MailboxDrainMode::One),
                        1,
                        trigger,
                        identity,
                    )
                    .await
                } else {
                    Ok(Vec::new())
                }
            }
            AgentRunMailboxScheduleTrigger::AgentLoopTurnBoundary => {
                self.claim_and_consume(
                    run_id,
                    agent_id,
                    runtime_session_id,
                    vec![ConsumptionBarrier::AgentLoopTurnBoundary],
                    Some(MailboxDrainMode::All),
                    AGENT_LOOP_DRAIN_LIMIT,
                    trigger,
                    identity,
                )
                .await
            }
            AgentRunMailboxScheduleTrigger::AgentRunTurnBoundary => {
                self.claim_and_consume(
                    run_id,
                    agent_id,
                    runtime_session_id,
                    vec![ConsumptionBarrier::AgentRunTurnBoundary],
                    Some(MailboxDrainMode::One),
                    1,
                    trigger,
                    identity,
                )
                .await
            }
            AgentRunMailboxScheduleTrigger::ManualResume => {
                let (barriers, drain_mode, limit) = if runtime_can_launch(&execution_state) {
                    (
                        vec![
                            ConsumptionBarrier::ImmediateIfIdle,
                            ConsumptionBarrier::AgentRunTurnBoundary,
                        ],
                        Some(MailboxDrainMode::One),
                        1,
                    )
                } else {
                    (
                        vec![ConsumptionBarrier::AgentLoopTurnBoundary],
                        Some(MailboxDrainMode::All),
                        AGENT_LOOP_DRAIN_LIMIT,
                    )
                };
                self.claim_and_consume(
                    run_id,
                    agent_id,
                    runtime_session_id,
                    barriers,
                    drain_mode,
                    limit,
                    trigger,
                    identity,
                )
                .await
            }
        }
    }

    pub async fn schedule_for_runtime_session(
        &self,
        runtime_session_id: &str,
        trigger: AgentRunMailboxScheduleTrigger,
    ) -> Result<Vec<AgentRunMailboxScheduleOutcome>, WorkflowApplicationError> {
        let (run, agent, _) = self.resolve_control_plane(runtime_session_id).await?;
        self.schedule(run.id, agent.id, runtime_session_id, trigger, None)
            .await
    }

    pub async fn drain_agent_run_turn_boundary_for_delegate(
        &self,
        runtime_session_id: &str,
    ) -> Result<Vec<AgentMessage>, WorkflowApplicationError> {
        let (run, agent, _) = self.resolve_control_plane(runtime_session_id).await?;
        self.mailbox_repo
            .recover_expired_consuming(Utc::now())
            .await?;
        let mut drained = Vec::new();
        let mut steering = self
            .claim_for_delegate(
                run.id,
                agent.id,
                runtime_session_id,
                Some(MailboxDrainMode::All),
                AGENT_LOOP_DRAIN_LIMIT,
            )
            .await?;
        drained.append(&mut steering);
        let mut turn_message = self
            .claim_for_delegate(
                run.id,
                agent.id,
                runtime_session_id,
                Some(MailboxDrainMode::One),
                1,
            )
            .await?;
        drained.append(&mut turn_message);
        Ok(drained)
    }

    #[allow(clippy::too_many_arguments)]
    async fn claim_and_consume(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
        barriers: Vec<ConsumptionBarrier>,
        drain_mode: Option<MailboxDrainMode>,
        limit: i64,
        trigger: AgentRunMailboxScheduleTrigger,
        identity: Option<AuthIdentity>,
    ) -> Result<Vec<AgentRunMailboxScheduleOutcome>, WorkflowApplicationError> {
        let claim_token = Uuid::new_v4();
        let claimed = self
            .mailbox_repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                runtime_session_id: Some(runtime_session_id.to_string()),
                barriers,
                drain_mode,
                limit,
                claim_token,
                claim_expires_at: Utc::now() + Duration::seconds(CLAIM_LEASE_SECONDS),
            })
            .await?;
        let mut outcomes = Vec::with_capacity(claimed.len());
        for message in claimed {
            outcomes.push(
                self.consume_claimed_message(message, trigger, identity.clone())
                    .await?,
            );
        }
        Ok(outcomes)
    }

    async fn claim_for_delegate(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
        drain_mode: Option<MailboxDrainMode>,
        limit: i64,
    ) -> Result<Vec<AgentMessage>, WorkflowApplicationError> {
        let claim_token = Uuid::new_v4();
        let claimed = self
            .mailbox_repo
            .claim_next(AgentRunMailboxClaimRequest {
                run_id,
                agent_id,
                runtime_session_id: Some(runtime_session_id.to_string()),
                barriers: vec![ConsumptionBarrier::AgentRunTurnBoundary],
                drain_mode,
                limit,
                claim_token,
                claim_expires_at: Utc::now() + Duration::seconds(CLAIM_LEASE_SECONDS),
            })
            .await?;
        let mut messages = Vec::with_capacity(claimed.len());
        for message in claimed {
            if let Some(agent_message) = self.consume_as_delegate_steering(message).await? {
                messages.push(agent_message);
            }
        }
        Ok(messages)
    }

    async fn consume_as_delegate_steering(
        &self,
        message: AgentRunMailboxMessage,
    ) -> Result<Option<AgentMessage>, WorkflowApplicationError> {
        let input = message_input(&message)?;
        let (run, agent, frame) = self
            .resolve_control_plane(&message.runtime_session_id)
            .await?;
        let active_turn_id = match self
            .session_core
            .inspect_session_execution_state(&message.runtime_session_id)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?
        {
            SessionExecutionState::Running {
                turn_id: Some(turn_id),
            } => turn_id,
            SessionExecutionState::Running { turn_id: None } => {
                let _ = self
                    .block_claimed_message(message, "active_agent_run_turn_missing")
                    .await?;
                return Ok(None);
            }
            _ => {
                let _ = self
                    .block_claimed_message(message, "agent_run_not_running")
                    .await?;
                return Ok(None);
            }
        };
        if let Some(expected) = message.expected_active_agent_run_turn_id.as_deref()
            && expected != active_turn_id
        {
            let _ = self
                .block_claimed_message(message, "expected_agent_run_turn_mismatch")
                .await?;
            return Ok(None);
        }

        let agent_message = AgentMessage::user_parts(user_input_blocks_to_content_parts(&input));
        self.session_eventing
            .emit_user_input_submitted(
                &message.runtime_session_id,
                &active_turn_id,
                &format!(
                    "{}:mailbox_delegate:{}:{}",
                    active_turn_id,
                    message.id,
                    Uuid::new_v4()
                ),
                UserInputSubmissionKind::Steer,
                input,
            )
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "AgentRun mailbox delegate steering 事件写入失败: {error}"
                ))
            })?;
        let updated = self
            .mailbox_repo
            .mark_message_status(
                message.id,
                message.claim_token,
                MailboxMessageStatus::Steered,
                Some(active_turn_id.clone()),
                None,
                None,
            )
            .await?;
        let refs = AgentRunAcceptedRefs {
            run_id: run.id,
            agent_id: agent.id,
            frame_id: Some(frame.id),
            frame_revision: Some(frame.revision),
            runtime_session_id: Some(message.runtime_session_id.clone()),
            agent_run_turn_id: Some(active_turn_id),
            protocol_turn_id: None,
        };
        self.complete_message_receipt(&updated, AgentRunMailboxCommandOutcome::Steered, Some(refs))
            .await?;
        self.mailbox_repo.cleanup_user_payload(updated.id).await?;
        Ok(Some(agent_message))
    }

    async fn consume_claimed_message(
        &self,
        message: AgentRunMailboxMessage,
        trigger: AgentRunMailboxScheduleTrigger,
        identity: Option<AuthIdentity>,
    ) -> Result<AgentRunMailboxScheduleOutcome, WorkflowApplicationError> {
        match &message.delivery {
            MailboxDelivery::LaunchOrContinueTurn => {
                let execution_state = self
                    .session_core
                    .inspect_session_execution_state(&message.runtime_session_id)
                    .await
                    .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
                if message.barrier == ConsumptionBarrier::AgentRunTurnBoundary
                    && matches!(
                        trigger,
                        AgentRunMailboxScheduleTrigger::AgentRunTurnBoundary
                    )
                    && matches!(
                        execution_state,
                        SessionExecutionState::Running { turn_id: Some(_) }
                    )
                {
                    self.consume_as_steering(message).await
                } else {
                    self.consume_as_launch(message, identity).await
                }
            }
            MailboxDelivery::SteerActiveTurn { .. } => self.consume_as_steering(message).await,
            MailboxDelivery::ResumeLaunchSource { launch_source } => {
                let launch_source = launch_source.clone();
                self.consume_as_resume_launch_source(message, launch_source)
                    .await
            }
        }
    }

    async fn consume_as_launch(
        &self,
        message: AgentRunMailboxMessage,
        identity: Option<AuthIdentity>,
    ) -> Result<AgentRunMailboxScheduleOutcome, WorkflowApplicationError> {
        let input = message_input(&message)?;
        let executor_config = message_executor_config(&message)?;
        let delivery = AgentRunMessageLaunchDeliveryPort::new(self.session_launch.clone());
        let turn_id = match delivery
            .deliver_user_message(AgentRunMessageDelivery {
                delivery_runtime_session_id: message.runtime_session_id.clone(),
                input,
                executor_config,
                identity,
            })
            .await
        {
            Ok(turn_id) => turn_id,
            Err(error) => {
                let failed = self
                    .mailbox_repo
                    .mark_message_status(
                        message.id,
                        message.claim_token,
                        MailboxMessageStatus::Failed,
                        None,
                        None,
                        Some(error.to_string()),
                    )
                    .await?;
                self.mark_message_receipt_terminal_failed(&failed, error)
                    .await;
                return Ok(AgentRunMailboxScheduleOutcome {
                    outcome: AgentRunMailboxCommandOutcome::Failed,
                    mailbox_message: failed,
                    accepted_refs: None,
                });
            }
        };
        let (run, agent, frame) = self
            .resolve_control_plane(&message.runtime_session_id)
            .await?;
        let refs = AgentRunAcceptedRefs {
            run_id: run.id,
            agent_id: agent.id,
            frame_id: Some(frame.id),
            frame_revision: Some(frame.revision),
            runtime_session_id: Some(message.runtime_session_id.clone()),
            agent_run_turn_id: Some(turn_id.clone()),
            protocol_turn_id: None,
        };
        let updated = self
            .mailbox_repo
            .mark_message_status(
                message.id,
                message.claim_token,
                MailboxMessageStatus::Dispatched,
                Some(turn_id),
                None,
                None,
            )
            .await?;
        self.complete_message_receipt(
            &updated,
            AgentRunMailboxCommandOutcome::Launched,
            Some(refs.clone()),
        )
        .await?;
        self.mailbox_repo.cleanup_user_payload(updated.id).await?;
        Ok(AgentRunMailboxScheduleOutcome {
            outcome: AgentRunMailboxCommandOutcome::Launched,
            mailbox_message: updated,
            accepted_refs: Some(refs),
        })
    }

    async fn consume_as_steering(
        &self,
        message: AgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxScheduleOutcome, WorkflowApplicationError> {
        let input = message_input(&message)?;
        let (run, agent, frame) = self
            .resolve_control_plane(&message.runtime_session_id)
            .await?;
        let active_turn_id = match self
            .session_core
            .inspect_session_execution_state(&message.runtime_session_id)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?
        {
            SessionExecutionState::Running {
                turn_id: Some(turn_id),
            } => turn_id,
            SessionExecutionState::Running { turn_id: None } => {
                return self
                    .block_claimed_message(message, "active_agent_run_turn_missing")
                    .await;
            }
            _ => {
                return self
                    .block_claimed_message(message, "agent_run_not_running")
                    .await;
            }
        };
        if let Some(expected) = message.expected_active_agent_run_turn_id.as_deref()
            && expected != active_turn_id
        {
            return self
                .block_claimed_message(message, "expected_agent_run_turn_mismatch")
                .await;
        }
        if !self
            .session_control
            .supports_session_steering(&message.runtime_session_id)
            .await
        {
            return self
                .block_claimed_message(message, "session_steering_unsupported")
                .await;
        }

        self.session_control
            .steer_session(SessionTurnSteerCommand {
                session_id: message.runtime_session_id.clone(),
                expected_turn_id: active_turn_id.clone(),
                input: input.clone(),
            })
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "AgentRun mailbox steer 投递失败: {error}"
                ))
            })?;
        self.session_eventing
            .emit_user_input_submitted(
                &message.runtime_session_id,
                &active_turn_id,
                &format!(
                    "{}:mailbox:{}:{}",
                    active_turn_id,
                    message.id,
                    Uuid::new_v4()
                ),
                UserInputSubmissionKind::Steer,
                input,
            )
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "AgentRun mailbox steer 事件写入失败: {error}"
                ))
            })?;
        let updated = self
            .mailbox_repo
            .mark_message_status(
                message.id,
                message.claim_token,
                MailboxMessageStatus::Steered,
                Some(active_turn_id.clone()),
                None,
                None,
            )
            .await?;
        let refs = AgentRunAcceptedRefs {
            run_id: run.id,
            agent_id: agent.id,
            frame_id: Some(frame.id),
            frame_revision: Some(frame.revision),
            runtime_session_id: Some(message.runtime_session_id.clone()),
            agent_run_turn_id: Some(active_turn_id),
            protocol_turn_id: None,
        };
        self.complete_message_receipt(
            &updated,
            AgentRunMailboxCommandOutcome::Steered,
            Some(refs.clone()),
        )
        .await?;
        self.mailbox_repo.cleanup_user_payload(updated.id).await?;
        Ok(AgentRunMailboxScheduleOutcome {
            outcome: AgentRunMailboxCommandOutcome::Steered,
            mailbox_message: updated,
            accepted_refs: Some(refs),
        })
    }

    async fn consume_as_resume_launch_source(
        &self,
        message: AgentRunMailboxMessage,
        launch_source: String,
    ) -> Result<AgentRunMailboxScheduleOutcome, WorkflowApplicationError> {
        let input = message_input(&message)?;
        let command = match launch_source.as_str() {
            "hook_auto_resume" => LaunchCommand::hook_auto_resume_input(UserPromptInput {
                input: Some(input),
                env: Default::default(),
                executor_config: None,
                backend_selection: None,
            }),
            other => {
                let failed = self
                    .mailbox_repo
                    .mark_message_status(
                        message.id,
                        message.claim_token,
                        MailboxMessageStatus::Failed,
                        None,
                        None,
                        Some(format!("未知 ResumeLaunchSource: {other}")),
                    )
                    .await?;
                self.mark_message_receipt_terminal_failed(
                    &failed,
                    WorkflowApplicationError::Conflict(format!(
                        "未知 mailbox resume launch source: {other}"
                    )),
                )
                .await;
                return Ok(AgentRunMailboxScheduleOutcome {
                    outcome: AgentRunMailboxCommandOutcome::Failed,
                    mailbox_message: failed,
                    accepted_refs: None,
                });
            }
        };
        let turn_id = match self
            .session_launch
            .launch_command(&message.runtime_session_id, command)
            .await
        {
            Ok(turn_id) => turn_id,
            Err(error) => {
                let failed = self
                    .mailbox_repo
                    .mark_message_status(
                        message.id,
                        message.claim_token,
                        MailboxMessageStatus::Failed,
                        None,
                        None,
                        Some(error.to_string()),
                    )
                    .await?;
                self.mark_message_receipt_terminal_failed(&failed, error.into())
                    .await;
                return Ok(AgentRunMailboxScheduleOutcome {
                    outcome: AgentRunMailboxCommandOutcome::Failed,
                    mailbox_message: failed,
                    accepted_refs: None,
                });
            }
        };
        let (run, agent, frame) = self
            .resolve_control_plane(&message.runtime_session_id)
            .await?;
        let refs = AgentRunAcceptedRefs {
            run_id: run.id,
            agent_id: agent.id,
            frame_id: Some(frame.id),
            frame_revision: Some(frame.revision),
            runtime_session_id: Some(message.runtime_session_id.clone()),
            agent_run_turn_id: Some(turn_id.clone()),
            protocol_turn_id: None,
        };
        let updated = self
            .mailbox_repo
            .mark_message_status(
                message.id,
                message.claim_token,
                MailboxMessageStatus::Dispatched,
                Some(turn_id),
                None,
                None,
            )
            .await?;
        self.complete_message_receipt(
            &updated,
            AgentRunMailboxCommandOutcome::Launched,
            Some(refs.clone()),
        )
        .await?;
        self.mailbox_repo.cleanup_user_payload(updated.id).await?;
        Ok(AgentRunMailboxScheduleOutcome {
            outcome: AgentRunMailboxCommandOutcome::Launched,
            mailbox_message: updated,
            accepted_refs: Some(refs),
        })
    }

    async fn block_claimed_message(
        &self,
        message: AgentRunMailboxMessage,
        reason: &str,
    ) -> Result<AgentRunMailboxScheduleOutcome, WorkflowApplicationError> {
        let blocked = self
            .mailbox_repo
            .mark_message_status(
                message.id,
                message.claim_token,
                MailboxMessageStatus::Blocked,
                None,
                None,
                Some(reason.to_string()),
            )
            .await?;
        self.complete_message_receipt(&blocked, AgentRunMailboxCommandOutcome::Blocked, None)
            .await?;
        Ok(AgentRunMailboxScheduleOutcome {
            outcome: AgentRunMailboxCommandOutcome::Blocked,
            mailbox_message: blocked,
            accepted_refs: None,
        })
    }

    async fn resolve_control_plane(
        &self,
        runtime_session_id: &str,
    ) -> Result<(LifecycleRun, LifecycleAgent, AgentFrame), WorkflowApplicationError> {
        let anchor = self
            .execution_anchor_repo
            .find_by_session(runtime_session_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "runtime_session 缺少 RuntimeSessionExecutionAnchor: {runtime_session_id}"
                ))
            })?;
        let agent = self
            .lifecycle_agent_repo
            .get(anchor.agent_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_agent 不存在: {}",
                    anchor.agent_id
                ))
            })?;
        if agent.run_id != anchor.run_id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "RuntimeSessionExecutionAnchor run {} 与 LifecycleAgent run {} 不一致",
                anchor.run_id, agent.run_id
            )));
        }
        if is_terminal_agent_status(&agent.status) {
            return Err(WorkflowApplicationError::Conflict(
                "当前 Agent 已结束，不能继续发送消息".to_string(),
            ));
        }
        let run = self
            .lifecycle_run_repo
            .get_by_id(anchor.run_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_run 不存在: {}",
                    anchor.run_id
                ))
            })?;
        let frame = self
            .agent_frame_repo
            .get_current(agent.id)
            .await?
            .or(self.agent_frame_repo.get(anchor.launch_frame_id).await?)
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_agent {} 没有 current AgentFrame",
                    agent.id
                ))
            })?;
        if frame.agent_id != agent.id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "AgentFrame {} 不属于 LifecycleAgent {}",
                frame.id, agent.id
            )));
        }
        Ok((run, agent, frame))
    }

    async fn base_refs_for_runtime(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
    ) -> Result<AgentRunAcceptedRefs, WorkflowApplicationError> {
        let (run, agent, frame) = self.resolve_control_plane(runtime_session_id).await?;
        ensure_command_target(&run, &agent, run_id, agent_id)?;
        Ok(base_refs(&run, &agent, Some(&frame), runtime_session_id))
    }

    async fn claim_control_receipt(
        &self,
        command: &AgentRunMailboxControlCommand,
        command_kind: AgentRunCommandKind,
        digest_kind: &str,
    ) -> Result<
        crate::workflow::command_receipt::ClaimedAgentRunCommandReceipt,
        WorkflowApplicationError,
    > {
        if command.client_command_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "client_command_id 不能为空".to_string(),
            ));
        }
        let request_digest = digest_command_request(&serde_json::json!({
            "kind": digest_kind,
            "run_id": command.run_id,
            "agent_id": command.agent_id,
            "runtime_session_id": command.runtime_session_id,
            "message_id": command.message_id,
        }))?;
        claim_agent_run_command_receipt(
            self.command_receipt_repo,
            "agent_run_mailbox",
            format!("{}:{}", command.run_id, command.agent_id),
            command_kind,
            command.client_command_id.clone(),
            request_digest,
        )
        .await
    }

    async fn accepted_command_receipt(
        &self,
        receipt_id: Uuid,
        outcome: AgentRunMailboxCommandOutcome,
        message: &AgentRunMailboxMessage,
        accepted_refs: Option<AgentRunAcceptedRefs>,
    ) -> Result<AgentRunCommandReceiptView, WorkflowApplicationError> {
        self.command_receipt_repo
            .attach_mailbox_message(receipt_id, message.id)
            .await?;
        self.accepted_control_receipt(receipt_id, outcome, Some(message), accepted_refs)
            .await
    }

    async fn accepted_control_receipt(
        &self,
        receipt_id: Uuid,
        outcome: AgentRunMailboxCommandOutcome,
        message: Option<&AgentRunMailboxMessage>,
        accepted_refs: Option<AgentRunAcceptedRefs>,
    ) -> Result<AgentRunCommandReceiptView, WorkflowApplicationError> {
        let refs = match accepted_refs {
            Some(refs) => refs,
            None => match message {
                Some(message) => {
                    self.base_refs_for_runtime(
                        message.run_id,
                        message.agent_id,
                        &message.runtime_session_id,
                    )
                    .await?
                }
                None => AgentRunAcceptedRefs {
                    run_id: Uuid::nil(),
                    agent_id: Uuid::nil(),
                    frame_id: None,
                    frame_revision: None,
                    runtime_session_id: None,
                    agent_run_turn_id: None,
                    protocol_turn_id: None,
                },
            },
        };
        let accepted = self
            .command_receipt_repo
            .mark_accepted(receipt_id, refs)
            .await?;
        let result_json = serde_json::json!({
            "outcome": outcome.as_str(),
            "mailbox_message_id": message.map(|message| message.id),
        });
        let stored = self
            .command_receipt_repo
            .store_result_json(receipt_id, result_json)
            .await?;
        Ok(AgentRunCommandReceiptView::from_record(
            if stored.updated_at >= accepted.updated_at {
                &stored
            } else {
                &accepted
            },
            false,
        ))
    }

    async fn complete_message_receipt(
        &self,
        message: &AgentRunMailboxMessage,
        outcome: AgentRunMailboxCommandOutcome,
        accepted_refs: Option<AgentRunAcceptedRefs>,
    ) -> Result<(), WorkflowApplicationError> {
        if let Some(receipt_id) = message.command_receipt_id {
            self.accepted_command_receipt(receipt_id, outcome, message, accepted_refs)
                .await?;
        }
        Ok(())
    }

    async fn mark_message_receipt_terminal_failed(
        &self,
        message: &AgentRunMailboxMessage,
        error: WorkflowApplicationError,
    ) {
        if let Some(receipt_id) = message.command_receipt_id {
            mark_command_terminal_failed(self.command_receipt_repo, receipt_id, &error).await;
        }
    }

    async fn replay_duplicate_command(
        &self,
        record: agentdash_domain::workflow::AgentRunCommandReceipt,
        frame: Option<AgentFrame>,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let mailbox_message = match record.mailbox_message_id {
            Some(message_id) => self.mailbox_repo.get_message(message_id).await?,
            None => None,
        };
        let outcome = mailbox_message
            .as_ref()
            .map(outcome_from_message)
            .or_else(|| {
                record
                    .result_json
                    .as_ref()
                    .and_then(outcome_from_result_json)
            })
            .unwrap_or(AgentRunMailboxCommandOutcome::Queued);
        let accepted_refs = match record.accepted_refs.clone() {
            Some(refs) => Some(refs),
            None => match (mailbox_message.as_ref(), frame.as_ref()) {
                (Some(message), Some(frame)) => Some(AgentRunAcceptedRefs {
                    run_id: message.run_id,
                    agent_id: message.agent_id,
                    frame_id: Some(frame.id),
                    frame_revision: Some(frame.revision),
                    runtime_session_id: Some(message.runtime_session_id.clone()),
                    agent_run_turn_id: message.accepted_agent_run_turn_id.clone(),
                    protocol_turn_id: message.accepted_protocol_turn_id.clone(),
                }),
                _ => None,
            },
        };
        let runtime_state = match mailbox_message.as_ref() {
            Some(message) => {
                self.inspect_state_optional(&message.runtime_session_id)
                    .await
            }
            None => None,
        };
        Ok(AgentRunMailboxCommandResult {
            command_receipt: AgentRunCommandReceiptView::from_record(&record, true),
            outcome,
            mailbox_message,
            accepted_refs,
            runtime_state,
        })
    }

    async fn inspect_state_optional(
        &self,
        runtime_session_id: &str,
    ) -> Option<SessionExecutionState> {
        self.session_core
            .inspect_session_execution_state(runtime_session_id)
            .await
            .ok()
    }
}

#[derive(Debug, Clone)]
struct UserMessagePolicy {
    delivery: MailboxDelivery,
    barrier: ConsumptionBarrier,
    drain_mode: MailboxDrainMode,
    queued_agent_run_turn_id: Option<String>,
    expected_active_agent_run_turn_id: Option<String>,
}

fn user_message_policy(
    execution_state: &SessionExecutionState,
    supports_steering: bool,
    delivery_intent: Option<&str>,
) -> UserMessagePolicy {
    let user_wants_steer = delivery_intent == Some("steer");
    match execution_state {
        SessionExecutionState::Running {
            turn_id: Some(active_turn_id),
        } if supports_steering && user_wants_steer => UserMessagePolicy {
            delivery: MailboxDelivery::SteerActiveTurn {
                stop_effect: SteeringStopEffect::None,
            },
            barrier: ConsumptionBarrier::AgentLoopTurnBoundary,
            drain_mode: MailboxDrainMode::All,
            queued_agent_run_turn_id: Some(active_turn_id.clone()),
            expected_active_agent_run_turn_id: Some(active_turn_id.clone()),
        },
        SessionExecutionState::Running { turn_id }
        | SessionExecutionState::Cancelling { turn_id } => UserMessagePolicy {
            delivery: MailboxDelivery::LaunchOrContinueTurn,
            barrier: ConsumptionBarrier::AgentRunTurnBoundary,
            drain_mode: MailboxDrainMode::One,
            queued_agent_run_turn_id: turn_id.clone(),
            expected_active_agent_run_turn_id: turn_id.clone(),
        },
        SessionExecutionState::Idle
        | SessionExecutionState::Completed { .. }
        | SessionExecutionState::Failed { .. }
        | SessionExecutionState::Interrupted { .. } => UserMessagePolicy {
            delivery: MailboxDelivery::LaunchOrContinueTurn,
            barrier: ConsumptionBarrier::ImmediateIfIdle,
            drain_mode: MailboxDrainMode::One,
            queued_agent_run_turn_id: None,
            expected_active_agent_run_turn_id: None,
        },
    }
}

fn runtime_can_launch(execution_state: &SessionExecutionState) -> bool {
    matches!(
        execution_state,
        SessionExecutionState::Idle
            | SessionExecutionState::Completed { .. }
            | SessionExecutionState::Failed { .. }
            | SessionExecutionState::Interrupted { .. }
    )
}

fn message_input(
    message: &AgentRunMailboxMessage,
) -> Result<Vec<UserInputBlock>, WorkflowApplicationError> {
    let Some(payload) = message.payload_json.clone() else {
        return Err(WorkflowApplicationError::Conflict(format!(
            "mailbox message {} 缺少 payload",
            message.id
        )));
    };
    serde_json::from_value(payload).map_err(|error| {
        WorkflowApplicationError::BadRequest(format!(
            "mailbox message {} payload 无效: {error}",
            message.id
        ))
    })
}

fn message_executor_config(
    message: &AgentRunMailboxMessage,
) -> Result<Option<AgentConfig>, WorkflowApplicationError> {
    message
        .executor_config_json
        .clone()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowApplicationError::BadRequest(format!(
                "mailbox message {} executor_config 无效: {error}",
                message.id
            ))
        })
}

fn build_input_preview(input: &[UserInputBlock]) -> String {
    input
        .iter()
        .find_map(|block| match block {
            UserInputBlock::Text { text, .. } => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(truncate_preview(trimmed, 80))
                }
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let preview = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!(
            "{}...",
            preview
                .chars()
                .take(max_chars.saturating_sub(3))
                .collect::<String>()
        )
    } else {
        preview
    }
}

fn input_has_images(input: &[UserInputBlock]) -> bool {
    input
        .iter()
        .any(|block| matches!(block, UserInputBlock::Image { .. }))
}

fn agent_message_to_user_input_blocks(message: &AgentMessage) -> Vec<UserInputBlock> {
    match message {
        AgentMessage::User { content, .. }
        | AgentMessage::Assistant { content, .. }
        | AgentMessage::ToolResult { content, .. } => content
            .iter()
            .filter_map(content_part_to_user_input)
            .collect(),
        AgentMessage::CompactionSummary { summary, .. } => {
            text_to_user_input(summary).into_iter().collect()
        }
    }
}

fn content_part_to_user_input(part: &ContentPart) -> Option<UserInputBlock> {
    match part {
        ContentPart::Text { text } | ContentPart::Reasoning { text, .. } => {
            text_to_user_input(text)
        }
        ContentPart::Image { mime_type, data } => Some(UserInputBlock::Image {
            detail: None,
            url: format!("data:{mime_type};base64,{data}"),
        }),
    }
}

fn text_to_user_input(text: &str) -> Option<UserInputBlock> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(UserInputBlock::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        })
    }
}

fn base_refs(
    run: &LifecycleRun,
    agent: &LifecycleAgent,
    frame: Option<&AgentFrame>,
    runtime_session_id: &str,
) -> AgentRunAcceptedRefs {
    AgentRunAcceptedRefs {
        run_id: run.id,
        agent_id: agent.id,
        frame_id: frame.map(|frame| frame.id),
        frame_revision: frame.map(|frame| frame.revision),
        runtime_session_id: Some(runtime_session_id.to_string()),
        agent_run_turn_id: None,
        protocol_turn_id: None,
    }
}

fn ensure_command_target(
    run: &LifecycleRun,
    agent: &LifecycleAgent,
    expected_run_id: Uuid,
    expected_agent_id: Uuid,
) -> Result<(), WorkflowApplicationError> {
    if run.id != expected_run_id || agent.id != expected_agent_id {
        return Err(WorkflowApplicationError::Conflict(format!(
            "runtime_session anchor 指向 {} / {}，不匹配请求 {} / {}",
            run.id, agent.id, expected_run_id, expected_agent_id
        )));
    }
    Ok(())
}

fn ensure_message_owner(
    message: &AgentRunMailboxMessage,
    run_id: Uuid,
    agent_id: Uuid,
) -> Result<(), WorkflowApplicationError> {
    if message.run_id != run_id || message.agent_id != agent_id {
        return Err(WorkflowApplicationError::Conflict(format!(
            "mailbox message {} 不属于 AgentRun {} / {}",
            message.id, run_id, agent_id
        )));
    }
    Ok(())
}

fn outcome_from_message(message: &AgentRunMailboxMessage) -> AgentRunMailboxCommandOutcome {
    match message.status {
        MailboxMessageStatus::Dispatched => AgentRunMailboxCommandOutcome::Launched,
        MailboxMessageStatus::Steered => AgentRunMailboxCommandOutcome::Steered,
        MailboxMessageStatus::Deleted => AgentRunMailboxCommandOutcome::Deleted,
        MailboxMessageStatus::Blocked => AgentRunMailboxCommandOutcome::Blocked,
        MailboxMessageStatus::Failed => AgentRunMailboxCommandOutcome::Failed,
        MailboxMessageStatus::Accepted
        | MailboxMessageStatus::Queued
        | MailboxMessageStatus::ReadyToConsume
        | MailboxMessageStatus::Consuming
        | MailboxMessageStatus::Paused => AgentRunMailboxCommandOutcome::Queued,
    }
}

fn outcome_from_result_json(value: &serde_json::Value) -> Option<AgentRunMailboxCommandOutcome> {
    match value.get("outcome").and_then(serde_json::Value::as_str)? {
        "launched" => Some(AgentRunMailboxCommandOutcome::Launched),
        "queued" => Some(AgentRunMailboxCommandOutcome::Queued),
        "steered" => Some(AgentRunMailboxCommandOutcome::Steered),
        "deleted" => Some(AgentRunMailboxCommandOutcome::Deleted),
        "resumed" => Some(AgentRunMailboxCommandOutcome::Resumed),
        "blocked" => Some(AgentRunMailboxCommandOutcome::Blocked),
        "failed" => Some(AgentRunMailboxCommandOutcome::Failed),
        _ => None,
    }
}

fn serialization_error(
    label: &'static str,
) -> impl FnOnce(serde_json::Error) -> WorkflowApplicationError {
    move |error| WorkflowApplicationError::BadRequest(format!("{label} 无法序列化: {error}"))
}

fn is_terminal_agent_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn is_terminal_message_status(status: &MailboxMessageStatus) -> bool {
    matches!(
        status,
        MailboxMessageStatus::Dispatched
            | MailboxMessageStatus::Steered
            | MailboxMessageStatus::Deleted
    )
}
