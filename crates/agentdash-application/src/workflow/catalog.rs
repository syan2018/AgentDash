use chrono::Utc;
use uuid::Uuid;

use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, WorkflowAgentRole, WorkflowAssignment,
    WorkflowAssignmentRepository, WorkflowDefinition, WorkflowDefinitionRepository,
};

use super::definition::BuiltinWorkflowBundle;
use super::error::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct AssignLifecycleCommand {
    pub project_id: Uuid,
    pub lifecycle_id: Uuid,
    pub role: WorkflowAgentRole,
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

    pub async fn upsert_lifecycle_definition(
        &self,
        lifecycle: LifecycleDefinition,
    ) -> Result<LifecycleDefinition, WorkflowApplicationError> {
        for step in &lifecycle.steps {
            let Some(workflow) = self
                .definition_repo
                .get_by_key(&step.primary_workflow_key)
                .await?
            else {
                return Err(WorkflowApplicationError::BadRequest(format!(
                    "lifecycle step `{}` 引用的 primary workflow `{}` 不存在",
                    step.key, step.primary_workflow_key
                )));
            };
            if workflow.target_kind != lifecycle.target_kind {
                return Err(WorkflowApplicationError::Conflict(format!(
                    "lifecycle step `{}` 引用的 workflow `{}` target_kind={:?}，与 lifecycle {:?} 不一致",
                    step.key, workflow.key, workflow.target_kind, lifecycle.target_kind
                )));
            }
        }

        if let Some(existing) = self.lifecycle_repo.get_by_key(&lifecycle.key).await? {
            if existing.target_kind != lifecycle.target_kind {
                return Err(WorkflowApplicationError::Conflict(format!(
                    "lifecycle `{}` 已绑定 target_kind={:?}，不能直接改为 {:?}",
                    lifecycle.key, existing.target_kind, lifecycle.target_kind
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
