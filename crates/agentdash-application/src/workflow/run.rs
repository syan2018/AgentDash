use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleRunStatus,
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

pub struct LifecycleRunService<'a, L: ?Sized, R: ?Sized> {
    lifecycle_repo: &'a L,
    run_repo: &'a R,
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
        }
    }

    pub async fn start_run(
        &self,
        cmd: StartLifecycleRunCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let lifecycle = self.resolve_lifecycle(&cmd).await?;

        if !lifecycle.is_active() {
            return Err(WorkflowApplicationError::Conflict(format!(
                "lifecycle `{}` 状态为 {:?}，不能启动 run",
                lifecycle.key, lifecycle.status
            )));
        }

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
}

