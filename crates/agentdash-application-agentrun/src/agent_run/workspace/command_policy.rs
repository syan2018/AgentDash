use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::workflow::{AgentFrame, LifecycleAgent, LifecycleRun};
use serde_json::Value;
use uuid::Uuid;

use crate::agent_run::runtime_session_boundary::{SessionCoreService, SessionExecutionState};
use crate::agent_run::{
    AgentFrameSurfaceExt, AgentRunCommandPreconditionModel, ConversationCommandAvailability,
    ConversationCommandAvailabilityInput, ConversationCommandAvailabilityResolver,
    ConversationCommandKindModel, ConversationCommandModel, ConversationExecutionStatusModel,
    ConversationModelConfigInput, ConversationModelConfigResolver,
    ConversationModelConfigStatusModel, DeliveryRuntimeSelection, DeliveryRuntimeSelectionError,
    DeliveryRuntimeSelectionService, conversation_command_id_for,
};
use crate::agent_run_repository_set::RepositorySet;
use crate::error::WorkflowApplicationError;

use super::projection::is_terminal_agent_status;
use super::query::mailbox_message_visible;

pub struct AgentRunWorkspaceCommandPolicyService<'a> {
    repos: &'a RepositorySet,
    session_core: SessionCoreService,
    session_control: crate::agent_run::runtime_session_boundary::SessionControlService,
}

impl<'a> AgentRunWorkspaceCommandPolicyService<'a> {
    pub fn new(
        repos: &'a RepositorySet,
        session_core: SessionCoreService,
        session_control: crate::agent_run::runtime_session_boundary::SessionControlService,
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
        let current_delivery = self.resolve_current_delivery(context).await?;
        ensure_context_targets_current_delivery(context, current_delivery.as_ref())?;
        let runtime_session_id = current_delivery
            .as_ref()
            .map(|selection| selection.runtime_session_id.as_str())
            .unwrap_or(context.runtime_session_id);
        let execution_state = self
            .session_core
            .inspect_session_execution_state(runtime_session_id)
            .await?;
        let frame = self
            .resolve_current_frame(context.agent, current_delivery.as_ref())
            .await?;
        let frame_ref = frame.as_ref().map(|frame| (frame.id, frame.revision));
        let terminal_agent = is_terminal_agent_status(&context.agent.status);
        let availability = self
            .resolve_command_availability(
                context,
                frame.as_ref(),
                frame_ref,
                execution_state.clone(),
                terminal_agent,
            )
            .await?;
        let detail = || {
            serde_json::json!({
                "run_id": context.run.id.to_string(),
                "agent_id": context.agent.id.to_string(),
                "runtime_session_id": runtime_session_id,
                "state": availability.execution_status,
                "active_turn_id": &availability.active_turn_id,
            })
        };
        let expected_kind = command.expected_kind();
        ensure_command_submission_matches_availability(
            command.command_precondition(),
            expected_kind,
            context,
            &availability,
        )?;

        match command {
            AgentRunWorkspaceCommandPrecondition::DeleteMailboxMessage { .. }
            | AgentRunWorkspaceCommandPrecondition::PromoteMailboxMessage { .. }
            | AgentRunWorkspaceCommandPrecondition::ResumeMailbox { .. }
            | AgentRunWorkspaceCommandPrecondition::Cancel { .. } => {
                ensure_availability_command_enabled(&availability, expected_kind, detail)
            }
        }
    }

