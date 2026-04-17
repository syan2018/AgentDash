use std::collections::BTreeMap;

use chrono::Utc;
use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, ValidationIssue, ValidationSeverity,
    WorkflowAssignment, WorkflowAssignmentRepository, WorkflowBindingRole, WorkflowDefinition,
    WorkflowDefinitionRepository,
};

use super::definition::BuiltinWorkflowBundle;
use super::error::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct AssignLifecycleCommand {
    pub project_id: Uuid,
    pub lifecycle_id: Uuid,
    pub role: WorkflowBindingRole,
    pub enabled: bool,
    pub is_default: bool,
}

pub struct WorkflowCatalogService<'a, D: ?Sized, L: ?Sized, A: ?Sized> {
    definition_repo: &'a D,
    lifecycle_repo: &'a L,
    assignment_repo: &'a A,
}

impl<'a, D: ?Sized, L: ?Sized, A: ?Sized> WorkflowCatalogService<'a, D, L, A>
where
    D: WorkflowDefinitionRepository,
    L: LifecycleDefinitionRepository,
    A: WorkflowAssignmentRepository,
{
    pub fn new(definition_repo: &'a D, lifecycle_repo: &'a L, assignment_repo: &'a A) -> Self {
        Self {
            definition_repo,
            lifecycle_repo,
            assignment_repo,
        }
    }

    pub async fn upsert_workflow_definition(
        &self,
        definition: WorkflowDefinition,
    ) -> Result<WorkflowDefinition, WorkflowApplicationError> {
        if let Some(existing) = self.definition_repo.get_by_key(&definition.key).await? {
            if existing.binding_kind != definition.binding_kind {
                return Err(WorkflowApplicationError::Conflict(format!(
                    "workflow `{}` 已绑定 binding_kind={:?}，不能直接改为 {:?}",
                    definition.key, existing.binding_kind, definition.binding_kind
                )));
            }

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

    pub async fn upsert_lifecycle_definition(
        &self,
        lifecycle: LifecycleDefinition,
    ) -> Result<LifecycleDefinition, WorkflowApplicationError> {
        let issues = self.validate_lifecycle_definition(&lifecycle).await?;
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

        if let Some(existing) = self.lifecycle_repo.get_by_key(&lifecycle.key).await? {
            if existing.binding_kind != lifecycle.binding_kind {
                return Err(WorkflowApplicationError::Conflict(format!(
                    "lifecycle `{}` 已绑定 binding_kind={:?}，不能直接改为 {:?}",
                    lifecycle.key, existing.binding_kind, lifecycle.binding_kind
                )));
            }

            let mut updated = lifecycle;
            updated.id = existing.id;
            updated.version = existing.version + 1;
            updated.created_at = existing.created_at;
            updated.updated_at = Utc::now();

            self.lifecycle_repo.update(&updated).await?;
            return Ok(updated);
        }

        self.lifecycle_repo.create(&lifecycle).await?;
        Ok(lifecycle)
    }

    pub async fn validate_lifecycle_definition(
        &self,
        lifecycle: &LifecycleDefinition,
    ) -> Result<Vec<ValidationIssue>, WorkflowApplicationError> {
        let mut issues = lifecycle.validate_full();
        issues.extend(self.validate_lifecycle_graph_contracts(lifecycle).await?);
        Ok(issues)
    }

    async fn validate_lifecycle_graph_contracts(
        &self,
        lifecycle: &LifecycleDefinition,
    ) -> Result<Vec<ValidationIssue>, WorkflowApplicationError> {
        let mut issues = Vec::new();
        let mut workflows_by_step: BTreeMap<String, WorkflowDefinition> = BTreeMap::new();
        let step_index_by_key: BTreeMap<&str, usize> = lifecycle
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| (step.key.as_str(), index))
            .collect();

        for (step_index, step) in lifecycle.steps.iter().enumerate() {
            let Some(workflow_key) = step
                .workflow_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            let Some(workflow) = self.definition_repo.get_by_key(workflow_key).await? else {
                issues.push(ValidationIssue::error(
                    "lifecycle_step_workflow_missing",
                    format!(
                        "lifecycle step `{}` 引用的 workflow `{}` 不存在",
                        step.key, workflow_key
                    ),
                    format!("steps[{step_index}].workflow_key"),
                ));
                continue;
            };

            if workflow.binding_kind != lifecycle.binding_kind {
                issues.push(ValidationIssue::error(
                    "lifecycle_step_workflow_binding_kind_mismatch",
                    format!(
                        "lifecycle step `{}` 引用的 workflow `{}` binding_kind={:?}，与 lifecycle {:?} 不一致",
                        step.key, workflow.key, workflow.binding_kind, lifecycle.binding_kind
                    ),
                    format!("steps[{step_index}].workflow_key"),
                ));
            }

            workflows_by_step.insert(step.key.clone(), workflow);
        }

        // port 归属已迁移到 step 级别，直接从 step.output_ports 读取
        let mut output_owner_by_port: BTreeMap<String, String> = BTreeMap::new();
        for (step_index, step) in lifecycle.steps.iter().enumerate() {
            for output_port in &step.output_ports {
                if let Some(existing_owner) =
                    output_owner_by_port.insert(output_port.key.clone(), step.key.clone())
                {
                    issues.push(ValidationIssue::error(
                        "lifecycle_output_port_key_not_unique",
                        format!(
                            "output port `{}` 在 lifecycle 内必须全局唯一，当前同时出现在 `{}` 和 `{}`",
                            output_port.key, existing_owner, step.key
                        ),
                        format!("steps[{step_index}].output_ports"),
                    ));
                }
            }
        }

        let mut incoming_edge_by_input: BTreeMap<(String, String), usize> = BTreeMap::new();
        for (edge_index, edge) in lifecycle.edges.iter().enumerate() {
            let input_key = (edge.to_node.clone(), edge.to_port.clone());
            if let Some(previous_edge_index) =
                incoming_edge_by_input.insert(input_key.clone(), edge_index)
            {
                issues.push(ValidationIssue::error(
                    "lifecycle_input_port_multiple_sources",
                    format!(
                        "input port `{}.{}` 只能接收一条 edge，当前与 lifecycle.edges[{}] 冲突",
                        input_key.0, input_key.1, previous_edge_index
                    ),
                    format!("edges[{edge_index}].to_port"),
                ));
            }

            if let Some(step_index) = step_index_by_key.get(edge.from_node.as_str()).copied() {
                validate_edge_port(
                    &mut issues,
                    lifecycle,
                    &workflows_by_step,
                    step_index,
                    edge_index,
                    edge.from_node.as_str(),
                    edge.from_port.as_str(),
                    true,
                );
            }

            if let Some(step_index) = step_index_by_key.get(edge.to_node.as_str()).copied() {
                validate_edge_port(
                    &mut issues,
                    lifecycle,
                    &workflows_by_step,
                    step_index,
                    edge_index,
                    edge.to_node.as_str(),
                    edge.to_port.as_str(),
                    false,
                );
            }
        }

        Ok(issues)
    }

    pub async fn upsert_bundle(
        &self,
        bundle: BuiltinWorkflowBundle,
    ) -> Result<BuiltinWorkflowBundle, WorkflowApplicationError> {
        let mut persisted_workflows = Vec::with_capacity(bundle.workflows.len());
        for workflow in bundle.workflows {
            persisted_workflows.push(self.upsert_workflow_definition(workflow).await?);
        }

        let lifecycle = self.upsert_lifecycle_definition(bundle.lifecycle).await?;
        Ok(BuiltinWorkflowBundle {
            workflows: persisted_workflows,
            lifecycle,
        })
    }

    pub async fn assign_to_project(
        &self,
        cmd: AssignLifecycleCommand,
    ) -> Result<WorkflowAssignment, WorkflowApplicationError> {
        if cmd.is_default && !cmd.enabled {
            return Err(WorkflowApplicationError::BadRequest(
                "默认 workflow assignment 必须保持 enabled".to_string(),
            ));
        }

        let lifecycle = self
            .lifecycle_repo
            .get_by_id(cmd.lifecycle_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "lifecycle_definition 不存在: {}",
                    cmd.lifecycle_id
                ))
            })?;

        if cmd.enabled && !lifecycle.is_active() {
            return Err(WorkflowApplicationError::Conflict(format!(
                "lifecycle `{}` 状态为 {:?}，不能创建启用态 assignment",
                lifecycle.key, lifecycle.status
            )));
        }

        let existing = self
            .assignment_repo
            .list_by_project_and_role(cmd.project_id, cmd.role)
            .await?;

        if cmd.is_default {
            for assignment in existing
                .iter()
                .filter(|item| item.is_default && item.lifecycle_id != cmd.lifecycle_id)
            {
                let mut demoted = assignment.clone();
                demoted.is_default = false;
                demoted.updated_at = Utc::now();
                self.assignment_repo.update(&demoted).await?;
            }
        }

        if let Some(current) = existing
            .into_iter()
            .find(|item| item.lifecycle_id == cmd.lifecycle_id)
        {
            let mut updated = current;
            updated.enabled = cmd.enabled;
            updated.is_default = cmd.is_default;
            updated.updated_at = Utc::now();
            self.assignment_repo.update(&updated).await?;
            return Ok(updated);
        }

        let mut assignment = WorkflowAssignment::new(cmd.project_id, cmd.lifecycle_id, cmd.role);
        assignment.enabled = cmd.enabled;
        assignment.is_default = cmd.is_default;
        assignment.updated_at = Utc::now();
        self.assignment_repo.create(&assignment).await?;
        Ok(assignment)
    }
}

