//! LifecycleOrchestrator — Orchestration runtime terminal bridge
//!
//! 职责：把 runtime node 子 session 的 terminal 事件与
//! `complete_lifecycle_node` 工具提交转换成 OrchestrationRuntimeEvent，
//! 再交给 common orchestration reducer 推进。
//!
//! 不维护自己的状态 — 所有状态读写都通过 LifecycleRun / session services。
//! 不是后台进程 — 通过事件驱动（advance tool / session terminal）被调用。
//!
//! 实现 lifecycle terminal convergence port，由 AgentRun control effect executor 在
//! delivery terminal 收敛后调用。

use std::sync::Arc;

use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository;
use agentdash_application_workflow::orchestration::{
    OrchestrationRuntimeError, OrchestrationRuntimeEvent, apply_orchestration_event_to_run,
};
use agentdash_application_workflow::{
    OrchestrationExecutorDrainResult, OrchestrationExecutorLauncher,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    NodePortValue, RuntimeNodeError, RuntimeNodeStatus, WorkflowSessionTerminalState,
};
use agentdash_spi::hooks::{HookRuntimeRefreshQuery, RuntimeAdapterProvenance, SharedHookRuntime};
use agentdash_spi::{FunctionRunner, PlatformToolExecutionContext};
use uuid::Uuid;

use crate::lifecycle::execution_log::{RuntimeNodeArtifactScope, load_scoped_port_output_map};
use crate::lifecycle::session_association::resolve_activity_runtime_association_from_message_stream_trace;
use crate::lifecycle::session_terminal_summary;

#[async_trait::async_trait]
pub trait LifecycleTerminalConvergencePort: Send + Sync + 'static {
    async fn observe_lifecycle_terminal(
        &self,
        session_id: &str,
        terminal_state: &str,
    ) -> Result<(), String>;
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
    pub owner: PlatformToolExecutionContext,
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
    deps: LifecycleOrchestratorDeps,
    function_runner: Option<Arc<dyn FunctionRunner>>,
}

#[derive(Clone)]
pub struct LifecycleOrchestratorDeps {
    pub run_repo: Arc<dyn LifecycleRunRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    pub inline_file_repo: Arc<dyn InlineFileRepository>,
    pub orchestration_launcher: OrchestrationExecutorLauncher,
}

