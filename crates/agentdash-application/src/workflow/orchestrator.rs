//! LifecycleOrchestrator — DAG 编排引擎
//!
//! 职责：当某个 lifecycle node 的 session 终止后，评估后继 node 的可达性，
//! 为 AgentNode 类型的后继创建独立 session 并启动 prompt。
//!
//! 不维护自己的状态 — 所有状态读写都通过 LifecycleRun / SessionBinding / SessionHub。
//! 不是后台进程 — 通过事件驱动（advance tool / session terminal）被调用。
//!
//! 实现 `SessionTerminalCallback`，由 `SessionHub` 在 session 完全终止后自动调用。

use agentdash_domain::session_binding::{SessionBinding, SessionOwnerCtx, SessionOwnerType};
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleEdge, LifecycleNodeType, LifecycleRun, LifecycleStepDefinition,
    LifecycleStepExecutionStatus, WorkflowDefinition,
};
use agentdash_spi::hooks::{SessionHookRefreshQuery, SharedHookSessionRuntime};
use tracing::{info, warn};
use uuid::Uuid;

use crate::platform_config::SharedPlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::{
    LifecycleNodeSpec, PromptSessionRequest, UserPromptInput, compose_lifecycle_node,
    finalize_request,
};

use super::session_association::{
    LIFECYCLE_NODE_LABEL_PREFIX, build_lifecycle_node_label, resolve_node_session_association,
};
use crate::session::SessionHub;
use crate::session::SessionTerminalCallback;
use crate::workflow::{
    ActivateLifecycleStepCommand, BindAndActivateLifecycleStepCommand,
    CompleteLifecycleStepCommand, FailLifecycleStepCommand, LifecycleRunService,
    RecordGateCollisionCommand, activate_step_with_platform, agent_mcp_entries_from_servers,
    apply_to_running_session, capability_delta_directives, capability_directives_from_keys,
    load_port_output_map,
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
    session_hub: SessionHub,
    repos: RepositorySet,
    platform_config: SharedPlatformConfig,
}

impl LifecycleOrchestrator {
    pub fn new(
        session_hub: SessionHub,
        repos: RepositorySet,
        platform_config: SharedPlatformConfig,
    ) -> Self {
        Self {
            session_hub,
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

    /// 在 complete_lifecycle_node tool / start_run 后调用。
    pub async fn after_node_advanced(
        &self,
        run_id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<OrchestrationResult>, String> {
        self.activate_ready_nodes(run_id, project_id).await
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
        );

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
                    );
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
        hook_session
            .refresh(SessionHookRefreshQuery {
                session_id: hook_session.session_id().to_string(),
                turn_id: Some(turn_id.to_string()),
                reason: Some("tool:complete_lifecycle_node".to_string()),
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

        let snapshot = hook_session.snapshot();
        let owner_ctx = resolve_owner_scope(&snapshot, run.project_id);
        let available_presets =
            crate::session::load_available_presets(&self.repos, run.project_id).await;

        let mut warnings = Vec::new();
        let mut current_caps = hook_session.current_capabilities();
        let mut runtime_mcp_servers = self
            .session_hub
            .get_runtime_mcp_servers(hook_session.session_id())
            .await;

        for phase in phases {
            let target_caps = target_capability_keys(
                phase
                    .workflow
                    .as_ref()
                    .map(|wf| wf.contract.capability_directives.as_slice())
                    .unwrap_or(&[]),
            );
            let baseline_override = capability_directives_from_keys(&current_caps);
            let runtime_delta = capability_delta_directives(&current_caps, &target_caps);
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
                    agent_declared_capabilities: None,
                    agent_mcp_servers,
                    available_presets: available_presets.clone(),
                    companion_slice_mode: None,
                    baseline_override: Some(baseline_override),
                    capability_directives: &runtime_delta,
                    ready_port_keys,
                },
                &self.platform_config,
            );

            match apply_to_running_session(
                &activation,
                hook_session,
                &self.session_hub,
                turn_id,
                &phase.node_key,
            )
            .await
            {
                Ok(_) => {
                    current_caps = activation.capability_keys.clone();
                    runtime_mcp_servers = activation.mcp_servers.clone();
                    tracing::info!(
                        phase_node = %phase.node_key,
                        capabilities = ?current_caps,
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
        let parent_session_id = &run.session_id;
        let lifecycle_key = &lifecycle.key;

        let session_title = format!("[{lifecycle_key}] {node_key}");
        let meta = self
            .session_hub
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
        );
        service
            .bind_session_and_activate_step(BindAndActivateLifecycleStepCommand {
                run_id,
                step_key: node_key.clone(),
                session_id: session_id.clone(),
            })
            .await
            .map_err(|e| format!("更新 lifecycle run 失败: {e}"))?;

        let parent_executor_config = self
            .session_hub
            .get_session_meta(parent_session_id)
            .await
            .map_err(|e| format!("读取父 session meta 失败: {e}"))?
            .and_then(|meta| meta.executor_config);
        if let Some(executor_config) = parent_executor_config.clone() {
            self.session_hub
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
        run: &LifecycleRun,
        lifecycle: &LifecycleDefinition,
        project_id: Uuid,
        step_def: &LifecycleStepDefinition,
        executor_config: Option<agentdash_spi::AgentConfig>,
    ) -> Result<(), String> {
        self.session_hub
            .mark_owner_bootstrap_pending(session_id)
            .await
            .map_err(|e| format!("标记 owner bootstrap pending 失败: {e}"))?;

        // 解析 step.workflow_key → WorkflowDefinition,作为 activate_step 的 workflow input
        let workflow = match step_def.effective_workflow_key() {
            Some(key) => self
                .repos
                .workflow_definition_repo
                .get_by_project_and_key(project_id, key)
                .await
                .ok()
                .flatten(),
            None => None,
        };

        let prepared = compose_lifecycle_node(
            &self.repos,
            &self.platform_config,
            LifecycleNodeSpec {
                run,
                lifecycle,
                step: step_def,
                workflow: workflow.as_ref(),
                inherited_executor_config: executor_config,
            },
        )
        .await?;

        let base = PromptSessionRequest::from_user_input(UserPromptInput::from_text(""));
        let req = finalize_request(base, prepared);

        self.session_hub
            .start_prompt(session_id, req)
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
        let Some(workflow_key) = step_def.effective_workflow_key() else {
            return None;
        };
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
        // 仅对正常完成的 session 触发后继评估；
        // failed / interrupted 的 session 不自动推进 DAG。
        if terminal_state != "completed" {
            return;
        }
        match self.on_session_terminal(session_id).await {
            Ok(Some(result)) => {
                info!(
                    run_id = %result.run_id,
                    activated = ?result.activated_nodes.iter().map(|n| &n.node_key).collect::<Vec<_>>(),
                    "Orchestrator callback: activated successor nodes"
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

fn target_capability_keys(
    directives: &[agentdash_domain::workflow::CapabilityDirective],
) -> std::collections::BTreeSet<String> {
    let reduction = agentdash_domain::workflow::reduce_capability_directives(directives);
    reduction
        .slots
        .iter()
        .filter_map(|(key, state)| {
            use agentdash_domain::workflow::CapabilitySlotState::*;
            match state {
                FullCapability | ToolWhitelist(_) => Some(key.clone()),
                NotDeclared | Blocked => None,
            }
        })
        .collect()
}
