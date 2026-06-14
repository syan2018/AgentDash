use agentdash_contracts::workflow::{AgentRunCommandPreconditionView, ConversationCommandKind};
use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use serde_json::Value;
use uuid::Uuid;

use crate::repository_set::RepositorySet;
use crate::session::{SessionCoreService, SessionExecutionState};
use crate::workflow::{
    WorkflowApplicationError, conversation_command_id_for, conversation_snapshot_id,
};

use super::projection::AgentRunWorkspaceProjection;
use super::projection::is_terminal_agent_status;
use super::query::mailbox_message_visible;
use super::types::AgentRunWorkspaceProjectionInput;

pub struct AgentRunWorkspaceCommandPolicyService<'a> {
    repos: &'a RepositorySet,
    session_core: SessionCoreService,
    session_control: crate::session::SessionControlService,
}

impl<'a> AgentRunWorkspaceCommandPolicyService<'a> {
    pub fn new(
        repos: &'a RepositorySet,
        session_core: SessionCoreService,
        session_control: crate::session::SessionControlService,
    ) -> Self {
        Self {
            repos,
            session_core,
            session_control,
        }
    }

    pub async fn ensure_command_allowed(
        &self,
        context: AgentRunWorkspaceCommandPolicyContext<'_>,
        command: AgentRunWorkspaceCommandPrecondition,
    ) -> Result<(), AgentRunWorkspaceCommandPolicyError> {
        let execution_state = self
            .session_core
            .inspect_session_execution_state(context.runtime_session_id)
            .await
            .map_err(WorkflowApplicationError::from)?;
        let frame_ref = self
            .resolve_current_frame_ref(context.run, context.agent)
            .await?;
        let terminal_agent = is_terminal_agent_status(&context.agent.status);
        let projection =
            AgentRunWorkspaceProjection::derive(AgentRunWorkspaceProjectionInput::new(
                &execution_state,
                &context.agent.status,
                true,
                frame_ref.is_some(),
            ));
        let detail = || {
            serde_json::json!({
                "run_id": context.run.id.to_string(),
                "agent_id": context.agent.id.to_string(),
                "runtime_session_id": context.runtime_session_id,
                "state": projection.state_code.as_str(),
                "active_turn_id": snapshot_active_turn_id(&execution_state),
            })
        };
        let expected_kind = command.expected_kind();
        ensure_command_submission_matches_snapshot(
            command.command_precondition(),
            expected_kind,
            context,
            frame_ref,
            &execution_state,
            terminal_agent,
            projection.state_code.as_str(),
        )?;

        if terminal_agent
            && !matches!(
                &command,
                AgentRunWorkspaceCommandPrecondition::Cancel { .. }
                    | AgentRunWorkspaceCommandPrecondition::DeleteMailboxMessage { .. }
            )
        {
            return Err(conflict(
                "当前 AgentRun 已结束，不能执行该命令。",
                "command_unavailable",
                None,
                detail(),
            ));
        }

        match command {
            AgentRunWorkspaceCommandPrecondition::DeleteMailboxMessage { .. } => Ok(()),
            AgentRunWorkspaceCommandPrecondition::PromoteMailboxMessage { .. } => {
                match &execution_state {
                    SessionExecutionState::Running { turn_id: Some(_) } => {}
                    SessionExecutionState::Running { turn_id: None } => {
                        return Err(conflict(
                            "当前 AgentRun 正在启动中，等待 active turn 建立后才能投递 mailbox 消息。",
                            "starting_claimed",
                            None,
                            detail(),
                        ));
                    }
                    _ => {
                        return Err(conflict(
                            "当前 AgentRun 不在可投递 mailbox 消息的运行状态。",
                            "command_unavailable",
                            None,
                            detail(),
                        ));
                    }
                }
                if !self
                    .session_control
                    .supports_session_steering(context.runtime_session_id)
                    .await
                {
                    return Err(conflict(
                        "当前执行器不支持对该 AgentRun 投递 mailbox steer。",
                        "connector_steer_unsupported",
                        None,
                        detail(),
                    ));
                }
                Ok(())
            }
            AgentRunWorkspaceCommandPrecondition::ResumeMailbox { .. } => {
                let messages = self
                    .repos
                    .agent_run_mailbox_repo
                    .list_messages(context.run.id, context.agent.id)
                    .await
                    .map_err(WorkflowApplicationError::from)?;
                let visible_message_count = messages
                    .iter()
                    .filter(|message| mailbox_message_visible(message))
                    .count();
                let mailbox_state = self
                    .repos
                    .agent_run_mailbox_repo
                    .get_state(context.run.id, context.agent.id)
                    .await
                    .map_err(WorkflowApplicationError::from)?;
                if mailbox_state.as_ref().is_some_and(|state| state.paused)
                    && visible_message_count > 0
                {
                    Ok(())
                } else {
                    Err(conflict(
                        "当前没有需要用户恢复的 mailbox。",
                        "command_unavailable",
                        None,
                        serde_json::json!({
                            "run_id": context.run.id.to_string(),
                            "agent_id": context.agent.id.to_string(),
                            "runtime_session_id": context.runtime_session_id,
                            "state": projection.state_code.as_str(),
                            "visible_message_count": visible_message_count,
                            "paused": mailbox_state.as_ref().is_some_and(|state| state.paused),
                        }),
                    ))
                }
            }
            AgentRunWorkspaceCommandPrecondition::Cancel { .. } => match &execution_state {
                SessionExecutionState::Running { .. }
                | SessionExecutionState::Cancelling { .. } => Ok(()),
                _ => Err(conflict(
                    "当前 AgentRun 没有正在执行的 turn。",
                    "command_unavailable",
                    None,
                    detail(),
                )),
            },
        }
    }