    pub async fn ensure_composer_submit_allowed(
        &self,
        context: AgentRunWorkspaceCommandPolicyContext<'_>,
        command: &AgentRunCommandPreconditionModel,
    ) -> Result<(), AgentRunWorkspaceCommandPolicyError> {
        diag!(Debug, Subsystem::AgentRun,

            run_id = %context.run.id,
            agent_id = %context.agent.id,
            runtime_session_id = %context.runtime_session_id,
            "AgentRun composer policy inspect state"
        );
        let current_delivery = self.resolve_current_delivery(context).await?;
        ensure_context_targets_current_delivery(context, current_delivery.as_ref())?;
        let runtime_session_id = current_delivery
            .as_ref()
            .map(|selection| selection.runtime_session_id.as_str())
            .unwrap_or(context.runtime_session_id);
        let execution_state = self
            .session_core
            .inspect_session_execution_state(runtime_session_id)
            .await?;
        diag!(Debug, Subsystem::AgentRun,

            run_id = %context.run.id,
            agent_id = %context.agent.id,
            runtime_session_id = %runtime_session_id,
            execution_state = ?execution_state,
            "AgentRun composer policy state resolved"
        );
        let frame = self
            .resolve_current_frame(context.agent, current_delivery.as_ref())
            .await?;
        let frame_ref = frame.as_ref().map(|frame| (frame.id, frame.revision));
        let terminal_agent = is_terminal_agent_status(&context.agent.status);
        let availability = self
            .resolve_command_availability(
                context,
                frame.as_ref(),
                frame_ref,
                execution_state,
                terminal_agent,
            )
            .await?;
        let result = ensure_composer_command_precondition_matches_availability(
            command,
            context,
            &availability,
        );
        diag!(Debug, Subsystem::AgentRun,

            run_id = %context.run.id,
            agent_id = %context.agent.id,
            runtime_session_id = %context.runtime_session_id,
            accepted = result.is_ok(),
            "AgentRun composer policy precondition checked"
        );
        result
    }

    async fn resolve_command_availability(
        &self,
        context: AgentRunWorkspaceCommandPolicyContext<'_>,
        frame: Option<&AgentFrame>,
        frame_ref: Option<(Uuid, i32)>,
        execution_state: SessionExecutionState,
        terminal_agent: bool,
    ) -> Result<ConversationCommandAvailability, AgentRunWorkspaceCommandPolicyError> {
        let supports_steering = match &execution_state {
            SessionExecutionState::Running { turn_id: Some(_) } => {
                self.session_control
                    .supports_session_steering(context.runtime_session_id)
                    .await
            }
            _ => false,
        };
        let messages = self
            .repos
            .agent_run_mailbox_repo
            .list_messages(context.run.id, context.agent.id)
            .await
            .map_err(WorkflowApplicationError::from)?;
        let mailbox_visible_message_count = messages
            .iter()
            .filter(|message| mailbox_message_visible(message))
            .count();
        let mailbox_state = self
            .repos
            .agent_run_mailbox_repo
            .get_state(context.run.id, context.agent.id)
            .await
            .map_err(WorkflowApplicationError::from)?;
        let mailbox_paused = mailbox_state.as_ref().is_some_and(|state| state.paused)
            && mailbox_visible_message_count > 0;
        let model_config_status = self.resolve_model_config_status(context, frame).await?;

        Ok(ConversationCommandAvailabilityResolver::resolve(
            ConversationCommandAvailabilityInput {
                run_id: context.run.id,
                agent_id: context.agent.id,
                frame_ref,
                delivery_runtime_session_id: Some(context.runtime_session_id.to_string()),
                execution_state,
                terminal_agent,
                supports_steering,
                mailbox_paused,
                mailbox_visible_message_count,
                model_config_status,
                ownership: crate::agent_run::AgentRunOwnershipModel::from_owner_fields(
                    context.run.created_by_user_id.clone(),
                    context.agent.created_by_user_id.clone(),
                    None,
                ),
            },
        ))
    }

    async fn resolve_current_delivery(
        &self,
        context: AgentRunWorkspaceCommandPolicyContext<'_>,
    ) -> Result<Option<DeliveryRuntimeSelection>, AgentRunWorkspaceCommandPolicyError> {
        if context.agent.current_delivery.is_none() {
            return Ok(None);
        }
        DeliveryRuntimeSelectionService::from_repository_set(self.repos)
            .select_current_delivery(context.run.id, context.agent.id)
            .await
            .map(Some)
            .map_err(workflow_error_from_selection_error)
            .map_err(AgentRunWorkspaceCommandPolicyError::from)
    }

    async fn resolve_current_frame(
        &self,
        agent: &LifecycleAgent,
        current_delivery: Option<&DeliveryRuntimeSelection>,
    ) -> Result<Option<AgentFrame>, AgentRunWorkspaceCommandPolicyError> {
        if let Some(selection) = current_delivery {
            return self
                .repos
                .agent_frame_repo
                .get(selection.current_frame_id)
                .await
                .map_err(WorkflowApplicationError::from)
                .map_err(AgentRunWorkspaceCommandPolicyError::from);
        }
        self.repos
            .agent_frame_repo
            .get_current(agent.id)
            .await
            .map_err(WorkflowApplicationError::from)
            .map_err(AgentRunWorkspaceCommandPolicyError::from)
    }

