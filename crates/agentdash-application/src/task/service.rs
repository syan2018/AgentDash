use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_domain::{
    common::AgentConfig,
    session_binding::SessionOwnerType,
    story::ChangeKind,
    task::{Task, TaskStatus},
};

use crate::address_space::RelayAddressSpaceService;
use crate::context::ContextContributorRegistry;
use crate::repository_set::RepositorySet;
use crate::session::{SessionExecutionState, SessionHub};
use crate::task::lock::TaskLockMap;
use crate::task::restart_tracker::RestartTracker;
use crate::workspace::BackendAvailability;

use super::execution::*;
use super::gateway::{
    PreparedTurnContext, TaskTurnServices, append_task_change as gw_append_task_change,
    bridge_task_status_event_to_session_notification,
    create_task_session as gw_create_task_session, get_session_overview as gw_get_session_overview,
    get_task as gw_get_task, map_domain_error, prepare_task_turn_context,
    resolve_project_scope_for_owner, resolve_task_backend_id,
};

/// 基础设施回调 — 仅封装 Application 层无法直接完成的操作
///
/// 主要涉及：执行分发（云端原生 / 远程中继）、取消路由、Turn 监控任务管理。
/// 由 API/Host 层提供具体实现。
#[async_trait]
pub trait TurnDispatcher: Send + Sync {
    /// 根据 PreparedTurnContext 将 turn 分发到合适的执行通道
    async fn dispatch_turn(
        &self,
        session_id: &str,
        ctx: PreparedTurnContext,
    ) -> Result<StartedTurn, TaskExecutionError>;

    /// 取消会话执行（自动路由到本地 Hub 或远程中继）
    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError>;

    /// 启动 turn 完成监控 — 在后台追踪 turn 结果并触发状态同步
    fn spawn_turn_monitor(
        &self,
        task_id: Uuid,
        session_id: String,
        turn_id: String,
        backend_id: String,
    );
}

/// Task 执行 Service — Application 层直接编排
///
/// 持有所有必要的 Application 层依赖（repos / hub / context services），
/// 仅通过 `TurnDispatcher` trait 依赖基础设施层的分发能力。
pub struct TaskLifecycleService {
    pub repos: RepositorySet,
    pub hub: SessionHub,
    pub address_space_service: Arc<RelayAddressSpaceService>,
    pub contributor_registry: Arc<ContextContributorRegistry>,
    pub mcp_base_url: Option<String>,
    pub backend_availability: Arc<dyn BackendAvailability>,
    pub dispatcher: Arc<dyn TurnDispatcher>,
    pub restart_tracker: Arc<RestartTracker>,
    pub lock_map: Arc<TaskLockMap>,
}

impl TaskLifecycleService {
    pub async fn start_task(
        &self,
        cmd: StartTaskCommand,
    ) -> Result<StartTaskResult, TaskExecutionError> {
        let svc = &self;
        svc.lock_map
            .with_lock(cmd.task_id, || async { svc.start_task_inner(cmd).await })
            .await
    }

    pub async fn continue_task(
        &self,
        cmd: ContinueTaskCommand,
    ) -> Result<ContinueTaskResult, TaskExecutionError> {
        let svc = &self;
        svc.lock_map
            .with_lock(cmd.task_id, || async { svc.continue_task_inner(cmd).await })
            .await
    }

    pub async fn cancel_task(&self, task_id: Uuid) -> Result<Task, TaskExecutionError> {
        let result = self
            .lock_map
            .with_lock(task_id, || async { self.cancel_task_inner(task_id).await })
            .await;
        if result.is_ok() {
            self.restart_tracker.clear(task_id);
        }
        result
    }

    pub async fn get_task_session(
        &self,
        task_id: Uuid,
    ) -> Result<TaskSessionResult, TaskExecutionError> {
        let task = gw_get_task(&self.repos, task_id).await?;
        let session_id = task.session_id.clone();

        let (session_title, last_activity, session_execution_status) =
            if let Some(sid) = session_id.as_deref() {
                match gw_get_session_overview(&self.hub, sid).await? {
                    Some(meta) => {
                        let execution_state =
                            self.hub
                                .inspect_session_execution_state(sid)
                                .await
                                .map_err(|error| TaskExecutionError::Internal(error.to_string()))?;
                        (
                            Some(meta.title),
                            Some(meta.updated_at),
                            Some(session_execution_state_tag(&execution_state).to_string()),
                        )
                    }
                    None => (None, None, None),
                }
            } else {
                (None, None, None)
            };

        Ok(TaskSessionResult {
            task_id: task.id,
            session_id,
            executor_session_id: task.executor_session_id,
            task_status: task.status,
            session_execution_status,
            agent_binding: task.agent_binding,
            session_title,
            last_activity,
        })
    }

    // ─── inner implementations ────────────────────────────────

