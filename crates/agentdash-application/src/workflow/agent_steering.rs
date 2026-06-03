use agentdash_agent_protocol::codex_app_server_protocol as codex;
use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgentRepository, LifecycleRunRepository,
    RuntimeSessionExecutionAnchorRepository,
};

use crate::session::{
    SessionControlService, SessionCoreService, SessionEventingService, SessionExecutionState,
    SessionTurnSteerCommand,
};
use crate::workflow::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct LifecycleAgentSteeringCommand {
    pub delivery_runtime_session_id: String,
    pub input: Vec<codex::UserInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleAgentSteeringDispatch {
    pub runtime_session_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub active_turn_id: String,
}

pub struct LifecycleAgentSteeringService<'a> {
    lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    agent_frame_repo: &'a dyn AgentFrameRepository,
    execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    session_core: SessionCoreService,
    session_control: SessionControlService,
    session_eventing: SessionEventingService,
}

impl<'a> LifecycleAgentSteeringService<'a> {
    pub fn new(
        lifecycle_run_repo: &'a dyn LifecycleRunRepository,
        lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
        agent_frame_repo: &'a dyn AgentFrameRepository,
        execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
        session_core: SessionCoreService,
        session_control: SessionControlService,
        session_eventing: SessionEventingService,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            execution_anchor_repo,
            session_core,
            session_control,
            session_eventing,
        }
    }

    pub async fn steer(
        &self,
        command: LifecycleAgentSteeringCommand,
    ) -> Result<LifecycleAgentSteeringDispatch, WorkflowApplicationError> {
        if command.delivery_runtime_session_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "delivery runtime session id 不能为空".to_string(),
            ));
        }
        if command.input.is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "input 不能为空".to_string(),
            ));
        }

        let anchor = self
            .execution_anchor_repo
            .find_by_session(&command.delivery_runtime_session_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "runtime_session 缺少 RuntimeSessionExecutionAnchor: {}",
                    command.delivery_runtime_session_id
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
                "当前 Agent 已结束，不能运行中 steer".to_string(),
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

        let active_turn_id = match self
            .session_core
            .inspect_session_execution_state(&command.delivery_runtime_session_id)
            .await
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?
        {
            SessionExecutionState::Running {
                turn_id: Some(turn_id),
            } => turn_id,
            SessionExecutionState::Running { turn_id: None } => {
                return Err(WorkflowApplicationError::Conflict(
                    "当前 Session 正在执行但缺少 active turn，不能运行中 steer".to_string(),
                ));
            }
            _ => {
                return Err(WorkflowApplicationError::Conflict(
                    "当前 Session 未在执行中，不能运行中 steer".to_string(),
                ));
            }
        };
        if !self
            .session_control
            .supports_session_steering(&command.delivery_runtime_session_id)
            .await
        {
            return Err(WorkflowApplicationError::Conflict(
                "当前执行器不支持对该运行中 Session steer".to_string(),
            ));
        }
        let input = command.input.clone();
        self.session_control
            .steer_session(SessionTurnSteerCommand {
                session_id: command.delivery_runtime_session_id.clone(),
                expected_turn_id: active_turn_id.clone(),
                input: input.clone(),
            })
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "LifecycleAgent steer 投递失败: {error}"
                ))
            })?;
        self.session_eventing
            .emit_user_input_submitted(
                &command.delivery_runtime_session_id,
                &active_turn_id,
                &format!("{}:steer:{}", active_turn_id, Uuid::new_v4()),
                agentdash_agent_protocol::UserInputSubmissionKind::Steer,
                input,
            )
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "LifecycleAgent steer 事件写入失败: {error}"
                ))
            })?;

        Ok(LifecycleAgentSteeringDispatch {
            runtime_session_id: command.delivery_runtime_session_id,
            run_id: run.id,
            agent_id: agent.id,
            frame_id: frame.id,
            active_turn_id,
        })
    }
}

fn is_terminal_agent_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}
