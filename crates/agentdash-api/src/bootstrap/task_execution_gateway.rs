use std::sync::Arc;

use crate::address_space_access::SessionMountTarget;
use agentdash_application::task::config::resolve_task_executor_config;
use agentdash_application::task::execution::{
    ContinueTaskCommand, ContinueTaskResult, ExecutionPhase, StartTaskCommand,
    StartTaskResult, StartedTurn, TaskExecutionError, TaskExecutionGateway, TaskSessionResult,
};
use agentdash_application::task::gateway::{
    append_task_change as gw_append_task_change,
    bridge_task_status_event_to_session_notification,
    create_task_session as gw_create_task_session,
    get_session_overview as gw_get_session_overview,
    get_task as gw_get_task,
    load_related_context,
    map_connector_error, map_domain_error, map_internal_error, normalize_backend_id,
    resolve_project_scope_for_owner,
    resolve_task_backend_id,
    run_turn_monitor, TurnOutcome,
};
use agentdash_domain::{
    session_binding::SessionOwnerType,
    story::ChangeKind,
    task::Task,
};
use agentdash_executor::PromptSessionRequest;
use agentdash_relay::{
    CommandCancelPayload, CommandPromptPayload, ExecutorConfigRelay, RelayMessage,
    ResponsePromptPayload,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_mcp::injection::McpInjectionConfig;

use crate::task_agent_context::BuiltTaskAgentContext;
use crate::{
    app_state::AppState,
    runtime_bridge::{
        connector_executor_config_to_runtime, mcp_injection_config_to_runtime_binding,
        runtime_executor_config_to_connector, runtime_mcp_servers_to_acp,
    },
    task_agent_context::{
        ContextContributor, McpContextContributor, StaticFragmentsContributor, TaskAgentBuildInput,
        TaskExecutionPhase, build_declared_source_warning_fragment, build_task_agent_context,
        resolve_workspace_declared_sources,
    },
    workspace_resolution::{AppStateBackendAvailability, ResolvedWorkspaceBinding, resolve_workspace_binding},
};

pub struct AppStateTaskExecutionGateway {
    state: Arc<AppState>,
}

impl AppStateTaskExecutionGateway {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    fn repos(&self) -> &agentdash_application::repository_set::RepositorySet {
        &self.state.repos
    }

    fn availability(&self) -> AppStateBackendAvailability {
        AppStateBackendAvailability::new(self.state.clone())
    }

    async fn find_active_lifecycle_run(
        &self,
        task: &Task,
    ) -> Result<Option<agentdash_domain::workflow::LifecycleRun>, TaskExecutionError> {
        let runs = self
            .state
            .repos
            .lifecycle_run_repo
            .list_by_target(
                agentdash_domain::workflow::WorkflowTargetKind::Task,
                task.id,
            )
            .await
            .map_err(|e| TaskExecutionError::Internal(format!("查询 lifecycle runs 失败: {e}")))?;
        Ok(agentdash_application::workflow::select_active_run(runs))
    }

    async fn resolve_lifecycle_key(&self, lifecycle_id: Uuid) -> String {
        match self
            .state
            .repos
            .lifecycle_definition_repo
            .get_by_id(lifecycle_id)
            .await
        {
            Ok(Some(def)) => def.key,
            _ => "unknown".to_string(),
        }
    }
}

#[async_trait]
impl TaskExecutionGateway<agentdash_executor::AgentDashExecutorConfig>
    for AppStateTaskExecutionGateway
{
    async fn get_task(&self, task_id: Uuid) -> Result<Task, TaskExecutionError> {
        gw_get_task(self.repos(), task_id).await
    }

    async fn update_task(&self, task: &Task) -> Result<(), TaskExecutionError> {
        self.state
            .repos
            .task_repo
            .update(task)
            .await
            .map_err(map_domain_error)
    }

    async fn get_backend_id_for_task(&self, task: &Task) -> Result<String, TaskExecutionError> {
        resolve_task_backend_id(self.repos(), &self.availability(), task).await
    }

    async fn append_task_change(
        &self,
        task_id: Uuid,
        backend_id: &str,
        kind: ChangeKind,
        payload: Value,
    ) -> Result<(), TaskExecutionError> {
        gw_append_task_change(self.repos(), task_id, backend_id, kind, payload)
            .await
            .map_err(map_domain_error)
    }

    async fn create_task_session(&self, task: &Task) -> Result<String, TaskExecutionError> {
        let session = gw_create_task_session(&self.state.services.executor_hub, task).await?;
        Ok(session.id)
    }

    async fn start_task_turn(
        &self,
        task: &Task,
        phase: ExecutionPhase,
        override_prompt: Option<&str>,
        additional_prompt: Option<&str>,
        executor_config: Option<agentdash_executor::AgentDashExecutorConfig>,
    ) -> Result<StartedTurn, TaskExecutionError> {
        let (story, project, workspace) = load_related_context(self.repos(), task)
            .await
            .map_err(map_internal_error)?;

        let mut extra_contributors: Vec<Box<dyn ContextContributor>> = Vec::new();
        let mut declared_sources = story.context.source_refs.clone();
        declared_sources.extend(task.agent_binding.context_sources.clone());
        let resolved_workspace_sources = resolve_workspace_declared_sources(
            &self.state,
            &declared_sources,
            workspace.as_ref(),
            86,
        )
        .await
        .map_err(TaskExecutionError::UnprocessableEntity)?;

        if !resolved_workspace_sources.fragments.is_empty() {
            extra_contributors.push(Box::new(StaticFragmentsContributor::new(
                resolved_workspace_sources.fragments,
            )));
        }
        if !resolved_workspace_sources.warnings.is_empty() {
            extra_contributors.push(Box::new(StaticFragmentsContributor::new(vec![
                build_declared_source_warning_fragment(
                    "declared_source_warnings",
                    96,
                    &resolved_workspace_sources.warnings,
                ),
            ])));
        }

        if let Some(base_url) = &self.state.config.mcp_base_url {
            let config = McpInjectionConfig::for_task(
                base_url.clone(),
                story.project_id,
                task.story_id,
                task.id,
            );
            extra_contributors.push(Box::new(McpContextContributor::new(
                mcp_injection_config_to_runtime_binding(&config),
            )));
        }

        let session_id = task
            .session_id
            .as_deref()
            .ok_or_else(|| TaskExecutionError::Internal("Task 未绑定 session".into()))?;

        let resolved_config = resolve_task_executor_config(
            executor_config
                .as_ref()
                .map(connector_executor_config_to_runtime),
            task,
            &project,
        )
        .map_err(map_internal_error)?;
        let use_cloud_native_agent = resolved_config
            .as_ref()
            .is_some_and(|config| runtime_executor_config_to_connector(config).is_native_agent());

        let address_space = if use_cloud_native_agent {
            let agent_type = resolved_config
                .as_ref()
                .map(|config| config.executor.as_str());
            let mut space = self
                    .state
                    .services
                    .address_space_service
                    .build_address_space(&project, Some(&story), workspace.as_ref(), SessionMountTarget::Task, agent_type)
                    .map_err(map_internal_error)?;

            if let Ok(Some(active_run)) = self.find_active_lifecycle_run(task).await {
                let lifecycle_key = self.resolve_lifecycle_key(active_run.lifecycle_id).await;
                space.mounts.push(
                    agentdash_application::address_space::build_lifecycle_mount(
                        active_run.id,
                        &lifecycle_key,
                    ),
                );
            }

            Some(space)
        } else {
            None
        };

        let built = build_task_agent_context(
            TaskAgentBuildInput {
                task,
                story: &story,
                project: &project,
                workspace: workspace.as_ref(),
                address_space: address_space.as_ref(),
                effective_agent_type: resolved_config
                    .as_ref()
                    .map(|config| config.executor.as_str()),
                phase: match phase {
                    ExecutionPhase::Start => TaskExecutionPhase::Start,
                    ExecutionPhase::Continue => TaskExecutionPhase::Continue,
                },
                override_prompt,
                additional_prompt,
                extra_contributors,
            },
            &self.state.services.contributor_registry,
        )
        .map_err(TaskExecutionError::UnprocessableEntity)?;

        if use_cloud_native_agent {
            let resolved_binding = if let Some(workspace) = workspace.as_ref() {
                Some(
                    resolve_workspace_binding(&self.state, workspace)
                        .await
                        .map_err(map_internal_error)?,
                )
            } else {
                None
            };
            let workspace_root = resolved_binding
                .as_ref()
                .map(|item| std::path::PathBuf::from(item.root_ref.clone()));
            let prompt_req = PromptSessionRequest {
                prompt: None,
                prompt_blocks: Some(built.prompt_blocks),
                working_dir: built.working_dir,
                env: Default::default(),
                executor_config: resolved_config
                    .as_ref()
                    .map(runtime_executor_config_to_connector),
                mcp_servers: runtime_mcp_servers_to_acp(&built.mcp_servers),
                workspace_root,
                address_space: address_space.clone(),
                flow_capabilities: Some(agentdash_executor::FlowCapabilities {
                    workflow_artifact: true,
                    companion_dispatch: false,
                    companion_complete: true,
                    resolve_hook_action: true,
                }),
                system_context: None,
            };

            let turn_id = self
                .state
                .services
                .executor_hub
                .start_prompt(session_id, prompt_req)
                .await
                .map_err(map_connector_error)?;

            Ok(StartedTurn {
                turn_id,
                context_sources: built.source_summary,
            })
        } else {
            let workspace = workspace.as_ref().ok_or_else(|| {
                TaskExecutionError::BadRequest(
                    "第三方 Agent 任务必须绑定 Workspace，且运行位置由 Workspace.backend_id 决定"
                        .into(),
                )
            })?;
            let resolved_binding = resolve_workspace_binding(&self.state, workspace)
                .await
                .map_err(map_internal_error)?;
            let backend_id = normalize_backend_id(&resolved_binding.backend_id)?;
            if !self
                .state
                .services
                .backend_registry
                .is_online(backend_id)
                .await
            {
                return Err(TaskExecutionError::Conflict(format!(
                    "目标 Workspace 所属 Backend 当前不在线: {backend_id}"
                )));
            }

            let turn_id = relay_start_prompt(
                &self.state,
                backend_id,
                session_id,
                &built,
                resolved_config.clone(),
                &resolved_binding,
            )
            .await?;

            self.state
                .remote_sessions
                .write()
                .await
                .insert(session_id.to_string(), backend_id.to_string());

            Ok(StartedTurn {
                turn_id,
                context_sources: built.source_summary,
            })
        }
    }

    async fn bind_session_to_owner(
        &self,
        session_id: &str,
        owner_type: &str,
        owner_id: Uuid,
        label: &str,
    ) -> Result<(), TaskExecutionError> {
        let owner_type = SessionOwnerType::from_str_loose(owner_type).ok_or_else(|| {
            TaskExecutionError::BadRequest(format!("无效的 owner_type: {owner_type}"))
        })?;
        let project_id = resolve_project_scope_for_owner(self.repos(), owner_type, owner_id).await?;
        let binding = agentdash_domain::session_binding::SessionBinding::new(
            project_id,
            session_id.to_string(),
            owner_type,
            owner_id,
            label,
        );
        self.state
            .repos
            .session_binding_repo
            .create(&binding)
            .await
            .map_err(map_domain_error)
    }

    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError> {
        let remote_backend = self
            .state
            .remote_sessions
            .read()
            .await
            .get(session_id)
            .cloned();

        if let Some(backend_id) = remote_backend {
            relay_cancel(&self.state, &backend_id, session_id).await
        } else {
            self.state
                .services
                .executor_hub
                .cancel(session_id)
                .await
                .map_err(map_connector_error)
        }
    }

    async fn get_session_overview(
        &self,
        session_id: &str,
    ) -> Result<Option<agentdash_application::task::execution::SessionOverview>, TaskExecutionError> {
        gw_get_session_overview(&self.state.services.executor_hub, session_id).await
    }

    async fn bridge_task_status_event_to_session(
        &self,
        session_id: &str,
        turn_id: &str,
        event_type: &str,
        message: &str,
        data: Value,
    ) {
        let notification = bridge_task_status_event_to_session_notification(
            session_id, turn_id, event_type, message, data,
        );
        if let Err(err) = self
            .state
            .services
            .executor_hub
            .inject_notification(session_id, notification)
            .await
        {
            tracing::warn!(
                session_id = %session_id,
                turn_id = %turn_id,
                event_type = %event_type,
                error = %err,
                "桥接 Task 生命周期事件到 session 流失败"
            );
        }
    }

    fn spawn_task_turn_monitor(
        &self,
        task_id: Uuid,
        session_id: String,
        turn_id: String,
        backend_id: String,
    ) {
        spawn_task_turn_monitor(self.state.clone(), task_id, session_id, turn_id, backend_id);
    }
}

// ─── public execute_* wrappers (api entry points) ───────────

pub async fn execute_start_task(
    state: Arc<AppState>,
    task_id: Uuid,
    override_prompt: Option<String>,
    executor_config: Option<agentdash_executor::AgentDashExecutorConfig>,
) -> Result<StartTaskResult, TaskExecutionError> {
    state
        .task_runtime
        .lock_map
        .with_lock(task_id, || async {
            let gateway = AppStateTaskExecutionGateway::new(state.clone());
            agentdash_application::task_execution::start_task(
                &gateway,
                StartTaskCommand {
                    task_id,
                    override_prompt,
                    executor_config,
                },
            )
            .await
        })
        .await
}

pub async fn execute_continue_task(
    state: Arc<AppState>,
    task_id: Uuid,
    additional_prompt: Option<String>,
    executor_config: Option<agentdash_executor::AgentDashExecutorConfig>,
) -> Result<ContinueTaskResult, TaskExecutionError> {
    state
        .task_runtime
        .lock_map
        .with_lock(task_id, || async {
            let gateway = AppStateTaskExecutionGateway::new(state.clone());
            agentdash_application::task_execution::continue_task(
                &gateway,
                ContinueTaskCommand {
                    task_id,
                    additional_prompt,
                    executor_config,
                },
            )
            .await
        })
        .await
}

pub async fn execute_cancel_task(
    state: Arc<AppState>,
    task_id: Uuid,
) -> Result<Task, TaskExecutionError> {
    let result = state
        .task_runtime
        .lock_map
        .with_lock(task_id, || async {
            let gateway = AppStateTaskExecutionGateway::new(state.clone());
            agentdash_application::task_execution::cancel_task(&gateway, task_id).await
        })
        .await;

    if result.is_ok() {
        state.task_runtime.restart_tracker.clear(task_id);
    }

    result
}

pub async fn execute_get_task_session(
    state: Arc<AppState>,
    task_id: Uuid,
) -> Result<TaskSessionResult, TaskExecutionError> {
    let gateway = AppStateTaskExecutionGateway::new(state);
    agentdash_application::task_execution::get_task_session(&gateway, task_id).await
}

// ─── turn monitoring (spawn + auto-retry stay in api) ───────

fn spawn_task_turn_monitor(
    state: Arc<AppState>,
    task_id: Uuid,
    session_id: String,
    turn_id: String,
    backend_id: String,
) {
    tokio::spawn(async move {
        let outcome = run_turn_monitor(
            &state.repos,
            &state.services.executor_hub,
            &state.task_runtime.restart_tracker,
            task_id,
            &session_id,
            &turn_id,
            &backend_id,
        )
        .await;

        if let TurnOutcome::NeedsRetry { delay, attempt } = outcome {
            schedule_auto_retry(state, task_id, session_id, backend_id, delay, attempt).await;
        }
    });
}

async fn schedule_auto_retry(
    state: Arc<AppState>,
    task_id: Uuid,
    session_id: String,
    backend_id: String,
    delay: std::time::Duration,
    attempt: u32,
) {
    tracing::info!(
        task_id = %task_id,
        session_id = %session_id,
        attempt = attempt,
        delay_ms = delay.as_millis() as u64,
        "等待退避延迟后自动重试"
    );

    tokio::time::sleep(delay).await;

    let retry_prompt = format!(
        "上一次执行失败，这是第 {} 次自动重试。请继续完成任务。",
        attempt
    );

    match execute_continue_task(state.clone(), task_id, Some(retry_prompt), None).await {
        Ok(result) => {
            tracing::info!(
                task_id = %task_id,
                session_id = %session_id,
                new_turn_id = %result.turn_id,
                attempt = attempt,
                "自动重试已成功发起新 turn"
            );
        }
        Err(err) => {
            tracing::error!(
                task_id = %task_id,
                session_id = %session_id,
                attempt = attempt,
                "自动重试发起失败，标记 Task 为 Failed: {}",
                err
            );
            let _ = agentdash_application::task::gateway::update_task_status(
                &state.repos,
                task_id,
                &backend_id,
                agentdash_domain::task::TaskStatus::Failed,
                "auto_retry_failed",
                json!({
                    "session_id": session_id,
                    "attempt": attempt,
                    "error": err.to_string(),
                }),
            )
            .await;
        }
    }
}

/// Re-export for backward compat within api crate
pub(crate) use agentdash_application::task::config::resolve_task_agent_config;

// ─── relay helpers (depend on api-layer BackendRegistry) ────

async fn relay_start_prompt(
    state: &Arc<AppState>,
    backend_id: &str,
    session_id: &str,
    built: &BuiltTaskAgentContext,
    executor_config: Option<agentdash_application::runtime::ExecutorConfig>,
    binding: &ResolvedWorkspaceBinding,
) -> Result<String, TaskExecutionError> {
    let relay_config = executor_config
        .as_ref()
        .map(runtime_executor_config_to_connector)
        .map(|c| ExecutorConfigRelay {
        executor: c.executor,
        variant: c.variant,
        provider_id: c.provider_id,
        model_id: c.model_id,
        agent_id: c.agent_id,
        thinking_level: c.thinking_level.map(|level| {
            match level {
                agentdash_executor::ThinkingLevel::Off => "off",
                agentdash_executor::ThinkingLevel::Minimal => "minimal",
                agentdash_executor::ThinkingLevel::Low => "low",
                agentdash_executor::ThinkingLevel::Medium => "medium",
                agentdash_executor::ThinkingLevel::High => "high",
                agentdash_executor::ThinkingLevel::Xhigh => "xhigh",
            }
            .to_string()
        }),
        permission_policy: c.permission_policy,
    });

    let cmd = RelayMessage::CommandPrompt {
        id: RelayMessage::new_id("prompt"),
        payload: CommandPromptPayload {
            session_id: session_id.to_string(),
            follow_up_session_id: None,
            prompt: None,
            prompt_blocks: Some(serde_json::Value::Array(built.prompt_blocks.clone())),
            workspace_root: binding.root_ref.clone(),
            working_dir: built.working_dir.clone(),
            env: Default::default(),
            executor_config: relay_config,
            mcp_servers: runtime_mcp_servers_to_acp(&built.mcp_servers)
                .iter()
                .filter_map(|s| serde_json::to_value(s).ok())
                .collect(),
        },
    };

    tracing::info!(
        backend_id = %backend_id,
        session_id = %session_id,
        "中继 command.prompt → 远程后端"
    );

    let resp = state
        .services
        .backend_registry
        .send_command(backend_id, cmd)
        .await
        .map_err(|e| TaskExecutionError::Internal(format!("中继 prompt 失败: {e}")))?;

    match resp {
        RelayMessage::ResponsePrompt {
            payload: Some(ResponsePromptPayload { turn_id, .. }),
            error: None,
            ..
        } => Ok(turn_id),
        RelayMessage::ResponsePrompt {
            error: Some(err), ..
        } => Err(TaskExecutionError::Internal(format!(
            "远程后端执行失败: {}",
            err.message
        ))),
        other => Err(TaskExecutionError::Internal(format!(
            "远程后端返回意外响应: {}",
            other.id()
        ))),
    }
}

async fn relay_cancel(
    state: &Arc<AppState>,
    backend_id: &str,
    session_id: &str,
) -> Result<(), TaskExecutionError> {
    tracing::info!(
        backend_id = %backend_id,
        session_id = %session_id,
        "中继 command.cancel → 远程后端"
    );

    let cmd = RelayMessage::CommandCancel {
        id: RelayMessage::new_id("cancel"),
        payload: CommandCancelPayload {
            session_id: session_id.to_string(),
        },
    };

    let resp = state
        .services
        .backend_registry
        .send_command(backend_id, cmd)
        .await
        .map_err(|e| TaskExecutionError::Internal(format!("中继 cancel 失败: {e}")))?;

    match resp {
        RelayMessage::ResponseCancel { error: None, .. } => Ok(()),
        RelayMessage::ResponseCancel {
            error: Some(err), ..
        } => Err(TaskExecutionError::Internal(format!(
            "远程取消失败: {}",
            err.message
        ))),
        _ => Ok(()),
    }
}