    async fn start_task_inner(
        &self,
        cmd: StartTaskCommand,
    ) -> Result<StartTaskResult, TaskExecutionError> {
        let mut task = gw_get_task(&self.repos, cmd.task_id).await?;

        if task.session_id.is_some() {
            return Err(TaskExecutionError::Conflict(
                "Task 已绑定 Session，请使用 continue 接口继续执行".into(),
            ));
        }

        let backend_id =
            resolve_task_backend_id(&self.repos, self.backend_availability.as_ref(), &task).await?;
        let session_meta = gw_create_task_session(&self.hub, &task).await?;
        let session_id = session_meta.id;
        let previous_status = task.status.clone();

        task.session_id = Some(session_id.clone());
        task.executor_session_id = None;
        task.status = TaskStatus::Running;
        self.repos
            .task_repo
            .update(&task)
            .await
            .map_err(map_domain_error)?;

        if let Err(err) = self
            .bind_session_to_owner(&session_id, "task", task.id, "execution")
            .await
        {
            tracing::warn!(
                task_id = %task.id, session_id = %session_id,
                "写入 session_binding 失败（不阻塞执行流）: {}", err
            );
        }

        gw_append_task_change(
            &self.repos,
            task.id,
            &backend_id,
            ChangeKind::TaskUpdated,
            json!({
                "reason": "task_session_bound",
                "task_id": task.id, "story_id": task.story_id,
                "session_id": task.session_id,
                "executor_session_id": task.executor_session_id,
            }),
        )
        .await
        .map_err(map_domain_error)?;

        if previous_status != task.status {
            gw_append_task_change(
                &self.repos,
                task.id,
                &backend_id,
                ChangeKind::TaskStatusChanged,
                json!({
                    "reason": "task_start_accepted",
                    "task_id": task.id, "story_id": task.story_id,
                    "session_id": task.session_id,
                    "executor_session_id": task.executor_session_id,
                    "from": previous_status.clone(), "to": task.status.clone(),
                }),
            )
            .await
            .map_err(map_domain_error)?;
        }

        let started_turn = match self
            .dispatch_prepared_turn(
                &task,
                ExecutionPhase::Start,
                cmd.override_prompt.as_deref(),
                None,
                cmd.executor_config.as_ref(),
            )
            .await
        {
            Ok(t) => t,
            Err(err) => {
                let mut fail_task = task.clone();
                fail_task.status = TaskStatus::Failed;
                let _ = self.repos.task_repo.update(&fail_task).await;
                let _ = gw_append_task_change(
                    &self.repos,
                    fail_task.id,
                    &backend_id,
                    ChangeKind::TaskStatusChanged,
                    json!({
                        "reason": "task_start_failed",
                        "task_id": fail_task.id, "story_id": fail_task.story_id,
                        "session_id": fail_task.session_id,
                        "executor_session_id": fail_task.executor_session_id,
                        "from": TaskStatus::Running, "to": TaskStatus::Failed,
                        "error": err.to_string(),
                    }),
                )
                .await;
                return Err(err);
            }
        };

        self.bridge_status_event(
            &session_id,
            &started_turn.turn_id,
            "task_start_accepted",
            "Task 已开始执行",
            json!({
                "task_id": task.id, "story_id": task.story_id,
                "session_id": session_id,
                "executor_session_id": task.executor_session_id,
                "from": previous_status, "to": task.status,
            }),
        )
        .await;

        self.dispatcher.spawn_turn_monitor(
            task.id,
            session_id.clone(),
            started_turn.turn_id.clone(),
            backend_id,
        );

        Ok(StartTaskResult {
            task_id: task.id,
            session_id,
            executor_session_id: task.executor_session_id.clone(),
            turn_id: started_turn.turn_id,
            status: task.status,
            context_sources: started_turn.context_sources,
        })
    }

    async fn continue_task_inner(
        &self,
        cmd: ContinueTaskCommand,
    ) -> Result<ContinueTaskResult, TaskExecutionError> {
        let mut task = gw_get_task(&self.repos, cmd.task_id).await?;
        let session_id = task.session_id.clone().ok_or_else(|| {
            TaskExecutionError::UnprocessableEntity("Task 尚未启动，请先执行 start".into())
        })?;

        if self.is_task_session_running(&session_id).await? {
            return Err(TaskExecutionError::Conflict("该任务已有执行进行中".into()));
        }

        let backend_id =
            resolve_task_backend_id(&self.repos, self.backend_availability.as_ref(), &task).await?;

        let started_turn = self
            .dispatch_prepared_turn(
                &task,
                ExecutionPhase::Continue,
                None,
                cmd.additional_prompt.as_deref(),
                cmd.executor_config.as_ref(),
            )
            .await?;

        let previous_status = task.status.clone();
        task.status = TaskStatus::Running;
        self.repos
            .task_repo
            .update(&task)
            .await
            .map_err(map_domain_error)?;

        if previous_status != task.status {
            gw_append_task_change(
                &self.repos,
                task.id,
                &backend_id,
                ChangeKind::TaskStatusChanged,
                json!({
                    "reason": "task_continue_accepted",
                    "task_id": task.id, "story_id": task.story_id,
                    "session_id": task.session_id,
                    "executor_session_id": task.executor_session_id,
                    "from": previous_status.clone(), "to": task.status.clone(),
                }),
            )
            .await
            .map_err(map_domain_error)?;
        }

        self.bridge_status_event(
            &session_id,
            &started_turn.turn_id,
            "task_continue_accepted",
            "Task 已继续执行",
            json!({
                "task_id": task.id, "story_id": task.story_id,
                "session_id": session_id,
                "executor_session_id": task.executor_session_id,
                "from": previous_status, "to": task.status,
            }),
        )
        .await;

        self.dispatcher.spawn_turn_monitor(
            task.id,
            session_id.clone(),
            started_turn.turn_id.clone(),
            backend_id,
        );

        Ok(ContinueTaskResult {
            task_id: task.id,
            session_id,
            executor_session_id: task.executor_session_id.clone(),
            turn_id: started_turn.turn_id,
            status: task.status,
            context_sources: started_turn.context_sources,
        })
    }

