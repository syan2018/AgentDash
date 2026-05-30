use uuid::Uuid;

use agentdash_domain::workflow::{
    ActivityCompletionPolicy, ActivityDefinition, ActivityExecutorSpec, ActivityIterationPolicy,
    ActivityJoinPolicy, ActivityLifecycleDefinition, ActivityLifecycleDefinitionRepository,
    AgentActivityExecutorSpec, AgentSessionPolicy, ArtifactAliasPolicy, LifecycleRun,
    LifecycleRunRepository, WorkflowBindingKind, WorkflowContract, WorkflowDefinition,
    WorkflowDefinitionRepository, WorkflowDefinitionSource,
};

use super::{LifecycleEngine, WorkflowApplicationError};

pub const FREEFORM_LIFECYCLE_KEY: &str = "builtin.freeform_session";
pub const FREEFORM_AGENT_WORKFLOW_KEY: &str = "builtin.freeform_agent";
pub const FREEFORM_ACTIVITY_KEY: &str = "main_conversation";
pub const FREEFORM_SESSION_LABEL: &str = "freeform";

pub struct FreeformLifecycleService<'a, W: ?Sized, A: ?Sized, R: ?Sized> {
    workflow_repo: &'a W,
    activity_lifecycle_repo: &'a A,
    run_repo: &'a R,
}

impl<'a, W: ?Sized, A: ?Sized, R: ?Sized> FreeformLifecycleService<'a, W, A, R>
where
    W: WorkflowDefinitionRepository,
    A: ActivityLifecycleDefinitionRepository,
    R: LifecycleRunRepository,
{
    pub fn new(workflow_repo: &'a W, activity_lifecycle_repo: &'a A, run_repo: &'a R) -> Self {
        Self {
            workflow_repo,
            activity_lifecycle_repo,
            run_repo,
        }
    }

    pub async fn ensure_definition(
        &self,
        project_id: Uuid,
    ) -> Result<ActivityLifecycleDefinition, WorkflowApplicationError> {
        if self
            .workflow_repo
            .get_by_project_and_key(project_id, FREEFORM_AGENT_WORKFLOW_KEY)
            .await?
            .is_none()
        {
            let workflow = build_freeform_workflow(project_id)?;
            self.workflow_repo.create(&workflow).await?;
        }

        if let Some(definition) = self
            .activity_lifecycle_repo
            .get_by_project_and_key(project_id, FREEFORM_LIFECYCLE_KEY)
            .await?
        {
            return Ok(definition);
        }

        let definition = build_freeform_lifecycle(project_id)?;
        self.activity_lifecycle_repo.create(&definition).await?;
        Ok(definition)
    }

    pub async fn ensure_run_for_session(
        &self,
        project_id: Uuid,
        session_id: &str,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        if let Some(existing) = self
            .run_repo
            .list_by_session(session_id)
            .await?
            .into_iter()
            .find(|run| run.activity_state.is_some())
        {
            return Ok(existing);
        }

        let definition = self.ensure_definition(project_id).await?;
        let state = LifecycleEngine::initialize(&definition)
            .map_err(|error| WorkflowApplicationError::BadRequest(error.to_string()))?;
        let run = LifecycleRun::new_activity(
            project_id,
            definition.id,
            Some(session_id.to_string()),
            state,
        )
        .map_err(WorkflowApplicationError::BadRequest)?;
        self.run_repo.create(&run).await?;
        Ok(run)
    }
}

pub fn build_freeform_workflow(
    project_id: Uuid,
) -> Result<WorkflowDefinition, WorkflowApplicationError> {
    WorkflowDefinition::new(
        project_id,
        FREEFORM_AGENT_WORKFLOW_KEY,
        "Freeform Agent",
        "普通自由会话的默认 Agent contract。",
        vec![WorkflowBindingKind::Project, WorkflowBindingKind::Story],
        WorkflowDefinitionSource::BuiltinSeed,
        WorkflowContract::default(),
    )
    .map_err(WorkflowApplicationError::BadRequest)
}

