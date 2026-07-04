use super::*;

pub(super) struct ResolvedAgentRunMailboxCommandTarget {
    pub(super) run: LifecycleRun,
    pub(super) agent: LifecycleAgent,
    pub(super) frame: AgentFrame,
    pub(super) message_stream: MessageStreamProjectionRef,
}

impl<'a> AgentRunMailboxService<'a> {
    pub(super) async fn resolve_command_target(
        &self,
        target: AgentRunMailboxCommandTarget,
    ) -> Result<ResolvedAgentRunMailboxCommandTarget, WorkflowApplicationError> {
        let resolved = self
            .resolve_current_delivery_target(target.address.run_id, target.address.agent_id)
            .await?;
        if target.address.frame_id != resolved.frame.id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "mailbox command target frame {} 不匹配当前 delivery frame {}",
                target.address.frame_id, resolved.frame.id
            )));
        }
        if let Some(message_stream) = target.message_stream
            && message_stream != resolved.message_stream
        {
            return Err(WorkflowApplicationError::Conflict(format!(
                "mailbox command target runtime_session {} 不匹配当前 delivery runtime_session {}",
                message_stream.runtime_session_id, resolved.message_stream.runtime_session_id
            )));
        }
        Ok(resolved)
    }

    pub(super) async fn resolve_current_delivery_target(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<ResolvedAgentRunMailboxCommandTarget, WorkflowApplicationError> {
        let selection =
            DeliveryRuntimeSelectionService::new(DeliveryRuntimeSelectionRepositories {
                lifecycle_runs: self.lifecycle_run_repo,
                lifecycle_agents: self.lifecycle_agent_repo,
                agent_frames: self.agent_frame_repo,
                execution_anchors: self.execution_anchor_repo,
                delivery_bindings: self.delivery_binding_repo,
            })
            .select_current_delivery(run_id, agent_id)
            .await
            .map_err(workflow_error_from_delivery_selection_error)?;
        let run = self
            .lifecycle_run_repo
            .get_by_id(selection.run_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_run 不存在: {}",
                    selection.run_id
                ))
            })?;
        let agent = self
            .lifecycle_agent_repo
            .get(selection.agent_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_agent 不存在: {}",
                    selection.agent_id
                ))
            })?;
        if is_terminal_agent_status(&agent.status) {
            return Err(WorkflowApplicationError::Conflict(
                "当前 Agent 已结束，不能继续发送消息".to_string(),
            ));
        }
        let frame = self
            .agent_frame_repo
            .get(selection.current_frame_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "agent_frame 不存在: {}",
                    selection.current_frame_id
                ))
            })?;
        if frame.agent_id != agent.id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "AgentFrame {} 不属于 LifecycleAgent {}",
                frame.id, agent.id
            )));
        }
        Ok(ResolvedAgentRunMailboxCommandTarget {
            run,
            agent,
            frame,
            message_stream: selection.message_stream,
        })
    }

    pub(super) async fn resolve_control_plane_for_delivery(
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
        let resolved = self
            .resolve_current_delivery_target(anchor.run_id, anchor.agent_id)
            .await?;
        if resolved.message_stream.runtime_session_id != runtime_session_id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "runtime_session {} 不匹配当前 delivery runtime_session {}",
                runtime_session_id, resolved.message_stream.runtime_session_id
            )));
        }
        Ok((resolved.run, resolved.agent, resolved.frame))
    }

    pub(super) async fn base_refs_for_runtime(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
    ) -> Result<AgentRunAcceptedRefs, WorkflowApplicationError> {
        let (run, agent, frame) = self
            .resolve_control_plane_for_delivery(runtime_session_id)
            .await?;
        ensure_command_target(&run, &agent, run_id, agent_id)?;
        Ok(base_refs(&run, &agent, Some(&frame), runtime_session_id))
    }

    pub(super) async fn base_refs_for_target(
        &self,
        target: &ResolvedAgentRunMailboxCommandTarget,
    ) -> Result<AgentRunAcceptedRefs, WorkflowApplicationError> {
        Ok(base_refs(
            &target.run,
            &target.agent,
            Some(&target.frame),
            &target.message_stream.runtime_session_id,
        ))
    }
}

pub(super) fn base_refs(
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

pub(super) fn ensure_command_target(
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

pub(super) fn ensure_message_owner(
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

fn workflow_error_from_delivery_selection_error(
    error: DeliveryRuntimeSelectionError,
) -> WorkflowApplicationError {
    match error {
        DeliveryRuntimeSelectionError::RunNotFound { .. }
        | DeliveryRuntimeSelectionError::AgentNotFound { .. }
        | DeliveryRuntimeSelectionError::CurrentFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::LaunchFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::SubjectNotFound { .. } => {
            WorkflowApplicationError::NotFound(error.to_string())
        }
        DeliveryRuntimeSelectionError::Repository(source) => WorkflowApplicationError::from(source),
        other => WorkflowApplicationError::Conflict(other.to_string()),
    }
}

fn is_terminal_agent_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

pub(super) fn is_terminal_message_status(status: &MailboxMessageStatus) -> bool {
    matches!(
        status,
        MailboxMessageStatus::Dispatched
            | MailboxMessageStatus::Steered
            | MailboxMessageStatus::Deleted
    )
}
