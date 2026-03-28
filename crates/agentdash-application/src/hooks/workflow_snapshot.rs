use std::sync::Arc;

use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
    WorkflowRecordArtifactType, WorkflowTargetKind,
};
use agentdash_executor::{
    HookError, HookOwnerSummary, HookStepAdvanceRequest, PendingExecutionLogEntry,
};
use uuid::Uuid;

use crate::workflow::{
    ActiveWorkflowProjection, CompleteLifecycleStepCommand, LifecycleRunService,
    WorkflowRecordArtifactDraft, resolve_active_workflow_projection,
};
use crate::workflow::execution_log as workflow_recording;

fn map_hook_error(error: agentdash_domain::DomainError) -> HookError {
    HookError::Runtime(error.to_string())
}

/// 根据 owner 信息构建 ActiveWorkflowProjection，以及 workflow 推进与日志写入。
pub struct WorkflowSnapshotBuilder {
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
}

impl WorkflowSnapshotBuilder {
    pub fn new(
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    ) -> Self {
        Self {
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

    pub async fn resolve_active_workflow(
        &self,
        owner: &HookOwnerSummary,
    ) -> Result<Option<ActiveWorkflowProjection>, HookError> {
        let owner_id = Uuid::parse_str(owner.owner_id.as_str())
            .map_err(|error| HookError::Runtime(format!("owner_id 不是有效 UUID: {error}")))?;
        let target_kind = match owner.owner_type.as_str() {
            "project" => WorkflowTargetKind::Project,
            "story" => WorkflowTargetKind::Story,
            "task" => WorkflowTargetKind::Task,
            other => {
                return Err(HookError::Runtime(format!(
                    "未知 session owner_type，无法映射 workflow target: {other}"
                )));
            }
        };

        resolve_active_workflow_projection(
            target_kind,
            owner_id,
            owner.label.clone(),
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
        workflow_recording::flush_execution_log_entries(
            self.lifecycle_run_repo.as_ref(),
            entries,
        )
        .await
        .map_err(|e| HookError::Runtime(format!("flush execution log: {e}")))
    }
}
