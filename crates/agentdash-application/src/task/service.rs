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

use crate::repository_set::RepositorySet;
use crate::session::{
    LaunchCommand, SessionCoreService, SessionEventingService, SessionExecutionState,
    SessionLaunchService, TaskLaunchPhase, UserPromptInput,
};
use crate::task::lock::TaskLockMap;
use crate::workspace::BackendAvailability;

use super::execution::*;
use super::gateway::{
    append_task_change as gw_append_task_change, bridge_task_status_event_to_envelope,
    clear_task_session_binding, create_task_session as gw_create_task_session,
    get_session_overview as gw_get_session_overview, get_task as gw_get_task, map_connector_error,
    map_domain_error, resolve_project_scope_for_owner, resolve_task_backend_id,
};

/// 基础设施回调 — 仅封装 Application 层无法直接完成的操作
///
/// 主要涉及：执行分发（云端原生 / 远程中继）、取消路由、Turn 监控任务管理。
/// 由 API/Host 层提供具体实现。
#[async_trait]
pub trait TurnDispatcher: Send + Sync {
    /// 取消会话执行（自动路由到本地 Hub 或远程中继）
    async fn cancel_session(&self, session_id: &str) -> Result<(), TaskExecutionError>;
}

/// Story step activation service — Application 层编排 step child session 的启动/续跑/取消。
///
/// 持有所有必要的 Application 层依赖（repos / session services / context services），
/// 仅通过 `TurnDispatcher` trait 依赖基础设施层的分发能力。
pub struct StoryStepActivationService {
    pub repos: RepositorySet,
    pub session_core: SessionCoreService,
    pub session_eventing: SessionEventingService,
    pub session_launch: SessionLaunchService,
    pub backend_availability: Arc<dyn BackendAvailability>,
    pub dispatcher: Arc<dyn TurnDispatcher>,
    pub lock_map: Arc<TaskLockMap>,
}

impl StoryStepActivationService {
    pub async fn start_task(
        &self,
        cmd: TaskExecutionCommand,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        debug_assert_eq!(cmd.phase, ExecutionPhase::Start);
        let svc = &self;
        svc.lock_map
            .with_lock(cmd.task_id, || async { svc.start_task_inner(cmd).await })
            .await
    }

    pub async fn continue_task(
        &self,
        cmd: TaskExecutionCommand,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        debug_assert_eq!(cmd.phase, ExecutionPhase::Continue);
        let svc = &self;
        svc.lock_map
            .with_lock(cmd.task_id, || async { svc.continue_task_inner(cmd).await })
            .await
    }

    pub async fn cancel_task(&self, task_id: Uuid) -> Result<Task, TaskExecutionError> {
        self.lock_map
            .with_lock(task_id, || async { self.cancel_task_inner(task_id).await })
            .await
    }

