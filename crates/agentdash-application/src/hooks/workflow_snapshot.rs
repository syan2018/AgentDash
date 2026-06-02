use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentAssignmentRepository, AgentFrameRepository, AgentProcedureRepository,
    LifecycleAgentRepository, LifecycleRunRepository, WorkflowGraphInstanceRepository,
    WorkflowGraphRepository,
};
use agentdash_spi::{HookError, hooks::HookControlTarget, hooks::PendingExecutionLogEntry};
use uuid::Uuid;

use crate::workflow::execution_log as workflow_recording;
use crate::workflow::{
    ActiveWorkflowProjection, resolve_active_workflow_projection_for_session,
    resolve_active_workflow_projection_for_target, select_assignment_for_frame,
};

fn map_hook_error(error: agentdash_domain::DomainError) -> HookError {
    HookError::Runtime(error.to_string())
}

/// 根据 session 信息构建 ActiveWorkflowProjection，以及 workflow 推进与日志写入。
pub struct WorkflowSnapshotBuilder {
    agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    agent_assignment_repo: Arc<dyn AgentAssignmentRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository>,
}

impl WorkflowSnapshotBuilder {
    pub fn new(
        agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
        workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
        agent_assignment_repo: Arc<dyn AgentAssignmentRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        workflow_graph_instance_repo: Arc<dyn WorkflowGraphInstanceRepository>,
    ) -> Self {
        Self {
            agent_procedure_repo,
            workflow_graph_repo,
            agent_frame_repo,
            lifecycle_agent_repo,
            agent_assignment_repo,
            lifecycle_run_repo,
            workflow_graph_instance_repo,
        }
    }

    pub async fn get_lifecycle_run(
        &self,
        run_id: Uuid,
    ) -> Result<Option<agentdash_domain::workflow::LifecycleRun>, HookError> {
        self.lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(map_hook_error)
    }

    /// 通过 session_id 查找关联的活跃 lifecycle run 并构建 workflow projection。
    pub async fn resolve_active_workflow(
        &self,
        session_id: &str,
    ) -> Result<Option<ActiveWorkflowProjection>, HookError> {
        resolve_active_workflow_projection_for_session(
            session_id,
            self.agent_procedure_repo.as_ref(),
            self.workflow_graph_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
            self.lifecycle_agent_repo.as_ref(),
            self.agent_assignment_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
            self.workflow_graph_instance_repo.as_ref(),
        )
        .await
        .map_err(HookError::Runtime)
    }

    pub async fn resolve_hook_control_target_for_runtime_session(
        &self,
        session_id: &str,
    ) -> Result<Option<HookControlTarget>, HookError> {
        let Some(frame) = self
            .agent_frame_repo
            .find_by_runtime_session(session_id)
            .await
            .map_err(map_hook_error)?
        else {
            return Ok(None);
        };
        let Some(agent) = self
            .lifecycle_agent_repo
            .get(frame.agent_id)
            .await
            .map_err(map_hook_error)?
        else {
            return Ok(None);
        };
        let assignment_id =
            select_assignment_for_frame(self.agent_assignment_repo.as_ref(), &frame)
                .await
                .map_err(|error| HookError::Runtime(error.to_string()))?
                .map(|assignment| assignment.id);
        Ok(Some(HookControlTarget {
            run_id: agent.run_id,
            agent_id: agent.id,
            frame_id: frame.id,
            assignment_id,
        }))
    }

    pub async fn resolve_active_workflow_for_target(
        &self,
        target: &HookControlTarget,
    ) -> Result<Option<ActiveWorkflowProjection>, HookError> {
        resolve_active_workflow_projection_for_target(
            target,
            self.agent_procedure_repo.as_ref(),
            self.workflow_graph_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
            self.agent_assignment_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
            self.workflow_graph_instance_repo.as_ref(),
        )
        .await
        .map_err(HookError::Runtime)
    }

    pub async fn append_execution_log(
        &self,
        entries: Vec<PendingExecutionLogEntry>,
    ) -> Result<(), HookError> {
        workflow_recording::flush_execution_log_entries(self.lifecycle_run_repo.as_ref(), entries)
            .await
            .map_err(|e| HookError::Runtime(format!("flush execution log: {e}")))
    }
}