    pub async fn ensure_composer_submit_allowed(
        &self,
        context: AgentRunWorkspaceCommandPolicyContext<'_>,
        command: &AgentRunCommandPreconditionView,
    ) -> Result<(), AgentRunWorkspaceCommandPolicyError> {
        let execution_state = self
            .session_core
            .inspect_session_execution_state(context.runtime_session_id)
            .await
            .map_err(WorkflowApplicationError::from)?;
        if is_terminal_agent_status(&context.agent.status) {
            let projection =
                AgentRunWorkspaceProjection::derive(AgentRunWorkspaceProjectionInput::new(
                    &execution_state,
                    &context.agent.status,
                    true,
                    true,
                ));
            return Err(conflict(
                "当前 AgentRun 已结束，不能继续发送消息。",
                "command_unavailable",
                None,
                serde_json::json!({
                    "run_id": context.run.id.to_string(),
                    "agent_id": context.agent.id.to_string(),
                    "runtime_session_id": context.runtime_session_id,
                    "state": projection.state_code.as_str(),
                }),
            ));
        }
        ensure_composer_command_precondition_matches_agent_run(command, context, &execution_state)
    }

    async fn resolve_current_frame_ref(
        &self,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
    ) -> Result<Option<(Uuid, i32)>, AgentRunWorkspaceCommandPolicyError> {
        let anchor_frame_id = self
            .repos
            .execution_anchor_repo
            .list_by_run(run.id)
            .await
            .map_err(WorkflowApplicationError::from)?
            .into_iter()
            .filter(|anchor| anchor.agent_id == agent.id)
            .max_by_key(|anchor| anchor.updated_at)
            .map(|anchor| anchor.launch_frame_id);
        let current_frame = self
            .repos
            .agent_frame_repo
            .get_current(agent.id)
            .await
            .map_err(WorkflowApplicationError::from)?;
        let frame = match (current_frame, anchor_frame_id) {
            (Some(frame), _) => Some(frame),
            (None, Some(frame_id)) => self
                .repos
                .agent_frame_repo
                .get(frame_id)
                .await
                .map_err(WorkflowApplicationError::from)?,
            (None, None) => None,
        };
        Ok(frame.map(|frame: AgentFrame| (frame.id, frame.revision)))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AgentRunWorkspaceCommandPolicyContext<'a> {
    pub run: &'a LifecycleRun,
    pub agent: &'a LifecycleAgent,
    pub runtime_session_id: &'a str,
}

#[derive(Debug, Clone)]
pub enum AgentRunWorkspaceCommandPrecondition {
    DeleteMailboxMessage {
        command: AgentRunCommandPreconditionView,
    },
    PromoteMailboxMessage {
        command: AgentRunCommandPreconditionView,
    },
    ResumeMailbox {
        command: AgentRunCommandPreconditionView,
    },
    Cancel {
        command: AgentRunCommandPreconditionView,
    },
}

impl AgentRunWorkspaceCommandPrecondition {
    fn expected_kind(&self) -> ConversationCommandKind {
        match self {
            AgentRunWorkspaceCommandPrecondition::DeleteMailboxMessage { .. } => {
                ConversationCommandKind::DeleteMailboxMessage
            }
            AgentRunWorkspaceCommandPrecondition::PromoteMailboxMessage { .. } => {
                ConversationCommandKind::PromoteMailboxMessage
            }
            AgentRunWorkspaceCommandPrecondition::ResumeMailbox { .. } => {
                ConversationCommandKind::ResumeMailbox
            }
            AgentRunWorkspaceCommandPrecondition::Cancel { .. } => ConversationCommandKind::Cancel,
        }
    }

