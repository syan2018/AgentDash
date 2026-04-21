use std::sync::Arc;

use agentdash_domain::session_binding::SessionOwnerCtx;
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::mcp_preset::McpPresetRepository;
use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{
    LifecycleDefinitionRepository, LifecycleRunRepository, LifecycleStepExecutionStatus,
    WorkflowDefinitionRepository,
};
use agentdash_spi::hooks::SessionHookSnapshot;
use agentdash_spi::schema::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use agentdash_spi::{ExecutionContext, SessionHookRefreshQuery};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::capability::{AgentMcpServerEntry, CapabilityResolver, CapabilityResolverInput};
use crate::platform_config::SharedPlatformConfig;
use crate::session::SessionHub;
use crate::workflow::{
    CompleteLifecycleStepCommand, LifecycleOrchestrator, LifecycleRunService,
};

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
            current_turn_id: context.turn_id.clone(),
            hook_session: context.hook_session.clone(),
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

        let current_run = self
            .repos
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
            self.repos.lifecycle_run_repo
                .update(&run)
                .await
                .map_err(|e| {
                    AgentToolError::ExecutionFailed(format!("标记 Failed 失败: {e}"))
                })?;

            if let Some(summary) = &params.summary {
                crate::workflow::materialize_step_summary(
                    self.repos.inline_file_repo.as_ref(),
                    locator.run_id,
                    &locator.step_key,
                    summary,
                )
                .await;
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
            .repos
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
            let port_output_map = crate::workflow::load_port_output_map(
                self.repos.inline_file_repo.as_ref(),
                current_run.id,
            )
            .await;

            let missing: Vec<&String> = required_output_keys
                .iter()
                .filter(|key| !port_output_map.contains_key(key.as_str()))
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
                        self.repos.lifecycle_run_repo
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
                    self.repos.lifecycle_run_repo
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
            self.repos.lifecycle_definition_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
        );

        let run = service
            .complete_step(CompleteLifecycleStepCommand {
                run_id: locator.run_id,
                step_key: locator.step_key.clone(),
                summary: params.summary.clone(),
            })
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(format!("推进失败: {e}")))?;

        if let Some(summary) = &params.summary {
            crate::workflow::materialize_step_summary(
                self.repos.inline_file_repo.as_ref(),
                locator.run_id,
                &locator.step_key,
                summary,
            )
            .await;
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
                session_hub.clone(),
                self.repos.clone(),
                self.platform_config.clone(),
            );
            match orchestrator
                .after_node_advanced(run.id, run.project_id)
                .await
            {
                Ok(Some(result)) => {
                    if !result.activated_phase_nodes.is_empty() {
                        let snapshot = hook_session.snapshot();
                        let owner_ctx = resolve_owner_scope(&snapshot, run.project_id);
                        for phase in &result.activated_phase_nodes {
                            // 新模型：phase node 的 baseline 来自 workflow.contract.capabilities。
                            // step 不再承担能力指令；Add/Remove 由 hook runtime 动态处理（out of scope）。
                            let new_caps_set: std::collections::BTreeSet<String> =
                                phase.baseline_capabilities.iter().cloned().collect();
                            if let Some(delta) =
                                hook_session.update_capabilities(new_caps_set.clone())
                            {
                                let runtime_mcp_servers = session_hub
                                    .get_runtime_mcp_servers(hook_session.session_id())
                                    .await;
                                let available_presets =
                                    crate::session::load_available_presets(&self.repos, run.project_id).await;
                                let cap_output = CapabilityResolver::resolve(
                                    &CapabilityResolverInput {
                                        owner_ctx,
                                        agent_declared_capabilities: None,
                                        workflow_ctx: crate::capability::SessionWorkflowContext {
                                            has_active_workflow: true,
                                            workflow_capabilities: Some(
                                                new_caps_set
                                                    .iter()
                                                    .cloned()
                                                    .collect(),
                                            ),
                                        },
                                        agent_mcp_servers: mcp_entries_from_servers(
                                            &runtime_mcp_servers,
                                        ),
                                        available_presets,
                                        companion_slice_mode: None,
                                    },
                                    &self.platform_config,
                                );
                                let mut next_mcp_servers: Vec<agent_client_protocol::McpServer> =
                                    cap_output
                                        .platform_mcp_configs
                                        .iter()
                                        .map(|cfg| cfg.to_acp_mcp_server())
                                        .collect();
                                next_mcp_servers
                                    .extend(cap_output.custom_mcp_servers.into_iter());
                                dedupe_mcp_servers(&mut next_mcp_servers);

                                if let Err(error) = session_hub
                                    .replace_runtime_mcp_servers(
                                        hook_session.session_id(),
                                        next_mcp_servers.clone(),
                                    )
                                    .await
                                {
                                    tracing::warn!(
                                        session_id = %hook_session.session_id(),
                                        phase_node = %phase.node_key,
                                        error = %error,
                                        "Phase node MCP 热更新失败"
                                    );
                                }

                                let added = delta.added.clone();
                                let removed = delta.removed.clone();
                                tracing::info!(
                                    phase_node = %phase.node_key,
                                    added = ?added,
                                    removed = ?removed,
                                    "Phase node capability delta detected"
                                );

                                // 将结构化 delta Markdown 注入到 agent 的 steering 队列，
                                // 让 LLM 在对话层面显式感知能力变化。
                                let delta_md = crate::capability::build_capability_delta_markdown(
                                    &phase.node_key,
                                    &delta,
                                    &new_caps_set,
                                );
                                if let Err(error) = session_hub
                                    .push_session_notification(
                                        hook_session.session_id(),
                                        delta_md,
                                    )
                                    .await
                                {
                                    tracing::warn!(
                                        session_id = %hook_session.session_id(),
                                        phase_node = %phase.node_key,
                                        error = %error,
                                        "Phase node capability delta 消息注入失败"
                                    );
                                }

                                session_hub
                                    .emit_capability_changed_hook(
                                        hook_session.session_id(),
                                        Some(self.current_turn_id.as_str()),
                                        serde_json::json!({
                                            "phase_node": phase.node_key,
                                            "added": added,
                                            "removed": removed,
                                            "capabilities": new_caps_set.iter().cloned().collect::<Vec<_>>(),
                                            "mcp_server_count": next_mcp_servers.len(),
                                        }),
                                    )
                                    .await;
                            }
                        }
                    }
                    None
                }
                Ok(None) => None,
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

fn resolve_owner_scope(
    snapshot: &SessionHookSnapshot,
    fallback_project_id: Uuid,
) -> SessionOwnerCtx {
    let Some(owner) = snapshot.owners.first() else {
        return SessionOwnerCtx::Project {
            project_id: fallback_project_id,
        };
    };

    let project_id = owner
        .project_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .unwrap_or(fallback_project_id);
    let story_id = owner
        .story_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok());
    let task_id = owner
        .task_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok());

    // 按 owner_type + 关联 ID 还原合法 sum type;若关联 ID 缺失则向上降级。
    match owner.owner_type {
        agentdash_domain::session_binding::SessionOwnerType::Task => match (story_id, task_id) {
            (Some(story_id), Some(task_id)) => SessionOwnerCtx::Task {
                project_id,
                story_id,
                task_id,
            },
            (Some(story_id), None) => SessionOwnerCtx::Story {
                project_id,
                story_id,
            },
            _ => SessionOwnerCtx::Project { project_id },
        },
        agentdash_domain::session_binding::SessionOwnerType::Story => match story_id {
            Some(story_id) => SessionOwnerCtx::Story {
                project_id,
                story_id,
            },
            None => SessionOwnerCtx::Project { project_id },
        },
        agentdash_domain::session_binding::SessionOwnerType::Project => {
            SessionOwnerCtx::Project { project_id }
        }
    }
}

