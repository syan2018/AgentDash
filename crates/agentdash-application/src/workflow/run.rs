use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use agentdash_domain::story::{ChangeKind, StateChangeRepository, StoryRepository};
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleRunStatus, LifecycleStepState,
};

use super::error::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct StartLifecycleRunCommand {
    pub project_id: Uuid,
    pub lifecycle_id: Option<Uuid>,
    pub lifecycle_key: Option<String>,
    /// 父 session ID — lifecycle run 直接关联 session。
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct ActivateLifecycleStepCommand {
    pub run_id: Uuid,
    pub step_key: String,
}

#[derive(Debug, Clone)]
pub struct CompleteLifecycleStepCommand {
    pub run_id: Uuid,
    pub step_key: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FailLifecycleStepCommand {
    pub run_id: Uuid,
    pub step_key: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecordGateCollisionCommand {
    pub run_id: Uuid,
    pub step_key: String,
}

#[derive(Debug, Clone)]
pub struct BindLifecycleStepSessionCommand {
    pub run_id: Uuid,
    pub step_key: String,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct BindAndActivateLifecycleStepCommand {
    pub run_id: Uuid,
    pub step_key: String,
    pub session_id: String,
}

pub fn select_active_run(runs: Vec<LifecycleRun>) -> Option<LifecycleRun> {
    runs.into_iter()
        .filter(|run| {
            run.current_step_key().is_some()
                && matches!(
                    run.status,
                    LifecycleRunStatus::Ready
                        | LifecycleRunStatus::Running
                        | LifecycleRunStatus::Blocked
                )
        })
        .max_by_key(|run| (active_run_status_priority(run.status), run.updated_at))
}

fn active_run_status_priority(status: LifecycleRunStatus) -> i32 {
    match status {
        LifecycleRunStatus::Running => 3,
        LifecycleRunStatus::Ready => 2,
        LifecycleRunStatus::Blocked => 1,
        LifecycleRunStatus::Draft
        | LifecycleRunStatus::Completed
        | LifecycleRunStatus::Failed
        | LifecycleRunStatus::Cancelled => 0,
    }
}

/// M2 projection sink — LifecycleStepState 推进到 Task view 的投影路径。
///
/// 决策 D-M2-2（α · service 内 explicit append）：projector 逻辑住在 service 内；
/// 当 `projector` 字段 `Some` 时，每次 step state 变更都会：
///
/// 1. 尝试从 `step.task_id` 反查所属 Story；若无 task_id 则跳过（非 Task 绑定 step）。
///    注：当前 `LifecycleStepState` 尚无 `task_id` 字段，此处通过 LifecycleDefinition 的
///    `workflow_key` / step metadata 推断关联 task。M2 阶段仅在 step 直接绑定到 task
///    的场景生效；更广义的 lifecycle-step-to-task 映射由 M5 补齐。
/// 2. 在 Story aggregate 上调 `apply_task_projection`，写 `stories.tasks JSONB`。
/// 3. 同事务追加 `state_changes` 全局投影索引（kind = TaskStatusChanged）。
///
/// 事务说明（D-M2-3）：当前实现为 tx-b（非事务），story update 与 state_change append
/// 两步独立提交。若任一步失败，会记 warning 但不阻塞主 run.update。
/// 事务化 (tx-a) 留待后续任务，参见 `.trellis/spec/backend/story-task-runtime.md` §9。
pub struct LifecycleStepProjector {
    pub story_repo: Arc<dyn StoryRepository>,
    pub state_change_repo: Arc<dyn StateChangeRepository>,
}

impl LifecycleStepProjector {
    /// 对单个 step state 应用投影：定位 task → 更新 Story → append state_change。
    ///
    /// 当前版本通过 step.session_id 上挂的 task_binding 来定位 task；由于 step 没有
    /// 显式 task_id 字段，M2 实现策略是：
    ///   - 扫描 session_binding 找到 owner_type=Task 的绑定
    ///   - 从 binding.owner_id 定位 task（走 story_repo.find_by_task_id）
    ///
    /// 为避免在本 M2 引入额外的 repo 依赖，projector 退化为接受调用方显式传入 task_id：
    /// 调用方（LifecycleRunService）负责从 step.session_id → task_id 的解析（目前通过
    /// 后续 M5 引入的 step.task_id 字段进一步收敛）。
    ///
    /// 本方法因此暂未在生产路径中被主动调用；占位是为了让主线 M5 可接手 projector 注入。
    /// 详见 implementation 报告中的 M2 验收说明。
    pub async fn project_step(
        &self,
        task_id: Uuid,
        step: &LifecycleStepState,
        run_id: Uuid,
        lifecycle_id: Uuid,
    ) -> Result<bool, WorkflowApplicationError> {
        let Some(mut story) = self
            .story_repo
            .find_by_task_id(task_id)
            .await
            .map_err(WorkflowApplicationError::from)?
        else {
            return Ok(false);
        };

        let previous_status = story
            .find_task(task_id)
            .map(|t| t.status().clone());

        let applied = story.apply_task_projection(task_id, step);
        let changed = matches!(applied, Some(true));
        if !changed {
            return Ok(false);
        }

        let project_id = story.project_id;
        let story_id = story.id;
        self.story_repo
            .update(&story)
            .await
            .map_err(WorkflowApplicationError::from)?;

        let next_status = story.find_task(task_id).map(|t| t.status().clone());
        let payload = json!({
            "reason": "lifecycle_step_projection",
            "task_id": task_id,
            "story_id": story_id,
            "run_id": run_id,
            "lifecycle_id": lifecycle_id,
            "step_key": step.step_key,
            "step_status": step.status,
            "from": previous_status,
            "to": next_status,
        });

        if let Err(err) = self
            .state_change_repo
            .append_change(
                project_id,
                task_id,
                ChangeKind::TaskStatusChanged,
                payload,
                None,
            )
            .await
        {
            tracing::warn!(
                task_id = %task_id,
                run_id = %run_id,
                error = %err,
                "M2 projector: state_change 追加失败（story 已更新），不阻塞主流程"
            );
        }
        Ok(true)
    }
}

pub struct LifecycleRunService<'a, L: ?Sized, R: ?Sized> {
    lifecycle_repo: &'a L,
    run_repo: &'a R,
    projector: Option<LifecycleStepProjector>,
}

impl<'a, L: ?Sized, R: ?Sized> LifecycleRunService<'a, L, R>
where
    L: LifecycleDefinitionRepository,
    R: LifecycleRunRepository,
{
    pub fn new(lifecycle_repo: &'a L, run_repo: &'a R) -> Self {
        Self {
            lifecycle_repo,
            run_repo,
            projector: None,
        }
    }

    /// 注入投影器（M2）。未注入时 service 行为与 M1 保持一致。
    pub fn with_projector(mut self, projector: LifecycleStepProjector) -> Self {
        self.projector = Some(projector);
        self
    }

    pub async fn start_run(
        &self,
        cmd: StartLifecycleRunCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let lifecycle = self.resolve_lifecycle(&cmd).await?;

        // 同一 session 不能同时有多个活跃 run
        let existing_runs = self.run_repo.list_by_session(&cmd.session_id).await?;
        let conflicting_run = existing_runs.iter().find(|run| {
            matches!(
                run.status,
                LifecycleRunStatus::Ready
                    | LifecycleRunStatus::Running
                    | LifecycleRunStatus::Blocked
            )
        });
        if let Some(conflicting) = conflicting_run {
            return Err(WorkflowApplicationError::Conflict(format!(
                "session {} 已存在进行中的 lifecycle run（lifecycle_id={}）",
                cmd.session_id, conflicting.lifecycle_id
            )));
        }

        let run = LifecycleRun::new(
            cmd.project_id,
            lifecycle.id,
            &cmd.session_id,
            &lifecycle.steps,
            &lifecycle.entry_step_key,
            &lifecycle.edges,
        )
        .map_err(WorkflowApplicationError::BadRequest)?;
        self.run_repo.create(&run).await?;
        Ok(run)
    }

    pub async fn activate_step(
        &self,
        cmd: ActivateLifecycleStepCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let mut run = self.load_run(cmd.run_id).await?;
        run.activate_step(&cmd.step_key)
            .map_err(WorkflowApplicationError::Conflict)?;
        self.run_repo.update(&run).await?;
        self.project_step_if_applicable(&run, &cmd.step_key).await;
        Ok(run)
    }

    pub async fn complete_step(
        &self,
        cmd: CompleteLifecycleStepCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let mut run = self.load_run(cmd.run_id).await?;
        let lifecycle = self.load_lifecycle(run.lifecycle_id).await?;

        run.complete_step(&cmd.step_key, cmd.summary, &lifecycle.edges)
            .map_err(WorkflowApplicationError::Conflict)?;
        self.run_repo.update(&run).await?;
        self.project_step_if_applicable(&run, &cmd.step_key).await;
        Ok(run)
    }

    pub async fn fail_step(
        &self,
        cmd: FailLifecycleStepCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let mut run = self.load_run(cmd.run_id).await?;
        run.fail_step(&cmd.step_key, cmd.summary)
            .map_err(WorkflowApplicationError::Conflict)?;
        self.run_repo.update(&run).await?;
        self.project_step_if_applicable(&run, &cmd.step_key).await;
        Ok(run)
    }

    pub async fn record_gate_collision(
        &self,
        cmd: RecordGateCollisionCommand,
    ) -> Result<(LifecycleRun, u32), WorkflowApplicationError> {
        let mut run = self.load_run(cmd.run_id).await?;
        let count = run
            .record_gate_collision(&cmd.step_key)
            .map_err(WorkflowApplicationError::Conflict)?;
        self.run_repo.update(&run).await?;
        Ok((run, count))
    }

    pub async fn bind_step_session(
        &self,
        cmd: BindLifecycleStepSessionCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let mut run = self.load_run(cmd.run_id).await?;
        run.bind_step_session(&cmd.step_key, cmd.session_id)
            .map_err(WorkflowApplicationError::Conflict)?;
        self.run_repo.update(&run).await?;
        Ok(run)
    }

    pub async fn bind_session_and_activate_step(
        &self,
        cmd: BindAndActivateLifecycleStepCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let mut run = self.load_run(cmd.run_id).await?;
        run.bind_step_session(&cmd.step_key, cmd.session_id)
            .map_err(WorkflowApplicationError::Conflict)?;
        run.activate_step(&cmd.step_key)
            .map_err(WorkflowApplicationError::Conflict)?;
        self.run_repo.update(&run).await?;
        self.project_step_if_applicable(&run, &cmd.step_key).await;
        Ok(run)
    }

    async fn resolve_lifecycle(
        &self,
        cmd: &StartLifecycleRunCommand,
    ) -> Result<LifecycleDefinition, WorkflowApplicationError> {
        match (&cmd.lifecycle_id, &cmd.lifecycle_key) {
            (Some(_), Some(_)) => Err(WorkflowApplicationError::BadRequest(
                "lifecycle_id 与 lifecycle_key 只能提供一个".to_string(),
            )),
            (None, None) => Err(WorkflowApplicationError::BadRequest(
                "必须提供 lifecycle_id 或 lifecycle_key".to_string(),
            )),
            (Some(lifecycle_id), None) => self.load_lifecycle(*lifecycle_id).await,
            (None, Some(lifecycle_key)) => self
                .lifecycle_repo
                .get_by_key(lifecycle_key)
                .await?
                .ok_or_else(|| {
                    WorkflowApplicationError::NotFound(format!(
                        "lifecycle_definition 不存在: {}",
                        lifecycle_key
                    ))
                }),
        }
    }

    async fn load_lifecycle(
        &self,
        lifecycle_id: Uuid,
    ) -> Result<LifecycleDefinition, WorkflowApplicationError> {
        self.lifecycle_repo
            .get_by_id(lifecycle_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_definition 不存在: {}",
                    lifecycle_id
                ))
            })
    }

    async fn load_run(&self, run_id: Uuid) -> Result<LifecycleRun, WorkflowApplicationError> {
        self.run_repo.get_by_id(run_id).await?.ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!("lifecycle_run 不存在: {}", run_id))
        })
    }

    /// M2 projection 钩子：若 projector 已注入，尝试把 step state 投影到对应 task。
    ///
    /// 由于 LifecycleStepState 暂未携带 `task_id` 字段，本实现通过 step.session_id
    /// 绑定的 SessionBinding（owner_type=Task）反查 task_id 的路径需要额外 repo 依赖，
    /// 本 M2 版本采用**保守路径**：不主动投影（除非显式通过 `project_step`）。
    ///
    /// 这是一个 `[UNRESOLVED]`：主线 M5 在引入 `LifecycleStepDefinition.task_id` 或
    /// 等价 step-task binding 后，这里改为直接 `projector.project_step(task_id, step, ...)`。
    async fn project_step_if_applicable(&self, _run: &LifecycleRun, _step_key: &str) {
        if self.projector.is_none() {
            return;
        }
        // 当前为 no-op；M5 补齐 step→task 定位后激活投影路径。
        // 设计约束：
        //   - step 与 task 的映射语义在本任务范围外（M5 接手）；
        //   - projector 已注册但暂不工作，其他路径（state_reconciler 的启动期重建）继续负责真相校准。
    }
}

