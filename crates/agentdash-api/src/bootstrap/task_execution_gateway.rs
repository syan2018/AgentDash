use std::sync::Arc;

use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
use agentdash_application::task::artifact::{
    build_tool_call_patch, build_tool_call_update_patch, upsert_tool_execution_artifact,
};
use agentdash_application::task::config::resolve_task_executor_config;
use agentdash_application::task::meta::{
    build_task_lifecycle_meta, parse_turn_event, turn_matches,
};
use agentdash_application::task_execution::{
    ContinueTaskCommand, ContinueTaskResult, ExecutionPhase, SessionOverview, StartTaskCommand,
    StartTaskResult, StartedTurn, TaskExecutionError, TaskExecutionGateway, TaskSessionResult,
};
use agentdash_application::task_restart_tracker::RestartDecision;
use agentdash_domain::{
    project::Project,
    session_binding::{SessionBinding, SessionOwnerType},
    story::{ChangeKind, Story},
    task::{Artifact, ArtifactType, Task, TaskExecutionMode, TaskStatus},
    workspace::Workspace,
};
use agentdash_executor::{ConnectorError, PromptSessionRequest, SessionMeta};
use agentdash_relay::{
    CommandCancelPayload, CommandPromptPayload, ExecutorConfigRelay, RelayMessage,
    ResponsePromptPayload,
};
use async_trait::async_trait;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use agentdash_mcp::injection::McpInjectionConfig;

use crate::task_agent_context::BuiltTaskAgentContext;
use crate::{
    app_state::AppState,
    task_agent_context::{
        ContextContributor, McpContextContributor, StaticFragmentsContributor, TaskAgentBuildInput,
        TaskExecutionPhase, build_declared_source_warning_fragment, build_task_agent_context,
        resolve_workspace_declared_sources,
    },
};

pub struct AppStateTaskExecutionGateway {
    state: Arc<AppState>,
}

