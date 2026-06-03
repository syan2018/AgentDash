//! LifecycleOrchestrator — Activity runtime event bridge
//!
//! 职责：把 Activity executor 子 session 的 terminal 事件与
//! `complete_lifecycle_node` 工具提交转换成 ActivityEvent，再交给
//! LifecycleEngine 与 durable scheduler 统一推进。
//!
//! 不维护自己的状态 — 所有状态读写都通过 LifecycleRun / session services。
//! 不是后台进程 — 通过事件驱动（advance tool / session terminal）被调用。
//!
//! 实现 `SessionTerminalCallback`，由 `session runtime` 在 session 完全终止后自动调用。

use std::sync::Arc;

use agentdash_domain::workflow::{
    ActivityCompletionPolicy, ActivityDefinition, ActivityPortValue, ExecutorRunRef, LifecycleRun,
    WorkflowGraphInstance, WorkflowSessionTerminalState,
};
use agentdash_spi::FunctionRunner;
use agentdash_spi::hooks::{HookRuntimeRefreshQuery, RuntimeAdapterProvenance, SharedHookRuntime};
use tracing::{info, warn};
use uuid::Uuid;

use crate::platform_config::SharedPlatformConfig;
use crate::repository_set::RepositorySet;

use super::session_association::resolve_activity_session_association;
use crate::session::SessionTerminalCallback;
use crate::session::{
    SessionCapabilityService, SessionCoreService, SessionHookService, SessionLaunchService,
};
use crate::workflow::execution_log::ActivityAttemptArtifactScope;
use crate::workflow::{
    ActivityEvent, ActivityLifecycleRunService, AgentActivityExecutorLauncher,
    AgentActivityLaunchContext, AgentActivityRuntimePort, load_scoped_port_output_map,
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
    pub graph_instance: WorkflowGraphInstance,
    pub activity_key: String,
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
            self.repos.agent_assignment_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            Some(self.repos.execution_anchor_repo.as_ref()),
        )
        .await
        .map_err(|error| error.to_string())?
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
            self.repos.workflow_graph_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.workflow_graph_instance_repo.as_ref(),
            self.repos.activity_execution_claim_repo.as_ref(),
        );
        let update = service
            .apply_event(association.graph_instance_id, event)
            .await
            .map_err(|error| format!("推进 activity lifecycle run 失败: {error}"))?;
        let activated_nodes = self
            .launch_ready_activity_attempts(&update.run, update.graph_instance.id)
            .await?;

        Ok(Some(OrchestrationResult {
            run_id: update.run.id,
            activated_nodes,
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
            self.repos.agent_assignment_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            Some(self.repos.execution_anchor_repo.as_ref()),
        )
        .await
        .map_err(|error| error.to_string())?
        else {
            return Err("当前 runtime session 没有关联 lifecycle activity attempt".to_string());
        };

        let graph_instance = self
            .repos
            .workflow_graph_instance_repo
            .get_by_run_and_id(association.run.id, association.graph_instance_id)
            .await
            .map_err(|error| format!("加载 workflow graph instance 失败: {error}"))?
            .ok_or_else(|| {
                format!(
                    "workflow graph instance 不存在: {}",
                    association.graph_instance_id
                )
            })?;
        let definition = self
            .repos
            .workflow_graph_repo
            .get_by_id(graph_instance.graph_id)
            .await
            .map_err(|error| format!("加载 activity lifecycle definition 失败: {error}"))?
            .ok_or_else(|| {
                format!(
                    "activity lifecycle definition 不存在: {}",
                    graph_instance.graph_id
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
            let artifact_scope = ActivityAttemptArtifactScope {
                run_id: association.run.id,
                graph_instance_id: association.graph_instance_id,
                activity_key: association.activity_key.clone(),
                attempt: association.attempt,
            };
            let port_output_map =
                load_scoped_port_output_map(self.repos.inline_file_repo.as_ref(), &artifact_scope)
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
                    graph_instance,
                    activity_key: association.activity_key,
                    status: AdvanceCurrentNodeStatus::GateRejected {
                        gate_collision_count: 0,
                        missing_output_keys,
                        terminal_failed: false,
                    },
                    orchestration_warning: None,
                });
            }
            let outputs = activity_outputs_from_scoped_port_map(activity, port_output_map)?;
            ActivityEvent::ActivityCompleted {
                activity_key: association.activity_key.clone(),
                attempt: association.attempt,
                outputs,
                summary: input.summary.clone(),
            }
        };

        let service = ActivityLifecycleRunService::new(
            self.repos.workflow_graph_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.workflow_graph_instance_repo.as_ref(),
            self.repos.activity_execution_claim_repo.as_ref(),
        );
        let update = service
            .apply_event(association.graph_instance_id, event)
            .await
            .map_err(|error| format!("推进 activity lifecycle run 失败: {error}"))?;
        self.refresh_hook_snapshot(&input.hook_runtime, &input.turn_id)
            .await?;

        let orchestration_warning = if input.outcome == LifecycleNodeAdvanceOutcome::Completed {
            match self
                .launch_ready_activity_attempts(&update.run, update.graph_instance.id)
                .await
            {
                Ok(_) => None,
                Err(error) => Some(format!(
                    "activity 已完成，但后继 executor 启动失败：{error}"
                )),
            }
        } else {
            None
        };
        let final_run = self.load_run(update.run.id).await?;
        let final_graph_instance = self
            .repos
            .workflow_graph_instance_repo
            .get_by_run_and_id(final_run.id, update.graph_instance.id)
            .await
            .map_err(|e| format!("加载 workflow graph instance 失败: {e}"))?
            .unwrap_or(update.graph_instance);
        Ok(AdvanceCurrentNodeResult {
            run: final_run,
            graph_instance: final_graph_instance,
            activity_key: association.activity_key,
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
        graph_instance_id: Uuid,
    ) -> Result<Vec<ActivatedNode>, String> {
        let service = ActivityLifecycleRunService::new(
            self.repos.workflow_graph_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.workflow_graph_instance_repo.as_ref(),
            self.repos.activity_execution_claim_repo.as_ref(),
        );
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext::detached(run.project_id, String::new()),
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
        let (_update, outcomes) = service
            .launch_ready_attempts(graph_instance_id, &launcher)
            .await
            .map_err(|error| format!("启动后继 activity executor 失败: {error}"))?;
        Ok(outcomes
            .into_iter()
            .filter_map(|outcome| {
                if !outcome.started {
                    return None;
                }
                match outcome.claim.executor_run_ref {
                    Some(ExecutorRunRef::RuntimeSession { session_id }) => Some(ActivatedNode {
                        node_key: outcome.claim.activity_key,
                        runtime_session_id: session_id,
                    }),
                    _ => None,
                }
            })
            .collect())
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

fn activity_outputs_from_scoped_port_map(
    activity: &ActivityDefinition,
    port_output_map: std::collections::BTreeMap<String, String>,
) -> Result<Vec<ActivityPortValue>, String> {
    let declared_output_keys = activity
        .output_ports
        .iter()
        .map(|port| port.key.as_str())
        .collect::<Vec<_>>();

    port_output_map
        .into_iter()
        .filter(|(port_key, _)| declared_output_keys.contains(&port_key.as_str()))
        .map(|(port_key, content)| {
            let value = serde_json::from_str(&content).map_err(|error| {
                format!(
                    "activity `{}` output port `{}` 必须写入 JSON 内容: {error}",
                    activity.key, port_key
                )
            })?;
            Ok(ActivityPortValue { port_key, value })
        })
        .collect()
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
    use std::collections::BTreeMap;

    use agentdash_domain::workflow::{
        ActivityExecutorSpec, ActivityIterationPolicy, ActivityJoinPolicy,
        AgentActivityExecutorSpec, OutputPortDefinition,
    };

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

    #[test]
    fn activity_outputs_parse_declared_json_ports() {
        let activity = test_activity_with_outputs(&["result"]);
        let outputs = activity_outputs_from_scoped_port_map(
            &activity,
            BTreeMap::from([
                ("ignored".to_string(), "\"not declared\"".to_string()),
                ("result".to_string(), "{\"ok\":true}".to_string()),
            ]),
        )
        .expect("json output");

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].port_key, "result");
        assert_eq!(outputs[0].value, serde_json::json!({ "ok": true }));
    }

    #[test]
    fn activity_outputs_reject_invalid_json_port_content() {
        let activity = test_activity_with_outputs(&["result"]);
        let error = activity_outputs_from_scoped_port_map(
            &activity,
            BTreeMap::from([("result".to_string(), "plain text".to_string())]),
        )
        .expect_err("invalid json");

        assert!(error.contains("output port `result`"));
        assert!(error.contains("必须写入 JSON 内容"));
    }

    fn test_activity_with_outputs(keys: &[&str]) -> ActivityDefinition {
        ActivityDefinition {
            key: "build".to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Agent(
                AgentActivityExecutorSpec::create_activity_agent("workflow"),
            ),
            input_ports: Vec::new(),
            output_ports: keys
                .iter()
                .map(|key| OutputPortDefinition {
                    key: (*key).to_string(),
                    description: String::new(),
                    gate_strategy: Default::default(),
                    gate_params: None,
                })
                .collect(),
            completion_policy: ActivityCompletionPolicy::default(),
            iteration_policy: ActivityIterationPolicy::default(),
            join_policy: ActivityJoinPolicy::default(),
        }
    }
}