/// edge→port 校验：port 归属已迁移到 step 级别，直接从 step.output_ports / step.input_ports 检查。
fn validate_edge_port(
    issues: &mut Vec<ValidationIssue>,
    lifecycle: &LifecycleDefinition,
    _workflows_by_step: &BTreeMap<String, WorkflowDefinition>,
    step_index: usize,
    edge_index: usize,
    node_key: &str,
    port_key: &str,
    is_output: bool,
) {
    let edge_field = if is_output { "from_port" } else { "to_port" };
    let step = &lifecycle.steps[step_index];

    let exists = if is_output {
        step.output_ports.iter().any(|port| port.key == port_key)
    } else {
        step.input_ports.iter().any(|port| port.key == port_key)
    };
    if exists {
        return;
    }

    issues.push(ValidationIssue::error(
        if is_output {
            "lifecycle_edge_source_port_missing"
        } else {
            "lifecycle_edge_target_port_missing"
        },
        format!(
            "edge 引用的 {} port `{}` 不存在于 node `{}` 的 step 级 ports 定义中",
            if is_output { "output" } else { "input" },
            port_key,
            node_key,
        ),
        format!("edges[{edge_index}].{edge_field}"),
    ));
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        ContextStrategy, GateStrategy, InputPortDefinition, LifecycleDefinitionRepository,
        LifecycleEdge, LifecycleNodeType, LifecycleStepDefinition, OutputPortDefinition,
        WorkflowBindingKind, WorkflowContract, WorkflowDefinitionSource,
        WorkflowDefinitionStatus,
    };

    use super::*;

    #[derive(Default)]
    struct TestWorkflowDefinitionRepo {
        items: Mutex<BTreeMap<String, WorkflowDefinition>>,
    }

    impl TestWorkflowDefinitionRepo {
        fn seed(&self, workflow: WorkflowDefinition) {
            self.items
                .lock()
                .expect("workflow repo lock")
                .insert(workflow.key.clone(), workflow);
        }
    }

    #[async_trait::async_trait]
    impl WorkflowDefinitionRepository for TestWorkflowDefinitionRepo {
        async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
            self.seed(workflow.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("workflow repo lock")
                .values()
                .find(|item| item.id == id)
                .cloned())
        }

        async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("workflow repo lock")
                .get(key)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("workflow repo lock")
                .values()
                .cloned()
                .collect())
        }

        async fn list_by_status(
            &self,
            status: WorkflowDefinitionStatus,
        ) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("workflow repo lock")
                .values()
                .filter(|item| item.status == status)
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
                .expect("workflow repo lock")
                .values()
                .filter(|item| item.binding_kind == binding_kind)
                .cloned()
                .collect())
        }

        async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
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
    struct TestLifecycleDefinitionRepo {
        items: Mutex<BTreeMap<String, LifecycleDefinition>>,
    }

    #[async_trait::async_trait]
    impl LifecycleDefinitionRepository for TestLifecycleDefinitionRepo {
        async fn create(&self, lifecycle: &LifecycleDefinition) -> Result<(), DomainError> {
            self.items
                .lock()
                .expect("lifecycle repo lock")
                .insert(lifecycle.key.clone(), lifecycle.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("lifecycle repo lock")
                .values()
                .find(|item| item.id == id)
                .cloned())
        }

        async fn get_by_key(&self, key: &str) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("lifecycle repo lock")
                .get(key)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("lifecycle repo lock")
                .values()
                .cloned()
                .collect())
        }

        async fn list_by_status(
            &self,
            status: WorkflowDefinitionStatus,
        ) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("lifecycle repo lock")
                .values()
                .filter(|item| item.status == status)
                .cloned()
                .collect())
        }

        async fn list_by_binding_kind(
            &self,
            binding_kind: WorkflowBindingKind,
        ) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("lifecycle repo lock")
                .values()
                .filter(|item| item.binding_kind == binding_kind)
                .cloned()
                .collect())
        }

        async fn update(&self, lifecycle: &LifecycleDefinition) -> Result<(), DomainError> {
            self.items
                .lock()
                .expect("lifecycle repo lock")
                .insert(lifecycle.key.clone(), lifecycle.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items
                .lock()
                .expect("lifecycle repo lock")
                .retain(|_, item| item.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestWorkflowAssignmentRepo;

    #[async_trait::async_trait]
    impl WorkflowAssignmentRepository for TestWorkflowAssignmentRepo {
        async fn create(&self, _assignment: &WorkflowAssignment) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, _id: Uuid) -> Result<Option<WorkflowAssignment>, DomainError> {
            Ok(None)
        }

        async fn list_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<WorkflowAssignment>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_by_project_and_role(
            &self,
            _project_id: Uuid,
            _role: WorkflowBindingRole,
        ) -> Result<Vec<WorkflowAssignment>, DomainError> {
            Ok(Vec::new())
        }

        async fn update(&self, _assignment: &WorkflowAssignment) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn workflow_with_ports(
        key: &str,
        output_ports: &[&str],
        input_ports: &[&str],
    ) -> WorkflowDefinition {
        let contract = WorkflowContract {
            recommended_output_ports: output_ports
                .iter()
                .map(|port_key| OutputPortDefinition {
                    key: (*port_key).to_string(),
                    description: format!("output {port_key}"),
                    gate_strategy: GateStrategy::Existence,
                    gate_params: None,
                })
                .collect(),
            recommended_input_ports: input_ports
                .iter()
                .map(|port_key| InputPortDefinition {
                    key: (*port_key).to_string(),
                    description: format!("input {port_key}"),
                    context_strategy: ContextStrategy::Full,
                    context_template: None,
                })
                .collect(),
            ..Default::default()
        };
        WorkflowDefinition::new(
            key,
            format!("workflow {key}"),
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            contract,
        )
        .expect("workflow definition")
    }

    fn lifecycle_with_edges(edges: Vec<LifecycleEdge>) -> LifecycleDefinition {
        LifecycleDefinition::new(
            "dag",
            "dag",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            "research",
            vec![
                LifecycleStepDefinition {
                    key: "research".to_string(),
                    description: "research".to_string(),
                    workflow_key: Some("wf_research".to_string()),
                    node_type: LifecycleNodeType::AgentNode,
                    output_ports: vec![],
                    input_ports: vec![],
                },
                LifecycleStepDefinition {
                    key: "implement".to_string(),
                    description: "implement".to_string(),
                    workflow_key: Some("wf_implement".to_string()),
                    node_type: LifecycleNodeType::AgentNode,
                    output_ports: vec![],
                    input_ports: vec![],
                },
                LifecycleStepDefinition {
                    key: "check".to_string(),
                    description: "check".to_string(),
                    workflow_key: Some("wf_check".to_string()),
                    node_type: LifecycleNodeType::AgentNode,
                    output_ports: vec![],
                    input_ports: vec![],
                },
            ],
            edges,
        )
        .expect("lifecycle definition")
    }

    #[tokio::test]
    async fn validate_lifecycle_definition_reports_port_contract_issues() {
        let workflow_repo = TestWorkflowDefinitionRepo::default();
        workflow_repo.seed(workflow_with_ports("wf_research", &["research_report"], &[]));
        workflow_repo.seed(workflow_with_ports("wf_implement", &["shared_output"], &["research_input"]));
        workflow_repo.seed(workflow_with_ports("wf_check", &["shared_output"], &["review_input"]));

        let lifecycle_repo = TestLifecycleDefinitionRepo::default();
        let assignment_repo = TestWorkflowAssignmentRepo;
        let service =
            WorkflowCatalogService::new(&workflow_repo, &lifecycle_repo, &assignment_repo);

        let lifecycle = LifecycleDefinition::new(
            "dag",
            "dag",
            "desc",
            WorkflowBindingKind::Task,
            WorkflowDefinitionSource::UserAuthored,
            "research",
            vec![
                LifecycleStepDefinition {
                    key: "research".to_string(),
                    description: "research".to_string(),
                    workflow_key: Some("wf_research".to_string()),
                    node_type: LifecycleNodeType::AgentNode,
                    output_ports: vec![OutputPortDefinition {
                        key: "research_report".to_string(),
                        description: "research output".to_string(),
                        gate_strategy: GateStrategy::Existence,
                        gate_params: None,
                    }],
                    input_ports: vec![],
                },
                LifecycleStepDefinition {
                    key: "implement".to_string(),
                    description: "implement".to_string(),
                    workflow_key: Some("wf_implement".to_string()),
                    node_type: LifecycleNodeType::AgentNode,
                    output_ports: vec![OutputPortDefinition {
                        key: "shared_output".to_string(),
                        description: "shared".to_string(),
                        gate_strategy: GateStrategy::Existence,
                        gate_params: None,
                    }],
                    input_ports: vec![InputPortDefinition {
                        key: "research_input".to_string(),
                        description: "input".to_string(),
                        context_strategy: ContextStrategy::Full,
                        context_template: None,
                    }],
                },
                LifecycleStepDefinition {
                    key: "check".to_string(),
                    description: "check".to_string(),
                    workflow_key: Some("wf_check".to_string()),
                    node_type: LifecycleNodeType::AgentNode,
                    output_ports: vec![OutputPortDefinition {
                        key: "shared_output".to_string(),
                        description: "shared".to_string(),
                        gate_strategy: GateStrategy::Existence,
                        gate_params: None,
                    }],
                    input_ports: vec![InputPortDefinition {
                        key: "review_input".to_string(),
                        description: "input".to_string(),
                        context_strategy: ContextStrategy::Full,
                        context_template: None,
                    }],
                },
            ],
            vec![
                LifecycleEdge {
                    from_node: "research".to_string(),
                    from_port: "missing_output".to_string(),
                    to_node: "implement".to_string(),
                    to_port: "research_input".to_string(),
                },
                LifecycleEdge {
                    from_node: "check".to_string(),
                    from_port: "shared_output".to_string(),
                    to_node: "implement".to_string(),
                    to_port: "research_input".to_string(),
                },
            ],
        )
        .expect("lifecycle definition");

        let issues = service
            .validate_lifecycle_definition(&lifecycle)
            .await
            .expect("validate lifecycle");
        let codes: BTreeSet<&str> = issues.iter().map(|item| item.code.as_str()).collect();

        assert!(codes.contains("lifecycle_edge_source_port_missing"));
        assert!(codes.contains("lifecycle_input_port_multiple_sources"));
        assert!(codes.contains("lifecycle_output_port_key_not_unique"));
    }

    #[tokio::test]
    async fn upsert_lifecycle_definition_rejects_invalid_edge_contracts() {
        let workflow_repo = TestWorkflowDefinitionRepo::default();
        workflow_repo.seed(workflow_with_ports("wf_research", &["research_report"], &[]));
        workflow_repo.seed(workflow_with_ports("wf_implement", &["implementation_report"], &["research_input"]));
        workflow_repo.seed(workflow_with_ports("wf_check", &["check_report"], &["review_input"]));

        let lifecycle_repo = TestLifecycleDefinitionRepo::default();
        let assignment_repo = TestWorkflowAssignmentRepo;
        let service =
            WorkflowCatalogService::new(&workflow_repo, &lifecycle_repo, &assignment_repo);

        let lifecycle = lifecycle_with_edges(vec![LifecycleEdge {
            from_node: "research".to_string(),
            from_port: "research_report".to_string(),
            to_node: "implement".to_string(),
            to_port: "missing_input".to_string(),
        }]);

        let error = service
            .upsert_lifecycle_definition(lifecycle)
            .await
            .expect_err("invalid lifecycle should be rejected");

        match error {
            WorkflowApplicationError::BadRequest(message) => {
                assert!(message.contains("missing_input"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
