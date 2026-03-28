use std::sync::Arc;

use agentdash_agent::tools::schema_value;
use agentdash_connector_contract::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, WorkflowDefinitionRepository,
    WorkflowRecordArtifactType,
};
use agentdash_executor::{ExecutionContext, SessionHookRefreshQuery, SessionHookSnapshot};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::workflow::{
    AppendLifecycleStepArtifactsCommand, LifecycleRunService, WorkflowRecordArtifactDraft,
};

#[derive(Clone)]
pub struct WorkflowArtifactReportTool {
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    current_session_id: Option<String>,
    current_turn_id: String,
    hook_session: Option<Arc<agentdash_executor::HookSessionRuntime>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowArtifactReportParams {
    pub content: String,
    pub artifact_type: Option<String>,
    pub title: Option<String>,
}

pub struct ActiveWorkflowLocator {
    run_id: Uuid,
    step_key: String,
}

impl WorkflowArtifactReportTool {
    pub fn new(
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            workflow_definition_repo,
            lifecycle_definition_repo,
            lifecycle_run_repo,
            current_session_id: context
                .hook_session
                .as_ref()
                .map(|session| session.session_id().to_string()),
            current_turn_id: context.turn_id.clone(),
            hook_session: context.hook_session.clone(),
        }
    }
}

#[async_trait]
impl AgentTool for WorkflowArtifactReportTool {
    fn name(&self) -> &str {
        "report_workflow_artifact"
    }

    fn description(&self) -> &str {
        "向当前 active workflow phase 追加结构化记录产物。支持 `phase_note` / `checklist_evidence` / `session_summary` / `journal_update` / `archive_suggestion`；当 phase 使用 checklist_passed 时，优先写入 `checklist_evidence`。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<WorkflowArtifactReportParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: WorkflowArtifactReportParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;
        let content = params.content.trim();
        if content.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "content 不能为空".to_string(),
            ));
        }

        let hook_session = self.hook_session.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有 hook runtime，无法写入 workflow 记录产物".to_string(),
            )
        })?;
        let locator =
            active_workflow_locator_from_snapshot(&hook_session.snapshot()).ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "当前 session 没有关联 active workflow，无法写入 workflow 记录产物".to_string(),
                )
            })?;

        let artifact_type =
            normalize_workflow_record_artifact_type(params.artifact_type.as_deref())?;
        let title = params
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                active_workflow_default_artifact_title_from_snapshot(&hook_session.snapshot())
            })
            .unwrap_or_else(|| format!("{} 阶段记录", locator.step_key));

        let service = LifecycleRunService::new(
            self.workflow_definition_repo.as_ref(),
            self.lifecycle_definition_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
        );
        let run = service
            .append_step_artifacts(AppendLifecycleStepArtifactsCommand {
                run_id: locator.run_id,
                step_key: locator.step_key.clone(),
                artifacts: vec![WorkflowRecordArtifactDraft {
                    artifact_type,
                    title: title.clone(),
                    content: content.to_string(),
                }],
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        hook_session
            .refresh(SessionHookRefreshQuery {
                session_id: hook_session.session_id().to_string(),
                turn_id: Some(self.current_turn_id.clone()),
                reason: Some("tool:report_workflow_artifact".to_string()),
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已写入 workflow 记录产物。\n- run_id: {}\n- step_key: {}\n- artifact_type: {}\n- title: {}",
                run.id,
                locator.step_key,
                workflow_record_artifact_type_key(artifact_type),
                title
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "session_id": self.current_session_id.clone(),
                "turn_id": self.current_turn_id.clone(),
                "run_id": run.id,
                "step_key": locator.step_key,
                "artifact_type": workflow_record_artifact_type_key(artifact_type),
                "title": title,
            })),
        })
    }
}

pub fn active_workflow_locator_from_snapshot(
    snapshot: &SessionHookSnapshot,
) -> Option<ActiveWorkflowLocator> {
    let aw = snapshot.metadata.as_ref()?.active_workflow.as_ref()?;
    let run_id = Uuid::parse_str(aw.run_id.as_deref()?).ok()?;
    let step_key = aw.step_key.clone()?;
    Some(ActiveWorkflowLocator { run_id, step_key })
}

pub fn active_workflow_default_artifact_title_from_snapshot(
    snapshot: &SessionHookSnapshot,
) -> Option<String> {
    snapshot
        .metadata
        .as_ref()?
        .active_workflow
        .as_ref()?
        .default_artifact_title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_workflow_record_artifact_type(
    value: Option<&str>,
) -> Result<WorkflowRecordArtifactType, AgentToolError> {
    match value.unwrap_or("phase_note").trim() {
        "" | "phase_note" => Ok(WorkflowRecordArtifactType::PhaseNote),
        "checklist_evidence" => Ok(WorkflowRecordArtifactType::ChecklistEvidence),
        "session_summary" => Ok(WorkflowRecordArtifactType::SessionSummary),
        "journal_update" => Ok(WorkflowRecordArtifactType::JournalUpdate),
        "archive_suggestion" => Ok(WorkflowRecordArtifactType::ArchiveSuggestion),
        other => Err(AgentToolError::InvalidArguments(format!(
            "artifact_type 不支持 `{other}`"
        ))),
    }
}

fn workflow_record_artifact_type_key(artifact_type: WorkflowRecordArtifactType) -> &'static str {
    match artifact_type {
        WorkflowRecordArtifactType::SessionSummary => "session_summary",
        WorkflowRecordArtifactType::JournalUpdate => "journal_update",
        WorkflowRecordArtifactType::ArchiveSuggestion => "archive_suggestion",
        WorkflowRecordArtifactType::PhaseNote => "phase_note",
        WorkflowRecordArtifactType::ChecklistEvidence => "checklist_evidence",
        WorkflowRecordArtifactType::ExecutionTrace => "execution_trace",
        WorkflowRecordArtifactType::DecisionRecord => "decision_record",
        WorkflowRecordArtifactType::ContextSnapshot => "context_snapshot",
    }
}
