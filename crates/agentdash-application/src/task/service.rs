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

use crate::canvas::append_visible_canvas_mounts;
use crate::context::SharedContextAuditBus;
use crate::repository_set::RepositorySet;
use crate::session::{
    PromptSessionRequest, SessionExecutionState, SessionHub, SessionRequestAssembler,
    StoryStepPhase, StoryStepSpec, UserPromptInput, finalize_request,
};
use crate::task::lock::TaskLockMap;
use crate::vfs::RelayVfsService;
use crate::workflow::{
    BindAndActivateLifecycleStepCommand, LifecycleRunService, build_step_projector_from_repos,
    resolve_workflow_projection_by_run,
};
use crate::workspace::BackendAvailability;

use super::execution::*;
use super::gateway::{
    append_task_change as gw_append_task_change, bridge_task_status_event_to_session_notification,
    clear_task_session_binding, create_task_session as gw_create_task_session,
    get_session_overview as gw_get_session_overview, get_task as gw_get_task, load_related_context,
    map_connector_error, map_domain_error, resolve_project_scope_for_owner,
    resolve_task_backend_id,
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
/// 持有所有必要的 Application 层依赖（repos / hub / context services），
/// 仅通过 `TurnDispatcher` trait 依赖基础设施层的分发能力。
pub struct StoryStepActivationService {
    pub repos: RepositorySet,
    pub hub: SessionHub,
    pub vfs_service: Arc<RelayVfsService>,
    pub platform_config: crate::platform_config::SharedPlatformConfig,
    pub backend_availability: Arc<dyn BackendAvailability>,
    pub dispatcher: Arc<dyn TurnDispatcher>,
    pub lock_map: Arc<TaskLockMap>,
    /// 上下文审计总线（可选）。
    pub audit_bus: Option<SharedContextAuditBus>,
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
        let result = self
            .lock_map
            .with_lock(task_id, || async { self.cancel_task_inner(task_id).await })
            .await;
        result
    }

    /// 统一激活 Story session 下某个 lifecycle step（M5 facade 入口）。
    ///
    /// 这是 Story-as-durable-session 模型下 task 启动 / 续跑的唯一领域级入口。
    /// `start_task` / `continue_task` 仅作为对外签名兼容的 facade，内部委托本方法。
    ///
    /// 内部链路（5 步）：
    /// 1. 通过 story_id → `SessionBinding(Story, "companion")` → `session_id`
    /// 2. `lifecycle_run_repo.list_by_session(session_id)` + `select_active_run` 找到活跃 run
    /// 3. 根据 step_key 定位 `LifecycleStepDefinition`；再从 Story aggregate 中找到
    ///    `Task.lifecycle_step_key == step_key` 的 Task
    /// 4. `compose_story_step(StoryStepSpec { ... })` 产出 `PreparedSessionInputs`
    /// 5. `finalize_request(base, prepared)` + `session_hub.start_prompt` 派发
    ///
    /// [M5 注]：由于当前 `start_task_inner` 仍负责 session 创建 / binding / 状态写入的
    /// 全流程，本方法目前作为分层预留入口，实际调用仍由 `start_task_inner` 与
    /// `continue_task_inner` 组合而成。`step_key` 的显式查询在后续 cleanup 任务中
    /// 与 `LifecycleRunService::activate_step` 对齐。
    #[allow(clippy::too_many_arguments)]
    pub async fn activate_story_step(
        &self,
        story_id: Uuid,
        step_key: Option<String>,
        phase: ExecutionPhase,
        override_prompt: Option<&str>,
        additional_prompt: Option<&str>,
        executor_config: Option<&AgentConfig>,
        identity: Option<agentdash_spi::auth::AuthIdentity>,
    ) -> Result<TaskExecutionResult, TaskExecutionError> {
        // 1. story → story session binding（label="companion"）
        let story_session_id = self.find_story_session_id(story_id).await?;

        // 2. 查活跃 lifecycle run
        let mut active_run = self
            .find_active_run_for_story_session(&story_session_id)
            .await?;

        // 3. 定位 step → Task，并补齐 compose 所需上下文
        let (task, step_key_resolved, lifecycle) = self
            .resolve_task_for_step(story_id, &active_run, step_key.as_deref())
            .await?;
        let step = lifecycle
            .steps
            .iter()
            .find(|item| item.key == step_key_resolved)
            .cloned()
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!(
                    "lifecycle {} 中不存在 step '{}'",
                    lifecycle.id, step_key_resolved
                ))
            })?;
        let (story, project, workspace) = load_related_context(&self.repos, &task).await?;
        let backend_id =
            resolve_task_backend_id(&self.repos, self.backend_availability.as_ref(), &task).await?;

        let session_id = match phase {
            ExecutionPhase::Start => {
                if self.resolve_execution_session_id(task.id).await?.is_some() {
                    return Err(TaskExecutionError::Conflict(
                        "Task 已绑定 Session，请使用 continue 接口继续执行".into(),
                    ));
                }

                let session_meta = gw_create_task_session(&self.hub, &task).await?;
                let session_id = session_meta.id;
                self.bind_session_to_owner(&session_id, "task", task.id, "execution")
                    .await?;

                let run_service = LifecycleRunService::new(
                    self.repos.lifecycle_definition_repo.as_ref(),
                    self.repos.lifecycle_run_repo.as_ref(),
                )
                .with_projector(build_step_projector_from_repos(&self.repos));

                match run_service
                    .bind_session_and_activate_step(BindAndActivateLifecycleStepCommand {
                        run_id: active_run.id,
                        step_key: step.key.clone(),
                        session_id: session_id.clone(),
                    })
                    .await
                {
                    Ok(updated_run) => {
                        active_run = updated_run;
                    }
                    Err(err) => {
                        let _ = self
                            .repos
                            .session_binding_repo
                            .delete_by_session_and_owner(
                                &session_id,
                                SessionOwnerType::Task,
                                task.id,
                            )
                            .await;
                        return Err(TaskExecutionError::Conflict(err.to_string()));
                    }
                }

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

        let active_workflow = resolve_workflow_projection_by_run(
            active_run.id,
            &step.key,
            self.repos.workflow_definition_repo.as_ref(),
            self.repos.lifecycle_definition_repo.as_ref(),
            self.repos.lifecycle_run_repo.as_ref(),
        )
        .await
        .map_err(TaskExecutionError::Internal)?;

        let mut assembler = SessionRequestAssembler::new(
            self.vfs_service.as_ref(),
            self.repos.canvas_repo.as_ref(),
            self.backend_availability.as_ref(),
            &self.repos,
            &self.platform_config,
        );
        if let Some(bus) = self.audit_bus.as_ref() {
            assembler = assembler.with_audit_bus(bus.clone());
        }
        let mut prepared = assembler
            .compose_story_step(StoryStepSpec {
                run: &active_run,
                lifecycle: &lifecycle,
                step: &step,
                task: &task,
                story: &story,
                project: &project,
                workspace: workspace.as_ref(),
                phase: match phase {
                    ExecutionPhase::Start => StoryStepPhase::Start,
                    ExecutionPhase::Continue => StoryStepPhase::Continue,
                },
                override_prompt,
                additional_prompt,
                explicit_executor_config: executor_config.cloned(),
                strict_config_resolution: true,
                active_workflow,
                audit_session_key: Some(session_id.clone()),
            })
            .await?;

        if let Some(vfs) = prepared.vfs.as_mut()
            && let Ok(Some(meta)) = self.hub.get_session_meta(&session_id).await
        {
            append_visible_canvas_mounts(
                self.repos.canvas_repo.as_ref(),
                task.project_id,
                vfs,
                &meta.visible_canvas_mount_ids,
            )
            .await
            .map_err(|error| TaskExecutionError::Internal(error.to_string()))?;
        }

        let base = PromptSessionRequest::from_user_input(UserPromptInput::from_text(""));
        let context_sources = prepared.source_summary.clone();
        let mut req = finalize_request(base, prepared);
        req.identity = identity;
        req.post_turn_handler = Some(Arc::new(
            super::gateway::effect_executor::TaskHookEffectExecutor {
                repos: self.repos.clone(),
                task_id: task.id,
                session_id: session_id.clone(),
                backend_id: backend_id.clone(),
            },
        )
            as crate::session::post_turn_handler::DynPostTurnHandler);

        let turn_id = match self.hub.start_prompt(&session_id, req).await {
            Ok(turn_id) => turn_id,
            Err(err) => {
                if phase == ExecutionPhase::Start {
                    let run_service = LifecycleRunService::new(
                        self.repos.lifecycle_definition_repo.as_ref(),
                        self.repos.lifecycle_run_repo.as_ref(),
                    )
                    .with_projector(build_step_projector_from_repos(&self.repos));
                    let _ = run_service
                        .fail_step(crate::workflow::FailLifecycleStepCommand {
                            run_id: active_run.id,
                            step_key: step.key.clone(),
                            summary: Some(format!("start_prompt_failed: {err}")),
                        })
                        .await;
                    clear_task_session_binding(&self.repos, task.id, &backend_id, "start_failed")
                        .await;
                }
                return Err(map_connector_error(err));
            }
        };

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

    async fn find_story_session_id(&self, story_id: Uuid) -> Result<String, TaskExecutionError> {
        let binding = self
            .repos
            .session_binding_repo
            .find_by_owner_and_label(SessionOwnerType::Story, story_id, "companion")
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!("Story {story_id} 未绑定 companion session"))
            })?;
        Ok(binding.session_id)
    }

    async fn find_active_run_for_story_session(
        &self,
        session_id: &str,
    ) -> Result<agentdash_domain::workflow::LifecycleRun, TaskExecutionError> {
        let runs = self
            .repos
            .lifecycle_run_repo
            .list_by_session(session_id)
            .await
            .map_err(map_domain_error)?;
        crate::workflow::select_active_run(runs).ok_or_else(|| {
            TaskExecutionError::UnprocessableEntity(format!(
                "Story session {session_id} 无活跃 lifecycle run"
            ))
        })
    }

    /// 根据 step_key（可选）从 lifecycle definition 定位 step，再通过
    /// `Task.lifecycle_step_key` 找到对应 Task。
    async fn resolve_task_for_step(
        &self,
        story_id: Uuid,
        run: &agentdash_domain::workflow::LifecycleRun,
        step_key_hint: Option<&str>,
    ) -> Result<
        (
            Task,
            String,
            agentdash_domain::workflow::LifecycleDefinition,
        ),
        TaskExecutionError,
    > {
        let lifecycle = self
            .repos
            .lifecycle_definition_repo
            .get_by_id(run.lifecycle_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!(
                    "lifecycle_definition {} 不存在",
                    run.lifecycle_id
                ))
            })?;

        let step = match step_key_hint {
            Some(key) => lifecycle
                .steps
                .iter()
                .find(|s| s.key == key)
                .cloned()
                .ok_or_else(|| {
                    TaskExecutionError::NotFound(format!(
                        "lifecycle {} 中不存在 step '{}'",
                        lifecycle.id, key
                    ))
                })?,
            None => {
                // 回退到当前活跃 step
                let active_key = run.current_step_key().ok_or_else(|| {
                    TaskExecutionError::UnprocessableEntity("LifecycleRun 无活跃 step".to_string())
                })?;
                lifecycle
                    .steps
                    .iter()
                    .find(|s| s.key == active_key)
                    .cloned()
                    .ok_or_else(|| {
                        TaskExecutionError::NotFound(format!(
                            "lifecycle {} 中不存在 step '{}'",
                            lifecycle.id, active_key
                        ))
                    })?
            }
        };

        let story = self
            .repos
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| TaskExecutionError::NotFound(format!("Story {story_id} 不存在")))?;
        let task = story
            .tasks
            .iter()
            .find(|task| task.lifecycle_step_key.as_deref() == Some(step.key.as_str()))
            .cloned()
            .ok_or_else(|| {
                TaskExecutionError::UnprocessableEntity(format!(
                    "Story {story_id} 中不存在绑定 lifecycle step '{}' 的 Task",
                    step.key
                ))
            })?;
        Ok((task, step.key, lifecycle))
    }

    pub async fn get_task_session(
        &self,
        task_id: Uuid,
    ) -> Result<TaskSessionResult, TaskExecutionError> {
        let task = gw_get_task(&self.repos, task_id).await?;
        let session_id = self.resolve_execution_session_id(task_id).await?;

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
        let step_key = self
            .resolve_or_bind_step_key_for_task(task.story_id, task.id)
            .await?;
        self.activate_story_step(
            task.story_id,
            Some(step_key),
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
        let step_key = self
            .resolve_or_bind_step_key_for_task(task.story_id, task.id)
            .await?;
        self.activate_story_step(
            task.story_id,
            Some(step_key),
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

    async fn resolve_or_bind_step_key_for_task(
        &self,
        story_id: Uuid,
        task_id: Uuid,
    ) -> Result<String, TaskExecutionError> {
        let story = self
            .repos
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| TaskExecutionError::NotFound(format!("Story {story_id} 不存在")))?;
        let task = story.find_task(task_id).ok_or_else(|| {
            TaskExecutionError::NotFound(format!("Task {task_id} 不属于 Story {story_id}"))
        })?;
        if let Some(step_key) = task.lifecycle_step_key.as_deref().filter(|s| !s.is_empty()) {
            self.validate_step_key_for_story(story_id, step_key).await?;
            return Ok(step_key.to_string());
        }

        let story_session_id = self.find_story_session_id(story_id).await?;
        let active_run = self
            .find_active_run_for_story_session(&story_session_id)
            .await?;
        let lifecycle = self
            .repos
            .lifecycle_definition_repo
            .get_by_id(active_run.lifecycle_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!(
                    "lifecycle_definition {} 不存在",
                    active_run.lifecycle_id
                ))
            })?;

        let step_key = active_run
            .active_node_keys
            .iter()
            .find(|key| {
                lifecycle.steps.iter().any(|step| step.key == **key)
                    && !story.tasks.iter().any(|task| {
                        task.id != task_id && task.lifecycle_step_key.as_deref() == Some(key.as_str())
                    })
            })
            .cloned()
            .or_else(|| {
                let only_step = lifecycle.steps.first()?;
                let occupied = story.tasks.iter().any(|task| {
                    task.id != task_id
                        && task.lifecycle_step_key.as_deref() == Some(only_step.key.as_str())
                });
                (!occupied && lifecycle.steps.len() == 1).then(|| only_step.key.clone())
            })
            .ok_or_else(|| {
                TaskExecutionError::UnprocessableEntity(format!(
                    "Task {task_id} 尚未绑定 lifecycle step，且 Story {story_id} 的活跃 lifecycle 无可自动绑定 step"
                ))
            })?;

        let mut story_to_update = self
            .repos
            .story_repo
            .get_by_id(story_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| TaskExecutionError::NotFound(format!("Story {story_id} 不存在")))?;
        let updated = story_to_update.update_task(task_id, |view| {
            *view.lifecycle_step_key = Some(step_key.clone());
        });
        if updated.is_none() {
            return Err(TaskExecutionError::NotFound(format!(
                "Task {task_id} 不属于 Story {story_id}"
            )));
        }
        self.repos
            .story_repo
            .update(&story_to_update)
            .await
            .map_err(map_domain_error)?;

        Ok(step_key)
    }

    async fn validate_step_key_for_story(
        &self,
        story_id: Uuid,
        step_key: &str,
    ) -> Result<(), TaskExecutionError> {
        let story_session_id = self.find_story_session_id(story_id).await?;
        let active_run = self
            .find_active_run_for_story_session(&story_session_id)
            .await?;
        let lifecycle = self
            .repos
            .lifecycle_definition_repo
            .get_by_id(active_run.lifecycle_id)
            .await
            .map_err(map_domain_error)?
            .ok_or_else(|| {
                TaskExecutionError::NotFound(format!(
                    "lifecycle_definition {} 不存在",
                    active_run.lifecycle_id
                ))
            })?;
        if lifecycle.steps.iter().any(|step| step.key == step_key) {
            Ok(())
        } else {
            Err(TaskExecutionError::UnprocessableEntity(format!(
                "Task 绑定的 lifecycle step '{step_key}' 不存在于 Story {story_id} 的活跃 lifecycle"
            )))
        }
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
        self.hub
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
