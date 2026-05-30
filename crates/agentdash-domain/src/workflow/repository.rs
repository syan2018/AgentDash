use uuid::Uuid;

use super::entity::{
    ActivityExecutionClaim, ActivityLifecycleDefinition, LifecycleRun, WorkflowDefinition,
};
use super::value_objects::WorkflowBindingKind;
use crate::common::error::DomainError;

#[async_trait::async_trait]
pub trait WorkflowDefinitionRepository: Send + Sync {
    async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowDefinition>, DomainError>;
    async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError>;
    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<WorkflowDefinition>, DomainError>;
    async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError>;
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<WorkflowDefinition>, DomainError>;
    async fn list_by_binding_kind(
        &self,
        binding_kind: WorkflowBindingKind,
    ) -> Result<Vec<WorkflowDefinition>, DomainError>;
    async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait ActivityLifecycleDefinitionRepository: Send + Sync {
    async fn create(&self, lifecycle: &ActivityLifecycleDefinition) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid)
    -> Result<Option<ActivityLifecycleDefinition>, DomainError>;
    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<ActivityLifecycleDefinition>, DomainError>;
    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ActivityLifecycleDefinition>, DomainError>;
    async fn update(&self, lifecycle: &ActivityLifecycleDefinition) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

#[derive(Debug, Clone)]
pub struct WorkflowTemplateInstallBundle {
    pub workflows: Vec<WorkflowDefinition>,
    pub lifecycle: ActivityLifecycleDefinition,
    pub overwrite: bool,
}

#[derive(Debug, Clone)]
pub struct WorkflowTemplateInstallResult {
    pub workflows: Vec<WorkflowDefinition>,
    pub lifecycle: ActivityLifecycleDefinition,
}

#[async_trait::async_trait]
pub trait WorkflowTemplateInstallRepository: Send + Sync {
    async fn install_workflow_template_bundle(
        &self,
        bundle: WorkflowTemplateInstallBundle,
    ) -> Result<WorkflowTemplateInstallResult, DomainError>;
}

#[async_trait::async_trait]
pub trait ActivityExecutionClaimRepository: Send + Sync {
    async fn create_or_get(
        &self,
        claim: &ActivityExecutionClaim,
    ) -> Result<ActivityExecutionClaim, DomainError>;
    async fn get_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<ActivityExecutionClaim>, DomainError>;
    async fn list_active_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<ActivityExecutionClaim>, DomainError>;
    async fn update(&self, claim: &ActivityExecutionClaim) -> Result<(), DomainError>;
    async fn abandon_claiming_before(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<ActivityExecutionClaim>, DomainError>;
}

#[async_trait::async_trait]
pub trait LifecycleRunRepository: Send + Sync {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError>;
    async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<LifecycleRun>, DomainError>;
    async fn list_by_lifecycle(&self, lifecycle_id: Uuid)
    -> Result<Vec<LifecycleRun>, DomainError>;
    /// Runtime/debug 用途：按 session 反查 run。业务查询应通过 LifecycleRunLink。
    async fn list_by_session(&self, session_id: &str) -> Result<Vec<LifecycleRun>, DomainError>;
    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}