pub fn build_freeform_lifecycle(
    project_id: Uuid,
) -> Result<ActivityLifecycleDefinition, WorkflowApplicationError> {
    ActivityLifecycleDefinition::new(
        project_id,
        FREEFORM_LIFECYCLE_KEY,
        "Freeform Session",
        "普通自由会话的无外围约束过程。",
        vec![WorkflowBindingKind::Project, WorkflowBindingKind::Story],
        WorkflowDefinitionSource::BuiltinSeed,
        FREEFORM_ACTIVITY_KEY,
        vec![ActivityDefinition {
            key: FREEFORM_ACTIVITY_KEY.to_string(),
            description: "普通自由会话主对话。".to_string(),
            executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                workflow_key: FREEFORM_AGENT_WORKFLOW_KEY.to_string(),
                session_policy: AgentSessionPolicy::ContinueRoot,
            }),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            completion_policy: ActivityCompletionPolicy::OpenEnded,
            iteration_policy: ActivityIterationPolicy {
                max_attempts: None,
                artifact_alias: ArtifactAliasPolicy::LatestAndHistory,
            },
            join_policy: ActivityJoinPolicy::All,
        }],
        Vec::new(),
    )
    .map_err(WorkflowApplicationError::BadRequest)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::DomainError;

    use super::*;

    #[derive(Default)]
    struct InMemoryWorkflowRepo {
        items: Mutex<Vec<WorkflowDefinition>>,
    }

    #[async_trait::async_trait]
    impl WorkflowDefinitionRepository for InMemoryWorkflowRepo {
        async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(workflow.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.id == id)
                .cloned())
        }

        async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.key == key)
                .cloned())
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.project_id == project_id && item.key == key)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self.items.lock().unwrap().clone())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_by_binding_kind(
            &self,
            binding_kind: WorkflowBindingKind,
        ) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.binding_kinds.contains(&binding_kind))
                .cloned()
                .collect())
        }

        async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|item| item.id == workflow.id) {
                *existing = workflow.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|item| item.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryActivityLifecycleRepo {
        items: Mutex<Vec<ActivityLifecycleDefinition>>,
    }

    #[async_trait::async_trait]
    impl ActivityLifecycleDefinitionRepository for InMemoryActivityLifecycleRepo {
        async fn create(&self, lifecycle: &ActivityLifecycleDefinition) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(lifecycle.clone());
            Ok(())
        }

        async fn get_by_id(
            &self,
            id: Uuid,
        ) -> Result<Option<ActivityLifecycleDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.id == id)
                .cloned())
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<ActivityLifecycleDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.project_id == project_id && item.key == key)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ActivityLifecycleDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, lifecycle: &ActivityLifecycleDefinition) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|item| item.id == lifecycle.id) {
                *existing = lifecycle.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|item| item.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryRunRepo {
        items: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait::async_trait]
    impl LifecycleRunRepository for InMemoryRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self.items.lock().unwrap().iter().filter(|r| ids.contains(&r.id)).cloned().collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_by_lifecycle(
            &self,
            lifecycle_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.lifecycle_id == lifecycle_id)
                .cloned()
                .collect())
        }

        async fn list_by_session(
            &self,
            session_id: &str,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.session_id.as_deref() == Some(session_id))
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|item| item.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|item| item.id != id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn ensure_run_for_session_creates_open_ended_freeform_attempt() {
        let workflow_repo = InMemoryWorkflowRepo::default();
        let lifecycle_repo = InMemoryActivityLifecycleRepo::default();
        let run_repo = InMemoryRunRepo::default();
        let service = FreeformLifecycleService::new(&workflow_repo, &lifecycle_repo, &run_repo);
        let project_id = Uuid::new_v4();

        let run = service
            .ensure_run_for_session(project_id, "sess-freeform")
            .await
            .expect("freeform run");

        assert_eq!(run.project_id, project_id);
        assert_eq!(run.session_id.as_deref(), Some("sess-freeform"));
        let state = run.activity_state.expect("activity state");
        assert_eq!(
            state.status,
            agentdash_domain::workflow::ActivityRunStatus::Ready
        );
        assert_eq!(state.attempts.len(), 1);
        assert_eq!(state.attempts[0].activity_key, FREEFORM_ACTIVITY_KEY);
        assert_eq!(
            workflow_repo.items.lock().unwrap()[0].key,
            FREEFORM_AGENT_WORKFLOW_KEY
        );
        let lifecycle = &lifecycle_repo.items.lock().unwrap()[0];
        assert_eq!(lifecycle.key, FREEFORM_LIFECYCLE_KEY);
        assert!(matches!(
            lifecycle.activities[0].completion_policy,
            ActivityCompletionPolicy::OpenEnded
        ));
    }

    #[tokio::test]
    async fn ensure_run_for_session_reuses_existing_activity_run() {
        let workflow_repo = InMemoryWorkflowRepo::default();
        let lifecycle_repo = InMemoryActivityLifecycleRepo::default();
        let run_repo = InMemoryRunRepo::default();
        let service = FreeformLifecycleService::new(&workflow_repo, &lifecycle_repo, &run_repo);
        let project_id = Uuid::new_v4();

        let first = service
            .ensure_run_for_session(project_id, "sess-freeform")
            .await
            .expect("first run");
        let second = service
            .ensure_run_for_session(project_id, "sess-freeform")
            .await
            .expect("second run");

        assert_eq!(first.id, second.id);
        assert_eq!(run_repo.items.lock().unwrap().len(), 1);
    }
}
