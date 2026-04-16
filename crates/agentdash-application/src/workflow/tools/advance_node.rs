use std::sync::Arc;

use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, LifecycleStepExecutionStatus,
    WorkflowDefinitionRepository,
};
use agentdash_spi::schema::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use agentdash_spi::{ExecutionContext, SessionHookRefreshQuery};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::active_workflow_locator_from_snapshot;
use crate::session::SessionHub;
use crate::workflow::{
    CompleteLifecycleStepCommand, LifecycleOrchestrator, LifecycleRunService,
};

/// Agent 主动声明当前 lifecycle node 完成或失败。
///
/// 这是 DAG 编排的唯一推进路径（决策 D2 / D8）：
/// agent 调用此工具 → Orchestrator 验证 → 放行或拒绝。
#[derive(Clone)]
pub struct CompleteLifecycleNodeTool {
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    inline_file_repo: Arc<dyn InlineFileRepository>,
    session_hub: Option<SessionHub>,
    current_turn_id: String,
    hook_session: Option<agentdash_spi::hooks::SharedHookSessionRuntime>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcome {
    Completed,
    Failed,
}

fn default_outcome() -> StepOutcome {
    StepOutcome::Completed
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompleteLifecycleNodeParams {
    /// 当前 node 的工作摘要
    #[serde(default)]
    pub summary: Option<String>,
    /// 流转结果：completed（默认）或 failed。
    /// 设为 failed 时跳过产物门禁，直接标记 node 失败且不触发后继编排。
    #[serde(default = "default_outcome")]
    pub outcome: StepOutcome,
}

impl CompleteLifecycleNodeTool {
    pub fn new(
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        workflow_definition_repo: Arc<dyn WorkflowDefinitionRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        session_hub: Option<SessionHub>,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            session_binding_repo,
            workflow_definition_repo,
            lifecycle_definition_repo,
            lifecycle_run_repo,
            inline_file_repo,
            session_hub,
            current_turn_id: context.turn_id.clone(),
            hook_session: context.hook_session.clone(),
        }
    }
}

#[async_trait]
impl AgentTool for CompleteLifecycleNodeTool {
    fn name(&self) -> &str {
        "complete_lifecycle_node"
    }

    fn description(&self) -> &str {
        "声明当前 lifecycle node 完成或失败。这是推进 lifecycle 的唯一方式。\n\
         - outcome=completed（默认）：检查产物门禁，通过后完成 node 并触发后继编排。\n\
         - outcome=failed：跳过门禁，直接标记 node 失败，不触发后继。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompleteLifecycleNodeParams>()
    }

