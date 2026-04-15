use std::sync::Arc;

use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
    WorkflowRecordArtifactType,
};
use agentdash_spi::{HookError, HookStepAdvanceRequest, hooks::PendingExecutionLogEntry};
use uuid::Uuid;

use crate::workflow::execution_log as workflow_recording;
use crate::workflow::{
    ActiveWorkflowProjection, CompleteLifecycleStepCommand, LifecycleRunService,
    WorkflowRecordArtifactDraft, resolve_active_workflow_projection_for_session,
};

fn map_hook_error(error: agentdash_domain::DomainError) -> HookError {
    HookError::Runtime(error.to_string())
}

/// 根据 owner 信息构建 ActiveWorkflowProjection，以及 workflow 推进与日志写入。
pub struct WorkflowSnapshotBuilder {
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
}

impl WorkflowSnapshotBuilder {
    pub fn new(
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    ) -> Self {
        Self {
            session_binding_repo,
            workflow_definition_repo,
            lifecycle_definition_repo,
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
            self.session_binding_repo.as_ref(),
            self.workflow_definition_repo.as_ref(),
            self.lifecycle_definition_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
        )
        .await
        .map_err(HookError::Runtime)
    }

    pub async fn advance_workflow_step(
        &self,
        request: HookStepAdvanceRequest,
    ) -> Result<(), HookError> {
        let run_id = Uuid::parse_str(&request.run_id)
            .map_err(|e| HookError::Runtime(format!("advance: invalid run_id: {e}")))?;

        let record_artifacts: Vec<WorkflowRecordArtifactDraft> = request
            .record_artifacts
            .into_iter()
            .filter_map(|value| {
                let title = value.get("title")?.as_str()?.to_string();
                let content = value.get("content")?.as_str()?.to_string();
                let artifact_type_str = value.get("artifact_type")?.as_str()?;
                let artifact_type: WorkflowRecordArtifactType =
                    serde_json::from_value(serde_json::json!(artifact_type_str)).ok()?;
                Some(WorkflowRecordArtifactDraft {
                    artifact_type,
                    title,
                    content,
                })
            })
            .collect();

        let service = LifecycleRunService::new(
            self.workflow_definition_repo.as_ref(),
            self.lifecycle_definition_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
        );
        service
            .complete_step(CompleteLifecycleStepCommand {
                run_id,
                step_key: request.step_key,
                summary: request.summary,
                record_artifacts,
            })
            .await
            .map_err(|e| HookError::Runtime(format!("advance_workflow_step: {e}")))?;

        Ok(())
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
