//! LifecycleOrchestrator — Activity runtime event bridge
//!
//! 职责：把 Activity executor 子 session 的 terminal 事件与
//! `complete_lifecycle_node` 工具提交转换成 ActivityEvent，再交给
//! LifecycleEngine 与 durable scheduler 统一推进。
//!
//! 不维护自己的状态 — 所有状态读写都通过 LifecycleRun / SessionBinding / session services。
//! 不是后台进程 — 通过事件驱动（advance tool / session terminal）被调用。
//!
//! 实现 `SessionTerminalCallback`，由 `session runtime` 在 session 完全终止后自动调用。

use std::sync::Arc;

use agentdash_domain::workflow::{
    ActivityCompletionPolicy, ActivityPortValue, ExecutorRunRef, LifecycleRun,
    WorkflowSessionTerminalState,
};
use agentdash_spi::FunctionRunner;
use agentdash_spi::hooks::{SessionHookRefreshQuery, SharedHookSessionRuntime};
use tracing::{info, warn};
use uuid::Uuid;

use crate::platform_config::SharedPlatformConfig;
use crate::repository_set::RepositorySet;

use super::session_association::resolve_activity_session_association;
use crate::session::SessionTerminalCallback;
use crate::session::{
    SessionCapabilityService, SessionCoreService, SessionHookService, SessionLaunchService,
};
use crate::workflow::{
    ActivityEvent, ActivityLifecycleRunService, AgentActivityExecutorLauncher,
    AgentActivityLaunchContext, AgentActivityRuntimePort, load_port_output_map,
    session_terminal_summary,
};

#[derive(Debug)]
pub struct OrchestrationResult {
    pub run_id: Uuid,
    pub activated_nodes: Vec<ActivatedNode>,
}

#[derive(Debug)]
pub struct ActivatedNode {
    pub node_key: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleNodeAdvanceOutcome {
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct AdvanceCurrentActivityInput {
    pub hook_session: SharedHookSessionRuntime,
    pub turn_id: String,
    pub session_id: String,
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
    pub step_key: String,
    pub status: AdvanceCurrentNodeStatus,
    pub orchestration_warning: Option<String>,
}

pub struct LifecycleOrchestrator {
    session_core: SessionCoreService,
    session_launch: SessionLaunchService,
    session_hooks: SessionHookService,
    session_capability: SessionCapabilityService,
    repos: RepositorySet,
    platform_config: SharedPlatformConfig,
    function_runner: Arc<dyn FunctionRunner>,
}

impl LifecycleOrchestrator {
    pub fn new(
        session_core: SessionCoreService,
        session_launch: SessionLaunchService,
        session_hooks: SessionHookService,
        session_capability: SessionCapabilityService,
        repos: RepositorySet,
        platform_config: SharedPlatformConfig,
        function_runner: Arc<dyn FunctionRunner>,
    ) -> Self {
        Self {
            session_core,
            session_launch,
            session_hooks,
            session_capability,
            repos,
            platform_config,
            function_runner,
        }
    }

    /// 当某个 session 进入 terminal 状态时调用。
    ///
    /// 通过 SessionBinding label 判断该 session 是否归属某个 lifecycle node，
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
            self.repos.lifecycle_run_repo.as_ref(),
        )
        .await?
        else {
            return Ok(None);
        };
        let Some(event) = activity_terminal_event(
            terminal_state,
            &association.activity_key,
            association.attempt,
        ) else {
            return Ok(None);
        };

        info!(
            run_id = %association.run.id,
            activity_key = %association.activity_key,
            attempt = association.attempt,
            terminal_state = terminal_state,
            "Orchestrator: activity session terminal, applying ActivityEvent"
        );

        let service = ActivityLifecycleRunService::new(
            self.repos.activity_lifecycle_definition_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.activity_execution_claim_repo.as_ref(),
        );
        let run = service
            .apply_event(association.run.id, event)
            .await
            .map_err(|error| format!("推进 activity lifecycle run 失败: {error}"))?;
        let activated_nodes = self.launch_ready_activity_attempts(&run).await?;

        Ok(Some(OrchestrationResult {
            run_id: run.id,
            activated_nodes,
        }))
    }