fn mcp_entries_from_servers(
    servers: &[agent_client_protocol::McpServer],
) -> Vec<AgentMcpServerEntry> {
    servers
        .iter()
        .filter_map(|server| {
            let name = match server {
                agent_client_protocol::McpServer::Http(http) => http.name.clone(),
                agent_client_protocol::McpServer::Sse(sse) => sse.name.clone(),
                agent_client_protocol::McpServer::Stdio(stdio) => stdio.name.clone(),
                _ => return None,
            };
            Some(AgentMcpServerEntry {
                name,
                server: server.clone(),
            })
        })
        .collect()
}

fn mcp_server_name(server: &agent_client_protocol::McpServer) -> Option<&str> {
    match server {
        agent_client_protocol::McpServer::Http(http) => Some(http.name.as_str()),
        agent_client_protocol::McpServer::Sse(sse) => Some(sse.name.as_str()),
        agent_client_protocol::McpServer::Stdio(stdio) => Some(stdio.name.as_str()),
        _ => None,
    }
}

fn dedupe_mcp_servers(servers: &mut Vec<agent_client_protocol::McpServer>) {
    let mut seen = std::collections::BTreeSet::<String>::new();
    servers.retain(|server| {
        let Some(name) = mcp_server_name(server) else {
            return true;
        };
        seen.insert(name.to_string())
    });
}

