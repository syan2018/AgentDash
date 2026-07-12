use std::sync::Arc;

use crate::SharedPlatformConfig;
use crate::lifecycle::{
    AdvanceCurrentActivityInput, AdvanceCurrentNodeResult, AdvanceCurrentNodeStatus,
    LifecycleNodeAdvanceOutcome, LifecycleOrchestrator, LifecycleOrchestratorDeps,
};
use agentdash_spi::ExecutionContext;
use agentdash_spi::FunctionRunner;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub struct SharedSessionToolServicesHandle;

impl SharedSessionToolServicesHandle {
    pub async fn get(&self) -> Option<()> {
        Some(())
    }
}

/// Agent session 节点主动提交当前 terminal outcome。
///
/// 工具调用会交给 Orchestrator 校验，并通过 common orchestration runtime
/// materialize 当前节点的 completed / failed outcome。
#[derive(Clone)]
pub struct CompleteLifecycleNodeTool {
    orchestrator_deps: LifecycleOrchestratorDeps,
    session_services_handle: SharedSessionToolServicesHandle,
    platform_config: SharedPlatformConfig,
    function_runner: Option<Arc<dyn FunctionRunner>>,
    current_turn_id: String,
    hook_runtime: Option<agentdash_spi::hooks::SharedHookRuntime>,
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
        orchestrator_deps: LifecycleOrchestratorDeps,
        session_services_handle: SharedSessionToolServicesHandle,
        function_runner: Option<Arc<dyn FunctionRunner>>,
        platform_config: SharedPlatformConfig,
        context: &ExecutionContext,
    ) -> Self {
        Self {
            orchestrator_deps,
            session_services_handle,
            platform_config,
            function_runner,
            current_turn_id: context.session.turn_id.clone(),
            hook_runtime: context.turn.hook_runtime.clone(),
        }
    }
}

#[async_trait]
impl AgentTool for CompleteLifecycleNodeTool {
    fn name(&self) -> &str {
        "complete_lifecycle_node"
    }

    fn description(&self) -> &str {
        "Agent session 节点主动提交当前 terminal outcome。\n\
         - outcome=completed（默认）：检查产物门禁，通过后完成 node 并触发后继编排。\n\
         - outcome=failed：跳过门禁，直接标记 node 失败，不触发后继。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CompleteLifecycleNodeParams>()
    }
    fn protocol_projector(&self) -> Option<agentdash_agent_types::ToolProtocolProjector> {
        Some(agentdash_agent_types::ToolProtocolProjector::LifecycleComplete)
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

        let hook_runtime = self.hook_runtime.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "当前 session 没有 hook runtime，无法推进 lifecycle node".to_string(),
            )
        })?;

        let _session_services = self.session_services_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed(
                "session services 尚未就绪，无法推进 lifecycle node".to_string(),
            )
        })?;
        let mut orchestrator = LifecycleOrchestrator::new_with_platform_config(
            self.orchestrator_deps.clone(),
            self.platform_config.clone(),
        );
        if let Some(function_runner) = &self.function_runner {
            orchestrator = orchestrator.with_function_runner(function_runner.clone());
        }
        let outcome = match params.outcome {
            StepOutcome::Completed => LifecycleNodeAdvanceOutcome::Completed,
            StepOutcome::Failed => LifecycleNodeAdvanceOutcome::Failed,
        };
        let snapshot = hook_runtime.snapshot();
        let result = orchestrator
            .advance_current_activity(AdvanceCurrentActivityInput {
                hook_runtime: hook_runtime.clone(),
                turn_id: self.current_turn_id.clone(),
                runtime_session_id: snapshot.runtime_adapter_session_id.clone(),
                outcome,
                summary: params.summary.clone(),
            })
            .await
            .map_err(AgentToolError::ExecutionFailed)?;

        build_tool_result(result)
    }
}

fn build_tool_result(result: AdvanceCurrentNodeResult) -> Result<AgentToolResult, AgentToolError> {
    match result.status {
        AdvanceCurrentNodeStatus::Failed => Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "Runtime node `{}` 已标记为 **Failed**。",
                result.node_path
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "run_id": result.run.id,
                "orchestration_id": result.orchestration_id,
                "node_path": result.node_path,
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
                    "Runtime node `{}` 因连续 {gate_collision_count} 次门禁碰撞已标记为 **Failed**。\n\
                     未交付的 output port: [{}]",
                    result.node_path, missing_list
                )
            } else {
                format!(
                    "**门禁拒绝**（碰撞 {gate_collision_count}/3）：Runtime node `{}` 尚有 {} 个 output port 未交付。\n\
                     缺失: [{}]\n\n\
                     请通过 `write_file` 写入 `lifecycle://artifacts/{{port_key}}` 完成交付后重试。",
                    result.node_path,
                    missing_output_keys.len(),
                    missing_list
                )
            };
            Ok(AgentToolResult {
                content: vec![ContentPart::text(content)],
                is_error: true,
                details: Some(serde_json::json!({
                    "run_id": result.run.id,
                    "orchestration_id": result.orchestration_id,
                    "node_path": result.node_path,
                    "gate_collision_count": gate_collision_count,
                    "missing_ports": missing_output_keys,
                    "status": if terminal_failed { "failed" } else { "gate_rejected" },
                })),
            })
        }
        AdvanceCurrentNodeStatus::Completed => {
            let message = if let Some(warning) = result.orchestration_warning.as_deref() {
                format!(
                    "Runtime node `{}` 已完成。\n[warning] {warning}",
                    result.node_path
                )
            } else {
                format!("Runtime node `{}` 已完成。", result.node_path)
            };
            Ok(AgentToolResult {
                content: vec![ContentPart::text(message)],
                is_error: false,
                details: Some(serde_json::json!({
                    "run_id": result.run.id,
                    "orchestration_id": result.orchestration_id,
                    "node_path": result.node_path,
                    "outcome": "completed",
                    "run_status": format!("{:?}", result.run.status),
                    "orchestration_warning": result.orchestration_warning,
                })),
            })
        }
    }
}
