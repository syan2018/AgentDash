use chrono::Utc;
use uuid::Uuid;

use agentdash_domain::workflow::{
    WorkflowAgentRole, WorkflowAssignment, WorkflowAssignmentRepository, WorkflowDefinition,
    WorkflowDefinitionRepository,
};

use super::error::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct AssignWorkflowCommand {
    pub project_id: Uuid,
    pub workflow_id: Uuid,
    pub role: WorkflowAgentRole,
    pub enabled: bool,
    pub is_default: bool,
}

pub struct WorkflowCatalogService<'a, D: ?Sized, A: ?Sized> {
    definition_repo: &'a D,
    assignment_repo: &'a A,
}

impl<'a, D: ?Sized, A: ?Sized> WorkflowCatalogService<'a, D, A>
where
    D: WorkflowDefinitionRepository,
    A: WorkflowAssignmentRepository,
{
    pub fn new(definition_repo: &'a D, assignment_repo: &'a A) -> Self {
        Self {
            definition_repo,
            assignment_repo,
        }
    }

    pub async fn upsert_definition(
        &self,
        definition: WorkflowDefinition,
    ) -> Result<WorkflowDefinition, WorkflowApplicationError> {
        if let Some(existing) = self.definition_repo.get_by_key(&definition.key).await? {
            if existing.target_kind != definition.target_kind {
                return Err(WorkflowApplicationError::Conflict(format!(
                    "workflow `{}` 已绑定 target_kind={:?}，不能直接改为 {:?}",
                    definition.key, existing.target_kind, definition.target_kind
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

    pub async fn assign_to_project(
        &self,
        cmd: AssignWorkflowCommand,
    ) -> Result<WorkflowAssignment, WorkflowApplicationError> {
        if cmd.is_default && !cmd.enabled {
            return Err(WorkflowApplicationError::BadRequest(
                "默认 workflow assignment 必须保持 enabled".to_string(),
            ));
        }

        let workflow = self
            .definition_repo
            .get_by_id(cmd.workflow_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "workflow_definition 不存在: {}",
                    cmd.workflow_id
                ))
            })?;

        if cmd.enabled && !workflow.enabled {
            return Err(WorkflowApplicationError::Conflict(format!(
                "workflow `{}` 已停用，不能创建启用态 assignment",
                workflow.key
            )));
        }

        let existing = self
            .assignment_repo
            .list_by_project_and_role(cmd.project_id, cmd.role)
            .await?;

        if cmd.is_default {
            for assignment in existing
                .iter()
                .filter(|item| item.is_default && item.workflow_id != cmd.workflow_id)
            {
                let mut demoted = assignment.clone();
                demoted.is_default = false;
                demoted.updated_at = Utc::now();
                self.assignment_repo.update(&demoted).await?;
            }
        }

        if let Some(current) = existing
            .into_iter()
            .find(|item| item.workflow_id == cmd.workflow_id)
        {
            let mut updated = current;
            updated.enabled = cmd.enabled;
            updated.is_default = cmd.is_default;
            updated.updated_at = Utc::now();
            self.assignment_repo.update(&updated).await?;
            return Ok(updated);
        }

        let mut assignment = WorkflowAssignment::new(cmd.project_id, cmd.workflow_id, cmd.role);
        assignment.enabled = cmd.enabled;
        assignment.is_default = cmd.is_default;
        assignment.updated_at = Utc::now();
        self.assignment_repo.create(&assignment).await?;
        Ok(assignment)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        WorkflowAssignment, WorkflowAssignmentRepository, WorkflowDefinition,
        WorkflowDefinitionRepository, WorkflowTargetKind,
    };

    use super::*;
    use crate::workflow::build_trellis_dev_workflow_definition;

    #[derive(Default)]
    struct MemoryWorkflowCatalogStore {
        definitions: Mutex<HashMap<Uuid, WorkflowDefinition>>,
        assignments: Mutex<HashMap<Uuid, WorkflowAssignment>>,
    }

    #[async_trait]
    impl WorkflowDefinitionRepository for MemoryWorkflowCatalogStore {
        async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
            self.definitions
                .lock()
                .expect("lock")
                .insert(workflow.id, workflow.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self.definitions.lock().expect("lock").get(&id).cloned())
        }

        async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .expect("lock")
                .values()
                .find(|workflow| workflow.key == key)
                .cloned())
        }

        async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .expect("lock")
                .values()
                .cloned()
                .collect())
        }

        async fn list_enabled(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .expect("lock")
                .values()
                .filter(|workflow| workflow.enabled)
                .cloned()
                .collect())
        }

        async fn list_by_target_kind(
            &self,
            target_kind: WorkflowTargetKind,
        ) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .expect("lock")
                .values()
                .filter(|workflow| workflow.target_kind == target_kind)
                .cloned()
                .collect())
        }

        async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
            self.definitions
                .lock()
                .expect("lock")
                .insert(workflow.id, workflow.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.definitions.lock().expect("lock").remove(&id);
            Ok(())
        }
    }

    #[async_trait]
    impl WorkflowAssignmentRepository for MemoryWorkflowCatalogStore {
        async fn create(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError> {
            self.assignments
                .lock()
                .expect("lock")
                .insert(assignment.id, assignment.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowAssignment>, DomainError> {
            Ok(self.assignments.lock().expect("lock").get(&id).cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<WorkflowAssignment>, DomainError> {
            Ok(self
                .assignments
                .lock()
                .expect("lock")
                .values()
                .filter(|assignment| assignment.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_by_project_and_role(
            &self,
            project_id: Uuid,
            role: WorkflowAgentRole,
        ) -> Result<Vec<WorkflowAssignment>, DomainError> {
            Ok(self
                .assignments
                .lock()
                .expect("lock")
                .values()
                .filter(|assignment| assignment.project_id == project_id && assignment.role == role)
                .cloned()
                .collect())
        }

        async fn update(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError> {
            self.assignments
                .lock()
                .expect("lock")
                .insert(assignment.id, assignment.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.assignments.lock().expect("lock").remove(&id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn upsert_definition_promotes_existing_definition_version() {
        let store = MemoryWorkflowCatalogStore::default();
        let service = WorkflowCatalogService::new(&store, &store);

        let created = service
            .upsert_definition(
                build_trellis_dev_workflow_definition(WorkflowTargetKind::Task)
                    .expect("definition"),
            )
            .await
            .expect("create definition");

        let mut replacement =
            build_trellis_dev_workflow_definition(WorkflowTargetKind::Task).expect("definition");
        replacement.description = "new description".to_string();

        let updated = service
            .upsert_definition(replacement)
            .await
            .expect("update definition");

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.version, created.version + 1);
        assert_eq!(updated.description, "new description");
    }

    #[tokio::test]
    async fn assign_to_project_keeps_only_one_default_per_role() {
        let store = MemoryWorkflowCatalogStore::default();
        let service = WorkflowCatalogService::new(&store, &store);
        let workflow_a = service
            .upsert_definition(
                build_trellis_dev_workflow_definition(WorkflowTargetKind::Task)
                    .expect("definition"),
            )
            .await
            .expect("workflow a");
        let mut workflow_b =
            build_trellis_dev_workflow_definition(WorkflowTargetKind::Task).expect("definition");
        workflow_b.key = "trellis_dev_workflow_v2".to_string();
        workflow_b.name = "Trellis Dev Workflow V2".to_string();
        let workflow_b = service
            .upsert_definition(workflow_b)
            .await
            .expect("workflow b");

        let project_id = Uuid::new_v4();
        service
            .assign_to_project(AssignWorkflowCommand {
                project_id,
                workflow_id: workflow_a.id,
                role: WorkflowAgentRole::TaskExecutionWorker,
                enabled: true,
                is_default: true,
            })
            .await
            .expect("assign a");

        let assignment_b = service
            .assign_to_project(AssignWorkflowCommand {
                project_id,
                workflow_id: workflow_b.id,
                role: WorkflowAgentRole::TaskExecutionWorker,
                enabled: true,
                is_default: true,
            })
            .await
            .expect("assign b");

        let assignments = store
            .list_by_project_and_role(project_id, WorkflowAgentRole::TaskExecutionWorker)
            .await
            .expect("list assignments");
        let default_ids = assignments
            .into_iter()
            .filter(|assignment| assignment.is_default)
            .map(|assignment| assignment.id)
            .collect::<Vec<_>>();

        assert_eq!(default_ids, vec![assignment_b.id]);
    }
}
