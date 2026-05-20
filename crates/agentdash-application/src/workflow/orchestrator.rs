//! LifecycleOrchestrator — DAG 编排引擎
//!
//! 职责：当某个 lifecycle node 的 session 终止后，评估后继 node 的可达性，
//! 为 AgentNode 类型的后继创建独立 session 并启动 prompt。
//!
//! 不维护自己的状态 — 所有状态读写都通过 LifecycleRun / SessionBinding / session services。
//! 不是后台进程 — 通过事件驱动（advance tool / session terminal）被调用。
//!
//! 实现 `SessionTerminalCallback`，由 `session runtime` 在 session 完全终止后自动调用。

use agentdash_domain::session_binding::{SessionBinding, SessionOwnerCtx, SessionOwnerType};
use agentdash_domain::workflow::{
    ActivityCompletionPolicy, ActivityPortValue, ExecutorRunRef, LifecycleDefinition,
    LifecycleEdge, LifecycleNodeType, LifecycleRun, LifecycleStepDefinition,
    LifecycleStepExecutionStatus, WorkflowDefinition, WorkflowSessionTerminalState,
};
use agentdash_spi::hooks::{SessionHookRefreshQuery, SharedHookSessionRuntime};
use tracing::{info, warn};
use uuid::Uuid;

use crate::platform_config::SharedPlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::hub::PendingRuntimeContextTransitionInput;
use crate::session::{LaunchCommand, UserPromptInput};

use super::session_association::{
    LIFECYCLE_NODE_LABEL_PREFIX, build_lifecycle_node_label, resolve_activity_session_association,
    resolve_node_session_association,
};
use crate::session::SessionTerminalCallback;
use crate::session::{
    SessionCapabilityService, SessionCoreService, SessionHookService, SessionLaunchService,
};
use crate::workflow::step_activation::apply_to_running_session;
use crate::workflow::{
    ActivateLifecycleStepCommand, ActivityEvent, ActivityLifecycleRunService,
    AgentActivityExecutorLauncher, AgentActivityLaunchContext, AgentActivityRuntimePort,
    BindAndActivateLifecycleStepCommand, CompleteLifecycleStepCommand, FailLifecycleStepCommand,
    LifecycleRunService, RecordGateCollisionCommand, activate_step_with_platform,
    agent_mcp_entries_from_servers, build_capability_state_for_activation,
    build_step_projector_from_repos, load_port_output_map, session_terminal_summary,
};

#[derive(Debug)]
pub struct OrchestrationResult {
    pub run_id: Uuid,
    pub activated_nodes: Vec<ActivatedNode>,
    /// PhaseNode 激活不产生新 session，仅标记步骤为 active。
    /// 调用方据此计算 capability delta 并通知当前 session。
    pub activated_phase_nodes: Vec<ActivatedPhaseNode>,
}