    async fn resolve_model_config_status(
        &self,
        context: AgentRunWorkspaceCommandPolicyContext<'_>,
        frame: Option<&AgentFrame>,
    ) -> Result<ConversationModelConfigStatusModel, AgentRunWorkspaceCommandPolicyError> {
        let project_agent_preset_config =
            if let Some(project_agent_id) = context.agent.project_agent_id {
                let project_agent = self
                    .repos
                    .project_agent_repo
                    .get_by_project_and_id(context.run.project_id, project_agent_id)
                    .await
                    .map_err(WorkflowApplicationError::from)?
                    .ok_or_else(|| {
                        WorkflowApplicationError::NotFound(format!(
                            "ProjectAgent {project_agent_id} not found"
                        ))
                    })?;
                Some(
                    project_agent
                        .preset_config()
                        .map(|preset| preset.to_agent_config(&project_agent.agent_type))
                        .map_err(WorkflowApplicationError::from)?,
                )
            } else {
                None
            };
        let frame_execution_profile = frame.and_then(|frame| frame.typed_execution_profile());
        let model_config = ConversationModelConfigResolver::resolve(ConversationModelConfigInput {
            project_agent_preset: project_agent_preset_config.as_ref(),
            frame_execution_profile: frame_execution_profile.as_ref(),
            ..Default::default()
        })
        .view;
        Ok(model_config.status)
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
        command: AgentRunCommandPreconditionModel,
    },
    PromoteMailboxMessage {
        command: AgentRunCommandPreconditionModel,
    },
    ResumeMailbox {
        command: AgentRunCommandPreconditionModel,
    },
    Cancel {
        command: AgentRunCommandPreconditionModel,
    },
}

impl AgentRunWorkspaceCommandPrecondition {
    fn expected_kind(&self) -> ConversationCommandKindModel {
        match self {
            AgentRunWorkspaceCommandPrecondition::DeleteMailboxMessage { .. } => {
                ConversationCommandKindModel::DeleteMailboxMessage
            }
            AgentRunWorkspaceCommandPrecondition::PromoteMailboxMessage { .. } => {
                ConversationCommandKindModel::PromoteMailboxMessage
            }
            AgentRunWorkspaceCommandPrecondition::ResumeMailbox { .. } => {
                ConversationCommandKindModel::ResumeMailbox
            }
            AgentRunWorkspaceCommandPrecondition::Cancel { .. } => {
                ConversationCommandKindModel::Cancel
            }
        }
    }

