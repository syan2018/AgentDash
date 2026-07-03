use super::payload::{message_executor_config, message_input, message_launch_planning_input};
use super::policy::runtime_can_launch;
use super::target::ensure_command_target;
use super::*;
use agentdash_application_ports::launch::{LaunchCommand, LaunchPromptInput};

#[derive(Clone, Copy)]
enum SteeringDeliveryMode {
    DelegateReturn,
    LiveSteer,
}

struct SteeringDeliveryResult {
    outcome: AgentRunMailboxScheduleOutcome,
    agent_message: Option<AgentMessage>,
}

impl<'a> AgentRunMailboxService<'a> {
    pub async fn schedule(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
        trigger: AgentRunMailboxScheduleTrigger,
        identity: Option<AuthIdentity>,
    ) -> Result<Vec<AgentRunMailboxScheduleOutcome>, WorkflowApplicationError> {
        let (run, agent, frame) = self
            .resolve_control_plane_for_delivery(runtime_session_id)
            .await?;
        ensure_command_target(&run, &agent, run_id, agent_id)?;
        self.schedule_for_target(
            AgentRunMailboxCommandTarget::from_runtime_session_adapter(
                run.id,
                agent.id,
                frame.id,
                runtime_session_id.to_string(),
            ),
            trigger,
            identity,
        )
        .await
    }

