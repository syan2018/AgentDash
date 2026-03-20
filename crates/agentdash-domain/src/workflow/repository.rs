use uuid::Uuid;

use super::entity::{WorkflowAssignment, WorkflowDefinition, WorkflowRun};
use super::value_objects::{WorkflowAgentRole, WorkflowTargetKind};
use crate::common::error::DomainError;

#[async_trait::async_trait]
pub trait WorkflowDefinitionRepository: Send + Sync {
    async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowDefinition>, DomainError>;
    async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError>;
    async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError>;
    async fn list_enabled(&self) -> Result<Vec<WorkflowDefinition>, DomainError>;
    async fn list_by_target_kind(
        &self,
        target_kind: WorkflowTargetKind,
    ) -> Result<Vec<WorkflowDefinition>, DomainError>;
    async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait WorkflowAssignmentRepository: Send + Sync {
    async fn create(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowAssignment>, DomainError>;
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<WorkflowAssignment>, DomainError>;
    async fn list_by_project_and_role(
        &self,
        project_id: Uuid,
        role: WorkflowAgentRole,
    ) -> Result<Vec<WorkflowAssignment>, DomainError>;
    async fn update(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait WorkflowRunRepository: Send + Sync {
    async fn create(&self, run: &WorkflowRun) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowRun>, DomainError>;
    async fn list_by_workflow(&self, workflow_id: Uuid) -> Result<Vec<WorkflowRun>, DomainError>;
    async fn list_by_target(
        &self,
        target_kind: WorkflowTargetKind,
        target_id: Uuid,
    ) -> Result<Vec<WorkflowRun>, DomainError>;
    async fn update(&self, run: &WorkflowRun) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