    fn command_precondition(&self) -> &AgentRunCommandPreconditionModel {
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
    Conflict(Box<AgentRunWorkspaceCommandConflict>),
}

impl std::fmt::Display for AgentRunWorkspaceCommandConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

fn ensure_command_submission_matches_availability(
    command: &AgentRunCommandPreconditionModel,
    expected_kind: ConversationCommandKindModel,
    context: AgentRunWorkspaceCommandPolicyContext<'_>,
    availability: &ConversationCommandAvailability,
) -> Result<(), AgentRunWorkspaceCommandPolicyError> {
    let current_active_turn_id = availability.active_turn_id.clone();
    let current_frame_id = availability.frame_id.clone();
    let current_runtime_session_id = availability.runtime_session_id.as_deref();
    let stale_detail = |reason: &str| {
        serde_json::json!({
            "reason": reason,
            "run_id": context.run.id.to_string(),
            "agent_id": context.agent.id.to_string(),
            "runtime_session_id": context.runtime_session_id,
            "state": availability.execution_status,
            "expected_command_kind": expected_kind,
            "submitted_command_kind": command.command_kind,
            "expected_command_id": conversation_command_id_for(expected_kind),
            "submitted_command_id": command.command_id,
            "expected_snapshot_id": availability.snapshot_id,
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
            &availability_execution_state(availability),
            is_terminal_availability(availability),
            stale_detail("command_kind_mismatch"),
        ));
    }
    if command.command_id != conversation_command_id_for(expected_kind) {
        return Err(stale_command_conflict(
            &availability_execution_state(availability),
            is_terminal_availability(availability),
            stale_detail("command_id_mismatch"),
        ));
    }
    if command.stale_guard.run_id != context.run.id.to_string()
        || command.stale_guard.agent_id != context.agent.id.to_string()
    {
        return Err(stale_command_conflict(
            &availability_execution_state(availability),
            is_terminal_availability(availability),
            stale_detail("agent_run_identity_mismatch"),
        ));
    }
    if command.stale_guard.runtime_session_id.as_deref() != current_runtime_session_id {
        return Err(stale_command_conflict(
            &availability_execution_state(availability),
            is_terminal_availability(availability),
            stale_detail("runtime_session_mismatch"),
        ));
    }
    if command.stale_guard.frame_id != current_frame_id {
        return Err(stale_command_conflict(
            &availability_execution_state(availability),
            is_terminal_availability(availability),
            stale_detail("frame_mismatch"),
        ));
    }
    if command.stale_guard.active_turn_id != current_active_turn_id {
        return Err(stale_command_conflict(
            &availability_execution_state(availability),
            is_terminal_availability(availability),
            stale_detail("active_turn_mismatch"),
        ));
    }
    if command.stale_guard.snapshot_id != availability.snapshot_id {
        return Err(stale_command_conflict(
            &availability_execution_state(availability),
            is_terminal_availability(availability),
            stale_detail("snapshot_id_mismatch"),
        ));
    }

    Ok(())
}

fn ensure_context_targets_current_delivery(
    context: AgentRunWorkspaceCommandPolicyContext<'_>,
    current_delivery: Option<&DeliveryRuntimeSelection>,
) -> Result<(), AgentRunWorkspaceCommandPolicyError> {
    let Some(selection) = current_delivery else {
        return Ok(());
    };
    if selection.runtime_session_id == context.runtime_session_id {
        return Ok(());
    }
    Err(stale_command_conflict(
        &SessionExecutionState::Idle,
        false,
        serde_json::json!({
            "reason": "runtime_session_mismatch",
            "run_id": context.run.id.to_string(),
            "agent_id": context.agent.id.to_string(),
            "expected_runtime_session_id": selection.runtime_session_id,
            "submitted_runtime_session_id": context.runtime_session_id,
            "snapshot_refresh_required": true,
        }),
    ))
}

fn workflow_error_from_selection_error(
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

fn ensure_composer_command_precondition_matches_availability(
    command: &AgentRunCommandPreconditionModel,
    context: AgentRunWorkspaceCommandPolicyContext<'_>,
    availability: &ConversationCommandAvailability,
) -> Result<(), AgentRunWorkspaceCommandPolicyError> {
    let detail = || {
        serde_json::json!({
            "run_id": context.run.id.to_string(),
            "agent_id": context.agent.id.to_string(),
            "runtime_session_id": context.runtime_session_id,
            "state": availability.execution_status,
            "submitted_command_kind": command.command_kind,
            "submitted_command_id": command.command_id,
            "expected_command_id": conversation_command_id_for(ConversationCommandKindModel::SubmitMessage),
            "submitted_guard": &command.stale_guard,
        })
    };

    ensure_command_submission_matches_availability(
        command,
        ConversationCommandKindModel::SubmitMessage,
        context,
        availability,
    )?;

    ensure_availability_command_enabled(
        availability,
        ConversationCommandKindModel::SubmitMessage,
        detail,
    )
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
    AgentRunWorkspaceCommandPolicyError::Conflict(Box::new(AgentRunWorkspaceCommandConflict {
        message: message.into(),
        error_code: error_code.into(),
        replacement_command: replacement_command.map(str::to_string),
        detail: Some(detail),
    }))
}

fn ensure_availability_command_enabled(
    availability: &ConversationCommandAvailability,
    kind: ConversationCommandKindModel,
    detail: impl Fn() -> Value,
) -> Result<(), AgentRunWorkspaceCommandPolicyError> {
    let Some(command) = availability_command(availability, kind) else {
        return Err(conflict(
            "当前 AgentRun command snapshot 缺少该命令。",
            "command_unavailable",
            None,
            detail(),
        ));
    };
    if command.enabled {
        return Ok(());
    }
    Err(conflict(
        command
            .unavailable_reason
            .clone()
            .unwrap_or_else(|| "当前 AgentRun 不可执行该命令。".to_string()),
        command
            .disabled_code
            .clone()
            .unwrap_or_else(|| "command_unavailable".to_string()),
        replacement_command_for_availability(availability),
        detail(),
    ))
}

fn availability_command(
    availability: &ConversationCommandAvailability,
    kind: ConversationCommandKindModel,
) -> Option<&ConversationCommandModel> {
    availability
        .commands
        .commands
        .iter()
        .find(|command| command.kind == kind)
}

fn replacement_command_for_availability(
    availability: &ConversationCommandAvailability,
) -> Option<&'static str> {
    if is_terminal_availability(availability) {
        None
    } else {
        Some("submit_message")
    }
}

fn is_terminal_availability(availability: &ConversationCommandAvailability) -> bool {
    availability.execution_status == ConversationExecutionStatusModel::Terminal
}

fn availability_execution_state(
    availability: &ConversationCommandAvailability,
) -> SessionExecutionState {
    match availability.execution_status {
        ConversationExecutionStatusModel::StartingClaimed => {
            SessionExecutionState::Running { turn_id: None }
        }
        ConversationExecutionStatusModel::RunningActive => SessionExecutionState::Running {
            turn_id: availability.active_turn_id.clone(),
        },
        ConversationExecutionStatusModel::Cancelling => SessionExecutionState::Cancelling {
            turn_id: availability.active_turn_id.clone(),
        },
        _ => SessionExecutionState::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{AgentSource, LifecycleRun};

    fn test_context() -> (LifecycleRun, LifecycleAgent) {
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
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

    fn availability(
        context: AgentRunWorkspaceCommandPolicyContext<'_>,
        execution_state: SessionExecutionState,
    ) -> ConversationCommandAvailability {
        availability_with(context, execution_state, false)
    }

    fn availability_with(
        context: AgentRunWorkspaceCommandPolicyContext<'_>,
        execution_state: SessionExecutionState,
        terminal_agent: bool,
    ) -> ConversationCommandAvailability {
        ConversationCommandAvailabilityResolver::resolve(ConversationCommandAvailabilityInput {
            run_id: context.run.id,
            agent_id: context.agent.id,
            frame_ref: Some((Uuid::new_v4(), 1)),
            delivery_runtime_session_id: Some(context.runtime_session_id.to_string()),
            execution_state,
            terminal_agent,
            supports_steering: true,
            mailbox_paused: false,
            mailbox_visible_message_count: 0,
            model_config_status: ConversationModelConfigStatusModel::Resolved,
            ownership: crate::agent_run::AgentRunOwnershipModel::from_owner_fields(
                context.run.created_by_user_id.clone(),
                context.agent.created_by_user_id.clone(),
                Some(context.run.created_by_user_id.as_str()),
            ),
        })
    }

    fn command_from_availability(
        availability: &ConversationCommandAvailability,
        kind: ConversationCommandKindModel,
    ) -> AgentRunCommandPreconditionModel {
        let command = availability
            .commands
            .commands
            .iter()
            .find(|command| command.kind == kind)
            .expect("resolver should expose requested command");
        AgentRunCommandPreconditionModel {
            command_id: command.command_id.clone(),
            command_kind: command.kind,
            stale_guard: command.stale_guard.clone(),
        }
    }

    fn assert_conflict_code(
        error: AgentRunWorkspaceCommandPolicyError,
        expected_code: &str,
    ) -> AgentRunWorkspaceCommandConflict {
        match error {
            AgentRunWorkspaceCommandPolicyError::Conflict(payload) => {
                assert_eq!(payload.error_code, expected_code);
                *payload
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn composer_submit_accepts_fresh_submit_message_intent_after_completed_turn() {
        let completed = SessionExecutionState::Completed {
            turn_id: "turn-1".to_string(),
        };
        let (run, agent) = test_context();
        let context = policy_context(&run, &agent);
        let availability = availability(context, completed);
        let command =
            command_from_availability(&availability, ConversationCommandKindModel::SubmitMessage);

        ensure_composer_command_precondition_matches_availability(&command, context, &availability)
            .expect("fresh submit message command should be accepted");
    }

    #[test]
    fn composer_submit_rejects_non_text_control_command_intent() {
        let running = SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        };
        let (run, agent) = test_context();
        let context = policy_context(&run, &agent);
        let availability = availability(context, running);
        let command =
            command_from_availability(&availability, ConversationCommandKindModel::Cancel);

        let error = ensure_composer_command_precondition_matches_availability(
            &command,
            context,
            &availability,
        )
        .expect_err("cancel is not a composer input command");

        let payload = assert_conflict_code(error, "stale_command");
        assert_eq!(
            payload.detail.expect("stale detail")["reason"],
            "command_kind_mismatch"
        );
    }

    #[test]
    fn composer_submit_accepts_running_submit_message() {
        let running = SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        };
        let (run, agent) = test_context();
        let context = policy_context(&run, &agent);
        let availability = availability(context, running);
        let command =
            command_from_availability(&availability, ConversationCommandKindModel::SubmitMessage);

        ensure_composer_command_precondition_matches_availability(&command, context, &availability)
            .expect("scheduler owns running submit policy");
    }

    #[test]
    fn composer_submit_rejects_stale_guard_mismatch() {
        let running = SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        };
        let (run, agent) = test_context();
        let context = policy_context(&run, &agent);
        let availability = availability(context, running);
        let mut command =
            command_from_availability(&availability, ConversationCommandKindModel::SubmitMessage);
        command.stale_guard.active_turn_id = Some("old-turn".to_string());

        let error = ensure_composer_command_precondition_matches_availability(
            &command,
            context,
            &availability,
        )
        .expect_err("composer submit should reject stale active turn guard");

        let payload = assert_conflict_code(error, "stale_command");
        assert_eq!(
            payload.detail.expect("stale detail")["reason"],
            "active_turn_mismatch"
        );
    }

    #[test]
    fn command_policy_rejects_stale_guard_mismatch() {
        let running = SessionExecutionState::Running {
            turn_id: Some("turn-1".to_string()),
        };
        let (run, agent) = test_context();
        let context = policy_context(&run, &agent);
        let current = availability(context, running);
        let mut command = command_from_availability(&current, ConversationCommandKindModel::Cancel);
        command.stale_guard.snapshot_id = "old-snapshot".to_string();

        let error = ensure_command_submission_matches_availability(
            &command,
            ConversationCommandKindModel::Cancel,
            context,
            &current,
        )
        .expect_err("stale guard should reject an old active turn");

        let payload = assert_conflict_code(error, "stale_command");
        let detail = payload.detail.expect("stale detail");
        assert_eq!(detail["reason"], "snapshot_id_mismatch");
        assert_eq!(detail["snapshot_refresh_required"], true);
    }

    #[test]
    fn command_policy_rejects_disabled_resolver_command() {
        let (run, agent) = test_context();
        let context = policy_context(&run, &agent);
        let availability = availability(context, SessionExecutionState::Idle);
        let command =
            command_from_availability(&availability, ConversationCommandKindModel::Cancel);

        ensure_command_submission_matches_availability(
            &command,
            ConversationCommandKindModel::Cancel,
            context,
            &availability,
        )
        .expect("fresh disabled command still has a matching stale guard");

        let error = ensure_availability_command_enabled(
            &availability,
            ConversationCommandKindModel::Cancel,
            || serde_json::json!({ "state": availability.execution_status }),
        )
        .expect_err("resolver-disabled cancel should be rejected");

        assert_conflict_code(error, "command_unavailable");
    }

    #[test]
    fn composer_submit_rejects_terminal_availability() {
        let completed = SessionExecutionState::Completed {
            turn_id: "turn-1".to_string(),
        };
        let (run, mut agent) = test_context();
        agent.status = "completed".to_string();
        let context = policy_context(&run, &agent);
        let availability = availability_with(context, completed, true);
        let command =
            command_from_availability(&availability, ConversationCommandKindModel::SubmitMessage);

        let error = ensure_composer_command_precondition_matches_availability(
            &command,
            context,
            &availability,
        )
        .expect_err("terminal AgentRun should reject submit_message");

        let payload = assert_conflict_code(error, "terminal");
        assert_eq!(payload.replacement_command, None);
    }
}
