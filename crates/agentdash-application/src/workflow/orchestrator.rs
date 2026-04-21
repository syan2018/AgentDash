//! LifecycleOrchestrator — DAG 编排引擎
//!
//! 职责：当某个 lifecycle node 的 session 终止后，评估后继 node 的可达性，
//! 为 AgentNode 类型的后继创建独立 session 并启动 prompt。
//!
//! 不维护自己的状态 — 所有状态读写都通过 LifecycleRun / SessionBinding / SessionHub。
//! 不是后台进程 — 通过事件驱动（advance tool / session terminal）被调用。
//!
//! 实现 `SessionTerminalCallback`，由 `SessionHub` 在 session 完全终止后自动调用。

use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleNodeType, LifecycleRun, LifecycleStepDefinition,
    LifecycleStepExecutionStatus,
};
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
use crate::workflow::{ActivateLifecycleStepCommand, LifecycleRunService};

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
    /// 该 phase 所指向的 workflow 基线 capability key 集合（取自 `WorkflowContract.capabilities`）。
    /// 若 step 未绑定 workflow 或 workflow 查不到,则为空 vec，表示"不改变能力集"。
    pub baseline_capabilities: Vec<String>,
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

    /// 核心逻辑：扫描 LifecycleRun 中所有 Ready 状态的 AgentNode，为它们创建 session。
    async fn activate_ready_nodes(
        &self,
        run_id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<OrchestrationResult>, String> {
        let run = self.repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(|e| format!("加载 lifecycle run 失败: {e}"))?
            .ok_or_else(|| format!("lifecycle run 不存在: {run_id}"))?;

        let lifecycle = self.repos
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
                        .create_agent_node_session(
                            &run,
                            &lifecycle,
                            project_id,
                            step_def,
                        )
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
                        let baseline_capabilities =
                            self.resolve_step_workflow_capabilities(step_def, project_id).await;
                        activated_phases.push(ActivatedPhaseNode {
                            node_key: node_state.step_key.clone(),
                            baseline_capabilities,
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

        let parent_bindings = self.repos
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
            self.repos.session_binding_repo
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
            self.repos.session_binding_repo
                .create(&binding)
                .await
                .map_err(|e| format!("创建 session binding 失败: {e}"))?;
        }

        // 更新 LifecycleRun.step_states[node_key].session_id 并 activate
        let latest_run = self.repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(|e| format!("加载 lifecycle run 失败: {e}"))?
            .ok_or_else(|| format!("lifecycle run 不存在: {run_id}"))?;

        let mut updated_run = latest_run;
        if let Some(state) = updated_run
            .step_states
            .iter_mut()
            .find(|s| s.step_key == *node_key)
        {
            state.session_id = Some(session_id.clone());
        }
        updated_run
            .activate_step(node_key.as_str())
            .map_err(|e| format!("activate step 失败: {e}"))?;
        self.repos.lifecycle_run_repo
            .update(&updated_run)
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

    /// 查找 step.workflow_key 指向的 WorkflowDefinition 并取其 `contract.capabilities`。
    /// 查不到时返回空 vec —— 等价于"不授予能力"。
    async fn resolve_step_workflow_capabilities(
        &self,
        step_def: &LifecycleStepDefinition,
        project_id: Uuid,
    ) -> Vec<String> {
        let Some(workflow_key) = step_def.effective_workflow_key() else {
            return Vec::new();
        };
        match self.repos
            .workflow_definition_repo
            .get_by_project_and_key(project_id, workflow_key)
            .await
        {
            Ok(Some(workflow)) => workflow.contract.capabilities.clone(),
            Ok(None) => {
                tracing::warn!(
                    project_id = %project_id,
                    workflow_key = %workflow_key,
                    step_key = %step_def.key,
                    "orchestrator: step.workflow_key 指向的 workflow 不存在,capability baseline 退化为空"
                );
                Vec::new()
            }
            Err(error) => {
                tracing::warn!(
                    project_id = %project_id,
                    workflow_key = %workflow_key,
                    step_key = %step_def.key,
                    error = %error,
                    "orchestrator: 加载 workflow 定义失败,capability baseline 退化为空"
                );
                Vec::new()
            }
        }
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
