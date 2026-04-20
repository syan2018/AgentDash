//! LifecycleOrchestrator — DAG 编排引擎
//!
//! 职责：当某个 lifecycle node 的 session 终止后，评估后继 node 的可达性，
//! 为 AgentNode 类型的后继创建独立 session 并启动 prompt。
//!
//! 不维护自己的状态 — 所有状态读写都通过 LifecycleRun / SessionBinding / SessionHub。
//! 不是后台进程 — 通过事件驱动（advance tool / session terminal）被调用。
//!
//! 实现 `SessionTerminalCallback`，由 `SessionHub` 在 session 完全终止后自动调用。

use std::sync::Arc;

use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::session_binding::{
    SessionBinding, SessionBindingRepository, SessionOwnerType,
};
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleNodeType, LifecycleRun,
    LifecycleRunRepository, LifecycleStepDefinition, LifecycleStepExecutionStatus,
    compute_effective_capabilities,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::capability::{CapabilityResolver, CapabilityResolverInput};
use crate::platform_config::SharedPlatformConfig;

use super::session_association::{
    LIFECYCLE_NODE_LABEL_PREFIX, build_lifecycle_node_label, resolve_node_session_association,
};
use crate::vfs::build_lifecycle_mount_with_ports;
use crate::runtime::Vfs;
use crate::session::SessionTerminalCallback;
use crate::session::{PromptSessionRequest, SessionHub, UserPromptInput};
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
    /// 该 step 的 CapabilityDirective 列表（从 LifecycleStepDefinition 中拷贝）
    pub capability_directives: Vec<agentdash_domain::workflow::CapabilityDirective>,
}

pub struct LifecycleOrchestrator {
    session_hub: SessionHub,
    session_binding_repo: Arc<dyn SessionBindingRepository>,
    lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    inline_file_repo: Arc<dyn InlineFileRepository>,
    platform_config: SharedPlatformConfig,
}