impl LifecycleOrchestrator {
    pub fn new(deps: LifecycleOrchestratorDeps) -> Self {
        Self {
            deps,
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
            self.deps.frame_repo.as_ref(),
            self.deps.agent_repo.as_ref(),
            self.deps.run_repo.as_ref(),
            Some(self.deps.binding_repo.as_ref()),
        )
        .await
        .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let Some(status) = runtime_node_terminal_status(terminal_state) else {
            return Ok(None);
        };

        diag!(
            Info,
            Subsystem::Lifecycle,
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
        self.deps
            .run_repo
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
        let run = self.load_run(input.owner.run_id).await?;
        if run.project_id != input.owner.project_id {
            return Err("Platform Tool owner project 与 LifecycleRun 不一致".to_string());
        }
        let orchestration_id = input.owner.orchestration_id.ok_or_else(|| {
            "Platform Tool owner context 缺少 orchestration_id，无法推进 lifecycle node".to_string()
        })?;
        let node_path = input.owner.node_path.clone().ok_or_else(|| {
            "Platform Tool owner context 缺少 node_path，无法推进 lifecycle node".to_string()
        })?;
        let attempt = input.owner.node_attempt.ok_or_else(|| {
            "Platform Tool owner context 缺少 node_attempt，无法推进 lifecycle node".to_string()
        })?;
        let node = run
            .orchestrations
            .iter()
            .find(|orchestration| orchestration.orchestration_id == orchestration_id)
            .and_then(|orchestration| {
                find_runtime_node_for_association(&orchestration.node_tree, &node_path, attempt)
            })
            .ok_or_else(|| {
                "Platform Tool owner context 指向的 lifecycle runtime node 不存在".to_string()
            })?;
        if let Some(executor_run_ref) = node.executor_run_ref.as_ref() {
            let matches_presentation = matches!(
                executor_run_ref,
                agentdash_domain::workflow::ExecutorRunRef::RuntimeSession { session_id }
                    if session_id == input.owner.presentation_thread_id.as_str()
            );
            if !matches_presentation {
                return Err(
                    "Platform Tool owner presentation thread 与 lifecycle runtime node 不一致"
                        .to_string(),
                );
            }
        }

        let status = if input.outcome == LifecycleNodeAdvanceOutcome::Failed {
            RuntimeNodeStatus::Failed
        } else {
            RuntimeNodeStatus::Completed
        };
        let outputs = if status == RuntimeNodeStatus::Completed {
            self.load_runtime_node_outputs(run.id, orchestration_id, &node_path, attempt)
                .await?
        } else {
            Vec::new()
        };
        let event = runtime_terminal_event(
            node_path.clone(),
            attempt,
            status,
            outputs,
            input.summary.clone(),
        );
        let run_before = run.clone();
        let updated_run = match apply_orchestration_event_to_run(run, orchestration_id, event) {
            Ok((run, _outcome)) => run,
            Err(OrchestrationRuntimeError::CompletionPolicyRejected {
                missing_output_ports,
                ..
            }) if input.outcome == LifecycleNodeAdvanceOutcome::Completed => {
                return Ok(AdvanceCurrentNodeResult {
                    run: run_before,
                    orchestration_id,
                    node_path,
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
                    orchestration_id,
                    node_path,
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
        self.deps
            .run_repo
            .update(&updated_run)
            .await
            .map_err(|error| format!("更新 LifecycleRun orchestration 失败: {error}"))?;
        let drain_result = self.drain_ready_nodes(updated_run.id).await?;
        self.refresh_hook_snapshot(&input.hook_runtime, &input.turn_id)
            .await?;

        let final_run = self.load_run(updated_run.id).await?;
        Ok(AdvanceCurrentNodeResult {
            run: final_run,
            orchestration_id,
            node_path,
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
    ) -> Result<OrchestrationExecutorDrainResult, String> {
        let mut launcher = self.deps.orchestration_launcher.clone();
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
        self.deps
            .run_repo
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
            load_scoped_port_output_map(self.deps.inline_file_repo.as_ref(), &scope).await;
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
impl LifecycleTerminalConvergencePort for LifecycleOrchestrator {
    async fn observe_lifecycle_terminal(
        &self,
        session_id: &str,
        terminal_state: &str,
    ) -> Result<(), String> {
        match self.on_session_terminal(session_id, terminal_state).await {
            Ok(Some(result)) => {
                diag!(
                    Info,
                    Subsystem::Lifecycle,
                    run_id = %result.run_id,
                    activated = ?result.activated_nodes.iter().map(|n| &n.node_key).collect::<Vec<_>>(),
                    "Orchestrator callback: activated successor activities"
                );
            }
            Ok(None) => {}
            Err(e) => {
                return Err(e);
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl agentdash_application_ports::agent_run_control_effect::AgentRunLifecycleTerminalConvergencePort
    for LifecycleOrchestrator
{
    async fn observe_lifecycle_terminal(
        &self,
        presentation_thread_id: &agentdash_agent_runtime_contract::PresentationThreadId,
        terminal: agentdash_agent_runtime_contract::RuntimeTurnTerminal,
    ) -> Result<(), String> {
        let terminal_state = match terminal {
            agentdash_agent_runtime_contract::RuntimeTurnTerminal::Completed => "completed",
            agentdash_agent_runtime_contract::RuntimeTurnTerminal::Interrupted => "interrupted",
            agentdash_agent_runtime_contract::RuntimeTurnTerminal::Refused
            | agentdash_agent_runtime_contract::RuntimeTurnTerminal::LimitReached
            | agentdash_agent_runtime_contract::RuntimeTurnTerminal::Failed
            | agentdash_agent_runtime_contract::RuntimeTurnTerminal::Lost => "failed",
        };
        LifecycleTerminalConvergencePort::observe_lifecycle_terminal(
            self,
            presentation_thread_id.as_str(),
            terminal_state,
        )
        .await
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

fn orchestration_warning_from_drain(result: &OrchestrationExecutorDrainResult) -> Option<String> {
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
