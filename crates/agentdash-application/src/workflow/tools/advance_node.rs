use crate::platform_config::SharedPlatformConfig;
use crate::session::SessionHub;
use crate::workflow::{
    AdvanceCurrentNodeInput, AdvanceCurrentNodeStatus, LifecycleNodeAdvanceOutcome,
    LifecycleOrchestrator,
};
use agentdash_domain::workflow::LifecycleStepExecutionStatus;
use agentdash_spi::ExecutionContext;
use agentdash_spi::schema::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::active_workflow_locator_from_snapshot;

/// Agent 主动声明当前 lifecycle node 完成或失败。
///
/// 这是 DAG 编排的唯一推进路径（决策 D2 / D8）：
/// agent 调用此工具 → Orchestrator 验证 → 放行或拒绝。
#[derive(Clone)]
pub struct CompleteLifecycleNodeTool {
    repos: crate::repository_set::RepositorySet,
    session_hub: Option<SessionHub>,
    current_turn_id: String,
    hook_session: Option<agentdash_spi::hooks::SharedHookSessionRuntime>,
    platform_config: SharedPlatformConfig,
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
        repos: crate::repository_set::RepositorySet,
        session_hub: Option<SessionHub>,
        context: &ExecutionContext,
        platform_config: SharedPlatformConfig,
    ) -> Self {
        Self {
            repos,
            session_hub,
            current_turn_id: context.session.turn_id.clone(),
            hook_session: context.turn.hook_session.clone(),
            platform_config,
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

        let session_hub = self.session_hub.clone().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "session hub 尚未就绪，无法推进 lifecycle node".to_string(),
            )
        })?;
        let orchestrator = LifecycleOrchestrator::new(
            session_hub,
            self.repos.clone(),
            self.platform_config.clone(),
        );
        let outcome = match params.outcome {
            StepOutcome::Completed => LifecycleNodeAdvanceOutcome::Completed,
            StepOutcome::Failed => LifecycleNodeAdvanceOutcome::Failed,
        };
        let result = orchestrator
            .advance_current_node(AdvanceCurrentNodeInput {
                hook_session: hook_session.clone(),
                turn_id: self.current_turn_id.clone(),
                run_id: locator.run_id,
                step_key: locator.step_key.clone(),
                outcome,
                summary: params.summary.clone(),
            })
            .await
            .map_err(AgentToolError::ExecutionFailed)?;

        build_tool_result(result)
    }
}

fn build_tool_result(
    result: crate::workflow::AdvanceCurrentNodeResult,
) -> Result<AgentToolResult, AgentToolError> {
    match result.status {
        AdvanceCurrentNodeStatus::Failed => Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "Node `{}` 已标记为 **Failed**。{}",
                result.step_key,
                result
                    .run
                    .step_states
                    .iter()
                    .find(|state| state.step_key == result.step_key)
                    .and_then(|state| state.summary.as_deref())
                    .unwrap_or("")
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "run_id": result.run.id,
                "step_key": result.step_key,
                "outcome": "failed",
                "run_status": format!("{:?}", result.run.status),
            })),
        }),
        AdvanceCurrentNodeStatus::GateRejected {
            gate_collision_count,
            missing_output_keys,
            terminal_failed,
        } => {
            let missing_list = missing_output_keys.join(", ");
            let content = if terminal_failed {
                format!(
                    "Node `{}` 因连续 {gate_collision_count} 次门禁碰撞已标记为 **Failed**。\n\
                     未交付的 output port: [{}]",
                    result.step_key, missing_list
                )
            } else {
                format!(
                    "**门禁拒绝**（碰撞 {gate_collision_count}/3）：Node `{}` 尚有 {} 个 output port 未交付。\n\
                     缺失: [{}]\n\n\
                     请通过 `write_file` 写入 `lifecycle://artifacts/{{port_key}}` 完成交付后重试。",
                    result.step_key,
                    missing_output_keys.len(),
                    missing_list
                )
            };
            Ok(AgentToolResult {
                content: vec![ContentPart::text(content)],
                is_error: true,
                details: Some(serde_json::json!({
                    "run_id": result.run.id,
                    "step_key": result.step_key,
                    "gate_collision_count": gate_collision_count,
                    "missing_ports": missing_output_keys,
                    "status": if terminal_failed { "failed" } else { "gate_rejected" },
                })),
            })
        }
        AdvanceCurrentNodeStatus::Completed => {
            let newly_ready: Vec<&str> = result
                .run
                .step_states
                .iter()
                .filter(|state| state.status == LifecycleStepExecutionStatus::Ready)
                .map(|state| state.step_key.as_str())
                .collect();
            let successor_info = if newly_ready.is_empty() {
                if result.run.active_node_keys.is_empty() {
                    "lifecycle 已全部完成。".to_string()
                } else {
                    format!("活跃 node: [{}]", result.run.active_node_keys.join(", "))
                }
            } else {
                format!("后继 node 已就绪: [{}]", newly_ready.join(", "))
            };
            let message = if let Some(warning) = result.orchestration_warning.as_deref() {
                format!(
                    "Node `{}` 已完成。{successor_info}\n[warning] {warning}",
                    result.step_key
                )
            } else {
                format!("Node `{}` 已完成。{successor_info}", result.step_key)
            };
            Ok(AgentToolResult {
                content: vec![ContentPart::text(message)],
                is_error: false,
                details: Some(serde_json::json!({
                    "run_id": result.run.id,
                    "step_key": result.step_key,
                    "outcome": "completed",
                    "run_status": format!("{:?}", result.run.status),
                    "active_node_keys": result.run.active_node_keys,
                    "orchestration_warning": result.orchestration_warning,
                })),
            })
        }
    }
}