#[derive(Debug)]
pub struct ActivatedNode {
    pub node_key: String,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct ActivatedPhaseNode {
    pub node_key: String,
    pub lifecycle_key: String,
    pub lifecycle_edges: Vec<LifecycleEdge>,
    pub step: LifecycleStepDefinition,
    pub workflow: Option<WorkflowDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleNodeAdvanceOutcome {
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct AdvanceCurrentNodeInput {
    pub hook_session: SharedHookSessionRuntime,
    pub turn_id: String,
    pub run_id: Uuid,
    pub step_key: String,
    pub outcome: LifecycleNodeAdvanceOutcome,
    pub summary: Option<String>,
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
}

impl LifecycleOrchestrator {
    pub fn new(
        session_core: SessionCoreService,
        session_launch: SessionLaunchService,
        session_hooks: SessionHookService,
        session_capability: SessionCapabilityService,
        repos: RepositorySet,
        platform_config: SharedPlatformConfig,
    ) -> Self {
        Self {
            session_core,
            session_launch,
            session_hooks,
            session_capability,
            repos,
            platform_config,
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
        if terminal_state != "completed" {
            return Ok(None);
        }
        self.on_node_session_terminal(session_id).await
    }

    async fn on_node_session_terminal(
        &self,
        session_id: &str,
    ) -> Result<Option<OrchestrationResult>, String> {
        let Some(association) = resolve_node_session_association(
            session_id,
            self.repos.session_binding_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
        )
        .await?
        else {
            return Ok(None);
        };

        info!(
            run_id = %association.run.id,
            completed_node = %association.node_key,
            "Orchestrator: session terminal, evaluating successors"
        );

        self.activate_ready_nodes(association.run.id, association.run.project_id)
            .await
    }

    async fn on_activity_session_terminal(
        &self,
        session_id: &str,
        terminal_state: &str,
    ) -> Result<Option<OrchestrationResult>, String> {
        let Some(association) = resolve_activity_session_association(
            session_id,
            self.repos.session_binding_repo.as_ref(),
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
            activated_phase_nodes: Vec::new(),
        }))
    }

    /// 在 complete_lifecycle_node tool / start_run 后调用。
    pub async fn after_node_advanced(
        &self,
        run_id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<OrchestrationResult>, String> {
        self.activate_ready_nodes(run_id, project_id).await
    }

    /// 将已激活的 PhaseNode 应用到 lifecycle run 绑定的 root session。
    ///
    /// 适用于 start_run / terminal callback 等没有直接持有 live hook_session 的路径。
    /// 若 root session 当前没有可热更的 live turn，会把解析后的 CapabilityState 暂存到
    /// session meta，下一轮 prompt 进入 pipeline 时再应用。
    pub async fn apply_activated_phase_nodes_for_run_session(
        &self,
        run: &LifecycleRun,
        phases: &[ActivatedPhaseNode],
        turn_id: Option<&str>,
    ) -> Vec<String> {
        if phases.is_empty() {
            return Vec::new();
        }

        if self
            .session_capability
            .get_current_capability_state(&run.session_id)
            .await
            .is_some()
        {
            return match self
                .session_hooks
                .ensure_hook_session_runtime(&run.session_id, turn_id)
                .await
            {
                Ok(Some(hook_session)) => {
                    self.apply_activated_phase_nodes(&hook_session, turn_id, run, phases)
                        .await
                }
                Ok(None) | Err(_) => {
                    self.queue_pending_phase_nodes(
                        run,
                        phases,
                        turn_id,
                        SessionOwnerCtx::Project {
                            project_id: run.project_id,
                        },
                    )
                    .await
                }
            };
        }

        let owner_ctx = self
            .session_hooks
            .get_hook_session_runtime(&run.session_id)
            .await
            .map(|hook_session| resolve_owner_scope(&hook_session.snapshot(), run.project_id))
            .unwrap_or(SessionOwnerCtx::Project {
                project_id: run.project_id,
            });
        self.queue_pending_phase_nodes(run, phases, turn_id, owner_ctx)
            .await
    }

    /// `complete_lifecycle_node` 的统一 workflow 入口。
    ///
    /// 负责串联：
    /// 1. 当前 node 完成/失败推进
    /// 2. output port gate / collision 处理
    /// 3. summary 物化
    /// 4. hook snapshot refresh
    /// 5. successor activation
    /// 6. PhaseNode runtime capability / MCP apply
    pub async fn advance_current_node(
        &self,
        input: AdvanceCurrentNodeInput,
    ) -> Result<AdvanceCurrentNodeResult, String> {
        let service = LifecycleRunService::new(
            self.repos.lifecycle_definition_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
        )
        .with_projector(build_step_projector_from_repos(&self.repos));

        let current_run = self.load_run(input.run_id).await?;

        if input.outcome == LifecycleNodeAdvanceOutcome::Failed {
            let run = service
                .fail_step(FailLifecycleStepCommand {
                    run_id: input.run_id,
                    step_key: input.step_key.clone(),
                    summary: input.summary.clone(),
                })
                .await
                .map_err(|e| format!("标记 Failed 失败: {e}"))?;

            if let Some(summary) = &input.summary {
                crate::workflow::materialize_step_summary(
                    self.repos.inline_file_repo.as_ref(),
                    input.run_id,
                    &input.step_key,
                    summary,
                )
                .await;
            }

            self.refresh_hook_snapshot(&input.hook_session, &input.turn_id)
                .await?;

            let final_run = self.load_run(run.id).await?;
            return Ok(AdvanceCurrentNodeResult {
                run: final_run,
                step_key: input.step_key,
                status: AdvanceCurrentNodeStatus::Failed,
                orchestration_warning: None,
            });
        }

        let lifecycle = self.load_lifecycle(current_run.lifecycle_id).await?;
        let required_output_keys: Vec<String> = lifecycle
            .steps
            .iter()
            .find(|step| step.key == input.step_key)
            .map(|step| {
                step.output_ports
                    .iter()
                    .map(|port| port.key.clone())
                    .collect()
            })
            .unwrap_or_default();

        if !required_output_keys.is_empty() {
            let port_output_map =
                load_port_output_map(self.repos.inline_file_repo.as_ref(), current_run.id).await;
            let missing_output_keys: Vec<String> = required_output_keys
                .into_iter()
                .filter(|key| !port_output_map.contains_key(key.as_str()))
                .collect();

            if !missing_output_keys.is_empty() {
                let (_, collision_count) = service
                    .record_gate_collision(RecordGateCollisionCommand {
                        run_id: input.run_id,
                        step_key: input.step_key.clone(),
                    })
                    .await
                    .map_err(|e| format!("更新 gate collision 失败: {e}"))?;

                let terminal_failed = if collision_count >= 3 {
                    service
                        .fail_step(FailLifecycleStepCommand {
                            run_id: input.run_id,
                            step_key: input.step_key.clone(),
                            summary: None,
                        })
                        .await
                        .map_err(|e| format!("更新 gate collision 失败: {e}"))?;
                    true
                } else {
                    false
                };

                self.refresh_hook_snapshot(&input.hook_session, &input.turn_id)
                    .await?;

                let final_run = self.load_run(input.run_id).await?;
                return Ok(AdvanceCurrentNodeResult {
                    run: final_run,
                    step_key: input.step_key,
                    status: AdvanceCurrentNodeStatus::GateRejected {
                        gate_collision_count: collision_count,
                        missing_output_keys,
                        terminal_failed,
                    },
                    orchestration_warning: None,
                });
            }
        }

        let run = service
            .complete_step(CompleteLifecycleStepCommand {
                run_id: input.run_id,
                step_key: input.step_key.clone(),
                summary: input.summary.clone(),
            })
            .await
            .map_err(|e| format!("推进失败: {e}"))?;

        if let Some(summary) = &input.summary {
            crate::workflow::materialize_step_summary(
                self.repos.inline_file_repo.as_ref(),
                input.run_id,
                &input.step_key,
                summary,
            )
            .await;
        }

        self.refresh_hook_snapshot(&input.hook_session, &input.turn_id)
            .await?;

        let orchestration_warning = match self.after_node_advanced(run.id, run.project_id).await {
            Ok(Some(result)) => {
                let warnings = self
                    .apply_activated_phase_nodes(
                        &input.hook_session,
                        Some(input.turn_id.as_str()),
                        &run,
                        &result.activated_phase_nodes,
                    )
                    .await;
                if warnings.is_empty() {
                    None
                } else {
                    Some(warnings.join("；"))
                }
            }
            Ok(None) => None,
            Err(error) => Some(format!("node 已完成，但后继编排触发失败：{error}")),
        };

        let final_run = self.load_run(run.id).await?;
        Ok(AdvanceCurrentNodeResult {
            run: final_run,
            step_key: input.step_key,
            status: AdvanceCurrentNodeStatus::Completed,
            orchestration_warning,
        })
    }

    pub async fn advance_current_activity(
        &self,
        input: AdvanceCurrentActivityInput,
    ) -> Result<AdvanceCurrentNodeResult, String> {
        let Some(association) = resolve_activity_session_association(
            &input.session_id,
            self.repos.session_binding_repo.as_ref(),
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
                root_session_id: run.session_id.clone(),
            },
            AgentActivityRuntimePort::new(
                self.session_core.clone(),
                self.session_launch.clone(),
                self.repos.clone(),
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

    /// 核心逻辑：扫描 LifecycleRun 中所有 Ready 状态的 AgentNode，为它们创建 session。
    async fn activate_ready_nodes(
        &self,
        run_id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<OrchestrationResult>, String> {
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(|e| format!("加载 lifecycle run 失败: {e}"))?
            .ok_or_else(|| format!("lifecycle run 不存在: {run_id}"))?;

        let lifecycle = self
            .repos
            .lifecycle_definition_repo
            .get_by_id(run.lifecycle_id)
            .await
            .map_err(|e| format!("加载 lifecycle definition 失败: {e}"))?
            .ok_or_else(|| format!("lifecycle definition 不存在: {}", run.lifecycle_id))?;

        let ready_nodes: Vec<_> = run
            .step_states
            .iter()
            .filter(|s| s.status == LifecycleStepExecutionStatus::Ready)
            .filter(|s| s.session_id.is_none())
            .collect();

        if ready_nodes.is_empty() {
            return Ok(None);
        }

        let mut activated = Vec::new();
        let mut activated_phases = Vec::new();

        for node_state in ready_nodes {
            let step_def = lifecycle
                .steps
                .iter()
                .find(|s| s.key == node_state.step_key);
            let Some(step_def) = step_def else {
                warn!(
                    node_key = %node_state.step_key,
                    "Orchestrator: step definition not found, skipping"
                );
                continue;
            };

            match step_def.node_type {
                LifecycleNodeType::AgentNode => {
                    match self
                        .create_agent_node_session(&run, &lifecycle, project_id, step_def)
                        .await
                    {
                        Ok(session_id) => {
                            activated.push(ActivatedNode {
                                node_key: node_state.step_key.clone(),
                                session_id,
                            });
                        }
                        Err(e) => {
                            warn!(
                                node_key = %node_state.step_key,
                                error = %e,
                                "Orchestrator: failed to create agent node session"
                            );
                        }
                    }
                }
                LifecycleNodeType::PhaseNode => {
                    let service = LifecycleRunService::new(
                        self.repos.lifecycle_definition_repo.as_ref(),
                        self.repos.lifecycle_run_repo.as_ref(),
                    )
                    .with_projector(build_step_projector_from_repos(&self.repos));
                    if let Err(e) = service
                        .activate_step(ActivateLifecycleStepCommand {
                            run_id,
                            step_key: node_state.step_key.clone(),
                        })
                        .await
                    {
                        warn!(
                            node_key = %node_state.step_key,
                            error = %e,
                            "Orchestrator: failed to activate phase node"
                        );
                    } else {
                        let workflow = self.resolve_step_workflow(step_def, project_id).await;
                        activated_phases.push(ActivatedPhaseNode {
                            node_key: node_state.step_key.clone(),
                            lifecycle_key: lifecycle.key.clone(),
                            lifecycle_edges: lifecycle.edges.clone(),
                            step: step_def.clone(),
                            workflow,
                        });
                    }
                }
            }
        }

        if activated.is_empty() && activated_phases.is_empty() {
            return Ok(None);
        }

        Ok(Some(OrchestrationResult {
            run_id,
            activated_nodes: activated,
            activated_phase_nodes: activated_phases,
        }))
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

    async fn apply_activated_phase_nodes(
        &self,
        hook_session: &SharedHookSessionRuntime,
        turn_id: Option<&str>,
        run: &LifecycleRun,
        phases: &[ActivatedPhaseNode],
    ) -> Vec<String> {
        if phases.is_empty() {
            return Vec::new();
        }

        let mut warnings = Vec::new();
        if let Err(error) = self
            .refresh_hook_snapshot_for_turn(
                hook_session,
                turn_id,
                "phase_node_activated:refresh_snapshot",
            )
            .await
        {
            warnings.push(format!("PhaseNode 激活后刷新 Hook snapshot 失败: {error}"));
            return warnings;
        }

        let snapshot = hook_session.snapshot();
        let owner_ctx = resolve_owner_scope(&snapshot, run.project_id);
        let available_presets =
            crate::session::load_available_presets(&self.repos, run.project_id).await;

        let mut runtime_mcp_servers = self
            .session_capability
            .get_runtime_mcp_servers(hook_session.session_id())
            .await;

        for phase in phases {
            let ready_port_keys = std::collections::BTreeSet::new();
            let agent_mcp_servers = agent_mcp_entries_from_servers(&runtime_mcp_servers);

            let activation = activate_step_with_platform(
                &crate::workflow::StepActivationInput {
                    owner_ctx,
                    active_step: &phase.step,
                    workflow: phase.workflow.as_ref(),
                    run_id: run.id,
                    lifecycle_key: &phase.lifecycle_key,
                    edges: &phase.lifecycle_edges,
                    agent_mcp_servers,
                    available_presets: available_presets.clone(),
                    companion_slice_mode: None,
                    baseline_override: None,
                    tool_directives: &[],
                    ready_port_keys,
                    available_companions: Vec::new(),
                },
                &self.platform_config,
            );

            match apply_to_running_session(
                &activation,
                hook_session,
                &self.session_capability,
                turn_id,
                &phase.node_key,
                Some(run.id),
                Some(&phase.lifecycle_key),
            )
            .await
            {
                Ok(outcome) => {
                    runtime_mcp_servers = activation.mcp_servers.clone();
                    tracing::info!(
                        phase_node = %phase.node_key,
                        capabilities = ?activation.capability_keys,
                        emitted_capability_change = outcome.emitted_capability_change,
                        "Phase node capability surface applied"
                    );
                }
                Err(error) => {
                    warn!(
                        session_id = %hook_session.session_id(),
                        phase_node = %phase.node_key,
                        error = %error,
                        "Phase node runtime apply failed"
                    );
                    warnings.push(error);
                }
            }
        }

        warnings
    }

    async fn queue_pending_phase_nodes(
        &self,
        run: &LifecycleRun,
        phases: &[ActivatedPhaseNode],
        turn_id: Option<&str>,
        owner_ctx: SessionOwnerCtx,
    ) -> Vec<String> {
        let available_presets =
            crate::session::load_available_presets(&self.repos, run.project_id).await;
        let mut base_surface = self
            .session_capability
            .get_latest_capability_state(&run.session_id)
            .await;
        let mut warnings = Vec::new();

        for phase in phases {
            let agent_mcp_servers = base_surface
                .as_ref()
                .map(|surface| agent_mcp_entries_from_servers(&surface.tool.mcp_servers))
                .unwrap_or_default();
            let activation = activate_step_with_platform(
                &crate::workflow::StepActivationInput {
                    owner_ctx,
                    active_step: &phase.step,
                    workflow: phase.workflow.as_ref(),
                    run_id: run.id,
                    lifecycle_key: &phase.lifecycle_key,
                    edges: &phase.lifecycle_edges,
                    agent_mcp_servers,
                    available_presets: available_presets.clone(),
                    companion_slice_mode: None,
                    baseline_override: None,
                    tool_directives: &[],
                    ready_port_keys: std::collections::BTreeSet::new(),
                    available_companions: Vec::new(),
                },
                &self.platform_config,
            );
            let surface = build_capability_state_for_activation(&activation, base_surface.as_ref());
            if let Err(error) = self
                .session_capability
                .enqueue_pending_runtime_context_transition(PendingRuntimeContextTransitionInput {
                    session_id: run.session_id.clone(),
                    turn_id: turn_id.map(ToString::to_string),
                    transition_id: format!("phase-{}-{}", phase.node_key, uuid::Uuid::new_v4()),
                    phase_node: phase.node_key.clone(),
                    run_id: run.id,
                    lifecycle_key: phase.lifecycle_key.clone(),
                    before_state: base_surface.clone(),
                    after_state: surface.clone(),
                    capability_keys: activation.capability_keys.clone(),
                    source_turn_id: turn_id.map(ToString::to_string),
                    created_at: chrono::Utc::now().timestamp_millis(),
                })
                .await
            {
                warnings.push(error);
            }

            base_surface = Some(surface);
        }

        warnings
    }

    /// 为 AgentNode 创建独立 session 并通过 SessionBinding 标记归属 lifecycle run。
    async fn create_agent_node_session(
        &self,
        run: &LifecycleRun,
        lifecycle: &LifecycleDefinition,
        project_id: Uuid,
        step_def: &LifecycleStepDefinition,
    ) -> Result<String, String> {
        let node_key = &step_def.key;
        let run_id = run.id;
        // run.session_id 即 Story root session（StorySessionId 别名）。
        // 后继创建的 agent node session 是它的 child session。
        let parent_session_id = &run.session_id;
        let lifecycle_key = &lifecycle.key;

        let session_title = format!("[{lifecycle_key}] {node_key}");
        let meta = self
            .session_core
            .create_session(&session_title)
            .await
            .map_err(|e| format!("创建 session 失败: {e}"))?;
        let session_id = meta.id.clone();

        let parent_bindings = self
            .repos
            .session_binding_repo
            .list_by_session(parent_session_id)
            .await
            .map_err(|e| format!("查询父 session binding 失败: {e}"))?;

        if let Some(parent_binding) = parent_bindings
            .iter()
            .find(|binding| !binding.label.starts_with(LIFECYCLE_NODE_LABEL_PREFIX))
            .or_else(|| parent_bindings.first())
        {
            let binding = SessionBinding::new(
                project_id,
                session_id.clone(),
                parent_binding.owner_type,
                parent_binding.owner_id,
                build_lifecycle_node_label(node_key),
            );
            self.repos
                .session_binding_repo
                .create(&binding)
                .await
                .map_err(|e| format!("创建 session binding 失败: {e}"))?;
        } else {
            let binding = SessionBinding::new(
                project_id,
                session_id.clone(),
                SessionOwnerType::Project,
                project_id,
                build_lifecycle_node_label(node_key),
            );
            self.repos
                .session_binding_repo
                .create(&binding)
                .await
                .map_err(|e| format!("创建 session binding 失败: {e}"))?;
        }

        let service = LifecycleRunService::new(
            self.repos.lifecycle_definition_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
        )
        .with_projector(build_step_projector_from_repos(&self.repos));
        service
            .bind_session_and_activate_step(BindAndActivateLifecycleStepCommand {
                run_id,
                step_key: node_key.clone(),
                session_id: session_id.clone(),
            })
            .await
            .map_err(|e| format!("更新 lifecycle run 失败: {e}"))?;

        let parent_executor_config = self
            .session_core
            .get_session_meta(parent_session_id)
            .await
            .map_err(|e| format!("读取父 session meta 失败: {e}"))?
            .and_then(|meta| meta.executor_config);
        if let Some(executor_config) = parent_executor_config.clone() {
            self.session_core
                .update_session_meta(&session_id, move |meta| {
                    meta.executor_config = Some(executor_config.clone());
                })
                .await
                .map_err(|e| format!("继承执行器配置失败: {e}"))?;
        }

        if let Err(error) = self
            .start_agent_node_prompt(
                &session_id,
                run,
                lifecycle,
                project_id,
                step_def,
                parent_executor_config,
            )
            .await
        {
            warn!(
                run_id = %run_id,
                node_key = %node_key,
                session_id = %session_id,
                error = %error,
                "Orchestrator: created node session but failed to auto-start prompt"
            );
        }

        info!(
            run_id = %run_id,
            node_key = %node_key,
            session_id = %session_id,
            "Orchestrator: created agent node session"
        );

        Ok(session_id)
    }

    /// 委托给 [`compose_lifecycle_node`]:构造 kickoff prompt + vfs + cap + mcp,
    /// 然后 start_prompt。
    async fn start_agent_node_prompt(
        &self,
        session_id: &str,
        _run: &LifecycleRun,
        _lifecycle: &LifecycleDefinition,
        _project_id: Uuid,
        _step_def: &LifecycleStepDefinition,
        executor_config: Option<agentdash_spi::AgentConfig>,
    ) -> Result<(), String> {
        self.session_core
            .mark_owner_bootstrap_pending(session_id)
            .await
            .map_err(|e| format!("标记 owner bootstrap pending 失败: {e}"))?;

        let mut user_input = UserPromptInput::from_text("");
        user_input.executor_config = executor_config;
        let command = LaunchCommand::workflow_orchestrator_input(user_input);
        self.session_launch
            .launch_command(session_id, command)
            .await
            .map_err(|e| format!("自动启动 node session prompt 失败: {e}"))?;
        Ok(())
    }

    /// 查找 step.workflow_key 指向的 WorkflowDefinition。
    /// 查不到时返回 None。
    async fn resolve_step_workflow(
        &self,
        step_def: &LifecycleStepDefinition,
        project_id: Uuid,
    ) -> Option<WorkflowDefinition> {
        let workflow_key = step_def.effective_workflow_key()?;
        match self
            .repos
            .workflow_definition_repo
            .get_by_project_and_key(project_id, workflow_key)
            .await
        {
            Ok(Some(workflow)) => Some(workflow),
            Ok(None) => {
                tracing::warn!(
                    project_id = %project_id,
                    workflow_key = %workflow_key,
                    step_key = %step_def.key,
                    "orchestrator: step.workflow_key 指向的 workflow 不存在"
                );
                None
            }
            Err(error) => {
                tracing::warn!(
                    project_id = %project_id,
                    workflow_key = %workflow_key,
                    step_key = %step_def.key,
                    error = %error,
                    "orchestrator: 加载 workflow 定义失败"
                );
                None
            }
        }
    }

    async fn load_run(&self, run_id: Uuid) -> Result<LifecycleRun, String> {
        self.repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(|e| format!("加载 lifecycle run 失败: {e}"))?
            .ok_or_else(|| format!("lifecycle run 不存在: {run_id}"))
    }

    async fn load_lifecycle(&self, lifecycle_id: Uuid) -> Result<LifecycleDefinition, String> {
        self.repos
            .lifecycle_definition_repo
            .get_by_id(lifecycle_id)
            .await
            .map_err(|e| format!("加载 lifecycle definition 失败: {e}"))?
            .ok_or_else(|| format!("lifecycle definition 不存在: {lifecycle_id}"))
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
                    "Orchestrator callback: activated successor nodes"
                );
                if !result.activated_phase_nodes.is_empty() {
                    match self.load_run(result.run_id).await {
                        Ok(run) => {
                            let warnings = self
                                .apply_activated_phase_nodes_for_run_session(
                                    &run,
                                    &result.activated_phase_nodes,
                                    None,
                                )
                                .await;
                            if !warnings.is_empty() {
                                warn!(
                                    run_id = %result.run_id,
                                    warnings = ?warnings,
                                    "Orchestrator callback: PhaseNode apply produced warnings"
                                );
                            }
                        }
                        Err(error) => {
                            warn!(
                                run_id = %result.run_id,
                                error = %error,
                                "Orchestrator callback: 无法加载 run 以应用 PhaseNode"
                            );
                        }
                    }
                }
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

fn resolve_owner_scope(
    snapshot: &agentdash_spi::hooks::SessionHookSnapshot,
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
