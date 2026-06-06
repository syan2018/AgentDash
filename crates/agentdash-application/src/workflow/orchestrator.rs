//! LifecycleOrchestrator — Orchestration runtime terminal bridge
//!
//! 职责：把 Activity executor 子 session 的 terminal 事件与
//! `complete_lifecycle_node` 工具提交转换成 ActivityEvent，再交给
//! LifecycleEngine 与 durable scheduler 统一推进。
//!
//! 不维护自己的状态 — 所有状态读写都通过 LifecycleRun / session services。
//! 不是后台进程 — 通过事件驱动（advance tool / session terminal）被调用。
//!
//! 实现 `SessionTerminalCallback`，由 `session runtime` 在 session 完全终止后自动调用。

use agentdash_domain::workflow::{
    LifecycleRun, RuntimeNodeError, RuntimeNodeStatus, WorkflowSessionTerminalState,
};
use agentdash_spi::hooks::{HookRuntimeRefreshQuery, RuntimeAdapterProvenance, SharedHookRuntime};
use tracing::{info, warn};
use uuid::Uuid;

use crate::repository_set::RepositorySet;

use super::session_association::resolve_activity_session_association;
use crate::session::SessionTerminalCallback;
use crate::workflow::session_terminal_summary;

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
}

impl LifecycleOrchestrator {
    pub fn new(repos: RepositorySet) -> Self {
        Self { repos }
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
        let Some(association) = resolve_activity_session_association(
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

        let run = materialize_runtime_node_terminal(
            association.run,
            association.orchestration_id,
            &association.node_path,
            association.attempt,
            status,
            Some(session_terminal_summary(
                workflow_terminal_state_from_str(terminal_state),
                None,
            )),
        )?;
        self.repos
            .lifecycle_run_repo
            .update(&run)
            .await
            .map_err(|error| format!("更新 LifecycleRun orchestration 失败: {error}"))?;

        Ok(Some(OrchestrationResult {
            run_id: run.id,
            activated_nodes: Vec::new(),
        }))
    }

    pub async fn advance_current_activity(
        &self,
        input: AdvanceCurrentActivityInput,
    ) -> Result<AdvanceCurrentNodeResult, String> {
        let Some(association) = resolve_activity_session_association(
            &input.runtime_session_id,
            self.repos.agent_frame_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            Some(self.repos.execution_anchor_repo.as_ref()),
        )
        .await
        .map_err(|error| error.to_string())?
        else {
            return Err("当前 runtime session 没有关联 lifecycle activity attempt".to_string());
        };

        let status = if input.outcome == LifecycleNodeAdvanceOutcome::Failed {
            RuntimeNodeStatus::Failed
        } else {
            RuntimeNodeStatus::Completed
        };
        let updated_run = materialize_runtime_node_terminal(
            association.run,
            association.orchestration_id,
            &association.node_path,
            association.attempt,
            status,
            input.summary.clone(),
        )?;
        self.repos
            .lifecycle_run_repo
            .update(&updated_run)
            .await
            .map_err(|error| format!("更新 LifecycleRun orchestration 失败: {error}"))?;
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
            orchestration_warning: None,
        })
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

fn materialize_runtime_node_terminal(
    mut run: LifecycleRun,
    orchestration_id: Uuid,
    node_path: &str,
    attempt: u32,
    status: RuntimeNodeStatus,
    summary: Option<String>,
) -> Result<LifecycleRun, String> {
    let orchestration = run
        .orchestrations
        .iter_mut()
        .find(|orchestration| orchestration.orchestration_id == orchestration_id)
        .ok_or_else(|| format!("orchestration 不存在: {orchestration_id}"))?;
    let node = orchestration
        .node_tree
        .iter_mut()
        .find(|node| node.node_path == node_path && node.attempt == attempt)
        .ok_or_else(|| {
            format!(
                "orchestration node 不存在: orchestration_id={orchestration_id}, node_path={node_path}, attempt={attempt}"
            )
        })?;

    if matches!(
        node.status,
        RuntimeNodeStatus::Completed | RuntimeNodeStatus::Failed | RuntimeNodeStatus::Cancelled
    ) {
        return Ok(run);
    }

    node.status = status;
    node.completed_at = Some(chrono::Utc::now());
    if status == RuntimeNodeStatus::Failed {
        node.error = Some(RuntimeNodeError {
            code: "runtime_session_terminal_failed".to_string(),
            message: summary.unwrap_or_else(|| "runtime session failed".to_string()),
            retryable: false,
            detail: None,
        });
    }
    orchestration.updated_at = chrono::Utc::now();
    Ok(run)
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
