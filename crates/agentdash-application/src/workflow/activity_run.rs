use uuid::Uuid;

use agentdash_domain::workflow::{
    ActivityExecutionClaimRepository, ActivityLifecycleDefinition,
    ActivityLifecycleDefinitionRepository, AgentAssignmentRepository, LifecycleRun,
    LifecycleRunRepository, LifecycleRunStatus,
};

use super::scheduler::{ActivityExecutorLaunchOutcome, ActivityExecutorLauncher};
use super::{
    ActivityEvent, ActivityExecutorScheduler, ActivityLifecycleRunState, LifecycleEngine,
    WorkflowApplicationError,
};

pub struct ActivityLifecycleRunService<'a, D: ?Sized, R: ?Sized, C: ?Sized> {
    definition_repo: &'a D,
    run_repo: &'a R,
    claim_repo: &'a C,
    assignment_repo: Option<&'a dyn AgentAssignmentRepository>,
}

#[derive(Debug, Clone)]
pub struct StartActivityLifecycleRunCommand {
    pub project_id: Uuid,
    pub lifecycle_id: Option<Uuid>,
    pub lifecycle_key: Option<String>,
    pub session_id: String,
}

impl<'a, D: ?Sized, R: ?Sized, C: ?Sized> ActivityLifecycleRunService<'a, D, R, C>
where
    D: ActivityLifecycleDefinitionRepository,
    R: LifecycleRunRepository,
    C: ActivityExecutionClaimRepository,
{
    pub fn new(definition_repo: &'a D, run_repo: &'a R, claim_repo: &'a C) -> Self {
        Self {
            definition_repo,
            run_repo,
            claim_repo,
            assignment_repo: None,
        }
    }

    pub fn with_assignment_repo(
        mut self,
        assignment_repo: &'a dyn AgentAssignmentRepository,
    ) -> Self {
        self.assignment_repo = Some(assignment_repo);
        self
    }

    pub async fn start_run(
        &self,
        cmd: StartActivityLifecycleRunCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let definition = self.resolve_definition(&cmd).await?;
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

        let graph_instance_id = uuid::Uuid::new_v4();
        let state = LifecycleEngine::initialize(&definition, graph_instance_id)
            .map_err(|error| WorkflowApplicationError::BadRequest(error.to_string()))?;
        let run =
            LifecycleRun::new_activity(cmd.project_id, definition.id, Some(cmd.session_id), state)
                .map_err(WorkflowApplicationError::BadRequest)?;
        self.run_repo.create(&run).await?;
        Ok(run)
    }

    pub async fn apply_event(
        &self,
        run_id: Uuid,
        event: ActivityEvent,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let (definition, mut run, mut state) = self.load_context(run_id).await?;
        LifecycleEngine::apply_event(&definition, &mut state, event)
            .map_err(|error| WorkflowApplicationError::Conflict(error.to_string()))?;
        run.replace_activity_state(state);
        self.run_repo.update(&run).await?;
        Ok(run)
    }

    pub async fn launch_ready_attempts<L>(
        &self,
        run_id: Uuid,
        launcher: &L,
    ) -> Result<(LifecycleRun, Vec<ActivityExecutorLaunchOutcome>), WorkflowApplicationError>
    where
        L: ActivityExecutorLauncher,
    {
        let (definition, mut run, mut state) = self.load_context(run_id).await?;
        let mut scheduler = ActivityExecutorScheduler::new(self.claim_repo);
        if let Some(assignment_repo) = self.assignment_repo {
            scheduler = scheduler.with_assignment_repo(assignment_repo);
        }
        let outcomes = scheduler
            .launch_ready_attempts(run.id, &definition, &mut state, launcher)
            .await?;
        run.replace_activity_state(state);
        self.run_repo.update(&run).await?;
        Ok((run, outcomes))
    }

    async fn load_context(
        &self,
        run_id: Uuid,
    ) -> Result<
        (
            ActivityLifecycleDefinition,
            LifecycleRun,
            ActivityLifecycleRunState,
        ),
        WorkflowApplicationError,
    > {
        let run = self.run_repo.get_by_id(run_id).await?.ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!("lifecycle_run 不存在: {run_id}"))
        })?;
        let state = run.activity_state.clone().ok_or_else(|| {
            WorkflowApplicationError::BadRequest(format!(
                "lifecycle_run {} 不是 Activity lifecycle run",
                run.id
            ))
        })?;
        let definition = self
            .definition_repo
            .get_by_id(run.lifecycle_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "activity_lifecycle_definition 不存在: {}",
                    run.lifecycle_id
                ))
            })?;
        if definition.project_id != run.project_id {
            return Err(WorkflowApplicationError::NotFound(format!(
                "activity_lifecycle_definition 不存在: {}",
                run.lifecycle_id
            )));
        }
        Ok((definition, run, state))
    }

    async fn resolve_definition(
        &self,
        cmd: &StartActivityLifecycleRunCommand,
    ) -> Result<ActivityLifecycleDefinition, WorkflowApplicationError> {
        match (&cmd.lifecycle_id, &cmd.lifecycle_key) {
            (Some(_), Some(_)) => Err(WorkflowApplicationError::BadRequest(
                "lifecycle_id 与 lifecycle_key 只能提供一个".to_string(),
            )),
            (None, None) => Err(WorkflowApplicationError::BadRequest(
                "必须提供 lifecycle_id 或 lifecycle_key".to_string(),
            )),
            (Some(lifecycle_id), None) => {
                let definition = self
                    .definition_repo
                    .get_by_id(*lifecycle_id)
                    .await?
                    .ok_or_else(|| {
                        WorkflowApplicationError::NotFound(format!(
                            "activity_lifecycle_definition 不存在: {lifecycle_id}"
                        ))
                    })?;
                if definition.project_id != cmd.project_id {
                    return Err(WorkflowApplicationError::NotFound(format!(
                        "activity_lifecycle_definition 不存在: {lifecycle_id}"
                    )));
                }
                Ok(definition)
            }
            (None, Some(lifecycle_key)) => self
                .definition_repo
                .get_by_project_and_key(cmd.project_id, lifecycle_key)
                .await?
                .ok_or_else(|| {
                    WorkflowApplicationError::NotFound(format!(
                        "activity_lifecycle_definition 不存在: {lifecycle_key}"
                    ))
                }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        ActivityAttemptStatus, ActivityCompletionPolicy, ActivityDefinition,
        ActivityExecutionClaim, ActivityExecutorSpec, AgentActivityExecutorSpec,
        AgentSessionPolicy, ExecutorRunRef, WorkflowBindingKind, WorkflowDefinitionSource,
    };

    use super::*;

    struct DefinitionRepo {
        definition: ActivityLifecycleDefinition,
    }

    #[async_trait::async_trait]
    impl ActivityLifecycleDefinitionRepository for DefinitionRepo {
        async fn create(
            &self,
            _lifecycle: &ActivityLifecycleDefinition,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(
            &self,
            id: Uuid,
        ) -> Result<Option<ActivityLifecycleDefinition>, DomainError> {
            Ok((self.definition.id == id).then(|| self.definition.clone()))
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<ActivityLifecycleDefinition>, DomainError> {
            Ok(
                (self.definition.project_id == project_id && self.definition.key == key)
                    .then(|| self.definition.clone()),
            )
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ActivityLifecycleDefinition>, DomainError> {
            Ok((self.definition.project_id == project_id)
                .then(|| vec![self.definition.clone()])
                .unwrap_or_default())
        }

        async fn update(
            &self,
            _lifecycle: &ActivityLifecycleDefinition,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct RunRepo {
        run: Mutex<LifecycleRun>,
    }

    #[async_trait::async_trait]
    impl LifecycleRunRepository for RunRepo {
        async fn create(&self, _run: &LifecycleRun) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            let run = self.run.lock().unwrap().clone();
            Ok((run.id == id).then_some(run))
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            let run = self.run.lock().unwrap().clone();
            Ok(if ids.contains(&run.id) {
                vec![run]
            } else {
                Vec::new()
            })
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            let run = self.run.lock().unwrap().clone();
            Ok((run.project_id == project_id)
                .then_some(vec![run])
                .unwrap_or_default())
        }

        async fn list_by_lifecycle(
            &self,
            lifecycle_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            let run = self.run.lock().unwrap().clone();
            Ok((run.lifecycle_id == lifecycle_id)
                .then_some(vec![run])
                .unwrap_or_default())
        }

        async fn list_by_session(
            &self,
            session_id: &str,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            let run = self.run.lock().unwrap().clone();
            Ok((run.session_id.as_deref() == Some(session_id))
                .then_some(vec![run])
                .unwrap_or_default())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            *self.run.lock().unwrap() = run.clone();
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct ClaimRepo;

    #[async_trait::async_trait]
    impl ActivityExecutionClaimRepository for ClaimRepo {
        async fn create_or_get(
            &self,
            claim: &ActivityExecutionClaim,
        ) -> Result<ActivityExecutionClaim, DomainError> {
            Ok(claim.clone())
        }

        async fn get_by_idempotency_key(
            &self,
            _idempotency_key: &str,
        ) -> Result<Option<ActivityExecutionClaim>, DomainError> {
            Ok(None)
        }

        async fn list_active_by_run(
            &self,
            _run_id: Uuid,
        ) -> Result<Vec<ActivityExecutionClaim>, DomainError> {
            Ok(Vec::new())
        }

        async fn update(&self, _claim: &ActivityExecutionClaim) -> Result<(), DomainError> {
            Ok(())
        }

        async fn abandon_claiming_before(
            &self,
            _cutoff: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<ActivityExecutionClaim>, DomainError> {
            Ok(Vec::new())
        }

        async fn find_running_by_executor_session(
            &self,
            _session_id: &str,
        ) -> Result<Option<ActivityExecutionClaim>, DomainError> {
            Ok(None)
        }
    }

    fn definition(project_id: Uuid) -> ActivityLifecycleDefinition {
        ActivityLifecycleDefinition::new(
            project_id,
            "activity_flow",
            "Activity Flow",
            "",
            vec![WorkflowBindingKind::Project],
            WorkflowDefinitionSource::UserAuthored,
            "main",
            vec![ActivityDefinition {
                key: "main".to_string(),
                description: "main".to_string(),
                executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                    workflow_key: "wf_main".to_string(),
                    session_policy: AgentSessionPolicy::SpawnChild,
                }),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
                iteration_policy: Default::default(),
                join_policy: Default::default(),
            }],
            Vec::new(),
        )
        .expect("definition")
    }

    #[tokio::test]
    async fn apply_event_persists_activity_state_to_lifecycle_run() {
        let project_id = Uuid::new_v4();
        let definition = definition(project_id);
        let state =
            LifecycleEngine::initialize(&definition, uuid::Uuid::new_v4()).expect("state");
        let run = LifecycleRun::new_activity(
            project_id,
            definition.id,
            Some("sess-activity".to_string()),
            state,
        )
        .expect("run");
        let run_id = run.id;
        let definition_repo = DefinitionRepo { definition };
        let run_repo = RunRepo {
            run: Mutex::new(run),
        };
        let claim_repo = ClaimRepo;
        let service = ActivityLifecycleRunService::new(&definition_repo, &run_repo, &claim_repo);

        service
            .apply_event(
                run_id,
                ActivityEvent::SchedulerClaimAccepted {
                    activity_key: "main".to_string(),
                    attempt: 1,
                },
            )
            .await
            .expect("claim");
        service
            .apply_event(
                run_id,
                ActivityEvent::ExecutorStarted {
                    activity_key: "main".to_string(),
                    attempt: 1,
                    executor_run: ExecutorRunRef::AgentSession {
                        session_id: "child".to_string(),
                    },
                },
            )
            .await
            .expect("start");
        let updated = service
            .apply_event(
                run_id,
                ActivityEvent::ActivityCompleted {
                    activity_key: "main".to_string(),
                    attempt: 1,
                    outputs: Vec::new(),
                    summary: Some("done".to_string()),
                },
            )
            .await
            .expect("complete");

        let state = updated.activity_state.expect("state");
        assert_eq!(state.attempts[0].status, ActivityAttemptStatus::Completed);
        assert_eq!(
            updated.status,
            agentdash_domain::workflow::LifecycleRunStatus::Completed
        );
        assert!(updated.active_node_keys.is_empty());
    }
}