    /// 直接以 task 为入口启动 / 续跑 execution session。
    ///
    /// task execution session 不挂 `lifecycle_activity:*` binding，因此装配应容忍
    /// 「无 active workflow」（走纯 task 装配，不带 lifecycle workflow injection）。
    /// 本方法不再触碰 lifecycle step 定位 / `LifecycleRunService` / Step repo。
    ///
    /// 内部链路（3 步）：
    /// 1. Start：建 execution session + bind 到 task owner；Continue：复用已绑定 session
    /// 2. `SessionLaunchService::launch_command` 派发
    /// 3. 桥接 task 生命周期事件到 session 流
    #[allow(clippy::too_many_arguments)]
    async fn launch_task_execution(
        &self,
        task: Task,
        phase: ExecutionPhase,
        override_prompt: Option<&str>,
        additional_prompt: Option<&str>,
        executor_config: Option<&AgentConfig>,
        identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        let backend_id =
            resolve_task_backend_id(&self.repos, self.backend_availability.as_ref(), &task).await?;

        let session_id = match phase {
            ExecutionPhase::Start => {
                if self.resolve_execution_session_id(task.id).await?.is_some() {
                    return Err(TaskExecutionError::Conflict(
                        "Task 已绑定 Session，请使用 continue 接口继续执行".into(),
                    ));
                }

                let session_meta = gw_create_task_session(&self.session_core, &task).await?;
                let session_id = session_meta.id;
                self.bind_session_to_owner(&session_id, "task", task.id, "execution")
                    .await?;

                session_id
            }
            ExecutionPhase::Continue => {
                let session_id = self
                    .resolve_execution_session_id(task.id)
                    .await?
                    .ok_or_else(|| {
                        TaskExecutionError::UnprocessableEntity(
                            "Task 尚未启动，请先执行 start".into(),
                        )
                    })?;

                if self.is_task_session_running(&session_id).await? {
                    return Err(TaskExecutionError::Conflict("该任务已有执行进行中".into()));
                }

                session_id
            }
        };

        let mut user_input = UserPromptInput::from_text("");
        user_input.executor_config = executor_config.cloned();
        let command = LaunchCommand::task_service_input(
            user_input,
            identity,
            match phase {
                ExecutionPhase::Start => TaskLaunchPhase::Start,
                ExecutionPhase::Continue => TaskLaunchPhase::Continue,
            },
            override_prompt.map(str::to_string),
            additional_prompt.map(str::to_string),
        );
        let launch_outcome = match self
            .session_launch
            .launch_command_with_outcome(&session_id, command)
            .await
        {
            Ok(outcome) => outcome,
            Err(err) => {
                if phase == ExecutionPhase::Start {
                    clear_task_session_binding(&self.repos, task.id, &backend_id, "start_failed")
                        .await;
                }
                return Err(map_connector_error(err));
            }
        };
        let turn_id = launch_outcome.turn_id;
        let context_sources = launch_outcome.context_sources;

        let latest_task = gw_get_task(&self.repos, task.id).await?;

        self.bridge_status_event(
            &session_id,
            &turn_id,
            match phase {
                ExecutionPhase::Start => "task_start_accepted",
                ExecutionPhase::Continue => "task_continue_accepted",
            },
            match phase {
                ExecutionPhase::Start => "Task 已开始执行",
                ExecutionPhase::Continue => "Task 已继续执行",
            },
            json!({
                "task_id": latest_task.id,
                "story_id": latest_task.story_id,
                "session_id": session_id,
                "phase": match phase {
                    ExecutionPhase::Start => "start",
                    ExecutionPhase::Continue => "continue",
                },
                "status": latest_task.status().clone(),
            }),
        )
        .await;

        Ok(TaskExecutionResult {
            task_id: latest_task.id,
            session_id,
            turn_id,
            status: latest_task.status().clone(),
            context_sources,
        })
    }

    pub async fn get_task_session(
        &self,
        task_id: Uuid,
    ) -> Result<TaskSessionResult, TaskExecutionError> {
        let task = gw_get_task(&self.repos, task_id).await?;
        let session_id = self.resolve_execution_session_id(task_id).await?;

        let (session_title, last_activity, session_execution_status) =
            if let Some(sid) = session_id.as_deref() {
                match gw_get_session_overview(&self.session_core, sid).await? {
                    Some(meta) => {
                        let execution_state = self
                            .session_core
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
            task_status: task.status().clone(),
            session_execution_status,
            agent_binding: task.agent_binding,
            session_title,
            last_activity,
        })
    }

    // ─── inner implementations ────────────────────────────────

    async fn start_task_inner(
        &self,
        cmd: TaskExecutionCommand,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        let task = gw_get_task(&self.repos, cmd.task_id).await?;
        self.launch_task_execution(
            task,
            ExecutionPhase::Start,
            cmd.prompt.as_deref(),
            None,
            cmd.executor_config.as_ref(),
            cmd.identity,
        )
        .await
    }

    async fn continue_task_inner(
        &self,
        cmd: TaskExecutionCommand,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        let task = gw_get_task(&self.repos, cmd.task_id).await?;
        self.launch_task_execution(
            task,
            ExecutionPhase::Continue,
            None,
            cmd.prompt.as_deref(),
            cmd.executor_config.as_ref(),
            cmd.identity,
        )
        .await
    }

    async fn cancel_task_inner(&self, task_id: Uuid) -> Result<Task, TaskExecutionError> {
        let mut task = gw_get_task(&self.repos, task_id).await?;
        let session_id = self
            .resolve_execution_session_id(task.id)
            .await?
            .ok_or_else(|| {
                TaskExecutionError::UnprocessableEntity("Task 尚未启动，无法取消执行".into())
            })?;
        let session_was_running = self.is_task_session_running(&session_id).await?;

        self.dispatcher.cancel_session(&session_id).await?;

        if session_was_running {
            let backend_id =
                resolve_task_backend_id(&self.repos, self.backend_availability.as_ref(), &task)
                    .await?;
            let previous_status = task.status().clone();
            task.set_status(TaskStatus::Failed);
            self.persist_task(&task).await?;

            gw_append_task_change(
                &self.repos,
                task.id,
                &backend_id,
                ChangeKind::TaskStatusChanged,
                json!({
                    "reason": "task_cancel_requested",
                    "task_id": task.id, "story_id": task.story_id,
                    "session_id": session_id,
                    "from": previous_status, "to": task.status().clone(),
                }),
            )
            .await
            .map_err(map_domain_error)?;
        }

        Ok(task)
    }

    // ─── private helpers ──────────────────────────────────────

    /// 将 Task mutation 通过 Story aggregate 写回持久层。
    ///
    /// M1-b：Task 已合入 Story aggregate（`stories.tasks` JSONB 列），
    /// 所有 task 写入必须经 `Story::update_task` 维护聚合不变量。
    async fn persist_task(&self, task: &Task) -> Result<(), TaskExecutionError> {
        let mut story = self
            .repos
            .story_repo
            .get_by_id(task.story_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!("Task 所属 Story {} 不存在", task.story_id))
            })?;

