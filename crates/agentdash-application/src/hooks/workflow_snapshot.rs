use std::sync::Arc;

use agentdash_domain::workflow::{
    ActivityExecutionClaimRepository, ActivityLifecycleDefinitionRepository,
    LifecycleRunRepository, WorkflowDefinitionRepository,
};
use agentdash_spi::{HookError, hooks::PendingExecutionLogEntry};
use uuid::Uuid;

use crate::workflow::execution_log as workflow_recording;
use crate::workflow::{ActiveWorkflowProjection, resolve_active_workflow_projection_for_session};

fn map_hook_error(error: agentdash_domain::DomainError) -> HookError {
    HookError::Runtime(error.to_string())
}

/// 根据 session 信息构建 ActiveWorkflowProjection，以及 workflow 推进与日志写入。
pub struct WorkflowSnapshotBuilder {
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    activity_lifecycle_definition_repo: Arc<dyn ActivityLifecycleDefinitionRepository>,
    activity_execution_claim_repo: Arc<dyn ActivityExecutionClaimRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
}

impl WorkflowSnapshotBuilder {
    pub fn new(
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        activity_lifecycle_definition_repo: Arc<dyn ActivityLifecycleDefinitionRepository>,
        activity_execution_claim_repo: Arc<dyn ActivityExecutionClaimRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    ) -> Self {
        Self {
            workflow_definition_repo,
            activity_lifecycle_definition_repo,
            activity_execution_claim_repo,
            lifecycle_run_repo,
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
            self.workflow_definition_repo.as_ref(),
            self.activity_lifecycle_definition_repo.as_ref(),
            self.activity_execution_claim_repo.as_ref(),
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
