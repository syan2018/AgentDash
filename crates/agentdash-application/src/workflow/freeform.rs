use uuid::Uuid;

use agentdash_domain::workflow::{
    ActivityCompletionPolicy, ActivityDefinition, ActivityExecutorSpec, ActivityIterationPolicy,
    ActivityJoinPolicy, AgentActivityExecutorSpec, AgentProcedure, AgentProcedureRepository,
    AgentSessionPolicy, ArtifactAliasPolicy, DefinitionSource, WorkflowContract, WorkflowGraph,
    WorkflowGraphRepository,
};

use super::WorkflowApplicationError;

pub const FREEFORM_LIFECYCLE_KEY: &str = "builtin.freeform_session";
pub const FREEFORM_AGENT_PROCEDURE_KEY: &str = "builtin.freeform_agent";
pub const FREEFORM_ACTIVITY_KEY: &str = "main_conversation";
pub const FREEFORM_SESSION_LABEL: &str = "freeform";

pub struct FreeformLifecycleService<'a, W: ?Sized, A: ?Sized> {
    workflow_repo: &'a W,
    activity_lifecycle_repo: &'a A,
}

impl<'a, W: ?Sized, A: ?Sized> FreeformLifecycleService<'a, W, A>
where
    W: AgentProcedureRepository,
    A: WorkflowGraphRepository,
{
    pub fn new(workflow_repo: &'a W, activity_lifecycle_repo: &'a A) -> Self {
        Self {
            workflow_repo,
            activity_lifecycle_repo,
        }
    }

    pub async fn ensure_definition(
        &self,
        project_id: Uuid,
    ) -> Result<WorkflowGraph, WorkflowApplicationError> {
        if self
            .workflow_repo
            .get_by_project_and_key(project_id, FREEFORM_AGENT_PROCEDURE_KEY)
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
}

pub fn build_freeform_workflow(
    project_id: Uuid,
) -> Result<AgentProcedure, WorkflowApplicationError> {
    AgentProcedure::new(
        project_id,
        FREEFORM_AGENT_PROCEDURE_KEY,
        "Freeform Agent",
        "普通自由会话的默认 Agent contract。",
        DefinitionSource::BuiltinSeed,
        WorkflowContract::default(),
    )
    .map_err(WorkflowApplicationError::BadRequest)
}

pub fn build_freeform_lifecycle(
    project_id: Uuid,
) -> Result<WorkflowGraph, WorkflowApplicationError> {
    WorkflowGraph::new(
        project_id,
        FREEFORM_LIFECYCLE_KEY,
        "Freeform Session",
        "普通自由会话的无外围约束过程。",
        DefinitionSource::BuiltinSeed,
        FREEFORM_ACTIVITY_KEY,
        vec![ActivityDefinition {
            key: FREEFORM_ACTIVITY_KEY.to_string(),
            description: "普通自由会话主对话。".to_string(),
            executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                procedure_key: FREEFORM_AGENT_PROCEDURE_KEY.to_string(),
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
        items: Mutex<Vec<AgentProcedure>>,
    }

    #[async_trait::async_trait]
    impl AgentProcedureRepository for InMemoryWorkflowRepo {
        async fn create(&self, workflow: &AgentProcedure) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(workflow.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.id == id)
                .cloned())
        }

        async fn get_by_key(&self, key: &str) -> Result<Option<AgentProcedure>, DomainError> {
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
        ) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.project_id == project_id && item.key == key)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self.items.lock().unwrap().clone())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, workflow: &AgentProcedure) -> Result<(), DomainError> {
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
        items: Mutex<Vec<WorkflowGraph>>,
    }

    #[async_trait::async_trait]
    impl WorkflowGraphRepository for InMemoryActivityLifecycleRepo {
        async fn create(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(lifecycle.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
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
        ) -> Result<Option<WorkflowGraph>, DomainError> {
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
        ) -> Result<Vec<WorkflowGraph>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
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

    #[tokio::test]
    async fn ensure_definition_creates_open_ended_freeform_graph() {
        let workflow_repo = InMemoryWorkflowRepo::default();
        let lifecycle_repo = InMemoryActivityLifecycleRepo::default();
        let service = FreeformLifecycleService::new(&workflow_repo, &lifecycle_repo);
        let project_id = Uuid::new_v4();

        let lifecycle = service
            .ensure_definition(project_id)
            .await
            .expect("freeform definition");

        assert_eq!(
            workflow_repo.items.lock().unwrap()[0].key,
            FREEFORM_AGENT_PROCEDURE_KEY
        );
        assert_eq!(lifecycle.key, FREEFORM_LIFECYCLE_KEY);
        assert!(matches!(
            lifecycle.activities[0].completion_policy,
            ActivityCompletionPolicy::OpenEnded
        ));
    }
}