        // M2：拆成"spec 字段 update_task（走 TaskSpecMut）" + "status 走 force_set_task_status"
        // 两条路径，保证投影字段不会被 closure 直接覆盖。
        let updated_spec = story.update_task(task.id, |view| {
            *view.title = task.title.clone();
            *view.description = task.description.clone();
            *view.workspace_id = task.workspace_id;
            *view.lifecycle_step_key = task.lifecycle_step_key.clone();
            *view.agent_binding = task.agent_binding.clone();
        });
        if updated_spec.is_none() {
            return Err(TaskExecutionError::NotFound(format!(
                "Task {} 不属于 Story {}",
                task.id, task.story_id
            )));
        }
        // 同步命令型 status（本路径的状态写入仍保留）
        story.force_set_task_status(task.id, task.status().clone());
        // 同步 artifacts（命令型覆写：直接替换）
        story.mutate_task_artifacts(task.id, |artifacts| {
            *artifacts = task.artifacts().to_vec();
        });

        self.repos
            .story_repo
            .update(&story)
            .await
            .map_err(map_domain_error)
    }

    async fn resolve_execution_session_id(
        &self,
        task_id: Uuid,
    ) -> Result<Option<String>, TaskExecutionError> {
        super::find_task_execution_session_id(self.repos.session_binding_repo.as_ref(), task_id)
            .await
            .map_err(map_domain_error)
    }

    async fn bind_session_to_owner(
        &self,
        session_id: &str,
        owner_type: &str,
        owner_id: Uuid,
        label: &str,
    ) -> Result<(), TaskExecutionError> {
        let owner_type = owner_type.parse::<SessionOwnerType>().map_err(|_| {
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
            .map_err(map_domain_error)?;
        self.session_core
            .mark_owner_bootstrap_pending(session_id)
            .await
            .map_err(|error| TaskExecutionError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn bridge_status_event(
        &self,
        session_id: &str,
        turn_id: &str,
        event_type: &str,
        message: &str,
        data: Value,
    ) {
        let envelope =
            bridge_task_status_event_to_envelope(session_id, turn_id, event_type, message, data);
        if let Err(err) = self
            .session_eventing
            .inject_notification(session_id, envelope)
            .await
        {
            tracing::warn!(
                session_id, turn_id, event_type, error = %err,
                "桥接 Task 生命周期事件到 session 流失败"
            );
        }
    }

    async fn is_task_session_running(&self, session_id: &str) -> Result<bool, TaskExecutionError> {
        let execution_state = self
            .session_core
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