impl LifecycleOrchestrator {
    pub fn new(
        session_hub: SessionHub,
        session_binding_repo: Arc<dyn SessionBindingRepository>,
        lifecycle_definition_repo: Arc<dyn LifecycleDefinitionRepository>,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        platform_config: SharedPlatformConfig,
    ) -> Self {
        Self {
            session_hub,
            session_binding_repo,
            lifecycle_definition_repo,
            lifecycle_run_repo,
            inline_file_repo,
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
            self.session_binding_repo.as_ref(),
            self.lifecycle_run_repo.as_ref(),
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
        let run = self
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await
            .map_err(|e| format!("加载 lifecycle run 失败: {e}"))?
            .ok_or_else(|| format!("lifecycle run 不存在: {run_id}"))?;

        let lifecycle = self
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
                        self.lifecycle_definition_repo.as_ref(),
                        self.lifecycle_run_repo.as_ref(),
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
                        activated_phases.push(ActivatedPhaseNode {
                            node_key: node_state.step_key.clone(),
                            capability_directives: step_def.capabilities.clone(),
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

        let parent_bindings = self
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
            self.session_binding_repo
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
            self.session_binding_repo
                .create(&binding)
                .await
                .map_err(|e| format!("创建 session binding 失败: {e}"))?;
        }

        // 更新 LifecycleRun.step_states[node_key].session_id 并 activate
        let latest_run = self
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
        self.lifecycle_run_repo
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

    /// 构建 kickoff prompt，附带 input port 上下文引用和 output port 交付要求。
    async fn start_agent_node_prompt(
        &self,
        session_id: &str,
        run: &LifecycleRun,
        lifecycle: &LifecycleDefinition,
        project_id: Uuid,
        step_def: &LifecycleStepDefinition,
        executor_config: Option<agentdash_spi::AgentConfig>,
    ) -> Result<(), String> {
        let node_key = &step_def.key;
        let node_description = &step_def.description;
        self.session_hub
            .mark_owner_bootstrap_pending(session_id)
            .await
            .map_err(|e| format!("标记 owner bootstrap pending 失败: {e}"))?;

        let lifecycle_key = &lifecycle.key;
        let node_title = if node_description.trim().is_empty() {
            format!("`{node_key}`")
        } else {
            format!("`{node_key}`（{}）", node_description.trim())
        };

        // ── output port 交付要求（从 step 级 ports 读取） ──
        let output_ports = &step_def.output_ports[..];
        let writable_port_keys: Vec<String> =
            output_ports.iter().map(|p| p.key.clone()).collect();

        let output_section = if output_ports.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = output_ports
                .iter()
                .map(|p| {
                    format!(
                        "- `lifecycle://artifacts/{}` — {}",
                        p.key, p.description
                    )
                })
                .collect();
            format!(
                "\n\n## 必须交付的产出\n\
                 请将以下产出通过 `write_file` 写入对应路径：\n{}\n\n\
                 **所有 output port 写入完成后**再调用 `complete_lifecycle_node`。",
                items.join("\n")
            )
        };

        // ── input port 上下文引用（从 step 级 ports + edges 推导前驱 port output） ──
        let input_ports = &step_def.input_ports[..];
        let input_section = if input_ports.is_empty() {
            String::new()
        } else {
            let port_output_map = crate::workflow::load_port_output_map(
                self.inline_file_repo.as_ref(),
                run.id,
            )
            .await;

            let mut items = Vec::new();
            for ip in input_ports {
                // 从 edges 中找到连入当前 node + port 的边
                let source_edges: Vec<_> = lifecycle
                    .edges
                    .iter()
                    .filter(|e| e.to_node == *node_key && e.to_port == ip.key)
                    .collect();
                if source_edges.is_empty() {
                    items.push(format!(
                        "- **{}**（{}）— 无前驱连接",
                        ip.key, ip.description
                    ));
                } else {
                    for edge in source_edges {
                        let status = if port_output_map.contains_key(&edge.from_port) {
                            "已就绪"
                        } else {
                            "未就绪"
                        };
                        items.push(format!(
                            "- **{}**（{}）← `lifecycle://artifacts/{}` [{status}]",
                            ip.key, ip.description, edge.from_port
                        ));
                    }
                }
            }
            format!(
                "\n\n## 输入上下文\n以下是来自前驱节点的产出，可通过 `read_file` 读取：\n{}",
                items.join("\n")
            )
        };

        let kickoff_prompt = format!(
            "你正在执行 lifecycle `{lifecycle_key}` 的 node {node_title}。\n\
             请先完成当前阶段工作，并在完成后调用 `complete_lifecycle_node` 工具提交总结与产物。\
             {output_section}{input_section}"
        );

        // ── 构建带 port 写入权限的 lifecycle mount ──
        let lifecycle_mount =
            build_lifecycle_mount_with_ports(run.id, lifecycle_key, &writable_port_keys);
        let vfs = Vfs {
            mounts: vec![lifecycle_mount],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let mut req =
            PromptSessionRequest::from_user_input(UserPromptInput::from_text(kickoff_prompt));
        req.user_input.executor_config = executor_config;
        req.vfs = Some(vfs);

        // ── step capability resolution ──
        let workflow_baseline_caps: Vec<String> = self
            .session_hub
            .get_hook_session_runtime(&run.session_id)
            .await
            .map(|runtime| runtime.current_capabilities().into_iter().collect())
            .unwrap_or_default();
        let step_effective_caps =
            compute_effective_capabilities(&workflow_baseline_caps, &step_def.capabilities);
        if !step_effective_caps.is_empty() || self.platform_config.mcp_base_url.is_some() {
            let parent_bindings = self
                .session_binding_repo
                .list_by_session(&run.session_id)
                .await
                .unwrap_or_default();
            let owner_type = parent_bindings
                .first()
                .map(|b| b.owner_type)
                .unwrap_or(SessionOwnerType::Project);

            let cap_input = CapabilityResolverInput {
                owner_type,
                project_id,
                story_id: None,
                task_id: None,
                agent_declared_capabilities: None,
                has_active_workflow: true,
                workflow_capabilities: Some(step_effective_caps),
                agent_mcp_servers: vec![],
                companion_slice_mode: None,
            };
            let cap_output = CapabilityResolver::resolve(&cap_input, &self.platform_config);
            req.flow_capabilities = Some(cap_output.flow_capabilities);
            req.effective_capability_keys = Some(
                cap_output
                    .effective_capabilities
                    .iter()
                    .map(|c| c.key().to_string())
                    .collect(),
            );

            let mcp_configs: Vec<_> = cap_output
                .platform_mcp_configs
                .into_iter()
                .map(|c| c.to_acp_mcp_server())
                .collect();
            req.mcp_servers.extend(mcp_configs);
            req.mcp_servers.extend(cap_output.custom_mcp_servers);
        }

        self.session_hub
            .start_prompt(session_id, req)
            .await
            .map_err(|e| format!("自动启动 node session prompt 失败: {e}"))?;
        Ok(())
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