    fn command_precondition(&self) -> &AgentRunCommandPreconditionView {
        match self {
            AgentRunWorkspaceCommandPrecondition::DeleteMailboxMessage { command }
            | AgentRunWorkspaceCommandPrecondition::PromoteMailboxMessage { command }
            | AgentRunWorkspaceCommandPrecondition::ResumeMailbox { command }
            | AgentRunWorkspaceCommandPrecondition::Cancel { command } => command,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunWorkspaceCommandConflict {
    pub message: String,
    pub error_code: String,
    pub replacement_command: Option<String>,
    pub detail: Option<Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRunWorkspaceCommandPolicyError {
    #[error("{0}")]
    Application(#[from] WorkflowApplicationError),
    #[error("{0}")]
    Conflict(AgentRunWorkspaceCommandConflict),
}

impl std::fmt::Display for AgentRunWorkspaceCommandConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

fn ensure_command_submission_matches_snapshot(
    command: &AgentRunCommandPreconditionView,
    expected_kind: ConversationCommandKind,
    context: AgentRunWorkspaceCommandPolicyContext<'_>,
    frame_ref: Option<(Uuid, i32)>,
    execution_state: &SessionExecutionState,
    terminal_agent: bool,
    state_code: &str,
) -> Result<(), AgentRunWorkspaceCommandPolicyError> {
    let current_active_turn_id = snapshot_active_turn_id(execution_state);
    let current_frame_id = frame_ref.map(|(frame_id, _)| frame_id.to_string());
    let current_snapshot_id = conversation_snapshot_id(
        context.run.id,
        context.agent.id,
        frame_ref,
        Some(context.runtime_session_id),
        execution_state,
        terminal_agent,
    );
    let stale_detail = |reason: &str| {
        serde_json::json!({
            "reason": reason,
            "run_id": context.run.id.to_string(),
            "agent_id": context.agent.id.to_string(),
            "runtime_session_id": context.runtime_session_id,
            "state": state_code,
            "expected_command_kind": expected_kind,
            "submitted_command_kind": command.command_kind,
            "expected_command_id": conversation_command_id_for(expected_kind),
            "submitted_command_id": command.command_id,
            "expected_snapshot_id": current_snapshot_id,
            "submitted_snapshot_id": command.stale_guard.snapshot_id,
            "expected_frame_id": current_frame_id,
            "submitted_frame_id": command.stale_guard.frame_id,
            "expected_active_turn_id": current_active_turn_id,
            "submitted_active_turn_id": command.stale_guard.active_turn_id,
            "snapshot_refresh_required": true,
        })
    };

    if command.command_kind != expected_kind {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("command_kind_mismatch"),
        ));
    }
    if command.command_id != conversation_command_id_for(expected_kind) {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("command_id_mismatch"),
        ));
    }
    if command.stale_guard.run_id != context.run.id.to_string()
        || command.stale_guard.agent_id != context.agent.id.to_string()
    {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("agent_run_identity_mismatch"),
        ));
    }
    if command.stale_guard.runtime_session_id.as_deref() != Some(context.runtime_session_id) {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("runtime_session_mismatch"),
        ));
    }
    if command.stale_guard.frame_id != current_frame_id {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("frame_mismatch"),
        ));
    }
    if command.stale_guard.active_turn_id != current_active_turn_id {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("active_turn_mismatch"),
        ));
    }
    if command.stale_guard.snapshot_id != current_snapshot_id {
        return Err(stale_command_conflict(
            execution_state,
            terminal_agent,
            stale_detail("snapshot_id_mismatch"),
        ));
    }

    Ok(())
}

fn ensure_composer_command_precondition_matches_agent_run(
    command: &AgentRunCommandPreconditionView,
    context: AgentRunWorkspaceCommandPolicyContext<'_>,
    execution_state: &SessionExecutionState,
) -> Result<(), AgentRunWorkspaceCommandPolicyError> {
    let state_code = AgentRunWorkspaceProjection::derive(AgentRunWorkspaceProjectionInput::new(
        execution_state,
        &context.agent.status,
        true,
        true,
    ))
    .state_code;
    let detail = || {
        serde_json::json!({
            "run_id": context.run.id.to_string(),
            "agent_id": context.agent.id.to_string(),
            "runtime_session_id": context.runtime_session_id,
            "state": state_code.as_str(),
            "submitted_command_kind": command.command_kind,
            "submitted_command_id": command.command_id,
            "submitted_guard": &command.stale_guard,
        })
    };

    if command.stale_guard.run_id != context.run.id.to_string()
        || command.stale_guard.agent_id != context.agent.id.to_string()
    {
        return Err(stale_command_conflict(
            execution_state,
            false,
            serde_json::json!({
                "reason": "agent_run_identity_mismatch",
                "run_id": context.run.id.to_string(),
                "agent_id": context.agent.id.to_string(),
                "runtime_session_id": context.runtime_session_id,
                "state": state_code.as_str(),
                "submitted_run_id": &command.stale_guard.run_id,
                "submitted_agent_id": &command.stale_guard.agent_id,
                "snapshot_refresh_required": true,
            }),
        ));
    }

    if command.command_kind != ConversationCommandKind::SubmitMessage {
        return Err(conflict(
            "当前输入提交只能使用 submit_message 命令意图。",
            "command_unavailable",
            replacement_command_for_state(execution_state, false),
            detail(),
        ));
    }

    Ok(())
}

