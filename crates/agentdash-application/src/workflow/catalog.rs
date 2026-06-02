use chrono::Utc;

use agentdash_domain::workflow::{
    ActivityExecutorSpec, AgentProcedure, AgentProcedureRepository, ValidationIssue,
    ValidationSeverity, WorkflowGraph, WorkflowGraphRepository,
};

use super::definition::BuiltinWorkflowBundle;
use super::error::WorkflowApplicationError;

pub struct WorkflowCatalogService<'a, D: ?Sized> {
    definition_repo: &'a D,
}

pub struct ActivityLifecycleCatalogService<'a, D: ?Sized, A: ?Sized> {
    definition_repo: &'a D,
    activity_lifecycle_repo: &'a A,
}

impl<'a, D: ?Sized, A: ?Sized> ActivityLifecycleCatalogService<'a, D, A>
where
    D: AgentProcedureRepository,
    A: WorkflowGraphRepository,
{
    pub fn new(definition_repo: &'a D, activity_lifecycle_repo: &'a A) -> Self {
        Self {
            definition_repo,
            activity_lifecycle_repo,
        }
    }

    pub async fn upsert_workflow_graph(
        &self,
        lifecycle: WorkflowGraph,
    ) -> Result<WorkflowGraph, WorkflowApplicationError> {
        let issues = self.validate_workflow_graph(&lifecycle).await?;
        let errors: Vec<&ValidationIssue> = issues
            .iter()
            .filter(|item| item.severity == ValidationSeverity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "校验失败: {}",
                errors
                    .iter()
                    .map(|item| format!("[{}] {}", item.field_path, item.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }

        if let Some(existing) = self
            .activity_lifecycle_repo
            .get_by_project_and_key(lifecycle.project_id, &lifecycle.key)
            .await?
        {
            let mut updated = lifecycle;
            updated.id = existing.id;
            updated.version = existing.version + 1;
            updated.created_at = existing.created_at;
            updated.updated_at = Utc::now();
            self.activity_lifecycle_repo.update(&updated).await?;
            return Ok(updated);
        }

        self.activity_lifecycle_repo.create(&lifecycle).await?;
        Ok(lifecycle)
    }

    pub async fn validate_workflow_graph(
        &self,
        lifecycle: &WorkflowGraph,
    ) -> Result<Vec<ValidationIssue>, WorkflowApplicationError> {
        let mut issues = lifecycle.validate_full();
        issues.extend(self.validate_agent_workflow_references(lifecycle).await?);
        Ok(issues)
    }

    async fn validate_agent_workflow_references(
        &self,
        lifecycle: &WorkflowGraph,
    ) -> Result<Vec<ValidationIssue>, WorkflowApplicationError> {
        let mut issues = Vec::new();
        for (activity_index, activity) in lifecycle.activities.iter().enumerate() {
            let ActivityExecutorSpec::Agent(agent) = &activity.executor else {
                continue;
            };
            let Some(workflow) = self
                .definition_repo
                .get_by_project_and_key(lifecycle.project_id, &agent.procedure_key)
                .await?
            else {
                issues.push(ValidationIssue::error(
                    "activity_workflow_missing",
                    format!(
                        "activity `{}` 引用的 workflow `{}` 在当前 project 中不存在",
                        activity.key, agent.procedure_key
                    ),
                    format!("activities[{activity_index}].executor.procedure_key"),
                ));
                continue;
            };
        }
        Ok(issues)
    }

    pub async fn upsert_bundle(
        &self,
        bundle: BuiltinWorkflowBundle,
    ) -> Result<BuiltinWorkflowBundle, WorkflowApplicationError> {
        let mut persisted_procedures = Vec::with_capacity(bundle.procedures.len());
        for procedure in bundle.procedures {
            persisted_procedures.push(self.upsert_agent_procedure(procedure).await?);
        }

        let graph = self.upsert_workflow_graph(bundle.graph).await?;
        Ok(BuiltinWorkflowBundle {
            procedures: persisted_procedures,
            graph,
        })
    }

    async fn upsert_agent_procedure(
        &self,
        definition: AgentProcedure,
    ) -> Result<AgentProcedure, WorkflowApplicationError> {
        if let Some(existing) = self
            .definition_repo
            .get_by_project_and_key(definition.project_id, &definition.key)
            .await?
        {
            let mut updated = definition;
            updated.id = existing.id;
            updated.version = existing.version + 1;
            updated.created_at = existing.created_at;
            updated.updated_at = Utc::now();
            self.definition_repo.update(&updated).await?;
            return Ok(updated);
        }

        self.definition_repo.create(&definition).await?;
        Ok(definition)
    }
}

impl<'a, D: ?Sized> WorkflowCatalogService<'a, D>
where
    D: AgentProcedureRepository,
{
    pub fn new(definition_repo: &'a D) -> Self {
        Self { definition_repo }
    }

    pub async fn upsert_agent_procedure(
        &self,
        definition: AgentProcedure,
    ) -> Result<AgentProcedure, WorkflowApplicationError> {
        if let Some(existing) = self
            .definition_repo
            .get_by_project_and_key(definition.project_id, &definition.key)
            .await?
        {
            let mut updated = definition;
            updated.id = existing.id;
            updated.version = existing.version + 1;
            updated.created_at = existing.created_at;
            updated.updated_at = Utc::now();

            self.definition_repo.update(&updated).await?;
            return Ok(updated);
        }

        self.definition_repo.create(&definition).await?;
        Ok(definition)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Mutex;

    use uuid::Uuid;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        ActivityCompletionPolicy, ActivityDefinition, ActivityExecutorSpec, ActivityTransition,
        AgentActivityExecutorSpec, ContextStrategy, DefinitionSource, GateStrategy,
        InputPortDefinition, OutputPortDefinition, WorkflowContract, WorkflowGraphRepository,
    };

    use super::*;

    #[derive(Default)]
    struct TestAgentProcedureRepo {
        items: Mutex<BTreeMap<String, AgentProcedure>>,
    }

    impl TestAgentProcedureRepo {
        fn seed(&self, workflow: AgentProcedure) {
            self.items
                .lock()
                .expect("workflow repo lock")
                .insert(workflow.key.clone(), workflow);
        }
    }

    #[async_trait::async_trait]
    impl AgentProcedureRepository for TestAgentProcedureRepo {
        async fn create(&self, workflow: &AgentProcedure) -> Result<(), DomainError> {
            self.seed(workflow.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("workflow repo lock")
                .values()
                .find(|item| item.id == id)
                .cloned())
        }

        async fn get_by_key(&self, key: &str) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("workflow repo lock")
                .get(key)
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
                .expect("workflow repo lock")
                .values()
                .find(|item| item.project_id == project_id && item.key == key)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("workflow repo lock")
                .values()
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("workflow repo lock")
                .values()
                .filter(|item| item.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, workflow: &AgentProcedure) -> Result<(), DomainError> {
            self.seed(workflow.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items
                .lock()
                .expect("workflow repo lock")
                .retain(|_, item| item.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestWorkflowGraphRepo {
        items: Mutex<BTreeMap<String, WorkflowGraph>>,
    }

    #[async_trait::async_trait]
    impl WorkflowGraphRepository for TestWorkflowGraphRepo {
        async fn create(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            self.items
                .lock()
                .expect("activity lifecycle repo lock")
                .insert(lifecycle.key.clone(), lifecycle.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("activity lifecycle repo lock")
                .values()
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
                .expect("activity lifecycle repo lock")
                .values()
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
                .expect("activity lifecycle repo lock")
                .values()
                .filter(|item| item.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            self.items
                .lock()
                .expect("activity lifecycle repo lock")
                .insert(lifecycle.key.clone(), lifecycle.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items
                .lock()
                .expect("activity lifecycle repo lock")
                .retain(|_, item| item.id != id);
            Ok(())
        }
    }

    fn workflow_with_ports(
        key: &str,
        output_ports: &[&str],
        input_ports: &[&str],
    ) -> AgentProcedure {
        workflow_with_ports_in_project(Uuid::new_v4(), key, output_ports, input_ports)
    }

    fn workflow_with_ports_in_project(
        project_id: Uuid,
        key: &str,
        output_ports: &[&str],
        input_ports: &[&str],
    ) -> AgentProcedure {
        let contract = WorkflowContract {
            output_ports: output_ports
                .iter()
                .map(|port_key| OutputPortDefinition {
                    key: (*port_key).to_string(),
                    description: format!("output {port_key}"),
                    gate_strategy: GateStrategy::Existence,
                    gate_params: None,
                })
                .collect(),
            input_ports: input_ports
                .iter()
                .map(|port_key| InputPortDefinition {
                    key: (*port_key).to_string(),
                    description: format!("input {port_key}"),
                    context_strategy: ContextStrategy::Full,
                    context_template: None,
                    standalone_fulfillment: Default::default(),
                })
                .collect(),
            ..Default::default()
        };
        AgentProcedure::new(
            project_id,
            key,
            format!("workflow {key}"),
            "desc",
            DefinitionSource::UserAuthored,
            contract,
        )
        .expect("workflow definition")
    }

    fn activity_lifecycle_with_agent(project_id: Uuid, procedure_key: &str) -> WorkflowGraph {
        WorkflowGraph::new(
            project_id,
            "activity_lc",
            "Activity lifecycle",
            "desc",
            DefinitionSource::UserAuthored,
            "plan",
            vec![ActivityDefinition {
                key: "plan".to_string(),
                description: "plan".to_string(),
                executor: ActivityExecutorSpec::Agent(
                    AgentActivityExecutorSpec::create_activity_agent(procedure_key),
                ),
                input_ports: vec![],
                output_ports: vec![],
                completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
                iteration_policy: Default::default(),
                join_policy: Default::default(),
            }],
            Vec::<ActivityTransition>::new(),
        )
        .expect("activity lifecycle definition")
    }

    #[tokio::test]
    async fn validate_workflow_graph_resolves_agent_workflow_in_same_project_only() {
        let lifecycle_project_id = Uuid::new_v4();
        let other_project_id = Uuid::new_v4();
        let workflow_repo = TestAgentProcedureRepo::default();
        workflow_repo.seed(workflow_with_ports_in_project(
            other_project_id,
            "wf_plan",
            &[],
            &[],
        ));
        let activity_lifecycle_repo = TestWorkflowGraphRepo::default();
        let service =
            ActivityLifecycleCatalogService::new(&workflow_repo, &activity_lifecycle_repo);

        let lifecycle = activity_lifecycle_with_agent(lifecycle_project_id, "wf_plan");

        let issues = service
            .validate_workflow_graph(&lifecycle)
            .await
            .expect("validate activity lifecycle");

        assert!(issues.iter().any(|issue| {
            issue.code == "activity_workflow_missing"
                && issue.message.contains("在当前 project 中不存在")
        }));
    }

    #[tokio::test]
    async fn upsert_activity_lifecycle_preserves_id_and_bumps_version() {
        let project_id = Uuid::new_v4();
        let workflow_repo = TestAgentProcedureRepo::default();
        workflow_repo.seed(workflow_with_ports_in_project(
            project_id,
            "wf_plan",
            &[],
            &[],
        ));
        let activity_lifecycle_repo = TestWorkflowGraphRepo::default();
        let service =
            ActivityLifecycleCatalogService::new(&workflow_repo, &activity_lifecycle_repo);

        let first = service
            .upsert_workflow_graph(activity_lifecycle_with_agent(project_id, "wf_plan"))
            .await
            .expect("create activity lifecycle");
        let second = service
            .upsert_workflow_graph(activity_lifecycle_with_agent(project_id, "wf_plan"))
            .await
            .expect("update activity lifecycle");

        assert_eq!(second.id, first.id);
        assert_eq!(second.version, first.version + 1);
    }

    #[tokio::test]
    async fn upsert_bundle_accepts_builtin_workflow_admin() {
        use crate::workflow::definition::{
            BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY, build_builtin_workflow_bundle,
        };

        let project_id = Uuid::new_v4();
        let bundle = build_builtin_workflow_bundle(project_id, BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY)
            .expect("build builtin_workflow_admin bundle");

        let workflow_repo = TestAgentProcedureRepo::default();
        let lifecycle_repo = TestWorkflowGraphRepo::default();
        let service = ActivityLifecycleCatalogService::new(&workflow_repo, &lifecycle_repo);

        let saved = service
            .upsert_bundle(bundle)
            .await
            .expect("bootstrap 内建工作流应通过所有校验");

        assert_eq!(saved.procedures.len(), 2);
        assert_eq!(saved.graph.key, BUILTIN_WORKFLOW_ADMIN_TEMPLATE_KEY);
        assert_eq!(saved.graph.activities.len(), 2);
        for workflow in &saved.procedures {
            assert!(
                workflow.contract.injection.guidance.is_some(),
                "内建 workflow `{}` 必须保留 guidance 注入",
                workflow.key
            );
            assert!(
                workflow
                    .contract
                    .capability_config
                    .tool_directives
                    .iter()
                    .any(|directive| directive.is_add() && directive.key() == "workflow_management"),
                "内建 workflow `{}` 必须保留 workflow_management 能力",
                workflow.key
            );
        }
    }

    #[tokio::test]
    async fn upsert_bundle_accepts_trellis_dag_task() {
        use crate::workflow::definition::{
            TRELLIS_DAG_TASK_TEMPLATE_KEY, build_builtin_workflow_bundle,
        };

        let project_id = Uuid::new_v4();
        let bundle = build_builtin_workflow_bundle(project_id, TRELLIS_DAG_TASK_TEMPLATE_KEY)
            .expect("build trellis_dag_task bundle");

        let workflow_repo = TestAgentProcedureRepo::default();
        let lifecycle_repo = TestWorkflowGraphRepo::default();
        let service = ActivityLifecycleCatalogService::new(&workflow_repo, &lifecycle_repo);

        let saved = service
            .upsert_bundle(bundle)
            .await
            .expect("bootstrap Trellis DAG Task 应通过 Activity port 校验");

        assert_eq!(saved.procedures.len(), 2);
        assert_eq!(saved.graph.key, TRELLIS_DAG_TASK_TEMPLATE_KEY);
        assert_eq!(saved.graph.activities.len(), 2);
    }
}
