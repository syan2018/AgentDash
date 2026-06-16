use std::sync::Arc;

use agentdash_domain::workflow::{
    AgentFrameRepository, AgentProcedureRepository, LifecycleAgentRepository,
    LifecycleRunRepository, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::{HookError, hooks::HookControlTarget, hooks::PendingExecutionLogEntry};
use uuid::Uuid;

use crate::lifecycle::{
    ActiveWorkflowProjection, resolve_active_workflow_projection_for_target,
    resolve_active_workflow_projection_from_message_stream_trace,
};
use crate::lifecycle::execution_log as workflow_recording;

fn map_hook_error(error: agentdash_domain::DomainError) -> HookError {
    HookError::Runtime(error.to_string())
}

/// 根据 session 信息构建 ActiveWorkflowProjection，以及 workflow 推进与日志写入。
pub struct WorkflowSnapshotBuilder {
    agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
}

impl WorkflowSnapshotBuilder {
    pub fn new(
        agent_procedure_repo: Arc<dyn AgentProcedureRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    ) -> Self {
        Self {
            agent_procedure_repo,
            agent_frame_repo,
            lifecycle_agent_repo,
            lifecycle_run_repo,
            execution_anchor_repo,
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
        resolve_active_workflow_projection_from_message_stream_trace(
            session_id,
            self.agent_procedure_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
            self.lifecycle_agent_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
            self.execution_anchor_repo.as_ref(),
        )
        .await
        .map_err(HookError::Runtime)
    }

    pub async fn resolve_active_workflow_for_target(
        &self,
        target: &HookControlTarget,
    ) -> Result<Option<ActiveWorkflowProjection>, HookError> {
        resolve_active_workflow_projection_for_target(
            target,
            self.agent_procedure_repo.as_ref(),
            self.agent_frame_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
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