impl AppStateTaskExecutionGateway {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl TaskExecutionGateway<agentdash_executor::AgentDashExecutorConfig>
    for AppStateTaskExecutionGateway
{
    async fn get_task(&self, task_id: Uuid) -> Result<Task, TaskExecutionError> {
        get_task(&self.state, task_id).await
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
        resolve_task_backend_id(&self.state, task).await
    }

    async fn append_task_change(
        &self,
        task_id: Uuid,
        backend_id: &str,
        kind: ChangeKind,
        payload: Value,
    ) -> Result<(), TaskExecutionError> {
        append_task_change(&self.state, task_id, backend_id, kind, payload)
            .await
            .map_err(map_domain_error)
    }

    async fn create_task_session(&self, task: &Task) -> Result<String, TaskExecutionError> {
        let session = create_task_session(&self.state, task)
            .await
            .map_err(map_internal_error)?;
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
        let (story, project, workspace) = load_related_context(&self.state, task)
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
            extra_contributors.push(Box::new(McpContextContributor::new(config)));
        }

        let session_id = task
            .session_id
            .as_deref()
            .ok_or_else(|| TaskExecutionError::Internal("Task 未绑定 session".into()))?;

        let resolved_config = resolve_task_executor_config(executor_config, task, &project)
            .map_err(map_internal_error)?;

        let use_cloud_native_agent = resolved_config
            .as_ref()
            .is_some_and(|config| config.is_native_agent());
        let address_space = if use_cloud_native_agent {
            let agent_type = resolved_config
                .as_ref()
                .map(|config| config.executor.as_str());
            Some(
                self.state
                    .services
                    .address_space_service
                    .build_task_address_space(&project, &story, workspace.as_ref(), agent_type)
                    .map_err(map_internal_error)?,
            )
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
            let workspace_root = workspace
                .as_ref()
                .map(|item| std::path::PathBuf::from(item.container_ref.clone()));
            let prompt_req = PromptSessionRequest {
                prompt: None,
                prompt_blocks: Some(built.prompt_blocks),
                working_dir: built.working_dir,
                env: Default::default(),
                executor_config: resolved_config,
                mcp_servers: built.mcp_servers,
                workspace_root,
                address_space,
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
            let backend_id = normalize_backend_id(&workspace.backend_id)?;
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
                resolved_config,
                workspace,
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
        let binding = SessionBinding::new(session_id.to_string(), owner_type, owner_id, label);
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
    ) -> Result<Option<SessionOverview>, TaskExecutionError> {
        let meta = self
            .state
            .services
            .executor_hub
            .get_session_meta(session_id)
            .await
            .map_err(map_internal_error)?;
        Ok(meta.map(|value| SessionOverview {
            title: value.title,
            updated_at: value.updated_at,
        }))
    }

    async fn bridge_task_status_event_to_session(
        &self,
        session_id: &str,
        turn_id: &str,
        event_type: &str,
        message: &str,
        data: Value,
    ) {
        bridge_task_status_event_to_session(
            &self.state,
            session_id,
            turn_id,
            event_type,
            message,
            data,
        )
        .await;
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

fn spawn_task_turn_monitor(
    state: Arc<AppState>,
    task_id: Uuid,
    session_id: String,
    turn_id: String,
    backend_id: String,
) {
    tokio::spawn(async move {
        let outcome = run_turn_monitor(&state, task_id, &session_id, &turn_id, &backend_id).await;

        if let TurnOutcome::NeedsRetry { delay, attempt } = outcome {
            schedule_auto_retry(state, task_id, session_id, backend_id, delay, attempt).await;
        }
    });
}

/// Turn 监听主循环，返回最终的 TurnOutcome
async fn run_turn_monitor(
    state: &Arc<AppState>,
    task_id: Uuid,
    session_id: &str,
    turn_id: &str,
    backend_id: &str,
) -> TurnOutcome {
    let execution_mode = match state.repos.task_repo.get_by_id(task_id).await {
        Ok(Some(task)) => task.execution_mode,
        _ => TaskExecutionMode::Standard,
    };

    let (history, mut rx) = state
        .services
        .executor_hub
        .subscribe_with_history(session_id)
        .await;

    for notification in history {
        match handle_turn_notification(
            state,
            task_id,
            session_id,
            turn_id,
            backend_id,
            &notification,
            &execution_mode,
        )
        .await
        {
            Ok(TurnOutcome::Continue) => {}
            Ok(outcome) => return outcome,
            Err(err) => {
                tracing::error!(
                    task_id = %task_id,
                    session_id = %session_id,
                    turn_id = %turn_id,
                    "处理历史会话事件失败: {}",
                    err
                );
            }
        }
    }

    loop {
        match rx.recv().await {
            Ok(notification) => {
                match handle_turn_notification(
                    state,
                    task_id,
                    session_id,
                    turn_id,
                    backend_id,
                    &notification,
                    &execution_mode,
                )
                .await
                {
                    Ok(TurnOutcome::Continue) => {}
                    Ok(outcome) => return outcome,
                    Err(err) => {
                        tracing::error!(
                            task_id = %task_id,
                            session_id = %session_id,
                            turn_id = %turn_id,
                            "处理实时会话事件失败: {}",
                            err
                        );
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    task_id = %task_id,
                    session_id = %session_id,
                    turn_id = %turn_id,
                    skipped = skipped,
                    "Task turn 监听落后，部分消息被跳过"
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::warn!(
                    task_id = %task_id,
                    session_id = %session_id,
                    turn_id = %turn_id,
                    "Task turn 监听通道关闭，未收到终态事件"
                );
                return resolve_failure_outcome(
                    state,
                    task_id,
                    session_id,
                    turn_id,
                    backend_id,
                    "turn_monitor_closed",
                    None,
                    &execution_mode,
                )
                .await
                .unwrap_or(TurnOutcome::Failed);
            }
        }
    }
}

/// 等待退避延迟后发起自动重试
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
            let _ = update_task_status(
                &state,
                task_id,
                &backend_id,
                TaskStatus::Failed,
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

/// Turn 监听处理结果
enum TurnOutcome {
    /// 继续监听后续事件
    Continue,
    /// Turn 正常完成，监听结束
    Completed,
    /// Turn 失败且不可重试，监听结束
    Failed,
    /// Turn 失败但允许重试，需等待指定延迟后发起新 turn
    NeedsRetry {
        delay: std::time::Duration,
        attempt: u32,
    },
}

async fn handle_turn_notification(
    state: &Arc<AppState>,
    task_id: Uuid,
    session_id: &str,
    turn_id: &str,
    backend_id: &str,
    notification: &SessionNotification,
    execution_mode: &TaskExecutionMode,
) -> Result<TurnOutcome, agentdash_domain::DomainError> {
    match &notification.update {
        SessionUpdate::ToolCall(tool_call) => {
            if turn_matches(tool_call.meta.as_ref(), turn_id) {
                persist_tool_call_artifact(
                    state,
                    task_id,
                    session_id,
                    turn_id,
                    &tool_call.tool_call_id.to_string(),
                    build_tool_call_patch(tool_call),
                    backend_id,
                    "tool_call",
                )
                .await?;
            }
        }
        SessionUpdate::ToolCallUpdate(update) => {
            if turn_matches(update.meta.as_ref(), turn_id) {
                persist_tool_call_artifact(
                    state,
                    task_id,
                    session_id,
                    turn_id,
                    &update.tool_call_id.to_string(),
                    build_tool_call_update_patch(update),
                    backend_id,
                    "tool_call_update",
                )
                .await?;
            }
        }
        SessionUpdate::SessionInfoUpdate(info) => {
            sync_task_executor_session_binding_from_hub(
                state, task_id, backend_id, session_id, turn_id,
            )
            .await?;

            if let Some((event_type, message)) = parse_turn_event(info.meta.as_ref(), turn_id) {
                match event_type.as_str() {
                    "turn_completed" => {
                        if *execution_mode == TaskExecutionMode::OneShot {
                            tracing::info!(
                                task_id = %task_id,
                                session_id = %session_id,
                                turn_id = %turn_id,
                                "Turn 完成 [OneShot]，直接标记 Completed 并清理 session"
                            );
                            let _ = update_task_status(
                                state,
                                task_id,
                                backend_id,
                                TaskStatus::Completed,
                                "turn_completed_oneshot",
                                json!({
                                    "session_id": session_id,
                                    "turn_id": turn_id,
                                    "execution_mode": "one_shot",
                                }),
                            )
                            .await?;
                            clear_task_session_binding(
                                state,
                                task_id,
                                backend_id,
                                "oneshot_completed",
                            )
                            .await;
                        } else {
                            state
                                .task_runtime
                                .restart_tracker
                                .record_stable_start(task_id);
                            let _ = update_task_status(
                                state,
                                task_id,
                                backend_id,
                                TaskStatus::AwaitingVerification,
                                "turn_completed",
                                json!({
                                    "session_id": session_id,
                                    "turn_id": turn_id,
                                }),
                            )
                            .await?;
                        }
                        return Ok(TurnOutcome::Completed);
                    }
                    "turn_failed" => {
                        if let Some(err_msg) = message.as_deref() {
                            persist_turn_failure_artifact(
                                state, task_id, backend_id, session_id, turn_id, err_msg,
                            )
                            .await?;
                        }

                        return Ok(resolve_failure_outcome(
                            state,
                            task_id,
                            session_id,
                            turn_id,
                            backend_id,
                            "turn_failed",
                            message.clone(),
                            execution_mode,
                        )
                        .await?);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    Ok(TurnOutcome::Continue)
}

/// 根据 RestartTracker 策略决定失败后的处理方式
async fn resolve_failure_outcome(
    state: &Arc<AppState>,
    task_id: Uuid,
    session_id: &str,
    turn_id: &str,
    backend_id: &str,
    reason: &str,
    error_message: Option<String>,
    execution_mode: &TaskExecutionMode,
) -> Result<TurnOutcome, agentdash_domain::DomainError> {
    match execution_mode {
        TaskExecutionMode::AutoRetry => {
            let decision = state.task_runtime.restart_tracker.report_failure(task_id);
            match decision {
                RestartDecision::Allowed { attempt, delay } => {
                    tracing::info!(
                        task_id = %task_id,
                        session_id = %session_id,
                        turn_id = %turn_id,
                        attempt = attempt,
                        delay_ms = delay.as_millis() as u64,
                        reason = reason,
                        "Turn 失败 [AutoRetry]，RestartTracker 允许重试"
                    );
                    let _ = update_task_status(
                        state,
                        task_id,
                        backend_id,
                        TaskStatus::AwaitingVerification,
                        &format!("{reason}_pending_retry"),
                        json!({
                            "session_id": session_id,
                            "turn_id": turn_id,
                            "error": error_message,
                            "retry_attempt": attempt,
                            "retry_delay_ms": delay.as_millis() as u64,
                            "execution_mode": "auto_retry",
                        }),
                    )
                    .await?;
                    Ok(TurnOutcome::NeedsRetry { delay, attempt })
                }
                RestartDecision::Denied { attempts_exhausted } => {
                    tracing::warn!(
                        task_id = %task_id,
                        session_id = %session_id,
                        turn_id = %turn_id,
                        attempts_exhausted = attempts_exhausted,
                        reason = reason,
                        "Turn 失败 [AutoRetry]，已达最大重试次数，标记为 Failed"
                    );
                    let _ = update_task_status(
                        state,
                        task_id,
                        backend_id,
                        TaskStatus::Failed,
                        reason,
                        json!({
                            "session_id": session_id,
                            "turn_id": turn_id,
                            "error": error_message,
                            "attempts_exhausted": attempts_exhausted,
                            "execution_mode": "auto_retry",
                        }),
                    )
                    .await?;
                    Ok(TurnOutcome::Failed)
                }
            }
        }
        TaskExecutionMode::OneShot => {
            tracing::info!(
                task_id = %task_id,
                session_id = %session_id,
                turn_id = %turn_id,
                reason = reason,
                "Turn 失败 [OneShot]，标记 Failed 并清理 session"
            );
            let _ = update_task_status(
                state,
                task_id,
                backend_id,
                TaskStatus::Failed,
                &format!("{reason}_oneshot"),
                json!({
                    "session_id": session_id,
                    "turn_id": turn_id,
                    "error": error_message,
                    "execution_mode": "one_shot",
                }),
            )
            .await?;
            clear_task_session_binding(state, task_id, backend_id, "oneshot_failed").await;
            Ok(TurnOutcome::Failed)
        }
        TaskExecutionMode::Standard => {
            tracing::info!(
                task_id = %task_id,
                session_id = %session_id,
                turn_id = %turn_id,
                reason = reason,
                "Turn 失败 [Standard]，标记为 Failed，等待人工介入"
            );
            let _ = update_task_status(
                state,
                task_id,
                backend_id,
                TaskStatus::Failed,
                reason,
                json!({
                    "session_id": session_id,
                    "turn_id": turn_id,
                    "error": error_message,
                    "execution_mode": "standard",
                }),
            )
            .await?;
            Ok(TurnOutcome::Failed)
        }
    }
}

/// 清理 Task 的 session 绑定 — OneShot 模式完成或失败后调用
///
/// 将 session_id 和 executor_session_id 置为 None，
/// 使 Task 回到可重新启动的"干净"状态。
async fn clear_task_session_binding(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    reason: &str,
) {
    let result: Result<(), agentdash_domain::DomainError> = async {
        let mut task = match state.repos.task_repo.get_by_id(task_id).await? {
            Some(task) => task,
            None => return Ok(()),
        };

        let cleared_session_id = task.session_id.take();
        let cleared_executor_session_id = task.executor_session_id.take();

        if cleared_session_id.is_none() && cleared_executor_session_id.is_none() {
            return Ok(());
        }

        state.repos.task_repo.update(&task).await?;
        state
            .repos
            .story_repo
            .append_change(
                task.id,
                ChangeKind::TaskUpdated,
                json!({
                    "reason": format!("session_cleared_{reason}"),
                    "task_id": task.id,
                    "story_id": task.story_id,
                    "cleared_session_id": cleared_session_id,
                    "cleared_executor_session_id": cleared_executor_session_id,
                }),
                backend_id,
            )
            .await?;

        tracing::info!(
            task_id = %task_id,
            reason = reason,
            "已清理 Task session 绑定"
        );

        Ok(())
    }
    .await;

    if let Err(err) = result {
        tracing::warn!(
            task_id = %task_id,
            reason = reason,
            error = %err,
            "清理 Task session 绑定失败（不阻塞主流程）"
        );
    }
}

async fn persist_tool_call_artifact(
    state: &Arc<AppState>,
    task_id: Uuid,
    session_id: &str,
    turn_id: &str,
    tool_call_id: &str,
    patch: Map<String, Value>,
    backend_id: &str,
    reason: &str,
) -> Result<(), agentdash_domain::DomainError> {
    let mut task = match state.repos.task_repo.get_by_id(task_id).await? {
        Some(task) => task,
        None => return Ok(()),
    };

    let changed =
        upsert_tool_execution_artifact(&mut task, session_id, turn_id, tool_call_id, patch);
    if !changed {
        return Ok(());
    }

    state.repos.task_repo.update(&task).await?;
    append_task_change(
        state,
        task.id,
        backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": reason,
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "tool_call_id": tool_call_id,
            "artifact_type": "tool_execution",
        }),
    )
    .await?;

    Ok(())
}

async fn persist_turn_failure_artifact(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
    error_message: &str,
) -> Result<(), agentdash_domain::DomainError> {
    let mut task = match state.repos.task_repo.get_by_id(task_id).await? {
        Some(task) => task,
        None => return Ok(()),
    };

    task.artifacts.push(Artifact {
        id: Uuid::new_v4(),
        artifact_type: ArtifactType::LogOutput,
        content: json!({
            "kind": "turn_error",
            "session_id": session_id,
            "turn_id": turn_id,
            "message": error_message,
            "created_at": chrono::Utc::now().to_rfc3339(),
        }),
        created_at: chrono::Utc::now(),
    });

    state.repos.task_repo.update(&task).await?;
    append_task_change(
        state,
        task.id,
        backend_id,
        ChangeKind::TaskArtifactAdded,
        json!({
            "reason": "turn_failed_error_summary",
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "artifact_type": "log_output",
        }),
    )
    .await?;

    Ok(())
}

async fn bind_executor_session_id(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
    executor_session_id: &str,
) -> Result<(), agentdash_domain::DomainError> {
    let Some(mut task) = state.repos.task_repo.get_by_id(task_id).await? else {
        return Ok(());
    };

    if task.executor_session_id.as_deref() == Some(executor_session_id) {
        return Ok(());
    }

    task.executor_session_id = Some(executor_session_id.to_string());
    state.repos.task_repo.update(&task).await?;

    append_task_change(
        state,
        task.id,
        backend_id,
        ChangeKind::TaskUpdated,
        json!({
            "reason": "executor_session_bound",
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "executor_session_id": executor_session_id,
        }),
    )
    .await?;

    Ok(())
}

async fn sync_task_executor_session_binding_from_hub(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    session_id: &str,
    turn_id: &str,
) -> Result<(), agentdash_domain::DomainError> {
    let meta = match state
        .services
        .executor_hub
        .get_session_meta(session_id)
        .await
    {
        Ok(Some(meta)) => meta,
        Ok(None) => return Ok(()),
        Err(err) => {
            return Err(agentdash_domain::DomainError::InvalidConfig(
                err.to_string(),
            ));
        }
    };

    let Some(executor_session_id) = meta
        .executor_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    bind_executor_session_id(
        state,
        task_id,
        backend_id,
        session_id,
        turn_id,
        executor_session_id,
    )
    .await
}

async fn update_task_status(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    next_status: TaskStatus,
    reason: &str,
    context: Value,
) -> Result<bool, agentdash_domain::DomainError> {
    let mut task = match state.repos.task_repo.get_by_id(task_id).await? {
        Some(task) => task,
        None => return Ok(false),
    };

    if task.status == next_status {
        return Ok(false);
    }

    let previous_status = task.status.clone();
    task.status = next_status.clone();
    state.repos.task_repo.update(&task).await?;

    append_task_change(
        state,
        task.id,
        backend_id,
        ChangeKind::TaskStatusChanged,
        json!({
            "reason": reason,
            "task_id": task.id,
            "story_id": task.story_id,
            "session_id": task.session_id,
            "executor_session_id": task.executor_session_id,
            "from": previous_status,
            "to": next_status,
            "context": context,
        }),
    )
    .await?;

    Ok(true)
}

async fn append_task_change(
    state: &Arc<AppState>,
    task_id: Uuid,
    backend_id: &str,
    kind: ChangeKind,
    payload: Value,
) -> Result<(), agentdash_domain::DomainError> {
    state
        .repos
        .story_repo
        .append_change(task_id, kind, payload, backend_id)
        .await
}

async fn bridge_task_status_event_to_session(
    state: &Arc<AppState>,
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    message: &str,
    data: Value,
) {
    let meta = build_task_lifecycle_meta(turn_id, event_type, message, data);
    let notification = SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(meta)),
    );

    if let Err(err) = state
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

async fn get_task(state: &Arc<AppState>, task_id: Uuid) -> Result<Task, TaskExecutionError> {
    state
        .repos
        .task_repo
        .get_by_id(task_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| TaskExecutionError::NotFound(format!("Task {task_id} 不存在")))
}

async fn load_related_context(
    state: &Arc<AppState>,
    task: &Task,
) -> Result<(Story, Project, Option<Workspace>), TaskExecutionError> {
    let story = state
        .repos
        .story_repo
        .get_by_id(task.story_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| {
            TaskExecutionError::NotFound(format!("Task 所属 Story {} 不存在", task.story_id))
        })?;

    let project = state
        .repos
        .project_repo
        .get_by_id(story.project_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| {
            TaskExecutionError::NotFound(format!("Story 所属 Project {} 不存在", story.project_id))
        })?;

    let workspace = if let Some(ws_id) = task.workspace_id {
        Some(
            state
                .repos
                .workspace_repo
                .get_by_id(ws_id)
                .await
                .map_err(map_domain_error)?
                .ok_or_else(|| {
                    TaskExecutionError::NotFound(format!("Task 关联 Workspace {ws_id} 不存在"))
                })?,
        )
    } else {
        None
    };

    Ok((story, project, workspace))
}

async fn create_task_session(
    state: &Arc<AppState>,
    task: &Task,
) -> Result<SessionMeta, TaskExecutionError> {
    let title = format!("Task: {}", task.title.trim());
    state
        .services
        .executor_hub
        .create_session(title.trim())
        .await
        .map_err(map_internal_error)
}

fn map_domain_error(err: agentdash_domain::DomainError) -> TaskExecutionError {
    match &err {
        agentdash_domain::DomainError::NotFound { .. } => {
            TaskExecutionError::NotFound(err.to_string())
        }
        agentdash_domain::DomainError::InvalidTransition { .. } => {
            TaskExecutionError::BadRequest(err.to_string())
        }
        agentdash_domain::DomainError::InvalidConfig(_) => {
            TaskExecutionError::BadRequest(err.to_string())
        }
        _ => TaskExecutionError::Internal(err.to_string()),
    }
}

fn map_internal_error<E: ToString>(err: E) -> TaskExecutionError {
    TaskExecutionError::Internal(err.to_string())
}

fn map_connector_error(err: ConnectorError) -> TaskExecutionError {
    match err {
        ConnectorError::InvalidConfig(message) => TaskExecutionError::BadRequest(message),
        ConnectorError::Runtime(message) => TaskExecutionError::Conflict(message),
        other => TaskExecutionError::Internal(other.to_string()),
    }
}

async fn resolve_task_backend_id(
    state: &Arc<AppState>,
    task: &Task,
) -> Result<String, TaskExecutionError> {
    // 优先级：Workspace.backend_id → Story.backend_id → Project.backend_id
    if let Some(workspace_id) = task.workspace_id {
        let workspace = state
            .repos
            .workspace_repo
            .get_by_id(workspace_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!("Task 关联 Workspace {workspace_id} 不存在"))
            })?;
        if let Ok(bid) = normalize_backend_id(&workspace.backend_id) {
            return Ok(bid.to_string());
        }
    }

    let story = state
        .repos
        .story_repo
        .get_by_id(task.story_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| TaskExecutionError::NotFound(format!("Story {} 不存在", task.story_id)))?;

    if let Ok(bid) = normalize_backend_id(&story.backend_id) {
        return Ok(bid.to_string());
    }

    let project = state
        .repos
        .project_repo
        .get_by_id(story.project_id)
        .await
        .map_err(map_domain_error)?
        .ok_or_else(|| {
            TaskExecutionError::NotFound(format!("Story 所属 Project {} 不存在", story.project_id))
        })?;

    normalize_backend_id(&project.backend_id).map(|value| value.to_string())
}

fn normalize_backend_id(raw: &str) -> Result<&str, TaskExecutionError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(TaskExecutionError::BadRequest(
            "backend_id 不能为空".to_string(),
        ));
    }
    Ok(trimmed)
}

/// Re-export for backward compat within api crate
pub(crate) use agentdash_application::task::config::resolve_task_agent_config;

// ─── 远程中继辅助函数 ──────────────────────────────────────

async fn relay_start_prompt(
    state: &Arc<AppState>,
    backend_id: &str,
    session_id: &str,
    built: &BuiltTaskAgentContext,
    executor_config: Option<agentdash_executor::AgentDashExecutorConfig>,
    workspace: &Workspace,
) -> Result<String, TaskExecutionError> {
    let relay_config = executor_config.map(|c| ExecutorConfigRelay {
        executor: c.executor,
        variant: c.variant,
        model_id: c.model_id,
        permission_policy: c.permission_policy,
    });

    let cmd = RelayMessage::CommandPrompt {
        id: RelayMessage::new_id("prompt"),
        payload: CommandPromptPayload {
            session_id: session_id.to_string(),
            follow_up_session_id: None,
            prompt: None,
            prompt_blocks: Some(serde_json::Value::Array(built.prompt_blocks.clone())),
            workspace_root: workspace.container_ref.clone(),
            working_dir: built.working_dir.clone(),
            env: Default::default(),
            executor_config: relay_config,
            mcp_servers: vec![],
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
