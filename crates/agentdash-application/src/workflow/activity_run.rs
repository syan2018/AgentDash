use uuid::Uuid;

use agentdash_domain::workflow::{
    ActivityExecutionClaimRepository, LifecycleRun, LifecycleRunRepository, WorkflowGraph,
    WorkflowGraphInstance, WorkflowGraphInstanceRepository, WorkflowGraphRepository,
};

use super::scheduler::{ActivityExecutorLaunchOutcome, ActivityExecutorLauncher};
use super::{
    ActivityEvent, ActivityExecutorScheduler, ActivityLifecycleRunState, LifecycleEngine,
    WorkflowApplicationError,
};

pub struct ActivityLifecycleRunService<'a, D: ?Sized, R: ?Sized, G: ?Sized, C: ?Sized> {
    definition_repo: &'a D,
    run_repo: &'a R,
    graph_instance_repo: &'a G,
    claim_repo: &'a C,
}

#[derive(Debug, Clone)]
pub struct ActivityGraphInstanceExecutionResult {
    pub run: LifecycleRun,
    pub graph_instance: WorkflowGraphInstance,
}

impl<'a, D: ?Sized, R: ?Sized, G: ?Sized, C: ?Sized> ActivityLifecycleRunService<'a, D, R, G, C>
where
    D: WorkflowGraphRepository,
    R: LifecycleRunRepository,
    G: WorkflowGraphInstanceRepository,
    C: ActivityExecutionClaimRepository,
{
    pub fn new(
        definition_repo: &'a D,
        run_repo: &'a R,
        graph_instance_repo: &'a G,
        claim_repo: &'a C,
    ) -> Self {
        Self {
            definition_repo,
            run_repo,
            graph_instance_repo,
            claim_repo,
        }
    }

    pub async fn apply_event(
        &self,
        graph_instance_id: Uuid,
        event: ActivityEvent,
    ) -> Result<ActivityGraphInstanceExecutionResult, WorkflowApplicationError> {
        let (definition, mut run, mut graph_instance, mut state) =
            self.load_context(graph_instance_id).await?;
        LifecycleEngine::apply_event(&definition, &mut state, event)
            .map_err(|error| WorkflowApplicationError::Conflict(error.to_string()))?;
        graph_instance
            .replace_activity_state(state)
            .map_err(WorkflowApplicationError::BadRequest)?;
        self.sync_run_projection(&mut run, &graph_instance).await?;
        self.graph_instance_repo.update(&graph_instance).await?;
        self.run_repo.update(&run).await?;
        Ok(ActivityGraphInstanceExecutionResult {
            run,
            graph_instance,
        })
    }

    pub async fn launch_ready_attempts<L>(
        &self,
        graph_instance_id: Uuid,
        launcher: &L,
    ) -> Result<
        (
            ActivityGraphInstanceExecutionResult,
            Vec<ActivityExecutorLaunchOutcome>,
        ),
        WorkflowApplicationError,
    >
    where
        L: ActivityExecutorLauncher,
    {
        let (definition, mut run, mut graph_instance, mut state) =
            self.load_context(graph_instance_id).await?;
        let scheduler = ActivityExecutorScheduler::new(self.claim_repo);
        let outcomes = scheduler
            .launch_ready_attempts(run.id, &definition, &mut state, launcher)
            .await?;
        graph_instance
            .replace_activity_state(state)
            .map_err(WorkflowApplicationError::BadRequest)?;
        self.sync_run_projection(&mut run, &graph_instance).await?;
        self.graph_instance_repo.update(&graph_instance).await?;
        self.run_repo.update(&run).await?;
        Ok((
            ActivityGraphInstanceExecutionResult {
                run,
                graph_instance,
            },
            outcomes,
        ))
    }

    async fn load_context(
        &self,
        graph_instance_id: Uuid,
    ) -> Result<
        (
            WorkflowGraph,
            LifecycleRun,
            WorkflowGraphInstance,
            ActivityLifecycleRunState,
        ),
        WorkflowApplicationError,
    > {
        let graph_instance = self
            .graph_instance_repo
            .get(graph_instance_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "workflow_graph_instance 不存在: {graph_instance_id}"
                ))
            })?;
        let run = self
            .run_repo
            .get_by_id(graph_instance.run_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_run 不存在: {}",
                    graph_instance.run_id
                ))
            })?;
        let graph_instance = self
            .graph_instance_repo
            .get_by_run_and_id(run.id, graph_instance.id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "workflow_graph_instance 不存在: {}",
                    graph_instance.id
                ))
            })?;
        let state = graph_instance.activity_state.clone().ok_or_else(|| {
            WorkflowApplicationError::BadRequest(format!(
                "workflow_graph_instance {} 缺少 Activity lifecycle state",
                graph_instance.id
            ))
        })?;
        if state.graph_instance_id != graph_instance.id {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "workflow_graph_instance {} 的 activity_state 归属不一致",
                graph_instance.id
            )));
        }
        let definition = self
            .definition_repo
            .get_by_id(graph_instance.graph_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "workflow_graph 不存在: {}",
                    graph_instance.graph_id
                ))
            })?;
        if definition.project_id != run.project_id {
            return Err(WorkflowApplicationError::NotFound(format!(
                "workflow_graph 不存在: {}",
                graph_instance.graph_id
            )));
        }
        Ok((definition, run, graph_instance, state))
    }

    async fn sync_run_projection(
        &self,
        run: &mut LifecycleRun,
        updated_graph_instance: &WorkflowGraphInstance,
    ) -> Result<(), WorkflowApplicationError> {
        let mut graph_instances = self.graph_instance_repo.list_by_run(run.id).await?;
        if let Some(existing) = graph_instances
            .iter_mut()
            .find(|instance| instance.id == updated_graph_instance.id)
        {
            *existing = updated_graph_instance.clone();
        } else {
            graph_instances.push(updated_graph_instance.clone());
        }
        run.sync_graph_instance_activity_projections(graph_instances.iter().filter_map(
            |instance| {
                instance
                    .activity_state
                    .as_ref()
                    .map(|state| (instance.id, state))
            },
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        ActivityAttemptStatus, ActivityCompletionPolicy, ActivityDefinition,
        ActivityExecutionClaim, ActivityExecutorSpec, AgentActivityExecutorSpec, DefinitionSource,
        ExecutorRunRef,
    };

    use super::*;

    struct DefinitionRepo {
        definition: WorkflowGraph,
    }

    #[async_trait::async_trait]
    impl WorkflowGraphRepository for DefinitionRepo {
        async fn create(&self, _lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok((self.definition.id == id).then(|| self.definition.clone()))
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(
                (self.definition.project_id == project_id && self.definition.key == key)
                    .then(|| self.definition.clone()),
            )
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<WorkflowGraph>, DomainError> {
            Ok((self.definition.project_id == project_id)
                .then(|| vec![self.definition.clone()])
                .unwrap_or_default())
        }

        async fn update(&self, _lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait::async_trait]
    impl LifecycleRunRepository for RunRepo {
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
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
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
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_by_root_graph(
            &self,
            root_graph_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.root_graph_id == root_graph_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut runs = self.runs.lock().unwrap();
            if let Some(existing) = runs.iter_mut().find(|existing| existing.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct GraphInstanceRepo {
        items: Mutex<Vec<WorkflowGraphInstance>>,
    }

    #[async_trait::async_trait]
    impl WorkflowGraphInstanceRepository for GraphInstanceRepo {
        async fn create(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(instance.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|instance| instance.id == id)
                .cloned())
        }

        async fn get_by_run_and_id(
            &self,
            run_id: Uuid,
            id: Uuid,
        ) -> Result<Option<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|instance| instance.run_id == run_id && instance.id == id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<WorkflowGraphInstance>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|instance| instance.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|existing| existing.id == instance.id) {
                *existing = instance.clone();
            }
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
    }

    fn definition(project_id: Uuid) -> WorkflowGraph {
        WorkflowGraph::new(
            project_id,
            "activity_flow",
            "Activity Flow",
            "",
            DefinitionSource::UserAuthored,
            "main",
            vec![ActivityDefinition {
                key: "main".to_string(),
                description: "main".to_string(),
                executor: ActivityExecutorSpec::Agent(
                    AgentActivityExecutorSpec::create_activity_agent("wf_main"),
                ),
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

    fn active_service<'a>(
        definition_repo: &'a DefinitionRepo,
        run_repo: &'a RunRepo,
        graph_instance_repo: &'a GraphInstanceRepo,
        claim_repo: &'a ClaimRepo,
    ) -> ActivityLifecycleRunService<'a, DefinitionRepo, RunRepo, GraphInstanceRepo, ClaimRepo>
    {
        ActivityLifecycleRunService::new(definition_repo, run_repo, graph_instance_repo, claim_repo)
    }

    async fn persist_initialized_run(
        definition: &WorkflowGraph,
        run_repo: &RunRepo,
        graph_instance_repo: &GraphInstanceRepo,
    ) -> ActivityGraphInstanceExecutionResult {
        let mut run = LifecycleRun::new_control(definition.project_id, definition.id);
        run_repo.create(&run).await.expect("create run");

        let mut graph_instance = WorkflowGraphInstance::new_root(run.id, definition.id);
        let state = LifecycleEngine::initialize(definition, graph_instance.id).expect("init state");
        graph_instance
            .replace_activity_state(state)
            .expect("replace state");
        graph_instance_repo
            .create(&graph_instance)
            .await
            .expect("create graph instance");
        run.sync_graph_instance_activity_projections(
            graph_instance
                .activity_state
                .as_ref()
                .map(|state| (graph_instance.id, state))
                .into_iter(),
        );
        run_repo.update(&run).await.expect("update run");

        ActivityGraphInstanceExecutionResult {
            run,
            graph_instance,
        }
    }

    #[tokio::test]
    async fn apply_event_persists_activity_state_to_graph_instance() {
        let project_id = Uuid::new_v4();
        let definition = definition(project_id);
        let definition_repo = DefinitionRepo {
            definition: definition.clone(),
        };
        let run_repo = RunRepo::default();
        let graph_instance_repo = GraphInstanceRepo::default();
        let claim_repo = ClaimRepo;
        let service = active_service(
            &definition_repo,
            &run_repo,
            &graph_instance_repo,
            &claim_repo,
        );
        let started = persist_initialized_run(&definition, &run_repo, &graph_instance_repo).await;

        service
            .apply_event(
                started.graph_instance.id,
                ActivityEvent::SchedulerClaimAccepted {
                    activity_key: "main".to_string(),
                    attempt: 1,
                },
            )
            .await
            .expect("claim");
        service
            .apply_event(
                started.graph_instance.id,
                ActivityEvent::ExecutorStarted {
                    activity_key: "main".to_string(),
                    attempt: 1,
                    executor_run: ExecutorRunRef::RuntimeSession {
                        session_id: "child".to_string(),
                    },
                },
            )
            .await
            .expect("start");
        let updated = service
            .apply_event(
                started.graph_instance.id,
                ActivityEvent::ActivityCompleted {
                    activity_key: "main".to_string(),
                    attempt: 1,
                    outputs: Vec::new(),
                    summary: Some("done".to_string()),
                },
            )
            .await
            .expect("complete");

        let persisted_run = run_repo
            .get_by_id(updated.run.id)
            .await
            .expect("run query")
            .expect("run");
        let persisted_instance = graph_instance_repo
            .get(updated.graph_instance.id)
            .await
            .expect("graph instance query")
            .expect("graph instance");
        let state = persisted_instance.activity_state.expect("state");
        assert_eq!(state.attempts[0].status, ActivityAttemptStatus::Completed);
        assert_eq!(
            persisted_run.status,
            agentdash_domain::workflow::LifecycleRunStatus::Completed
        );
        assert!(persisted_run.active_node_keys.is_empty());
    }

    #[tokio::test]
    async fn same_run_graph_instances_with_same_activity_key_keep_separate_state() {
        let project_id = Uuid::new_v4();
        let definition = definition(project_id);
        let definition_repo = DefinitionRepo {
            definition: definition.clone(),
        };
        let run_repo = RunRepo::default();
        let graph_instance_repo = GraphInstanceRepo::default();
        let claim_repo = ClaimRepo;
        let run = LifecycleRun::new_control(project_id, definition.id);
        run_repo.create(&run).await.expect("run create");
        let mut first = WorkflowGraphInstance::new_root(run.id, definition.id);
        let first_state = LifecycleEngine::initialize(&definition, first.id).expect("state");
        first
            .replace_activity_state(first_state)
            .expect("first state");
        graph_instance_repo
            .create(&first)
            .await
            .expect("first instance");
        let mut second = WorkflowGraphInstance::new(run.id, definition.id, "task_execution");
        let second_state = LifecycleEngine::initialize(&definition, second.id).expect("state");
        second
            .replace_activity_state(second_state)
            .expect("second state");
        graph_instance_repo
            .create(&second)
            .await
            .expect("second instance");
        let service = active_service(
            &definition_repo,
            &run_repo,
            &graph_instance_repo,
            &claim_repo,
        );

        service
            .apply_event(
                first.id,
                ActivityEvent::SchedulerClaimAccepted {
                    activity_key: "main".to_string(),
                    attempt: 1,
                },
            )
            .await
            .expect("claim first");
        service
            .apply_event(
                first.id,
                ActivityEvent::ExecutorStarted {
                    activity_key: "main".to_string(),
                    attempt: 1,
                    executor_run: ExecutorRunRef::RuntimeSession {
                        session_id: "first".to_string(),
                    },
                },
            )
            .await
            .expect("start first");
        service
            .apply_event(
                first.id,
                ActivityEvent::ActivityCompleted {
                    activity_key: "main".to_string(),
                    attempt: 1,
                    outputs: Vec::new(),
                    summary: Some("done".to_string()),
                },
            )
            .await
            .expect("complete first");

        let first = graph_instance_repo
            .get(first.id)
            .await
            .expect("first query")
            .expect("first");
        let second = graph_instance_repo
            .get(second.id)
            .await
            .expect("second query")
            .expect("second");
        assert_eq!(
            first.activity_state.as_ref().expect("first state").attempts[0].status,
            ActivityAttemptStatus::Completed
        );
        assert_eq!(
            second
                .activity_state
                .as_ref()
                .expect("second state")
                .attempts[0]
                .status,
            ActivityAttemptStatus::Ready
        );
        let run = run_repo
            .get_by_id(run.id)
            .await
            .expect("run query")
            .expect("run");
        assert_eq!(run.active_node_keys, vec![format!("{}:main", second.id)]);
    }
}
