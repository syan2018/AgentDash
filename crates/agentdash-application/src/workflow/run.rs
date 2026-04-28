use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use agentdash_domain::session_binding::{SessionBindingRepository, SessionOwnerType};
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
/// 1. 通过 `run.session_id` 找到 Story owner binding，再在 Story aggregate 的
///    `Task.lifecycle_step_key` 上定位对应 Task。Lifecycle definition 是可复用模板，
///    不持有具体 task id。
/// 2. 在 Story aggregate 上调 `apply_task_projection`，写 `stories.tasks JSONB`。
/// 3. 同事务追加 `state_changes` 全局投影索引（kind = TaskStatusChanged）。
///
/// 事务说明（D-M2-3）：当前实现为 tx-b（非事务），story update 与 state_change append
/// 两步独立提交。若任一步失败，会记 warning 但不阻塞主 run.update。
/// 事务化 (tx-a) 留待后续任务，参见 `.trellis/spec/backend/story-task-runtime.md` §9。
pub struct LifecycleStepProjector {
    pub story_repo: Arc<dyn StoryRepository>,
    pub state_change_repo: Arc<dyn StateChangeRepository>,
    pub session_binding_repo: Arc<dyn SessionBindingRepository>,
}

impl LifecycleStepProjector {
    /// 对单个 step state 应用投影：Story session → Story → step key → Task。
    pub async fn project_step(
        &self,
        run: &LifecycleRun,
        step: &LifecycleStepState,
    ) -> Result<bool, WorkflowApplicationError> {
        let Some(mut story) = self.story_for_run_session(&run.session_id).await? else {
            return Ok(false);
        };
        let Some(task_id) = story
            .tasks
            .iter()
            .find(|task| task.lifecycle_step_key.as_deref() == Some(step.step_key.as_str()))
            .map(|task| task.id)
        else {
            return Ok(false);
        };

        let previous_status = story.find_task(task_id).map(|t| t.status().clone());

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
            "run_id": run.id,
            "lifecycle_id": run.lifecycle_id,
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
                run_id = %run.id,
                error = %err,
                "M2 projector: state_change 追加失败（story 已更新），不阻塞主流程"
            );
        }
        Ok(true)
    }

    async fn story_for_run_session(
        &self,
        session_id: &str,
    ) -> Result<Option<agentdash_domain::story::Story>, WorkflowApplicationError> {
        let bindings = self
            .session_binding_repo
            .list_by_session(session_id)
            .await
            .map_err(WorkflowApplicationError::from)?;
        let Some(story_binding) = bindings
            .iter()
            .find(|binding| binding.owner_type == SessionOwnerType::Story)
        else {
            return Ok(None);
        };
        self.story_repo
            .get_by_id(story_binding.owner_id)
            .await
            .map_err(WorkflowApplicationError::from)
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
        session_binding_repo: repos.session_binding_repo.clone(),
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
    /// 绑定信号：Story aggregate 内 `Task.lifecycle_step_key == step_key`。
    /// Lifecycle definition 是可复用模板，不持有具体 Task id。
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

        if let Err(err) = projector.project_step(run, state).await {
            tracing::warn!(
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
    use agentdash_domain::session_binding::{ProjectSessionBinding, SessionBinding};
    use agentdash_domain::story::{
        ChangeKind, StateChange, StateChangeRepository, Story, StoryRepository,
    };
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
            Ok(self
                .stories
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.id == id)
                .cloned())
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
        async fn latest_event_id_by_project(&self, _project_id: Uuid) -> Result<i64, DomainError> {
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
        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.id == id)
                .cloned())
        }
        async fn get_by_key(&self, key: &str) -> Result<Option<LifecycleDefinition>, DomainError> {
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
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.id == id)
                .cloned())
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

    struct InMemorySessionBindingRepo {
        bindings: Mutex<Vec<SessionBinding>>,
    }

    #[async_trait]
    impl SessionBindingRepository for InMemorySessionBindingRepo {
        async fn create(&self, binding: &SessionBinding) -> Result<(), DomainError> {
            self.bindings.lock().unwrap().push(binding.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.bindings.lock().unwrap().retain(|b| b.id != id);
            Ok(())
        }

        async fn delete_by_session_and_owner(
            &self,
            session_id: &str,
            owner_type: SessionOwnerType,
            owner_id: Uuid,
        ) -> Result<(), DomainError> {
            self.bindings.lock().unwrap().retain(|b| {
                !(b.session_id == session_id
                    && b.owner_type == owner_type
                    && b.owner_id == owner_id)
            });
            Ok(())
        }

        async fn list_by_owner(
            &self,
            owner_type: SessionOwnerType,
            owner_id: Uuid,
        ) -> Result<Vec<SessionBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .filter(|b| b.owner_type == owner_type && b.owner_id == owner_id)
                .cloned()
                .collect())
        }

        async fn list_by_session(
            &self,
            session_id: &str,
        ) -> Result<Vec<SessionBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .filter(|b| b.session_id == session_id)
                .cloned()
                .collect())
        }

        async fn find_by_owner_and_label(
            &self,
            owner_type: SessionOwnerType,
            owner_id: Uuid,
            label: &str,
        ) -> Result<Option<SessionBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .find(|b| b.owner_type == owner_type && b.owner_id == owner_id && b.label == label)
                .cloned())
        }

        async fn list_bound_session_ids(&self) -> Result<Vec<String>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .map(|b| b.session_id.clone())
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectSessionBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .unwrap()
                .iter()
                .filter(|b| b.project_id == project_id)
                .cloned()
                .map(|binding| ProjectSessionBinding {
                    binding,
                    story_title: None,
                    story_id: None,
                    owner_title: None,
                })
                .collect())
        }
    }

    // ── fixtures ─────────────────────────────────────────────────
    fn make_step(key: &str) -> LifecycleStepDefinition {
        LifecycleStepDefinition {
            key: key.to_string(),
            description: String::new(),
            workflow_key: None,
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
        }
    }

    fn setup_story_with_task(step_key: Option<&str>) -> (Story, Uuid, Uuid) {
        let project_id = Uuid::new_v4();
        let mut story = Story::new(project_id, "S".into(), "".into());
        let mut task = Task::new(project_id, story.id, "T".into(), String::new());
        task.lifecycle_step_key = step_key.map(str::to_string);
        let task_id = task.id;
        story.add_task(task);
        (story, project_id, task_id)
    }

    fn session_bindings_for_story(
        project_id: Uuid,
        story_id: Uuid,
        session_id: &str,
    ) -> Arc<dyn SessionBindingRepository> {
        Arc::new(InMemorySessionBindingRepo {
            bindings: Mutex::new(vec![SessionBinding::new(
                project_id,
                session_id.to_string(),
                SessionOwnerType::Story,
                story_id,
                "companion",
            )]),
        })
    }

    // ── 实际断言测试 ─────────────────────────────────────────────

    #[tokio::test]
    async fn projector_projects_task_on_complete_step() {
        let (story, project_id, task_id) = setup_story_with_task(Some("start"));
        let story_id = story.id;
        let session_id = "sess-test-m2b";

        let steps = vec![make_step("start"), make_step("finish")];
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
            session_id,
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
                session_binding_repo: session_bindings_for_story(project_id, story_id, session_id),
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
        let (story, project_id, task_id) = setup_story_with_task(Some("only"));
        let story_id = story.id;
        let session_id = "sess-m2b-activate";
        let steps = vec![make_step("only")];
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
            session_id,
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
                session_binding_repo: session_bindings_for_story(project_id, story_id, session_id),
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
        let (story, project_id, task_id) = setup_story_with_task(Some("only"));
        let story_id = story.id;
        let session_id = "sess-m2b-fail";
        let steps = vec![make_step("only")];
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
            session_id,
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
                session_binding_repo: session_bindings_for_story(project_id, story_id, session_id),
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
    async fn projector_skips_when_task_has_no_step_binding() {
        let (story, project_id, task_id) = setup_story_with_task(None);
        let story_id = story.id;
        let session_id = "sess-m2b-notask";
        let original_status = story.find_task(task_id).unwrap().status().clone();
        let steps = vec![make_step("only")]; // Task 未绑定 step key
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
            session_id,
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
                session_binding_repo: session_bindings_for_story(project_id, story_id, session_id),
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
            "task 未绑定 lifecycle_step_key 时,task 状态不应被投影改写"
        );
    }

    #[tokio::test]
    async fn service_without_projector_noops_silently() {
        let (story, project_id, task_id) = setup_story_with_task(Some("only"));
        let steps = vec![make_step("only")];
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
