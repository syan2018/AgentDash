//! LifecycleOrchestrator — Orchestration runtime terminal bridge
//!
//! 职责：把 runtime node 子 session 的 terminal 事件与
//! `complete_lifecycle_node` 工具提交转换成 OrchestrationRuntimeEvent，
//! 再交给 common orchestration reducer 推进。
//!
//! 不维护自己的状态 — 所有状态读写都通过 LifecycleRun / session services。
//! 不是后台进程 — 通过事件驱动（advance tool / session terminal）被调用。
//!
//! 实现 `SessionTerminalCallback`，由 `session runtime` 在 session 完全终止后自动调用。

use std::sync::Arc;

use agentdash_domain::workflow::{
    LifecycleRun, NodePortValue, RuntimeNodeError, RuntimeNodeStatus, WorkflowSessionTerminalState,
};
use agentdash_spi::FunctionRunner;
use agentdash_spi::hooks::{HookRuntimeRefreshQuery, RuntimeAdapterProvenance, SharedHookRuntime};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{RepositorySet, SharedPlatformConfig};

use super::session_association::resolve_activity_runtime_association_from_message_stream_trace;
use crate::lifecycle::execution_log::{RuntimeNodeArtifactScope, load_scoped_port_output_map};
use crate::lifecycle::session_terminal_summary;
use crate::workflow::orchestration::{
    OrchestrationExecutorLauncher, OrchestrationRuntimeError, OrchestrationRuntimeEvent,
    apply_orchestration_event_to_run,
};

#[async_trait::async_trait]
pub trait SessionTerminalCallback: Send + Sync + 'static {
    async fn on_session_terminal(&self, session_id: &str, terminal_state: &str);
}

#[derive(Debug)]
pub struct OrchestrationResult {
    pub run_id: Uuid,
    pub activated_nodes: Vec<ActivatedNode>,
}

