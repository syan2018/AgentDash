use super::commands::AgentRunMailboxMoveCommandResult;
use super::*;

impl<'a> AgentRunMailboxService<'a> {
    pub(super) async fn claim_control_receipt(
        &self,
        command: &AgentRunMailboxControlTargetCommand,
        command_kind: AgentRunCommandKind,
        digest_kind: &str,
    ) -> Result<
        crate::agent_run::command_receipt::ClaimedAgentRunCommandReceipt,
        WorkflowApplicationError,
    > {
        if command.client_command_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "client_command_id 不能为空".to_string(),
            ));
        }
        let request_digest = digest_command_request(&serde_json::json!({
            "kind": digest_kind,
            "target": {
                "run_id": command.target.address.run_id,
                "agent_id": command.target.address.agent_id,
                "frame_id": command.target.address.frame_id,
            },
            "message_stream": command.target.message_stream.as_ref().map(|message_stream| {
                serde_json::json!({
                    "runtime_session_id": message_stream.runtime_session_id.clone(),
                    "trace_kind": message_stream.trace_kind,
                })
            }),
            "message_id": command.message_id,
            "after_message_id": command.after_message_id,
        }))?;
        claim_agent_run_command_receipt(
            self.command_receipt_repo,
            "agent_run_mailbox",
            format!(
                "{}:{}",
                command.target.address.run_id, command.target.address.agent_id
            ),
            command_kind,
            command.client_command_id.clone(),
            request_digest,
        )
        .await
    }

    pub(super) async fn accepted_command_receipt(
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

    pub(super) async fn accepted_control_receipt(
        &self,
        receipt_id: Uuid,
        outcome: AgentRunMailboxCommandOutcome,
        message: Option<&AgentRunMailboxMessage>,
        accepted_refs: Option<AgentRunAcceptedRefs>,
    ) -> Result<AgentRunCommandReceiptView, WorkflowApplicationError> {
        let refs = match accepted_refs {
            Some(refs) => refs,
            None => match message {
                Some(message) => match message.delivery_runtime_session_id.as_deref() {
                    Some(runtime_session_id) => {
                        self.base_refs_for_runtime(
                            message.run_id,
                            message.agent_id,
                            runtime_session_id,
                        )
                        .await?
                    }
                    None => AgentRunAcceptedRefs {
                        run_id: message.run_id,
                        agent_id: message.agent_id,
                        frame_id: None,
                        frame_revision: None,
                        runtime_session_id: None,
                        agent_run_turn_id: message.accepted_agent_run_turn_id.clone(),
                        protocol_turn_id: message.accepted_protocol_turn_id.clone(),
                    },
                },
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
            "order_key": message.map(|message| message.order_key),
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

    pub(super) async fn complete_message_receipt(
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

    pub(super) async fn mark_message_receipt_terminal_failed(
        &self,
        message: &AgentRunMailboxMessage,
        error: WorkflowApplicationError,
    ) {
        if let Some(receipt_id) = message.command_receipt_id {
            mark_command_terminal_failed(self.command_receipt_repo, receipt_id, &error).await;
        }
    }

    pub(super) async fn replay_duplicate_command(
        &self,
        record: agentdash_domain::workflow::AgentRunCommandReceipt,
        frame: Option<AgentFrame>,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let mailbox_message = match record.mailbox_message_id {
            Some(message_id) => self.mailbox_repo.get_message(message_id).await?,
            None => None,
        };
        let result_json = record.result_json.as_ref();
        let outcome = result_json
            .and_then(outcome_from_result_json)
            .or_else(|| mailbox_message.as_ref().map(outcome_from_message))
            .unwrap_or(AgentRunMailboxCommandOutcome::Queued);
        let accepted_refs = match record.accepted_refs.clone() {
            Some(refs) => Some(refs),
            None => match (mailbox_message.as_ref(), frame.as_ref()) {
                (Some(message), Some(frame)) => Some(AgentRunAcceptedRefs {
                    run_id: message.run_id,
                    agent_id: message.agent_id,
                    frame_id: Some(frame.id),
                    frame_revision: Some(frame.revision),
                    runtime_session_id: message.delivery_runtime_session_id.clone(),
                    agent_run_turn_id: message.accepted_agent_run_turn_id.clone(),
                    protocol_turn_id: message.accepted_protocol_turn_id.clone(),
                }),
                _ => None,
            },
        };
        let runtime_state = match mailbox_message.as_ref() {
            Some(message) => match message.delivery_runtime_session_id.as_deref() {
                Some(runtime_session_id) => self.inspect_state_optional(runtime_session_id).await,
                None => None,
            },
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

    pub(super) async fn replay_duplicate_move_command(
        &self,
        record: agentdash_domain::workflow::AgentRunCommandReceipt,
    ) -> Result<AgentRunMailboxMoveCommandResult, WorkflowApplicationError> {
        let order_key = record
            .result_json
            .as_ref()
            .and_then(order_key_from_result_json)
            .ok_or_else(|| {
                WorkflowApplicationError::Conflict(
                    "mailbox move command 缺少稳定 order_key".to_string(),
                )
            })?;
        let result = self.replay_duplicate_command(record, None).await?;
        Ok(AgentRunMailboxMoveCommandResult {
            command_receipt: result.command_receipt,
            outcome: result.outcome,
            mailbox_message: result.mailbox_message,
            accepted_refs: result.accepted_refs,
            runtime_state: result.runtime_state,
            order_key,
        })
    }

    pub(super) async fn inspect_state_optional(
        &self,
        runtime_session_id: &str,
    ) -> Option<SessionExecutionState> {
        self.session_core
            .inspect_session_execution_state(runtime_session_id)
            .await
            .ok()
    }
}

pub(crate) fn outcome_from_message(
    message: &AgentRunMailboxMessage,
) -> AgentRunMailboxCommandOutcome {
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

pub(crate) fn outcome_from_result_json(
    value: &serde_json::Value,
) -> Option<AgentRunMailboxCommandOutcome> {
    match value.get("outcome").and_then(serde_json::Value::as_str)? {
        "launched" => Some(AgentRunMailboxCommandOutcome::Launched),
        "queued" => Some(AgentRunMailboxCommandOutcome::Queued),
        "steered" => Some(AgentRunMailboxCommandOutcome::Steered),
        "deleted" => Some(AgentRunMailboxCommandOutcome::Deleted),
        "moved" => Some(AgentRunMailboxCommandOutcome::Moved),
        "resumed" => Some(AgentRunMailboxCommandOutcome::Resumed),
        "blocked" => Some(AgentRunMailboxCommandOutcome::Blocked),
        "failed" => Some(AgentRunMailboxCommandOutcome::Failed),
        _ => None,
    }
}

pub(crate) fn order_key_from_result_json(value: &serde_json::Value) -> Option<i64> {
    value.get("order_key").and_then(serde_json::Value::as_i64)
}
