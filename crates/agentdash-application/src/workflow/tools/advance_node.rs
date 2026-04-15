use std::sync::Arc;

use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, LifecycleStepExecutionStatus,
    WorkflowDefinitionRepository, WorkflowRecordArtifactType,
};
use agentdash_spi::schema::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use agentdash_spi::{ExecutionContext, SessionHookRefreshQuery};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::artifact_report::active_workflow_locator_from_snapshot;
use crate::session::SessionHub;
use crate::workflow::{
    CompleteLifecycleStepCommand, LifecycleOrchestrator, LifecycleRunService,
    WorkflowRecordArtifactDraft,
};

/// Agent 主动声明当前 lifecycle node 完成。
///
/// 这是 DAG 编排的唯一推进路径（决策 D2 / D8）：
/// agent 调用此工具 → Orchestrator 验证 → 放行或拒绝。
#[derive(Clone)]
pub struct AdvanceLifecycleNodeTool {
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    session_hub: Option<SessionHub>,
    current_turn_id: String,
    hook_session: Option<agentdash_spi::hooks::SharedHookSessionRuntime>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AdvanceLifecycleNodeParams {
    /// 当前 node 的工作摘要
    #[serde(default)]
    pub summary: Option<String>,
    /// 附带提交的 artifact（可选）
    #[serde(default)]
    pub artifacts: Option<Vec<ArtifactDraft>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArtifactDraft {
    /// Artifact 类型，如 session_summary / phase_note / checklist_evidence
    pub artifact_type: String,
    pub title: String,
    pub content: String,
}

impl AdvanceLifecycleNodeTool {
    pub fn new(
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        session_hub: Option<SessionHub>,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            session_binding_repo,
            workflow_definition_repo,
            lifecycle_definition_repo,
            lifecycle_run_repo,
            session_hub,
            current_turn_id: context.turn_id.clone(),
            hook_session: context.hook_session.clone(),
        }
    }
}

#[async_trait]
impl AgentTool for AdvanceLifecycleNodeTool {
    fn name(&self) -> &str {
        "advance_lifecycle_node"
    }

    fn description(&self) -> &str {
        "声明当前 lifecycle node 已完成。这是推进 lifecycle 到下一个 node 的唯一方式。未调用此工具前，session 不允许正常结束。调用后系统会自动评估后继 node 的可达性并启动下一阶段。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<AdvanceLifecycleNodeParams>()
    }

    async fn execute(
        &self,
        _tool_use_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: AdvanceLifecycleNodeParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("参数解析失败: {e}")))?;

        let hook_session = self.hook_session.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有 hook runtime，无法推进 lifecycle node".to_string(),
            )
        })?;

        let locator =
            active_workflow_locator_from_snapshot(&hook_session.snapshot()).ok_or_else(|| {
                AgentToolError::ExecutionFailed(
                    "当前 session 没有关联 active workflow，无法推进 lifecycle node".to_string(),
                )
            })?;

        let record_artifacts: Vec<WorkflowRecordArtifactDraft> = params
            .artifacts
            .unwrap_or_default()
            .into_iter()
            .filter_map(|draft| {
                let artifact_type = parse_artifact_type(&draft.artifact_type)?;
                Some(WorkflowRecordArtifactDraft {
                    artifact_type,
                    title: draft.title,
                    content: draft.content,
                })
            })
            .collect();

        let service = LifecycleRunService::new(
            self.workflow_definition_repo.as_ref(),
            self.lifecycle_definition_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
        );

        let run = service
            .complete_step(CompleteLifecycleStepCommand {
                run_id: locator.run_id,
                step_key: locator.step_key.clone(),
                summary: params.summary.clone(),
                record_artifacts,
            })
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(format!("推进失败: {e}")))?;

        // 刷新 hook snapshot 使后续 hook evaluation 看到最新状态
        hook_session
            .refresh(SessionHookRefreshQuery {
                session_id: hook_session.session_id().to_string(),
                turn_id: Some(self.current_turn_id.clone()),
                reason: Some("tool:advance_lifecycle_node".to_string()),
            })
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        let orchestration_warning = if let Some(session_hub) = self.session_hub.clone() {
            let orchestrator = LifecycleOrchestrator::new(
                session_hub,
                self.session_binding_repo.clone(),
                self.workflow_definition_repo.clone(),
                self.lifecycle_definition_repo.clone(),
                self.lifecycle_run_repo.clone(),
            );
            match orchestrator
                .after_node_advanced(run.id, run.project_id)
                .await
            {
                Ok(_) => None,
                Err(error) => Some(format!("node 已完成，但后继编排触发失败：{error}")),
            }
        } else {
            Some("node 已完成，但 session hub 尚未就绪，未触发后继编排".to_string())
        };

        let newly_ready: Vec<&str> = run
            .step_states
            .iter()
            .filter(|s| s.status == LifecycleStepExecutionStatus::Ready)
            .map(|s| s.step_key.as_str())
            .collect();
        let successor_info = if newly_ready.is_empty() {
            if run.active_node_keys.is_empty() {
                "lifecycle 已全部完成。".to_string()
            } else {
                format!("活跃 node: [{}]", run.active_node_keys.join(", "))
            }
        } else {
            format!("后继 node 已就绪: [{}]", newly_ready.join(", "))
        };
        let message = if let Some(warning) = orchestration_warning.as_deref() {
            format!(
                "Node `{}` 已完成。{successor_info}\n[warning] {warning}",
                locator.step_key
            )
        } else {
            format!("Node `{}` 已完成。{successor_info}", locator.step_key)
        };

        Ok(AgentToolResult {
            content: vec![ContentPart::text(message)],
            is_error: false,
            details: Some(serde_json::json!({
                "run_id": run.id,
                "step_key": locator.step_key,
                "run_status": format!("{:?}", run.status),
                "active_node_keys": run.active_node_keys,
                "orchestration_warning": orchestration_warning,
            })),
        })
    }
}

fn parse_artifact_type(s: &str) -> Option<WorkflowRecordArtifactType> {
    let quoted = format!("\"{s}\"");
    serde_json::from_str(&quoted).ok()
}