    pub async fn advance_current_activity(
        &self,
        input: AdvanceCurrentActivityInput,
    ) -> Result<AdvanceCurrentNodeResult, String> {
        let Some(association) = resolve_activity_session_association(
            &input.session_id,
            self.repos.lifecycle_run_repo.as_ref(),
        )
        .await?
        else {
            return Err("当前 session 没有关联 lifecycle activity attempt".to_string());
        };

        let definition = self
            .repos
            .activity_lifecycle_definition_repo
            .get_by_id(association.run.lifecycle_id)
            .await
            .map_err(|error| format!("加载 activity lifecycle definition 失败: {error}"))?
            .ok_or_else(|| {
                format!(
                    "activity lifecycle definition 不存在: {}",
                    association.run.lifecycle_id
                )
            })?;
        let activity = definition
            .activities
            .iter()
            .find(|activity| activity.key == association.activity_key)
            .ok_or_else(|| format!("activity 不存在: {}", association.activity_key))?;

        let event = if input.outcome == LifecycleNodeAdvanceOutcome::Failed {
            ActivityEvent::ActivityFailed {
                activity_key: association.activity_key.clone(),
                attempt: association.attempt,
                error: input
                    .summary
                    .clone()
                    .unwrap_or_else(|| "agent 主动标记 activity failed".to_string()),
            }
        } else {
            let port_output_map =
                load_port_output_map(self.repos.inline_file_repo.as_ref(), association.run.id)
                    .await;
            let required_output_keys = match &activity.completion_policy {
                ActivityCompletionPolicy::OutputPorts { required_ports } => required_ports.clone(),
                _ => Vec::new(),
            };
            let missing_output_keys = required_output_keys
                .iter()
                .filter(|key| !port_output_map.contains_key(key.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if !missing_output_keys.is_empty() {
                return Ok(AdvanceCurrentNodeResult {
                    run: association.run,
                    step_key: association.activity_key,
                    status: AdvanceCurrentNodeStatus::GateRejected {
                        gate_collision_count: 0,
                        missing_output_keys,
                        terminal_failed: false,
                    },
                    orchestration_warning: None,
                });
            }
            let declared_output_keys = activity
                .output_ports
                .iter()
                .map(|port| port.key.as_str())
                .collect::<Vec<_>>();
            let outputs = port_output_map
                .into_iter()
                .filter(|(port_key, _)| declared_output_keys.contains(&port_key.as_str()))
                .map(|(port_key, content)| ActivityPortValue {
                    port_key,
                    value: serde_json::from_str(&content)
                        .unwrap_or_else(|_| serde_json::Value::String(content)),
                })
                .collect::<Vec<_>>();
            ActivityEvent::ActivityCompleted {
                activity_key: association.activity_key.clone(),
                attempt: association.attempt,
                outputs,
                summary: input.summary.clone(),
            }
        };

        let service = ActivityLifecycleRunService::new(
            self.repos.activity_lifecycle_definition_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.activity_execution_claim_repo.as_ref(),
        );
        let run = service
            .apply_event(association.run.id, event)
            .await
            .map_err(|error| format!("推进 activity lifecycle run 失败: {error}"))?;
        self.refresh_hook_snapshot(&input.hook_session, &input.turn_id)
            .await?;

        let orchestration_warning = if input.outcome == LifecycleNodeAdvanceOutcome::Completed {
            match self.launch_ready_activity_attempts(&run).await {
                Ok(_) => None,
                Err(error) => Some(format!(
                    "activity 已完成，但后继 executor 启动失败：{error}"
                )),
            }
        } else {
            None
        };
        let final_run = self.load_run(run.id).await?;
        Ok(AdvanceCurrentNodeResult {
            run: final_run,
            step_key: association.activity_key,
            status: if input.outcome == LifecycleNodeAdvanceOutcome::Failed {
                AdvanceCurrentNodeStatus::Failed
            } else {
                AdvanceCurrentNodeStatus::Completed
            },
            orchestration_warning,
        })
    }

    async fn launch_ready_activity_attempts(
        &self,
        run: &LifecycleRun,
    ) -> Result<Vec<ActivatedNode>, String> {
        let service = ActivityLifecycleRunService::new(
            self.repos.activity_lifecycle_definition_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.activity_execution_claim_repo.as_ref(),
        );
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id: run.project_id,
                lifecycle_key: String::new(),
                root_session_id: run.session_id.clone().unwrap_or_default(),
            },
            AgentActivityRuntimePort::new(
                self.session_core.clone(),
                self.session_launch.clone(),
                self.repos.clone(),
                self.function_runner.clone(),
            )
            .with_runtime_context(
                self.session_hooks.clone(),
                self.session_capability.clone(),
                self.platform_config.clone(),
            ),
        );
        let (_run, outcomes) = service
            .launch_ready_attempts(run.id, &launcher)
            .await
            .map_err(|error| format!("启动后继 activity executor 失败: {error}"))?;
        Ok(outcomes
            .into_iter()
            .filter_map(|outcome| {
                if !outcome.started {
                    return None;
                }
                match outcome.claim.executor_run_ref {
                    Some(ExecutorRunRef::AgentSession { session_id }) => Some(ActivatedNode {
                        node_key: outcome.claim.activity_key,
                        session_id,
                    }),
                    _ => None,
                }
            })
            .collect())
    }