    pub async fn schedule_for_target(
        &self,
        target: AgentRunMailboxCommandTarget,
        trigger: AgentRunMailboxScheduleTrigger,
        identity: Option<AuthIdentity>,
    ) -> Result<Vec<AgentRunMailboxScheduleOutcome>, WorkflowApplicationError> {
        let target = self.resolve_command_target(target).await?;
        let run_id = target.run.id;
        let agent_id = target.agent.id;
        let runtime_session_id = target.message_stream.runtime_session_id;
        diag!(Debug, Subsystem::AgentRun,

            run_id = %run_id,
            agent_id = %agent_id,
            runtime_session_id = %runtime_session_id,
            trigger = ?trigger,
            "AgentRun mailbox schedule entered"
        );
        let now = Utc::now();
        let _ = self.mailbox_repo.recover_expired_consuming(now).await?;
        let execution_state = self
            .session_core
            .inspect_session_execution_state(&runtime_session_id)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        diag!(Debug, Subsystem::AgentRun,

            run_id = %run_id,
            agent_id = %agent_id,
            runtime_session_id = %runtime_session_id,
            trigger = ?trigger,
            execution_state = ?execution_state,
            "AgentRun mailbox schedule state resolved"
        );
        match trigger {
            AgentRunMailboxScheduleTrigger::UserMessageSubmitted => {
                if runtime_can_launch(&execution_state) {
                    self.claim_and_consume(
                        run_id,
                        agent_id,
                        &runtime_session_id,
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
                    &runtime_session_id,
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
                    &runtime_session_id,
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
                    &runtime_session_id,
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
        let (run, agent, frame) = self
            .resolve_control_plane_for_delivery(runtime_session_id)
            .await?;
        self.schedule_for_target(
            AgentRunMailboxCommandTarget::from_runtime_session_adapter(
                run.id,
                agent.id,
                frame.id,
                runtime_session_id.to_string(),
            ),
            trigger,
            None,
        )
        .await
    }

    pub async fn drain_agent_run_turn_boundary_for_delegate(
        &self,
        runtime_session_id: &str,
    ) -> Result<Vec<AgentMessage>, WorkflowApplicationError> {
        let (run, agent, _) = self
            .resolve_control_plane_for_delivery(runtime_session_id)
            .await?;
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
        diag!(Debug, Subsystem::AgentRun,

            run_id = %run_id,
            agent_id = %agent_id,
            runtime_session_id = %runtime_session_id,
            trigger = ?trigger,
            barriers = ?barriers,
            drain_mode = ?drain_mode,
            limit,
            claim_token = %claim_token,
            "AgentRun mailbox claim starting"
        );
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
        diag!(Debug, Subsystem::AgentRun,

            run_id = %run_id,
            agent_id = %agent_id,
            runtime_session_id = %runtime_session_id,
            trigger = ?trigger,
            claimed_count = claimed.len(),
            claim_token = %claim_token,
            "AgentRun mailbox claim completed"
        );
        let mut outcomes = Vec::with_capacity(claimed.len());
        for message in claimed {
            diag!(Debug, Subsystem::AgentRun,

                run_id = %run_id,
                agent_id = %agent_id,
                runtime_session_id = %runtime_session_id,
                trigger = ?trigger,
                mailbox_message_id = %message.id,
                delivery = ?message.delivery,
                barrier = ?message.barrier,
                "AgentRun mailbox consuming claimed message"
            );
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
        self.execute_steering_delivery(message, SteeringDeliveryMode::DelegateReturn)
            .await
            .map(|result| result.agent_message)
    }

    async fn consume_claimed_message(
        &self,
        message: AgentRunMailboxMessage,
        trigger: AgentRunMailboxScheduleTrigger,
        identity: Option<AuthIdentity>,
    ) -> Result<AgentRunMailboxScheduleOutcome, WorkflowApplicationError> {
        diag!(Debug, Subsystem::AgentRun,

            runtime_session_id = %message.runtime_session_id,
            mailbox_message_id = %message.id,
            delivery = ?message.delivery,
            barrier = ?message.barrier,
            trigger = ?trigger,
            "AgentRun mailbox consume claimed message entered"
        );
        match &message.delivery {
            MailboxDelivery::LaunchOrContinueTurn => {
                let execution_state = self
                    .session_core
                    .inspect_session_execution_state(&message.runtime_session_id)
                    .await
                    .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
                diag!(Debug, Subsystem::AgentRun,

                    runtime_session_id = %message.runtime_session_id,
                    mailbox_message_id = %message.id,
                    execution_state = ?execution_state,
                    trigger = ?trigger,
                    "AgentRun mailbox launch-or-continue state resolved"
                );
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
        diag!(Debug, Subsystem::AgentRun,

            runtime_session_id = %message.runtime_session_id,
            mailbox_message_id = %message.id,
            "AgentRun mailbox launch consumption entered"
        );
        let input = message_input(&message)?;
        let executor_config = message_executor_config(&message)?;
        let planning_input = message_launch_planning_input(&message)?;
        let delivery = SessionTurnMessageDeliveryPort::new(self.session_launch.clone());
        diag!(Debug, Subsystem::AgentRun,

            runtime_session_id = %message.runtime_session_id,
            mailbox_message_id = %message.id,
            input_blocks = input.len(),
            has_executor_config = executor_config.is_some(),
            "AgentRun mailbox delivering launch message"
        );
        let turn_id = match delivery
            .deliver_user_message(AgentRunMessageDelivery {
                delivery_runtime_session_id: message.runtime_session_id.clone(),
                input,
                executor_config,
                planning_input,
                identity,
            })
            .await
        {
            Ok(turn_id) => turn_id,
            Err(error) => {
                diag!(Debug, Subsystem::AgentRun,

                    runtime_session_id = %message.runtime_session_id,
                    mailbox_message_id = %message.id,
                    error = %error,
                    "AgentRun mailbox launch delivery failed"
                );
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
        diag!(Debug, Subsystem::AgentRun,

            runtime_session_id = %message.runtime_session_id,
            mailbox_message_id = %message.id,
            turn_id = %turn_id,
            "AgentRun mailbox launch delivery accepted"
        );
        let (run, agent, frame) = self
            .resolve_control_plane_for_delivery(&message.runtime_session_id)
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
        self.execute_steering_delivery(message, SteeringDeliveryMode::LiveSteer)
            .await
            .map(|result| result.outcome)
    }

    async fn execute_steering_delivery(
        &self,
        message: AgentRunMailboxMessage,
        mode: SteeringDeliveryMode,
    ) -> Result<SteeringDeliveryResult, WorkflowApplicationError> {
        let input = message_input(&message)?;
        let (run, agent, frame) = self
            .resolve_control_plane_for_delivery(&message.runtime_session_id)
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
                let outcome = self
                    .block_claimed_message(message, "active_agent_run_turn_missing")
                    .await?;
                return Ok(SteeringDeliveryResult {
                    outcome,
                    agent_message: None,
                });
            }
            _ => {
                let outcome = self
                    .block_claimed_message(message, "agent_run_not_running")
                    .await?;
                return Ok(SteeringDeliveryResult {
                    outcome,
                    agent_message: None,
                });
            }
        };
        if let Some(expected) = message.expected_active_agent_run_turn_id.as_deref()
            && expected != active_turn_id
        {
            let outcome = self
                .block_claimed_message(message, "expected_agent_run_turn_mismatch")
                .await?;
            return Ok(SteeringDeliveryResult {
                outcome,
                agent_message: None,
            });
        }

        let agent_message = match mode {
            SteeringDeliveryMode::DelegateReturn => Some(AgentMessage::user_parts(
                user_input_blocks_to_content_parts(&input),
            )),
            SteeringDeliveryMode::LiveSteer => {
                if !self
                    .session_control
                    .supports_session_steering(&message.runtime_session_id)
                    .await
                {
                    let outcome = self
                        .block_claimed_message(message, "session_steering_unsupported")
                        .await?;
                    return Ok(SteeringDeliveryResult {
                        outcome,
                        agent_message: None,
                    });
                }

                if let Err(error) = self
                    .session_control
                    .steer_session(SessionTurnSteerCommand {
                        session_id: message.runtime_session_id.clone(),
                        expected_turn_id: active_turn_id.clone(),
                        input: input.clone(),
                    })
                    .await
                {
                    let outcome = self
                        .fail_claimed_message(
                            message,
                            format!("AgentRun mailbox steer 投递失败: {error}"),
                        )
                        .await?;
                    return Ok(SteeringDeliveryResult {
                        outcome,
                        agent_message: None,
                    });
                }
                None
            }
        };

        let event_route = match mode {
            SteeringDeliveryMode::DelegateReturn => "delegate",
            SteeringDeliveryMode::LiveSteer => "scheduler",
        };
        let event_error = match self
            .session_eventing
            .emit_user_input_submitted(
                &message.runtime_session_id,
                &active_turn_id,
                &format!(
                    "{}:mailbox_steering:{}:{}:{}",
                    active_turn_id,
                    event_route,
                    message.id,
                    Uuid::new_v4()
                ),
                UserInputSubmissionKind::Steer,
                input,
            )
            .await
        {
            Ok(()) => None,
            Err(error) => {
                let event_error = format!("AgentRun mailbox steering 事件写入失败: {error}");
                let diagnostic_context =
                    DiagnosticErrorContext::new("agent_run.mailbox.scheduler", "emit_steer_event");
                diag_error!(Warn, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    runtime_session_id = %message.runtime_session_id,
                    run_id = %run.id,
                    agent_id = %agent.id,
                    frame_id = %frame.id,
                    mailbox_message_id = %message.id,
                    active_turn_id = %active_turn_id,
                    delivery_mode = event_route,
                    "AgentRun mailbox steering accepted but event projection failed"
                );
                Some(event_error)
            }
        };
        let updated = self
            .mailbox_repo
            .mark_message_status(
                message.id,
                message.claim_token,
                MailboxMessageStatus::Steered,
                Some(active_turn_id.clone()),
                None,
                event_error,
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
        let outcome = AgentRunMailboxScheduleOutcome {
            outcome: AgentRunMailboxCommandOutcome::Steered,
            mailbox_message: updated,
            accepted_refs: Some(refs),
        };
        Ok(SteeringDeliveryResult {
            outcome,
            agent_message,
        })
    }

    async fn consume_as_resume_launch_source(
        &self,
        message: AgentRunMailboxMessage,
        launch_source: String,
    ) -> Result<AgentRunMailboxScheduleOutcome, WorkflowApplicationError> {
        let input = message_input(&message)?;
        let command = match launch_source.as_str() {
            "hook_auto_resume" => LaunchCommand::hook_auto_resume_input(LaunchPromptInput {
                input: Some(input),
                environment_variables: Default::default(),
                executor_config: None,
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
            .launch_command_in_task(
                message.runtime_session_id.clone(),
                command,
                message_launch_planning_input(&message)?,
            )
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
            .resolve_control_plane_for_delivery(&message.runtime_session_id)
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

    async fn fail_claimed_message(
        &self,
        message: AgentRunMailboxMessage,
        error_message: String,
    ) -> Result<AgentRunMailboxScheduleOutcome, WorkflowApplicationError> {
        let failed = self
            .mailbox_repo
            .mark_message_status(
                message.id,
                message.claim_token,
                MailboxMessageStatus::Failed,
                None,
                None,
                Some(error_message.clone()),
            )
            .await?;
        self.mark_message_receipt_terminal_failed(
            &failed,
            WorkflowApplicationError::Internal(error_message),
        )
        .await;
        Ok(AgentRunMailboxScheduleOutcome {
            outcome: AgentRunMailboxCommandOutcome::Failed,
            mailbox_message: failed,
            accepted_refs: None,
        })
    }
}