    async fn cancel_task_inner(&self, task_id: Uuid) -> Result<Task, TaskExecutionError> {
        let mut task = gw_get_task(&self.repos, task_id).await?;
        let session_id = task.session_id.clone().ok_or_else(|| {
            TaskExecutionError::UnprocessableEntity("Task 尚未启动，无法取消执行".into())
        })?;
        let session_was_running = self.is_task_session_running(&session_id).await?;

        self.dispatcher.cancel_session(&session_id).await?;

        if session_was_running {
            let previous_status = task.status.clone();
            task.status = TaskStatus::Failed;
            self.repos
                .task_repo
                .update(&task)
                .await
                .map_err(map_domain_error)?;

            let backend_id =
                resolve_task_backend_id(&self.repos, self.backend_availability.as_ref(), &task)
                    .await
                    .unwrap_or_else(|_| "unknown".to_string());

            gw_append_task_change(
                &self.repos,
                task.id,
                &backend_id,
                ChangeKind::TaskStatusChanged,
                json!({
                    "reason": "task_cancel_requested",
                    "task_id": task.id, "story_id": task.story_id,
                    "session_id": task.session_id,
                    "executor_session_id": task.executor_session_id,
                    "from": previous_status, "to": task.status,
                }),
            )
            .await
            .map_err(map_domain_error)?;
        }

        Ok(task)
    }

    // ─── private helpers ──────────────────────────────────────

    async fn dispatch_prepared_turn(
        &self,
        task: &Task,
        phase: ExecutionPhase,
        override_prompt: Option<&str>,
        additional_prompt: Option<&str>,
        executor_config: Option<&AgentConfig>,
    ) -> Result<StartedTurn, TaskExecutionError> {
        let session_id = task
            .session_id
            .as_deref()
            .ok_or_else(|| TaskExecutionError::Internal("Task 未绑定 session".into()))?;

        let svc = TaskTurnServices {
            repos: &self.repos,
            availability: self.backend_availability.as_ref(),
            address_space_service: &self.address_space_service,
            contributor_registry: &self.contributor_registry,
            mcp_base_url: self.mcp_base_url.as_deref(),
        };
        let ctx = prepare_task_turn_context(
            &svc,
            task,
            phase,
            override_prompt,
            additional_prompt,
            executor_config,
        )
        .await?;

        self.dispatcher.dispatch_turn(session_id, ctx).await
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
        let project_id = resolve_project_scope_for_owner(&self.repos, owner_type, owner_id).await?;
        let binding = agentdash_domain::session_binding::SessionBinding::new(
            project_id,
            session_id.to_string(),
            owner_type,
            owner_id,
            label,
        );
        self.repos
            .session_binding_repo
            .create(&binding)
            .await
            .map_err(map_domain_error)
    }

    async fn bridge_status_event(
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
        if let Err(err) = self.hub.inject_notification(session_id, notification).await {
            tracing::warn!(
                session_id, turn_id, event_type, error = %err,
                "桥接 Task 生命周期事件到 session 流失败"
            );
        }
    }

    async fn is_task_session_running(&self, session_id: &str) -> Result<bool, TaskExecutionError> {
        let execution_state = self
            .hub
            .inspect_session_execution_state(session_id)
            .await
            .map_err(|error| TaskExecutionError::Internal(error.to_string()))?;
        Ok(matches!(
            execution_state,
            SessionExecutionState::Running { .. }
        ))
    }
}

fn session_execution_state_tag(state: &SessionExecutionState) -> &'static str {
    match state {
        SessionExecutionState::Idle => "idle",
        SessionExecutionState::Running { .. } => "running",
        SessionExecutionState::Completed { .. } => "completed",
        SessionExecutionState::Failed { .. } => "failed",
        SessionExecutionState::Interrupted { .. } => "interrupted",
    }
}