    async fn refresh_hook_snapshot(
        &self,
        hook_session: &SharedHookSessionRuntime,
        turn_id: &str,
    ) -> Result<(), String> {
        self.refresh_hook_snapshot_for_turn(
            hook_session,
            Some(turn_id),
            "tool:complete_lifecycle_node",
        )
        .await
    }

    async fn refresh_hook_snapshot_for_turn(
        &self,
        hook_session: &SharedHookSessionRuntime,
        turn_id: Option<&str>,
        reason: &'static str,
    ) -> Result<(), String> {
        hook_session
            .refresh(SessionHookRefreshQuery {
                session_id: hook_session.session_id().to_string(),
                turn_id: turn_id.map(ToString::to_string),
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

fn activity_terminal_event(
    terminal_state: &str,
    activity_key: &str,
    attempt: u32,
) -> Option<ActivityEvent> {
    match terminal_state {
        "completed" => Some(ActivityEvent::ActivityCompleted {
            activity_key: activity_key.to_string(),
            attempt,
            outputs: Vec::new(),
            summary: Some(session_terminal_summary(
                WorkflowSessionTerminalState::Completed,
                None,
            )),
        }),
        "failed" => Some(ActivityEvent::ActivityFailed {
            activity_key: activity_key.to_string(),
            attempt,
            error: session_terminal_summary(WorkflowSessionTerminalState::Failed, None),
        }),
        "interrupted" => Some(ActivityEvent::ActivityFailed {
            activity_key: activity_key.to_string(),
            attempt,
            error: session_terminal_summary(WorkflowSessionTerminalState::Interrupted, None),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_terminal_failed_maps_to_failed_event() {
        assert_eq!(
            activity_terminal_event("failed", "plan", 1),
            Some(ActivityEvent::ActivityFailed {
                activity_key: "plan".to_string(),
                attempt: 1,
                error: "关联 session 以失败终态结束".to_string(),
            })
        );
    }

    #[test]
    fn activity_terminal_completed_maps_to_completed_event() {
        assert_eq!(
            activity_terminal_event("completed", "plan", 1),
            Some(ActivityEvent::ActivityCompleted {
                activity_key: "plan".to_string(),
                attempt: 1,
                outputs: Vec::new(),
                summary: Some("关联 session 已自然结束".to_string()),
            })
        );
    }
}
