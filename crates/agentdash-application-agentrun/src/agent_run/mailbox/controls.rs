use super::target::{
    base_refs, ensure_command_target, ensure_message_owner, is_terminal_message_status,
};
use super::*;

impl<'a> AgentRunMailboxService<'a> {
    pub async fn delete_message(
        &self,
        command: AgentRunMailboxControlCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let (run, agent, frame) = self
            .resolve_control_plane_for_delivery(&command.runtime_session_id)
            .await?;
        ensure_command_target(&run, &agent, command.run_id, command.agent_id)?;
        self.delete_message_for_target(AgentRunMailboxControlTargetCommand {
            target: AgentRunMailboxCommandTarget::from_runtime_session_adapter(
                run.id,
                agent.id,
                frame.id,
                command.runtime_session_id,
            ),
            message_id: command.message_id,
            client_command_id: command.client_command_id,
        })
        .await
    }

    pub async fn delete_message_for_target(
        &self,
        command: AgentRunMailboxControlTargetCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let message_id = command.message_id.ok_or_else(|| {
            WorkflowApplicationError::BadRequest("message_id 不能为空".to_string())
        })?;
        let target = self.resolve_command_target(command.target.clone()).await?;
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
        ensure_message_owner(&message, target.run.id, target.agent.id)?;
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
        let refs = Some(base_refs(
            &target.run,
            &target.agent,
            Some(&target.frame),
            &target.message_stream.runtime_session_id,
        ));
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
                .inspect_state_optional(&target.message_stream.runtime_session_id)
                .await,
        })
    }

    pub async fn promote_message(
        &self,
        command: AgentRunMailboxControlCommand,
        _identity: Option<AuthIdentity>,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let (run, agent, frame) = self
            .resolve_control_plane_for_delivery(&command.runtime_session_id)
            .await?;
        ensure_command_target(&run, &agent, command.run_id, command.agent_id)?;
        self.promote_message_for_target(
            AgentRunMailboxControlTargetCommand {
                target: AgentRunMailboxCommandTarget::from_runtime_session_adapter(
                    run.id,
                    agent.id,
                    frame.id,
                    command.runtime_session_id,
                ),
                message_id: command.message_id,
                client_command_id: command.client_command_id,
            },
            _identity,
        )
        .await
    }

    pub async fn promote_message_for_target(
        &self,
        command: AgentRunMailboxControlTargetCommand,
        _identity: Option<AuthIdentity>,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let message_id = command.message_id.ok_or_else(|| {
            WorkflowApplicationError::BadRequest("message_id 不能为空".to_string())
        })?;
        let target = self.resolve_command_target(command.target.clone()).await?;
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
        ensure_message_owner(&message, target.run.id, target.agent.id)?;
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
        let accepted_refs = Some(base_refs(
            &target.run,
            &target.agent,
            Some(&target.frame),
            &target.message_stream.runtime_session_id,
        ));
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
                .inspect_state_optional(&target.message_stream.runtime_session_id)
                .await,
        })
    }

    pub async fn resume_mailbox(
        &self,
        command: AgentRunMailboxControlCommand,
        identity: Option<AuthIdentity>,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let (run, agent, frame) = self
            .resolve_control_plane_for_delivery(&command.runtime_session_id)
            .await?;
        ensure_command_target(&run, &agent, command.run_id, command.agent_id)?;
        self.resume_mailbox_for_target(
            AgentRunMailboxControlTargetCommand {
                target: AgentRunMailboxCommandTarget::from_runtime_session_adapter(
                    run.id,
                    agent.id,
                    frame.id,
                    command.runtime_session_id,
                ),
                message_id: command.message_id,
                client_command_id: command.client_command_id,
            },
            identity,
        )
        .await
    }

    pub async fn resume_mailbox_for_target(
        &self,
        command: AgentRunMailboxControlTargetCommand,
        identity: Option<AuthIdentity>,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let target = self.resolve_command_target(command.target.clone()).await?;
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
                target.run.id,
                target.agent.id,
                target.message_stream.runtime_session_id.clone(),
            )
            .await?;
        let outcomes = self
            .schedule_for_target(
                AgentRunMailboxCommandTarget::from_runtime_session_adapter(
                    target.run.id,
                    target.agent.id,
                    target.frame.id,
                    target.message_stream.runtime_session_id.clone(),
                ),
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
            None => self.base_refs_for_target(&target).await.ok(),
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
                .inspect_state_optional(&target.message_stream.runtime_session_id)
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
}
