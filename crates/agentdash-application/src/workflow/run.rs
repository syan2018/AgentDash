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

/// 从 [`RepositorySet`] 构造 projector — M2-b 统一注入入口。
///
/// 调用方通常写成：
///
/// ```ignore
/// let service = LifecycleRunService::new(
///     repos.lifecycle_definition_repo.as_ref(),
///     repos.lifecycle_run_repo.as_ref(),
/// )
/// .with_projector(build_step_projector_from_repos(repos));
/// ```
pub fn build_step_projector_from_repos(
    repos: &crate::repository_set::RepositorySet,
) -> LifecycleStepProjector {
    LifecycleStepProjector {
        story_repo: repos.story_repo.clone(),
        state_change_repo: repos.state_change_repo.clone(),
    }
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

    /// M2-b projection 钩子：若 projector 已注入，把 step state 投影到对应 task。
    ///
    /// 绑定信号：`LifecycleStepDefinition.task_id`（M2-b 引入）。
    /// 仅当 definition 声明了 `task_id` 时触发投影；否则静默跳过（非 task-bound step）。
    ///
    /// 失败策略：projector 失败仅记 warn，不阻塞主 run.update（与 tx-b 非事务语义一致）。
    /// 详见 [`LifecycleStepProjector`] 文档。
    async fn project_step_if_applicable(&self, run: &LifecycleRun, step_key: &str) {
        let Some(projector) = self.projector.as_ref() else {
            return;
        };
        let Some(state) = run.step_states.iter().find(|s| s.step_key == step_key) else {
            return;
        };

        // 从 lifecycle definition 找 step.task_id 绑定
        let task_id = match self.lifecycle_repo.get_by_id(run.lifecycle_id).await {
            Ok(Some(lifecycle)) => lifecycle
                .steps
                .iter()
                .find(|s| s.key == step_key)
                .and_then(|s| s.task_id),
            Ok(None) => {
                tracing::warn!(
                    run_id = %run.id,
                    lifecycle_id = %run.lifecycle_id,
                    step_key = %step_key,
                    "M2-b projector: lifecycle definition 不存在，跳过投影"
                );
                return;
            }
            Err(err) => {
                tracing::warn!(
                    run_id = %run.id,
                    step_key = %step_key,
                    error = %err,
                    "M2-b projector: 加载 lifecycle definition 失败，跳过投影"
                );
                return;
            }
        };

        let Some(task_id) = task_id else {
            return;
        };

        if let Err(err) = projector
            .project_step(task_id, state, run.id, run.lifecycle_id)
            .await
        {
            tracing::warn!(
                task_id = %task_id,
                run_id = %run.id,
                step_key = %step_key,
                error = %err,
                "M2-b projector: step 投影失败，不阻塞主流程"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::story::{ChangeKind, StateChange, StateChangeRepository, Story, StoryRepository};
    use agentdash_domain::task::{Task, TaskStatus};
    use agentdash_domain::workflow::{
        LifecycleDefinition, LifecycleDefinitionRepository, LifecycleEdge, LifecycleRun,
        LifecycleRunRepository, LifecycleStepDefinition, WorkflowBindingKind,
        WorkflowDefinitionSource,
    };

    // ── In-memory 测试双 ────────────────────────────────────────
    struct InMemoryStoryRepo {
        stories: Mutex<Vec<Story>>,
    }

    impl InMemoryStoryRepo {
        fn new(story: Story) -> Self {
            Self {
                stories: Mutex::new(vec![story]),
            }
        }
    }

    #[async_trait]
    impl StoryRepository for InMemoryStoryRepo {
        async fn create(&self, story: &Story) -> Result<(), DomainError> {
            self.stories.lock().unwrap().push(story.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<Story>, DomainError> {
            Ok(self.stories.lock().unwrap().iter().find(|s| s.id == id).cloned())
        }

        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Story>, DomainError> {
            Ok(self
                .stories
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, story: &Story) -> Result<(), DomainError> {
            let mut guard = self.stories.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|s| s.id == story.id) {
                *existing = story.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.stories.lock().unwrap().retain(|s| s.id != id);
            Ok(())
        }

        async fn find_by_task_id(&self, task_id: Uuid) -> Result<Option<Story>, DomainError> {
            Ok(self
                .stories
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.tasks.iter().any(|t| t.id == task_id))
                .cloned())
        }
    }

    struct InMemoryStateChangeRepo {
        changes: Mutex<Vec<(Uuid, Uuid, ChangeKind, serde_json::Value)>>,
    }

    impl InMemoryStateChangeRepo {
        fn new() -> Self {
            Self {
                changes: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl StateChangeRepository for InMemoryStateChangeRepo {
        async fn get_changes_since(
            &self,
            _since_id: i64,
            _limit: i64,
        ) -> Result<Vec<StateChange>, DomainError> {
            Ok(vec![])
        }
        async fn get_changes_since_by_project(
            &self,
            _project_id: Uuid,
            _since_id: i64,
            _limit: i64,
        ) -> Result<Vec<StateChange>, DomainError> {
            Ok(vec![])
        }
        async fn latest_event_id(&self) -> Result<i64, DomainError> {
            Ok(0)
        }
        async fn latest_event_id_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<i64, DomainError> {
            Ok(0)
        }
        async fn append_change(
            &self,
            project_id: Uuid,
            entity_id: Uuid,
            kind: ChangeKind,
            payload: serde_json::Value,
            _backend_id: Option<&str>,
        ) -> Result<(), DomainError> {
            self.changes
                .lock()
                .unwrap()
                .push((project_id, entity_id, kind, payload));
            Ok(())
        }
    }

    struct InMemoryLifecycleDefRepo {
        definitions: Mutex<Vec<LifecycleDefinition>>,
    }

    #[async_trait]
    impl LifecycleDefinitionRepository for InMemoryLifecycleDefRepo {
        async fn create(&self, def: &LifecycleDefinition) -> Result<(), DomainError> {
            self.definitions.lock().unwrap().push(def.clone());
            Ok(())
        }
        async fn get_by_id(
            &self,
            id: Uuid,
        ) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.id == id)
                .cloned())
        }
        async fn get_by_key(
            &self,
            key: &str,
        ) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.key == key)
                .cloned())
        }
        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.project_id == project_id && d.key == key)
                .cloned())
        }
        async fn list_all(&self) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self.definitions.lock().unwrap().clone())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .filter(|d| d.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn list_by_binding_kind(
            &self,
            binding_kind: WorkflowBindingKind,
        ) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .filter(|d| d.binding_kind == binding_kind)
                .cloned()
                .collect())
        }
        async fn update(&self, def: &LifecycleDefinition) -> Result<(), DomainError> {
            let mut guard = self.definitions.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|d| d.id == def.id) {
                *existing = def.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.definitions.lock().unwrap().retain(|d| d.id != id);
            Ok(())
        }
    }

    struct InMemoryLifecycleRunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for InMemoryLifecycleRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().unwrap().push(run.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self.runs.lock().unwrap().iter().find(|r| r.id == id).cloned())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn list_by_lifecycle(
            &self,
            lifecycle_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.lifecycle_id == lifecycle_id)
                .cloned()
                .collect())
        }
        async fn list_by_session(
            &self,
            session_id: &str,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.session_id == session_id)
                .cloned()
                .collect())
        }
        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut guard = self.runs.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|r| r.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.runs.lock().unwrap().retain(|r| r.id != id);
            Ok(())
        }
    }

    // ── fixtures ─────────────────────────────────────────────────
    fn make_step_with_task(key: &str, task_id: Option<Uuid>) -> LifecycleStepDefinition {
        LifecycleStepDefinition {
            key: key.to_string(),
            description: String::new(),
            workflow_key: None,
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
            task_id,
        }
    }

    fn setup_story_with_task() -> (Story, Uuid, Uuid) {
        let project_id = Uuid::new_v4();
        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = Task::new(project_id, story.id, "T".into(), String::new());
        let task_id = task.id;
        story.add_task(task);
        (story, project_id, task_id)
    }

    // ── 实际断言测试 ─────────────────────────────────────────────

    #[tokio::test]
    async fn projector_projects_task_on_complete_step() {
        let (story, project_id, task_id) = setup_story_with_task();

        let steps = vec![
            make_step_with_task("start", Some(task_id)),
            make_step_with_task("finish", None),
        ];
        let edges = vec![LifecycleEdge::flow("start", "finish")];
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "lc",
            "Lifecycle",
            "",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "start",
            steps,
            edges.clone(),
        )
        .expect("lifecycle");
        let lifecycle_id = lifecycle.id;

        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo::new(story));
        let state_change_repo: Arc<dyn StateChangeRepository> =
            Arc::new(InMemoryStateChangeRepo::new());

        let lifecycle_def_repo = InMemoryLifecycleDefRepo {
            definitions: Mutex::new(vec![lifecycle.clone()]),
        };
        let run_repo = InMemoryLifecycleRunRepo {
            runs: Mutex::new(Vec::new()),
        };

        // 构造 run
        let run = LifecycleRun::new(
            project_id,
            lifecycle_id,
            "sess-test-m2b",
            &lifecycle.steps,
            "start",
            &lifecycle.edges,
        )
        .expect("run");
        let run_id = run.id;
        run_repo.create(&run).await.unwrap();

        let service = LifecycleRunService::new(&lifecycle_def_repo, &run_repo).with_projector(
            LifecycleStepProjector {
                story_repo: story_repo.clone(),
                state_change_repo: state_change_repo.clone(),
            },
        );

        // complete start step → 应投影到 task.status = AwaitingVerification（Completed step
        // 映射到 AwaitingVerification，见 Task::apply_projection）
        service
            .complete_step(CompleteLifecycleStepCommand {
                run_id,
                step_key: "start".to_string(),
                summary: Some("done".into()),
            })
            .await
            .expect("complete_step");

        let updated_story = story_repo
            .find_by_task_id(task_id)
            .await
            .unwrap()
            .expect("story");
        let task = updated_story.find_task(task_id).expect("task");
        assert_eq!(
            *task.status(),
            TaskStatus::AwaitingVerification,
            "complete_step 后 task 应自动投影为 AwaitingVerification"
        );
    }

    #[tokio::test]
    async fn projector_projects_task_on_activate_step() {
        let (story, project_id, task_id) = setup_story_with_task();
        let steps = vec![make_step_with_task("only", Some(task_id))];
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "lc",
            "Lifecycle",
            "",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "only",
            steps,
            vec![],
        )
        .expect("lifecycle");
        let lifecycle_id = lifecycle.id;

        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo::new(story));
        let state_change_repo: Arc<dyn StateChangeRepository> =
            Arc::new(InMemoryStateChangeRepo::new());
        let lifecycle_def_repo = InMemoryLifecycleDefRepo {
            definitions: Mutex::new(vec![lifecycle.clone()]),
        };
        let run_repo = InMemoryLifecycleRunRepo {
            runs: Mutex::new(Vec::new()),
        };
        let run = LifecycleRun::new(
            project_id,
            lifecycle_id,
            "sess-m2b-activate",
            &lifecycle.steps,
            "only",
            &[],
        )
        .expect("run");
        let run_id = run.id;
        run_repo.create(&run).await.unwrap();

        let service = LifecycleRunService::new(&lifecycle_def_repo, &run_repo).with_projector(
            LifecycleStepProjector {
                story_repo: story_repo.clone(),
                state_change_repo,
            },
        );

        service
            .activate_step(ActivateLifecycleStepCommand {
                run_id,
                step_key: "only".to_string(),
            })
            .await
            .expect("activate");

        let updated_story = story_repo
            .find_by_task_id(task_id)
            .await
            .unwrap()
            .expect("story");
        let task = updated_story.find_task(task_id).expect("task");
        assert_eq!(
            *task.status(),
            TaskStatus::Running,
            "activate_step 后 task 应为 Running"
        );
    }

    #[tokio::test]
    async fn projector_projects_task_on_fail_step() {
        let (story, project_id, task_id) = setup_story_with_task();
        let steps = vec![make_step_with_task("only", Some(task_id))];
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "lc",
            "Lifecycle",
            "",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "only",
            steps,
            vec![],
        )
        .expect("lifecycle");
        let lifecycle_id = lifecycle.id;

        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo::new(story));
        let state_change_repo: Arc<dyn StateChangeRepository> =
            Arc::new(InMemoryStateChangeRepo::new());
        let lifecycle_def_repo = InMemoryLifecycleDefRepo {
            definitions: Mutex::new(vec![lifecycle.clone()]),
        };
        let run_repo = InMemoryLifecycleRunRepo {
            runs: Mutex::new(Vec::new()),
        };
        let run = LifecycleRun::new(
            project_id,
            lifecycle_id,
            "sess-m2b-fail",
            &lifecycle.steps,
            "only",
            &[],
        )
        .expect("run");
        let run_id = run.id;
        run_repo.create(&run).await.unwrap();

        let service = LifecycleRunService::new(&lifecycle_def_repo, &run_repo).with_projector(
            LifecycleStepProjector {
                story_repo: story_repo.clone(),
                state_change_repo,
            },
        );

        service
            .fail_step(FailLifecycleStepCommand {
                run_id,
                step_key: "only".to_string(),
                summary: Some("boom".into()),
            })
            .await
            .expect("fail");

        let updated_story = story_repo
            .find_by_task_id(task_id)
            .await
            .unwrap()
            .expect("story");
        let task = updated_story.find_task(task_id).expect("task");
        assert_eq!(
            *task.status(),
            TaskStatus::Failed,
            "fail_step 后 task 应为 Failed"
        );
    }

    #[tokio::test]
    async fn projector_skips_when_step_has_no_task_binding() {
        let (story, project_id, task_id) = setup_story_with_task();
        let original_status = story.find_task(task_id).unwrap().status().clone();
        let steps = vec![make_step_with_task("only", None)]; // 无 task_id 绑定
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "lc",
            "Lifecycle",
            "",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "only",
            steps,
            vec![],
        )
        .expect("lifecycle");
        let lifecycle_id = lifecycle.id;

        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo::new(story));
        let state_change_repo: Arc<dyn StateChangeRepository> =
            Arc::new(InMemoryStateChangeRepo::new());
        let lifecycle_def_repo = InMemoryLifecycleDefRepo {
            definitions: Mutex::new(vec![lifecycle.clone()]),
        };
        let run_repo = InMemoryLifecycleRunRepo {
            runs: Mutex::new(Vec::new()),
        };
        let run = LifecycleRun::new(
            project_id,
            lifecycle_id,
            "sess-m2b-notask",
            &lifecycle.steps,
            "only",
            &[],
        )
        .expect("run");
        let run_id = run.id;
        run_repo.create(&run).await.unwrap();

        let service = LifecycleRunService::new(&lifecycle_def_repo, &run_repo).with_projector(
            LifecycleStepProjector {
                story_repo: story_repo.clone(),
                state_change_repo,
            },
        );

        service
            .activate_step(ActivateLifecycleStepCommand {
                run_id,
                step_key: "only".to_string(),
            })
            .await
            .expect("activate");

        // step 未绑定 task，task 状态不应变
        let updated_story = story_repo
            .find_by_task_id(task_id)
            .await
            .unwrap()
            .expect("story");
        let task = updated_story.find_task(task_id).expect("task");
        assert_eq!(
            *task.status(),
            original_status,
            "step 未绑定 task_id 时,task 状态不应被投影改写"
        );
    }

    #[tokio::test]
    async fn service_without_projector_noops_silently() {
        let (story, project_id, task_id) = setup_story_with_task();
        let steps = vec![make_step_with_task("only", Some(task_id))];
        let lifecycle = LifecycleDefinition::new(
            project_id,
            "lc",
            "Lifecycle",
            "",
            WorkflowBindingKind::Story,
            WorkflowDefinitionSource::BuiltinSeed,
            "only",
            steps,
            vec![],
        )
        .expect("lifecycle");
        let lifecycle_id = lifecycle.id;

        let story_repo: Arc<dyn StoryRepository> = Arc::new(InMemoryStoryRepo::new(story.clone()));
        let lifecycle_def_repo = InMemoryLifecycleDefRepo {
            definitions: Mutex::new(vec![lifecycle.clone()]),
        };
        let run_repo = InMemoryLifecycleRunRepo {
            runs: Mutex::new(Vec::new()),
        };
        let run = LifecycleRun::new(
            project_id,
            lifecycle_id,
            "sess-m2b-noinject",
            &lifecycle.steps,
            "only",
            &[],
        )
        .expect("run");
        let run_id = run.id;
        run_repo.create(&run).await.unwrap();

        // 不注入 projector — 行为应当仍然正常,只是 task view 不会被更新
        let service = LifecycleRunService::new(&lifecycle_def_repo, &run_repo);
        service
            .activate_step(ActivateLifecycleStepCommand {
                run_id,
                step_key: "only".to_string(),
            })
            .await
            .expect("activate without projector should still succeed");

        let untouched_story = story_repo
            .find_by_task_id(task_id)
            .await
            .unwrap()
            .expect("story");
        let task = untouched_story.find_task(task_id).expect("task");
        assert_eq!(
            *task.status(),
            TaskStatus::Pending,
            "未注入 projector 时 task 保持初始状态"
        );
    }
}