fn stale_command_conflict(
    execution_state: &SessionExecutionState,
    terminal_agent: bool,
    detail: Value,
) -> AgentRunWorkspaceCommandPolicyError {
    conflict(
        "AgentRun command snapshot 已过期，请使用最新 workspace state 重试。",
        "stale_command",
        replacement_command_for_state(execution_state, terminal_agent),
        detail,
    )
}

fn replacement_command_for_state(
    _execution_state: &SessionExecutionState,
    terminal_agent: bool,
) -> Option<&'static str> {
    if terminal_agent {
        None
    } else {
        Some("submit_message")
    }
}

fn conflict(
    message: impl Into<String>,
    error_code: impl Into<String>,
    replacement_command: Option<&str>,
    detail: Value,
) -> AgentRunWorkspaceCommandPolicyError {
    AgentRunWorkspaceCommandPolicyError::Conflict(AgentRunWorkspaceCommandConflict {
        message: message.into(),
        error_code: error_code.into(),
        replacement_command: replacement_command.map(str::to_string),
        detail: Some(detail),
    })
}

fn snapshot_active_turn_id(execution_state: &SessionExecutionState) -> Option<String> {
    match execution_state {
        SessionExecutionState::Running { turn_id }
        | SessionExecutionState::Cancelling { turn_id }
        | SessionExecutionState::Interrupted { turn_id, .. } => turn_id.clone(),
        SessionExecutionState::Completed { turn_id }
        | SessionExecutionState::Failed { turn_id, .. } => Some(turn_id.clone()),
        SessionExecutionState::Idle => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_contracts::workflow::ConversationCommandStaleGuardView;
    use agentdash_domain::workflow::LifecycleRun;

    fn test_context() -> (LifecycleRun, LifecycleAgent) {
        let run = LifecycleRun::new_graphless(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, "PI_AGENT");
        (run, agent)
    }

    fn policy_context<'a>(
        run: &'a LifecycleRun,
        agent: &'a LifecycleAgent,
    ) -> AgentRunWorkspaceCommandPolicyContext<'a> {
        AgentRunWorkspaceCommandPolicyContext {
            run,
            agent,
            runtime_session_id: "session-1",
        }
    }

    fn command(
        kind: ConversationCommandKind,
        context: AgentRunWorkspaceCommandPolicyContext<'_>,
    ) -> AgentRunCommandPreconditionView {
        AgentRunCommandPreconditionView {
            command_id: conversation_command_id_for(kind).to_string(),
            command_kind: kind,
            stale_guard: ConversationCommandStaleGuardView {
                snapshot_id: "stale-snapshot".to_string(),
                run_id: context.run.id.to_string(),
                agent_id: context.agent.id.to_string(),
                frame_id: Some(Uuid::new_v4().to_string()),
                runtime_session_id: Some("old-session".to_string()),
                active_turn_id: Some("old-turn".to_string()),
            },
        }
    }

    #[test]
    fn composer_submit_accepts_single_submit_message_intent_after_terminal() {
        let completed = SessionExecutionState::Completed {
            turn_id: "turn-1".to_string(),
        };
        let (run, agent) = test_context();
        let context = policy_context(&run, &agent);
        let command = command(ConversationCommandKind::SubmitMessage, context);

        ensure_composer_command_precondition_matches_agent_run(&command, context, &completed)
            .expect("composer input should not require stale frame or turn guard");
    }

    #[test]
    fn composer_submit_rejects_non_text_control_command_intent() {
        let running = SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        };
        let (run, agent) = test_context();
        let context = policy_context(&run, &agent);
        let command = command(ConversationCommandKind::Cancel, context);

        let error =
            ensure_composer_command_precondition_matches_agent_run(&command, context, &running)
                .expect_err("cancel is not a composer input command");

        match error {
            AgentRunWorkspaceCommandPolicyError::Conflict(payload) => {
                assert_eq!(payload.error_code, "command_unavailable");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn composer_submit_accepts_running_submit_message() {
        let running = SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        };
        let (run, agent) = test_context();
        let context = policy_context(&run, &agent);
        let command = command(ConversationCommandKind::SubmitMessage, context);

        ensure_composer_command_precondition_matches_agent_run(&command, context, &running)
            .expect("scheduler owns running submit policy");
    }
}
