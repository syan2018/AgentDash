use super::payload::{
    agent_message_to_user_input_blocks, build_input_preview, input_has_images, serialization_error,
};
use super::policy::user_message_policy;
use super::target::{ResolvedAgentRunMailboxCommandTarget, base_refs, ensure_command_target};
use super::*;

impl<'a> AgentRunMailboxService<'a> {
    pub async fn accept_user_message(
        &self,
        command: AgentRunMailboxUserMessageCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let (run, agent, frame) = self
            .resolve_control_plane_for_delivery(&command.runtime_session_id)
            .await?;
        ensure_command_target(&run, &agent, command.run_id, command.agent_id)?;
        let target = AgentRunMailboxCommandTarget::from_runtime_session_adapter(
            run.id,
            agent.id,
            frame.id,
            command.runtime_session_id,
        );
        self.accept_user_message_for_target(AgentRunMailboxUserMessageTargetCommand {
            target,
            source: command.source,
            schedule_on_submit: command.schedule_on_submit,
            input: command.input,
            client_command_id: command.client_command_id,
            executor_config: command.executor_config,
            identity: command.identity,
            delivery_intent: command.delivery_intent,
        })
        .await
    }

    pub async fn accept_user_message_for_target(
        &self,
        command: AgentRunMailboxUserMessageTargetCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        diag!(Debug, Subsystem::AgentRun,

            run_id = %command.target.address.run_id,
            agent_id = %command.target.address.agent_id,
            frame_id = %command.target.address.frame_id,
            runtime_session_id = command
                .target
                .message_stream
                .as_ref()
                .map(|message_stream| message_stream.runtime_session_id.as_str())
                .unwrap_or("<resolved-later>"),
            input_blocks = command.input.len(),
            schedule_on_submit = command.schedule_on_submit,
            "AgentRun mailbox accept user message entered"
        );
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

        let ResolvedAgentRunMailboxCommandTarget {
            run,
            agent,
            frame,
            message_stream,
        } = self.resolve_command_target(command.target).await?;
        let runtime_session_id = message_stream.runtime_session_id;
        diag!(Debug, Subsystem::AgentRun,

            run_id = %run.id,
            agent_id = %agent.id,
            runtime_session_id = %runtime_session_id,
            frame_id = %frame.id,
            frame_revision = frame.revision,
            "AgentRun mailbox control plane resolved"
        );
        let execution_state = self
            .session_core
            .inspect_session_execution_state(&runtime_session_id)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        diag!(Debug, Subsystem::AgentRun,

            run_id = %run.id,
            agent_id = %agent.id,
            runtime_session_id = %runtime_session_id,
            execution_state = ?execution_state,
            "AgentRun mailbox execution state resolved"
        );
        let supports_steering = match execution_state {
            SessionExecutionState::Running { turn_id: Some(_) } => {
                self.session_control
                    .supports_session_steering(&runtime_session_id)
                    .await
            }
            _ => false,
        };

        let request_digest = digest_command_request(&serde_json::json!({
            "kind": "agent_run_mailbox_message_submit",
            "target": {
                "run_id": run.id,
                "agent_id": agent.id,
                "frame_id": frame.id,
            },
            "message_stream": {
                "runtime_session_id": runtime_session_id,
                "trace_kind": message_stream.trace_kind,
            },
            "input": command.input,
            "executor_config": command.executor_config,
        }))?;
        let claim = claim_agent_run_command_receipt(
            self.command_receipt_repo,
            "agent_run_message",
            format!("{}:{}", run.id, agent.id),
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
                run_id: run.id,
                agent_id: agent.id,
                runtime_session_id: runtime_session_id.clone(),
                origin: MailboxMessageOrigin::User,
                source: command.source,
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
        diag!(Debug, Subsystem::AgentRun,

            run_id = %run.id,
            agent_id = %agent.id,
            runtime_session_id = %runtime_session_id,
            mailbox_message_id = %message.id,
            delivery = ?message.delivery,
            barrier = ?message.barrier,
            "AgentRun mailbox message persisted"
        );
        let _ = self
            .command_receipt_repo
            .attach_mailbox_message(claim.record.id, message.id)
            .await?;

        let outcomes = if command.schedule_on_submit {
            diag!(Debug, Subsystem::AgentRun,

                run_id = %run.id,
                agent_id = %agent.id,
                runtime_session_id = %runtime_session_id,
                mailbox_message_id = %message.id,
                "AgentRun mailbox scheduling submitted message"
            );
            self.schedule_for_target(
                AgentRunMailboxCommandTarget::from_runtime_session_adapter(
                    run.id,
                    agent.id,
                    frame.id,
                    runtime_session_id.clone(),
                ),
                AgentRunMailboxScheduleTrigger::UserMessageSubmitted,
                command.identity,
            )
            .await?
        } else {
            Vec::new()
        };
        diag!(Debug, Subsystem::AgentRun,

            run_id = %run.id,
            agent_id = %agent.id,
            runtime_session_id = %runtime_session_id,
            mailbox_message_id = %message.id,
            outcome_count = outcomes.len(),
            "AgentRun mailbox scheduling completed"
        );
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
            let runtime_state = self.inspect_state_optional(&runtime_session_id).await;
            return Ok(AgentRunMailboxCommandResult {
                command_receipt: receipt,
                outcome: outcome.outcome,
                mailbox_message: Some(outcome.mailbox_message),
                accepted_refs: outcome.accepted_refs,
                runtime_state,
            });
        }

        let accepted_refs = Some(base_refs(&run, &agent, Some(&frame), &runtime_session_id));
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
        let (run, agent, _) = self
            .resolve_control_plane_for_delivery(runtime_session_id)
            .await?;
        let payload_json =
            serde_json::to_value(&input).map_err(serialization_error("hook auto-resume input"))?;
        let _message = self
            .mailbox_repo
            .create_message_idempotent(NewAgentRunMailboxMessage {
                run_id: run.id,
                agent_id: agent.id,
                runtime_session_id: runtime_session_id.to_string(),
                origin: MailboxMessageOrigin::Hook,
                source: MailboxSourceIdentity::hook_auto_resume(),
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
        source: MailboxSourceIdentity,
        barrier: ConsumptionBarrier,
        stop_effect: SteeringStopEffect,
        drain_mode: MailboxDrainMode,
        source_event_key: &str,
        messages: Vec<AgentMessage>,
    ) -> Result<Vec<AgentRunMailboxMessage>, WorkflowApplicationError> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }
        let (run, agent, _) = self
            .resolve_control_plane_for_delivery(runtime_session_id)
            .await?;
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
                    source: source.clone(),
                    delivery: MailboxDelivery::SteerActiveTurn { stop_effect },
                    barrier,
                    drain_mode,
                    priority: 100,
                    source_dedup_key: Some(format!(
                        "hook_delivery:{}:{runtime_session_id}:{turn_key}:{source_event_key}:{index}",
                        source.dedup_fragment(),
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
}