#[derive(Debug)]
pub struct ActivatedNode {
    pub node_key: String,
    pub runtime_session_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleNodeAdvanceOutcome {
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct AdvanceCurrentActivityInput {
    pub hook_runtime: SharedHookRuntime,
    pub turn_id: String,
    pub runtime_session_id: String,
    pub outcome: LifecycleNodeAdvanceOutcome,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AdvanceCurrentNodeStatus {
    Completed,
    Failed,
    GateRejected {
        gate_collision_count: u32,
        missing_output_keys: Vec<String>,
        terminal_failed: bool,
    },
}

#[derive(Debug, Clone)]
pub struct AdvanceCurrentNodeResult {
    pub run: LifecycleRun,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub status: AdvanceCurrentNodeStatus,
    pub orchestration_warning: Option<String>,
}

pub struct LifecycleOrchestrator {
    repos: RepositorySet,
    platform_config: SharedPlatformConfig,
    function_runner: Option<Arc<dyn FunctionRunner>>,
}

impl LifecycleOrchestrator {
    pub fn new_with_platform_config(
        repos: RepositorySet,
        platform_config: SharedPlatformConfig,
    ) -> Self {
        Self {
            repos,
            platform_config,
            function_runner: None,
        }
    }

    pub fn with_function_runner(mut self, function_runner: Arc<dyn FunctionRunner>) -> Self {
        self.function_runner = Some(function_runner);
        self
    }

    /// 当某个 session 进入 terminal 状态时调用。
    ///
    /// 通过 RuntimeSession trace 反查 AgentFrame / Assignment，
    /// 若是，则评估后继 node 并启动新 session。
    pub async fn on_session_terminal(
        &self,
        session_id: &str,
        terminal_state: &str,
    ) -> Result<Option<OrchestrationResult>, String> {
        if let Some(result) = self
            .on_activity_session_terminal(session_id, terminal_state)
            .await?
        {
            return Ok(Some(result));
        }
        Ok(None)
    }

    async fn on_activity_session_terminal(
        &self,
        session_id: &str,
        terminal_state: &str,
    ) -> Result<Option<OrchestrationResult>, String> {
        let Some(association) = resolve_activity_runtime_association_from_message_stream_trace(
            session_id,
            self.repos.agent_frame_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            Some(self.repos.execution_anchor_repo.as_ref()),
        )
        .await
        .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let Some(status) = runtime_node_terminal_status(terminal_state) else {
            return Ok(None);
        };

        info!(
            run_id = %association.run.id,
            orchestration_id = %association.orchestration_id,
            node_path = %association.node_path,
            attempt = association.attempt,
            terminal_state = terminal_state,
            "Orchestrator: runtime session terminal, materializing orchestration node"
        );

        let outputs = if status == RuntimeNodeStatus::Completed
            && !association_node_is_terminal(
                &association.run,
                association.orchestration_id,
                &association.node_path,
                association.attempt,
            ) {
            self.load_runtime_node_outputs(
                association.run.id,
                association.orchestration_id,
                &association.node_path,
                association.attempt,
            )
            .await?
        } else {
            Vec::new()
        };
        let terminal_summary =
            session_terminal_summary(workflow_terminal_state_from_str(terminal_state), None);
        let event = runtime_terminal_event(
            association.node_path.clone(),
            association.attempt,
            status,
            outputs,
            Some(terminal_summary),
        );
        let (run, _outcome) =
            apply_orchestration_event_to_run(association.run, association.orchestration_id, event)
                .map_err(|error| error.to_string())?;
        self.repos
            .lifecycle_run_repo
            .update(&run)
            .await
            .map_err(|error| format!("更新 LifecycleRun orchestration 失败: {error}"))?;
        let drain_result = self.drain_ready_nodes(run.id).await?;

        Ok(Some(OrchestrationResult {
            run_id: run.id,
            activated_nodes: drain_result
                .launched_agent_nodes
                .into_iter()
                .map(|node| ActivatedNode {
                    node_key: node.node_path,
                    runtime_session_id: node.runtime_session_id,
                })
                .collect(),
        }))
    }

    pub async fn advance_current_activity(
        &self,
        input: AdvanceCurrentActivityInput,
    ) -> Result<AdvanceCurrentNodeResult, String> {
        let Some(association) = resolve_activity_runtime_association_from_message_stream_trace(
            &input.runtime_session_id,
            self.repos.agent_frame_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            Some(self.repos.execution_anchor_repo.as_ref()),
        )
        .await
        .map_err(|error| error.to_string())?
        else {
            return Err("当前 runtime session 没有关联 lifecycle runtime node".to_string());
        };

        let status = if input.outcome == LifecycleNodeAdvanceOutcome::Failed {
            RuntimeNodeStatus::Failed
        } else {
            RuntimeNodeStatus::Completed
        };
        let outputs = if status == RuntimeNodeStatus::Completed {
            self.load_runtime_node_outputs(
                association.run.id,
                association.orchestration_id,
                &association.node_path,
                association.attempt,
            )
            .await?
        } else {
            Vec::new()
        };
        let event = runtime_terminal_event(
            association.node_path.clone(),
            association.attempt,
            status,
            outputs,
            input.summary.clone(),
        );
        let run_before = association.run.clone();
        let updated_run = match apply_orchestration_event_to_run(
            association.run,
            association.orchestration_id,
            event,
        ) {
            Ok((run, _outcome)) => run,
            Err(OrchestrationRuntimeError::CompletionPolicyRejected {
                missing_output_ports,
                ..
            }) if input.outcome == LifecycleNodeAdvanceOutcome::Completed => {
                return Ok(AdvanceCurrentNodeResult {
                    run: run_before,
                    orchestration_id: association.orchestration_id,
                    node_path: association.node_path,
                    status: AdvanceCurrentNodeStatus::GateRejected {
                        gate_collision_count: 1,
                        missing_output_keys: missing_output_ports,
                        terminal_failed: false,
                    },
                    orchestration_warning: None,
                });
            }
            Err(OrchestrationRuntimeError::StateExchangeMissingOutput { from_port, .. })
                if input.outcome == LifecycleNodeAdvanceOutcome::Completed =>
            {
                return Ok(AdvanceCurrentNodeResult {
                    run: run_before,
                    orchestration_id: association.orchestration_id,
                    node_path: association.node_path,
                    status: AdvanceCurrentNodeStatus::GateRejected {
                        gate_collision_count: 1,
                        missing_output_keys: vec![from_port],
                        terminal_failed: false,
                    },
                    orchestration_warning: None,
                });
            }
            Err(error) => return Err(error.to_string()),
        };
        self.repos
            .lifecycle_run_repo
            .update(&updated_run)
            .await
            .map_err(|error| format!("更新 LifecycleRun orchestration 失败: {error}"))?;
        let drain_result = self.drain_ready_nodes(updated_run.id).await?;
        self.refresh_hook_snapshot(&input.hook_runtime, &input.turn_id)
            .await?;

        let final_run = self.load_run(updated_run.id).await?;
        Ok(AdvanceCurrentNodeResult {
            run: final_run,
            orchestration_id: association.orchestration_id,
            node_path: association.node_path,
            status: if input.outcome == LifecycleNodeAdvanceOutcome::Failed {
                AdvanceCurrentNodeStatus::Failed
            } else {
                AdvanceCurrentNodeStatus::Completed
            },
            orchestration_warning: orchestration_warning_from_drain(&drain_result),
        })
    }

    async fn drain_ready_nodes(
        &self,
        run_id: Uuid,
    ) -> Result<crate::workflow::OrchestrationExecutorDrainResult, String> {
        let mut launcher = OrchestrationExecutorLauncher::new_with_platform_config(
            self.repos.clone(),
            self.platform_config.clone(),
        );
        if let Some(function_runner) = &self.function_runner {
            launcher = launcher.with_function_runner(function_runner.clone());
        }
        launcher
            .drain_ready_nodes(run_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn refresh_hook_snapshot(
        &self,
        hook_runtime: &SharedHookRuntime,
        turn_id: &str,
    ) -> Result<(), String> {
        self.refresh_hook_snapshot_for_turn(
            hook_runtime,
            Some(turn_id),
            "tool:complete_lifecycle_node",
        )
        .await
    }

    async fn refresh_hook_snapshot_for_turn(
        &self,
        hook_runtime: &SharedHookRuntime,
        turn_id: Option<&str>,
        reason: &'static str,
    ) -> Result<(), String> {
        hook_runtime
            .refresh_from_provenance(HookRuntimeRefreshQuery {
                provenance: RuntimeAdapterProvenance::runtime_session(
                    hook_runtime.session_id().to_string(),
                    turn_id.map(ToString::to_string),
                    "workflow_orchestrator_hook_refresh",
                ),
                reason: Some(reason.to_string()),
            })
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn load_run(&self, run_id: Uuid) -> Result<LifecycleRun, String> {
        self.repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(|e| format!("加载 lifecycle run 失败: {e}"))?
            .ok_or_else(|| format!("lifecycle run 不存在: {run_id}"))
    }

    async fn load_runtime_node_outputs(
        &self,
        run_id: Uuid,
        orchestration_id: Uuid,
        node_path: &str,
        attempt: u32,
    ) -> Result<Vec<NodePortValue>, String> {
        let scope = RuntimeNodeArtifactScope {
            run_id,
            orchestration_id,
            node_path: node_path.to_string(),
            attempt,
        };
        let output_map =
            load_scoped_port_output_map(self.repos.inline_file_repo.as_ref(), &scope).await;
        output_map
            .into_iter()
            .map(|(port_key, content)| {
                let value = serde_json::from_str(&content)
                    .unwrap_or_else(|_| serde_json::Value::String(content));
                Ok(NodePortValue { port_key, value })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl SessionTerminalCallback for LifecycleOrchestrator {
    async fn on_session_terminal(&self, session_id: &str, terminal_state: &str) {
        match self.on_session_terminal(session_id, terminal_state).await {
            Ok(Some(result)) => {
                info!(
                    run_id = %result.run_id,
                    activated = ?result.activated_nodes.iter().map(|n| &n.node_key).collect::<Vec<_>>(),
                    "Orchestrator callback: activated successor activities"
                );
            }
            Ok(None) => {}
            Err(e) => {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "Orchestrator callback failed"
                );
            }
        }
    }
}

fn runtime_node_terminal_status(terminal_state: &str) -> Option<RuntimeNodeStatus> {
    match terminal_state {
        "completed" | "succeeded" | "success" => Some(RuntimeNodeStatus::Completed),
        "failed" | "error" => Some(RuntimeNodeStatus::Failed),
        "interrupted" | "cancelled" | "canceled" => Some(RuntimeNodeStatus::Cancelled),
        _ => None,
    }
}

fn workflow_terminal_state_from_str(terminal_state: &str) -> WorkflowSessionTerminalState {
    match terminal_state {
        "completed" | "succeeded" | "success" => WorkflowSessionTerminalState::Completed,
        "interrupted" | "cancelled" | "canceled" => WorkflowSessionTerminalState::Interrupted,
        _ => WorkflowSessionTerminalState::Failed,
    }
}

fn runtime_terminal_event(
    node_path: String,
    attempt: u32,
    status: RuntimeNodeStatus,
    outputs: Vec<NodePortValue>,
    summary: Option<String>,
) -> OrchestrationRuntimeEvent {
    let timestamp = chrono::Utc::now();
    match status {
        RuntimeNodeStatus::Completed => OrchestrationRuntimeEvent::NodeCompleted {
            node_path,
            attempt,
            outputs,
            timestamp,
        },
        RuntimeNodeStatus::Cancelled => OrchestrationRuntimeEvent::NodeCancelled {
            node_path,
            attempt,
            reason: summary,
            timestamp,
        },
        RuntimeNodeStatus::Failed => OrchestrationRuntimeEvent::NodeFailed {
            node_path,
            attempt,
            error: RuntimeNodeError {
                code: "runtime_session_terminal_failed".to_string(),
                message: summary.unwrap_or_else(|| "runtime session failed".to_string()),
                retryable: false,
                detail: None,
            },
            timestamp,
        },
        _ => OrchestrationRuntimeEvent::NodeCancelled {
            node_path,
            attempt,
            reason: Some(format!("unsupported terminal status: {status:?}")),
            timestamp,
        },
    }
}

fn association_node_is_terminal(
    run: &LifecycleRun,
    orchestration_id: Uuid,
    node_path: &str,
    attempt: u32,
) -> bool {
    run.orchestrations
        .iter()
        .find(|orchestration| orchestration.orchestration_id == orchestration_id)
        .and_then(|orchestration| {
            find_runtime_node_for_association(&orchestration.node_tree, node_path, attempt)
        })
        .is_some_and(|node| {
            matches!(
                node.status,
                RuntimeNodeStatus::Completed
                    | RuntimeNodeStatus::Failed
                    | RuntimeNodeStatus::Cancelled
                    | RuntimeNodeStatus::Skipped
            )
        })
}

fn orchestration_warning_from_drain(
    result: &crate::workflow::OrchestrationExecutorDrainResult,
) -> Option<String> {
    if result.failed_nodes.is_empty() {
        return None;
    }
    Some(format!(
        "后继 runtime node 启动失败: {}",
        result.failed_nodes.join(", ")
    ))
}

fn find_runtime_node_for_association<'a>(
    nodes: &'a [agentdash_domain::workflow::RuntimeNodeState],
    node_path: &str,
    attempt: u32,
) -> Option<&'a agentdash_domain::workflow::RuntimeNodeState> {
    for node in nodes {
        if node.node_path == node_path && node.attempt == attempt {
            return Some(node);
        }
        if let Some(found) = find_runtime_node_for_association(&node.children, node_path, attempt) {
            return Some(found);
        }
    }
    None
}
