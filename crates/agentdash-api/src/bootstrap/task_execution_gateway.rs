use std::sync::Arc;

#[allow(deprecated)]
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
    map_connector_error, map_domain_error, map_internal_error, normalize_backend_id,
    prepare_task_turn_context,
    resolve_project_scope_for_owner,
    resolve_task_backend_id,
    run_turn_monitor, TurnOutcome,
};
use agentdash_domain::{
    session_binding::SessionOwnerType,
    story::ChangeKind,
    task::Task,
};
use agentdash_application::session::{PromptSessionRequest, UserPromptInput};
use agentdash_relay::{
    CommandCancelPayload, CommandPromptPayload, AgentConfigRelay, RelayMessage,
    ResponsePromptPayload,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_domain::common::ThinkingLevel;
use crate::task_agent_context::BuiltTaskAgentContext;
use crate::{
    app_state::AppState,
    runtime_bridge::runtime_mcp_servers_to_acp,
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
}

#[allow(deprecated)]
#[async_trait]
impl TaskExecutionGateway for AppStateTaskExecutionGateway {
    async fn get_task(&self, task_id: Uuid) -> Result<Task, TaskExecutionError> {
        gw_get_task(self.repos(), task_id).await
    }

    async fn update_task(&self, task: &Task) -> Result<(), TaskExecutionError> {
        self.state.repos.task_repo.update(task).await.map_err(map_domain_error)
    }

    async fn get_backend_id_for_task(&self, task: &Task) -> Result<String, TaskExecutionError> {
        resolve_task_backend_id(self.repos(), &self.availability(), task).await
    }

    async fn append_task_change(
        &self, task_id: Uuid, backend_id: &str, kind: ChangeKind, payload: Value,
    ) -> Result<(), TaskExecutionError> {
        gw_append_task_change(self.repos(), task_id, backend_id, kind, payload)
            .await.map_err(map_domain_error)
    }

    async fn create_task_session(&self, task: &Task) -> Result<String, TaskExecutionError> {
        Ok(gw_create_task_session(&self.state.services.session_hub, task).await?.id)
    }

    async fn start_task_turn(
        &self,
        task: &Task,
        phase: ExecutionPhase,
        override_prompt: Option<&str>,
        additional_prompt: Option<&str>,
        executor_config: Option<agentdash_domain::common::AgentConfig>,
    ) -> Result<StartedTurn, TaskExecutionError> {
        let session_id = task.session_id.as_deref()
            .ok_or_else(|| TaskExecutionError::Internal("Task 未绑定 session".into()))?;

        let ctx = prepare_task_turn_context(
            self.repos(),
            &self.availability(),
            &self.state.services.address_space_service,
            &self.state.services.contributor_registry,
            task, phase, override_prompt, additional_prompt,
            executor_config.as_ref(),
            self.state.config.mcp_base_url.as_deref(),
        ).await?;

        if ctx.use_cloud_native_agent {
            dispatch_cloud_native(
                &self.state, session_id, ctx.built, ctx.address_space,
                ctx.resolved_config.as_ref(), ctx.workspace.as_ref(),
            ).await
        } else {
            dispatch_relay(
                &self.state, session_id, &ctx.built, ctx.resolved_config,
                ctx.workspace.as_ref(),
            ).await
        }
    }

    async fn bind_session_to_owner(
        &self, session_id: &str, owner_type: &str, owner_id: Uuid, label: &str,
    ) -> Result<(), TaskExecutionError> {
        let owner_type = SessionOwnerType::from_str_loose(owner_type).ok_or_else(|| {
            TaskExecutionError::BadRequest(format!("无效的 owner_type: {owner_type}"))
        })?;
        let project_id = resolve_project_scope_for_owner(self.repos(), owner_type, owner_id).await?;
        let binding = agentdash_domain::session_binding::SessionBinding::new(
            project_id, session_id.to_string(), owner_type, owner_id, label,
        );
        self.state.repos.session_binding_repo.create(&binding).await.map_err(map_domain_error)
    }

    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError> {
        let remote_backend = self.state.remote_sessions.read().await.get(session_id).cloned();
        if let Some(backend_id) = remote_backend {
            relay_cancel(&self.state, &backend_id, session_id).await
        } else {
            self.state.services.session_hub.cancel(session_id).await.map_err(map_connector_error)
        }
    }

    async fn get_session_overview(
        &self, session_id: &str,
    ) -> Result<Option<agentdash_application::task::execution::SessionOverview>, TaskExecutionError> {
        gw_get_session_overview(&self.state.services.session_hub, session_id).await
    }

    async fn bridge_task_status_event_to_session(
        &self, session_id: &str, turn_id: &str, event_type: &str, message: &str, data: Value,
    ) {
        let notification = bridge_task_status_event_to_session_notification(
            session_id, turn_id, event_type, message, data,
        );
        if let Err(err) = self.state.services.session_hub
            .inject_notification(session_id, notification).await
        {
            tracing::warn!(session_id, turn_id, event_type, error = %err,
                "桥接 Task 生命周期事件到 session 流失败");
        }
    }

    fn spawn_task_turn_monitor(
        &self, task_id: Uuid, session_id: String, turn_id: String, backend_id: String,
    ) {
        spawn_task_turn_monitor(self.state.clone(), task_id, session_id, turn_id, backend_id);
    }
}

// ─── dispatch helpers ───────────────────────────────────────

async fn dispatch_cloud_native(
    state: &Arc<AppState>,
    session_id: &str,
    built: BuiltTaskAgentContext,
    address_space: Option<agentdash_domain::common::AddressSpace>,
    resolved_config: Option<&agentdash_application::runtime::AgentConfig>,
    workspace: Option<&agentdash_domain::workspace::Workspace>,
) -> Result<StartedTurn, TaskExecutionError> {
    let resolved_binding = if let Some(ws) = workspace {
        Some(resolve_workspace_binding(state, ws).await.map_err(map_internal_error)?)
    } else {
        None
    };
    let workspace_root = resolved_binding
        .as_ref()
        .map(|item| std::path::PathBuf::from(item.root_ref.clone()));

    let prompt_req = PromptSessionRequest {
        user_input: UserPromptInput {
            prompt: None,
            prompt_blocks: Some(built.prompt_blocks),
            working_dir: built.working_dir,
            env: Default::default(),
            executor_config: resolved_config.cloned(),
        },
        mcp_servers: runtime_mcp_servers_to_acp(&built.mcp_servers),
        workspace_root,
        address_space: address_space.clone(),
        flow_capabilities: Some(agentdash_executor::FlowCapabilities {
            workflow_artifact: true,
            companion_dispatch: false,
            companion_complete: true,
            resolve_hook_action: true,
        }),
        system_context: built.system_context.clone(),
    };

    let turn_id = state.services.session_hub
        .start_prompt(session_id, prompt_req).await
        .map_err(map_connector_error)?;

    Ok(StartedTurn { turn_id, context_sources: built.source_summary })
}

async fn dispatch_relay(
    state: &Arc<AppState>,
    session_id: &str,
    built: &BuiltTaskAgentContext,
    resolved_config: Option<agentdash_application::runtime::AgentConfig>,
    workspace: Option<&agentdash_domain::workspace::Workspace>,
) -> Result<StartedTurn, TaskExecutionError> {
    let ws = workspace.ok_or_else(|| {
        TaskExecutionError::BadRequest(
            "第三方 Agent 任务必须绑定 Workspace，且运行位置由 Workspace.backend_id 决定".into(),
        )
    })?;
    let resolved_binding = resolve_workspace_binding(state, ws).await.map_err(map_internal_error)?;
    let backend_id = normalize_backend_id(&resolved_binding.backend_id)?;

    if !state.services.backend_registry.is_online(backend_id).await {
        return Err(TaskExecutionError::Conflict(format!(
            "目标 Workspace 所属 Backend 当前不在线: {backend_id}"
        )));
    }

    let turn_id = relay_start_prompt(
        state, backend_id, session_id, built, resolved_config, &resolved_binding,
    ).await?;

    state.remote_sessions.write().await
        .insert(session_id.to_string(), backend_id.to_string());

    Ok(StartedTurn { turn_id, context_sources: built.source_summary.clone() })
}

// ─── public execute_* wrappers ──────────────────────────────

#[allow(deprecated)]
pub async fn execute_start_task(
    state: Arc<AppState>, task_id: Uuid,
    override_prompt: Option<String>,
    executor_config: Option<agentdash_domain::common::AgentConfig>,
) -> Result<StartTaskResult, TaskExecutionError> {
    state.task_runtime.lock_map.with_lock(task_id, || async {
        let gw = AppStateTaskExecutionGateway::new(state.clone());
        agentdash_application::task_execution::start_task(
            &gw, StartTaskCommand { task_id, override_prompt, executor_config },
        ).await
    }).await
}

#[allow(deprecated)]
pub async fn execute_continue_task(
    state: Arc<AppState>, task_id: Uuid,
    additional_prompt: Option<String>,
    executor_config: Option<agentdash_domain::common::AgentConfig>,
) -> Result<ContinueTaskResult, TaskExecutionError> {
    state.task_runtime.lock_map.with_lock(task_id, || async {
        let gw = AppStateTaskExecutionGateway::new(state.clone());
        agentdash_application::task_execution::continue_task(
            &gw, ContinueTaskCommand { task_id, additional_prompt, executor_config },
        ).await
    }).await
}

#[allow(deprecated)]
pub async fn execute_cancel_task(
    state: Arc<AppState>, task_id: Uuid,
) -> Result<Task, TaskExecutionError> {
    let result = state.task_runtime.lock_map.with_lock(task_id, || async {
        let gw = AppStateTaskExecutionGateway::new(state.clone());
        agentdash_application::task_execution::cancel_task(&gw, task_id).await
    }).await;
    if result.is_ok() { state.task_runtime.restart_tracker.clear(task_id); }
    result
}

#[allow(deprecated)]
pub async fn execute_get_task_session(
    state: Arc<AppState>, task_id: Uuid,
) -> Result<TaskSessionResult, TaskExecutionError> {
    let gw = AppStateTaskExecutionGateway::new(state);
    agentdash_application::task_execution::get_task_session(&gw, task_id).await
}

// ─── turn monitoring ────────────────────────────────────────

fn spawn_task_turn_monitor(
    state: Arc<AppState>, task_id: Uuid,
    session_id: String, turn_id: String, backend_id: String,
) {
    tokio::spawn(async move {
        let outcome = run_turn_monitor(
            &state.repos, &state.services.session_hub,
            &state.task_runtime.restart_tracker,
            task_id, &session_id, &turn_id, &backend_id,
        ).await;
        if let TurnOutcome::NeedsRetry { delay, attempt } = outcome {
            schedule_auto_retry(state, task_id, session_id, backend_id, delay, attempt).await;
        }
    });
}

async fn schedule_auto_retry(
    state: Arc<AppState>, task_id: Uuid,
    session_id: String, backend_id: String,
    delay: std::time::Duration, attempt: u32,
) {
    tracing::info!(task_id = %task_id, session_id = %session_id,
        attempt, delay_ms = delay.as_millis() as u64, "等待退避延迟后自动重试");
    tokio::time::sleep(delay).await;

    let retry_prompt = format!("上一次执行失败，这是第 {} 次自动重试。请继续完成任务。", attempt);
    match execute_continue_task(state.clone(), task_id, Some(retry_prompt), None).await {
        Ok(result) => {
            tracing::info!(task_id = %task_id, session_id = %session_id,
                new_turn_id = %result.turn_id, attempt, "自动重试已成功发起新 turn");
        }
        Err(err) => {
            tracing::error!(task_id = %task_id, session_id = %session_id,
                attempt, "自动重试发起失败，标记 Task 为 Failed: {}", err);
            let _ = agentdash_application::task::gateway::update_task_status(
                &state.repos, task_id, &backend_id,
                agentdash_domain::task::TaskStatus::Failed, "auto_retry_failed",
                json!({ "session_id": session_id, "attempt": attempt, "error": err.to_string() }),
            ).await;
        }
    }
}

pub(crate) use agentdash_application::task::config::resolve_task_agent_config;

// ─── relay helpers ──────────────────────────────────────────

async fn relay_start_prompt(
    state: &Arc<AppState>, backend_id: &str, session_id: &str,
    built: &BuiltTaskAgentContext,
    executor_config: Option<agentdash_domain::common::AgentConfig>,
    binding: &ResolvedWorkspaceBinding,
) -> Result<String, TaskExecutionError> {
    let relay_config = executor_config.as_ref()
        .map(|c| AgentConfigRelay {
            executor: c.executor.clone(), variant: c.variant.clone(),
            provider_id: c.provider_id.clone(), model_id: c.model_id.clone(),
            agent_id: c.agent_id.clone(),
            thinking_level: c.thinking_level.map(|level| match level {
                ThinkingLevel::Off => "off",
                ThinkingLevel::Minimal => "minimal",
                ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High => "high",
                ThinkingLevel::Xhigh => "xhigh",
            }.to_string()),
            permission_policy: c.permission_policy.clone(),
        });

    let cmd = RelayMessage::CommandPrompt {
        id: RelayMessage::new_id("prompt"),
        payload: Box::new(CommandPromptPayload {
            session_id: session_id.to_string(),
            follow_up_session_id: None,
            prompt: None,
            prompt_blocks: Some(serde_json::Value::Array(built.prompt_blocks.clone())),
            workspace_root: binding.root_ref.clone(),
            working_dir: built.working_dir.clone(),
            env: Default::default(),
            executor_config: relay_config,
            mcp_servers: runtime_mcp_servers_to_acp(&built.mcp_servers)
                .iter().filter_map(|s| serde_json::to_value(s).ok()).collect(),
        }),
    };

    tracing::info!(backend_id, session_id, "中继 command.prompt → 远程后端");
    let resp = state.services.backend_registry.send_command(backend_id, cmd).await
        .map_err(|e| TaskExecutionError::Internal(format!("中继 prompt 失败: {e}")))?;

    match resp {
        RelayMessage::ResponsePrompt { payload: Some(ResponsePromptPayload { turn_id, .. }), error: None, .. } => Ok(turn_id),
        RelayMessage::ResponsePrompt { error: Some(err), .. } =>
            Err(TaskExecutionError::Internal(format!("远程后端执行失败: {}", err.message))),
        other => Err(TaskExecutionError::Internal(format!("远程后端返回意外响应: {}", other.id()))),
    }
}

async fn relay_cancel(
    state: &Arc<AppState>, backend_id: &str, session_id: &str,
) -> Result<(), TaskExecutionError> {
    tracing::info!(backend_id, session_id, "中继 command.cancel → 远程后端");
    let cmd = RelayMessage::CommandCancel {
        id: RelayMessage::new_id("cancel"),
        payload: CommandCancelPayload { session_id: session_id.to_string() },
    };
    let resp = state.services.backend_registry.send_command(backend_id, cmd).await
        .map_err(|e| TaskExecutionError::Internal(format!("中继 cancel 失败: {e}")))?;

    match resp {
        RelayMessage::ResponseCancel { error: None, .. } => Ok(()),
        RelayMessage::ResponseCancel { error: Some(err), .. } =>
            Err(TaskExecutionError::Internal(format!("远程取消失败: {}", err.message))),
        _ => Ok(()),
    }
}