    async fn execute(
        &self,
        _tool_use_id: &str,
        args: serde_json::Value,
        _cancel: CancellationToken,
        _update: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: CompleteLifecycleNodeParams = serde_json::from_value(args)
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

        let current_run = self
            .lifecycle_run_repo
            .get_by_id(locator.run_id)
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(format!("加载 run 失败: {e}")))?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!("run 不存在: {}", locator.run_id))
            })?;

        // ── outcome == Failed: 跳过门禁，直接标记失败 ──
        if params.outcome == StepOutcome::Failed {
            let mut run = current_run.clone();
            if let Some(state) = run
                .step_states
                .iter_mut()
                .find(|s| s.step_key == locator.step_key)
            {
                state.status = LifecycleStepExecutionStatus::Failed;
                state.completed_at = Some(chrono::Utc::now());
                state.summary = params.summary.clone();
            }
            run.last_activity_at = chrono::Utc::now();
            // Remove from active nodes
            run.active_node_keys
                .retain(|k| k != &locator.step_key);
            if run.current_step_key.as_deref() == Some(&locator.step_key) {
                run.current_step_key = run.active_node_keys.first().cloned();
            }
            // Check if all nodes are terminal -> mark run as Failed
            let all_terminal = run.step_states.iter().all(|s| {
                matches!(
                    s.status,
                    LifecycleStepExecutionStatus::Completed
                        | LifecycleStepExecutionStatus::Failed
                        | LifecycleStepExecutionStatus::Skipped
                )
            });
            if all_terminal {
                run.status = agentdash_domain::workflow::LifecycleRunStatus::Failed;
            }
            self.lifecycle_run_repo
                .update(&run)
                .await
                .map_err(|e| {
                    AgentToolError::ExecutionFailed(format!("标记 Failed 失败: {e}"))
                })?;

            // Still materialize summary
            if let Some(summary) = &params.summary {
                let file = InlineFile::new(
                    InlineFileOwnerKind::LifecycleRun,
                    locator.run_id,
                    "session_records",
                    format!("{}/summary", locator.step_key),
                    summary.clone(),
                );
                let _ = self.inline_file_repo.upsert_file(&file).await;
            }

            // Refresh hook snapshot
            hook_session
                .refresh(SessionHookRefreshQuery {
                    session_id: hook_session.session_id().to_string(),
                    turn_id: Some(self.current_turn_id.clone()),
                    reason: Some("tool:complete_lifecycle_node".to_string()),
                })
                .await
                .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

            // Do NOT trigger orchestration for failed nodes
            return Ok(AgentToolResult {
                content: vec![ContentPart::text(format!(
                    "Node `{}` 已标记为 **Failed**。{}",
                    locator.step_key,
                    params.summary.as_deref().unwrap_or("")
                ))],
                is_error: false,
                details: Some(serde_json::json!({
                    "run_id": locator.run_id,
                    "step_key": locator.step_key,
                    "outcome": "failed",
                    "run_status": format!("{:?}", run.status),
                })),
            });
        }

        // ── outcome == Completed (default): gate collision 检查 + 正常完成 ──
        let lifecycle_def = self
            .lifecycle_definition_repo
            .get_by_id(current_run.lifecycle_id)
            .await
            .map_err(|e| {
                AgentToolError::ExecutionFailed(format!("加载 lifecycle definition 失败: {e}"))
            })?
            .ok_or_else(|| {
                AgentToolError::ExecutionFailed(format!(
                    "lifecycle definition 不存在: {}",
                    current_run.lifecycle_id
                ))
            })?;
        let step_def = lifecycle_def
            .steps
            .iter()
            .find(|s| s.key == locator.step_key);
        // 从 step 级 output_ports 获取 required keys（port 归属已迁移到 step）
        let required_output_keys: Vec<String> = step_def
            .map(|s| s.output_ports.iter().map(|p| p.key.clone()).collect())
            .unwrap_or_default();

        if !required_output_keys.is_empty() {
            // Load port outputs from inline_fs for gate check
            let port_output_files = self.inline_file_repo
                .list_files(InlineFileOwnerKind::LifecycleRun, current_run.id, "port_outputs")
                .await
                .map_err(|e| AgentToolError::ExecutionFailed(format!("加载 port outputs 失败: {e}")))?;
            let port_output_map: std::collections::HashMap<String, String> = port_output_files
                .into_iter()
                .map(|f| (f.path, f.content))
                .collect();

            let missing: Vec<&String> = required_output_keys
                .iter()
                .filter(|key| {
                    !port_output_map
                        .get(key.as_str())
                        .is_some_and(|v| !v.trim().is_empty())
                })
                .collect();

            if !missing.is_empty() {
                // 递增 gate_collision_count
                let mut updated_run = current_run.clone();
                if let Some(state) = updated_run
                    .step_states
                    .iter_mut()
                    .find(|s| s.step_key == locator.step_key)
                {
                    state.gate_collision_count += 1;
                    let collision = state.gate_collision_count;

                    if collision >= 3 {
                        state.status = LifecycleStepExecutionStatus::Failed;
                        state.completed_at = Some(chrono::Utc::now());
                        updated_run.last_activity_at = chrono::Utc::now();
                        self.lifecycle_run_repo
                            .update(&updated_run)
                            .await
                            .map_err(|e| {
                                AgentToolError::ExecutionFailed(format!(
                                    "更新 gate collision 失败: {e}"
                                ))
                            })?;

                        return Ok(AgentToolResult {
                            content: vec![ContentPart::text(format!(
                                "Node `{}` 因连续 {collision} 次门禁碰撞已标记为 **Failed**。\n\
                                 未交付的 output port: [{}]",
                                locator.step_key,
                                missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                            ))],
                            is_error: true,
                            details: Some(serde_json::json!({
                                "run_id": current_run.id,
                                "step_key": locator.step_key,
                                "gate_collision_count": collision,
                                "missing_ports": missing,
                                "status": "failed",
                            })),
                        });
                    }

                    updated_run.last_activity_at = chrono::Utc::now();
                    self.lifecycle_run_repo
                        .update(&updated_run)
                        .await
                        .map_err(|e| {
                            AgentToolError::ExecutionFailed(format!(
                                "更新 gate collision 失败: {e}"
                            ))
                        })?;

                    return Ok(AgentToolResult {
                        content: vec![ContentPart::text(format!(
                            "**门禁拒绝**（碰撞 {collision}/3）：Node `{}` 尚有 {} 个 output port 未交付。\n\
                             缺失: [{}]\n\n\
                             请通过 `write_file` 写入 `lifecycle://artifacts/{{port_key}}` 完成交付后重试。",
                            locator.step_key,
                            missing.len(),
                            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        ))],
                        is_error: true,
                        details: Some(serde_json::json!({
                            "run_id": current_run.id,
                            "step_key": locator.step_key,
                            "gate_collision_count": collision,
                            "missing_ports": missing,
                        })),
                    });
                }
            }
        }

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
            })
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(format!("推进失败: {e}")))?;

        // ── 物化 session summary 到 inline_fs ──
        if let Some(summary) = &params.summary {
            let file = InlineFile::new(
                InlineFileOwnerKind::LifecycleRun,
                locator.run_id,
                "session_records",
                format!("{}/summary", locator.step_key),
                summary.clone(),
            );
            let _ = self.inline_file_repo.upsert_file(&file).await;
        }

        // 刷新 hook snapshot 使后续 hook evaluation 看到最新状态
        hook_session
            .refresh(SessionHookRefreshQuery {
                session_id: hook_session.session_id().to_string(),
                turn_id: Some(self.current_turn_id.clone()),
                reason: Some("tool:complete_lifecycle_node".to_string()),
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
                self.inline_file_repo.clone(),
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
                "outcome": "completed",
                "run_status": format!("{:?}", run.status),
                "active_node_keys": run.active_node_keys,
                "orchestration_warning": orchestration_warning,
            })),
        })
    }
}

