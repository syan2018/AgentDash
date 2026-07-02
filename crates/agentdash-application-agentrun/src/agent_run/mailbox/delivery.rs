use super::payload::{
    agent_message_to_user_input_blocks, build_input_preview, input_has_images, serialization_error,
};
use super::policy::user_message_policy;
use super::target::{ResolvedAgentRunMailboxCommandTarget, base_refs, ensure_command_target};
use super::*;
use agentdash_application_ports::launch::{
    BackendSelectionInput, BackendSelectionInputMode, LaunchPlanningInput,
};

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
        self.accept_intake_message_for_target(AgentRunMailboxIntakeTargetCommand {
            target,
            origin: MailboxMessageOrigin::User,
            source: command.source,
            retain_payload: false,
            schedule_on_submit: command.schedule_on_submit,
            input: command.input,
            client_command_id: command.client_command_id,
            source_dedup_key: None,
            executor_config: command.executor_config,
            backend_selection: command.backend_selection,
            identity: command.identity,
            delivery_intent: command.delivery_intent,
        })
        .await
    }

    pub async fn accept_intake_message(
        &self,
        command: AgentRunMailboxIntakeCommand,
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
        self.accept_intake_message_for_target(AgentRunMailboxIntakeTargetCommand {
            target,
            origin: command.origin,
            source: command.source,
            retain_payload: command.retain_payload,
            schedule_on_submit: command.schedule_on_submit,
            input: command.input,
            client_command_id: command.client_command_id,
            source_dedup_key: command.source_dedup_key,
            executor_config: command.executor_config,
            backend_selection: command.backend_selection,
            identity: command.identity,
            delivery_intent: command.delivery_intent,
        })
        .await
    }

    pub async fn accept_user_message_for_target(
        &self,
        command: AgentRunMailboxUserMessageTargetCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        self.accept_intake_message_for_target(AgentRunMailboxIntakeTargetCommand {
            target: command.target,
            origin: MailboxMessageOrigin::User,
            source: command.source,
            retain_payload: false,
            schedule_on_submit: command.schedule_on_submit,
            input: command.input,
            client_command_id: command.client_command_id,
            source_dedup_key: None,
            executor_config: command.executor_config,
            backend_selection: command.backend_selection,
            identity: command.identity,
            delivery_intent: command.delivery_intent,
        })
        .await
    }

    pub async fn accept_intake_message_for_target(
        &self,
        command: AgentRunMailboxIntakeTargetCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        diag!(Debug, Subsystem::AgentRun,

            run_id = %command.target.address.run_id,
            agent_id = %command.target.address.agent_id,
            frame_id = %command.target.address.frame_id,
            origin = command.origin.as_str(),
            source_namespace = %command.source.namespace,
            source_kind = %command.source.kind,
            runtime_session_id = command
                .target
                .message_stream
                .as_ref()
                .map(|message_stream| message_stream.runtime_session_id.as_str())
                .unwrap_or("<resolved-later>"),
            input_blocks = command.input.len(),
            schedule_on_submit = command.schedule_on_submit,
            "AgentRun mailbox accept intake message entered"
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

        let source_dedup_key = command.stable_source_dedup_key();
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
        let launch_planning_input = self
            .resolve_launch_planning_input(
                run.project_id,
                run.id,
                agent.id,
                agent.project_agent_id,
                &runtime_session_id,
                command.backend_selection.clone(),
            )
            .await?;
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
            "origin": command.origin.as_str(),
            "source": {
                "namespace": command.source.namespace,
                "kind": command.source.kind,
                "source_ref": command.source.source_ref,
                "correlation_ref": command.source.correlation_ref,
                "actor": command.source.actor,
                "route": command.source.route,
                "display_label_key": command.source.display_label_key,
                "metadata": command.source.metadata,
            },
            "input": command.input,
            "retain_payload": command.retain_payload,
            "source_dedup_key": source_dedup_key,
            "executor_config": command.executor_config,
            "launch_planning_input": &launch_planning_input,
            "delivery_intent": command.delivery_intent,
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
        let launch_planning_json = serde_json::to_value(&launch_planning_input)
            .map_err(serialization_error("mailbox launch_planning_input"))?;
        let message = self
            .mailbox_repo
            .create_message_idempotent(NewAgentRunMailboxMessage {
                run_id: run.id,
                agent_id: agent.id,
                runtime_session_id: runtime_session_id.clone(),
                origin: command.origin,
                source: command.source,
                delivery: policy.delivery,
                barrier: policy.barrier,
                drain_mode: policy.drain_mode,
                priority: 0,
                source_dedup_key: source_dedup_key
                    .or_else(|| Some(format!("command_receipt:{}", claim.record.id))),
                queued_agent_run_turn_id: policy.queued_agent_run_turn_id,
                expected_active_agent_run_turn_id: policy.expected_active_agent_run_turn_id,
                command_receipt_id: Some(claim.record.id),
                payload_json: Some(payload_json),
                executor_config_json,
                launch_planning_input: Some(launch_planning_json),
                preview: build_input_preview(&command.input),
                has_images: input_has_images(&command.input),
                retain_payload: command.retain_payload,
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
                launch_planning_input: None,
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
                    launch_planning_input: None,
                    preview: build_input_preview(&input),
                    has_images: input_has_images(&input),
                    retain_payload: true,
                })
                .await?;
            created.push(mailbox_message);
        }
        Ok(created)
    }

    async fn resolve_launch_planning_input(
        &self,
        project_id: Uuid,
        run_id: Uuid,
        agent_id: Uuid,
        project_agent_id: Option<Uuid>,
        runtime_session_id: &str,
        requested_selection: Option<BackendSelectionInput>,
    ) -> Result<LaunchPlanningInput, WorkflowApplicationError> {
        let active_accesses = self
            .project_backend_access_repo
            .list_active_by_project(project_id)
            .await?;
        let authorized_backend_ids = active_accesses
            .into_iter()
            .map(|access| access.backend_id.trim().to_string())
            .filter(|backend_id| !backend_id.is_empty())
            .collect::<Vec<_>>();

        let selection = match requested_selection {
            Some(selection) => {
                self.ensure_backend_selection_authorized(&selection, &authorized_backend_ids)?;
                if selection.mode == BackendSelectionInputMode::Explicit {
                    let preference = serde_json::to_value(&selection)
                        .map_err(serialization_error("backend selection preference"))?;
                    self.mailbox_repo
                        .set_backend_selection_preference(
                            run_id,
                            agent_id,
                            runtime_session_id.to_string(),
                            preference,
                        )
                        .await?;
                }
                Some(selection)
            }
            None => {
                let state = self.mailbox_repo.get_state(run_id, agent_id).await?;
                let preference = state
                    .and_then(|state| state.backend_selection_preference)
                    .map(serde_json::from_value::<BackendSelectionInput>)
                    .transpose()
                    .map_err(|error| {
                        WorkflowApplicationError::BadRequest(format!(
                            "backend selection preference 无效: {error}"
                        ))
                    })?;
                match preference {
                    Some(selection)
                        if self.backend_selection_is_authorized(
                            &selection,
                            &authorized_backend_ids,
                        ) =>
                    {
                        Some(selection)
                    }
                    _ => None,
                }
            }
        };
        let backend_requirement = match project_agent_id {
            Some(project_agent_id) => {
                let Some(project_agent) = self
                    .project_agent_repo
                    .get_by_project_and_id(project_id, project_agent_id)
                    .await?
                else {
                    return Err(WorkflowApplicationError::NotFound(format!(
                        "ProjectAgent {project_agent_id} 不存在"
                    )));
                };
                Some(
                    project_agent
                        .preset_config()
                        .map_err(|error| WorkflowApplicationError::BadRequest(error.to_string()))?
                        .backend_requirement_or_default(),
                )
            }
            None => None,
        };

        Ok(LaunchPlanningInput {
            backend_selection: selection,
            backend_requirement,
            authorized_backend_ids,
        })
    }

    fn ensure_backend_selection_authorized(
        &self,
        selection: &BackendSelectionInput,
        authorized_backend_ids: &[String],
    ) -> Result<(), WorkflowApplicationError> {
        if self.backend_selection_is_authorized(selection, authorized_backend_ids) {
            return Ok(());
        }
        let backend_id = selection.backend_id.as_deref().unwrap_or("<auto>");
        Err(WorkflowApplicationError::BadRequest(format!(
            "backend `{backend_id}` 不在当前 Project 授权范围内"
        )))
    }

    fn backend_selection_is_authorized(
        &self,
        selection: &BackendSelectionInput,
        authorized_backend_ids: &[String],
    ) -> bool {
        match selection.mode {
            BackendSelectionInputMode::AutoIdle => !authorized_backend_ids.is_empty(),
            BackendSelectionInputMode::Explicit | BackendSelectionInputMode::WorkspaceBinding => {
                selection
                    .backend_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|backend_id| !backend_id.is_empty())
                    .is_some_and(|backend_id| {
                        authorized_backend_ids
                            .iter()
                            .any(|authorized| authorized == backend_id)
                    })
            }
        }
    }
}
