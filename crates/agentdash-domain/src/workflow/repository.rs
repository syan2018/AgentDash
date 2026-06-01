use uuid::Uuid;

use super::agent_assignment::AgentAssignment;
use super::agent_frame::AgentFrame;
use super::agent_lineage::AgentLineage;
use super::entity::{ActivityExecutionClaim, AgentProcedure, LifecycleRun, WorkflowGraph};
use super::lifecycle_agent::LifecycleAgent;
use super::lifecycle_gate::LifecycleGate;
use super::lifecycle_subject_association::{LifecycleSubjectAssociation, SubjectRef};
use super::workflow_graph_instance::WorkflowGraphInstance;
use crate::common::error::DomainError;

#[async_trait::async_trait]
pub trait AgentProcedureRepository: Send + Sync {
    async fn create(&self, procedure: &AgentProcedure) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<AgentProcedure>, DomainError>;
    async fn get_by_key(&self, key: &str) -> Result<Option<AgentProcedure>, DomainError>;
    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<AgentProcedure>, DomainError>;
    async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<AgentProcedure>, DomainError>;
    async fn update(&self, procedure: &AgentProcedure) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait WorkflowGraphRepository: Send + Sync {
    async fn create(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowGraph>, DomainError>;
    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<WorkflowGraph>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<WorkflowGraph>, DomainError>;
    async fn update(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

#[derive(Debug, Clone)]
pub struct WorkflowTemplateInstallBundle {
    pub procedures: Vec<AgentProcedure>,
    pub graph: WorkflowGraph,
    pub overwrite: bool,
}

#[derive(Debug, Clone)]
pub struct WorkflowTemplateInstallResult {
    pub procedures: Vec<AgentProcedure>,
    pub graph: WorkflowGraph,
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
    /// 通过 executor_run_ref 中的 session_id 查找当前 running 的 claim。
    /// 用于 terminal callback 路径：不再依赖 LifecycleRun.session_id。
    async fn find_running_by_executor_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ActivityExecutionClaim>, DomainError>;
}

#[async_trait::async_trait]
pub trait LifecycleRunRepository: Send + Sync {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError>;
    async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<LifecycleRun>, DomainError>;
    async fn list_by_lifecycle(&self, lifecycle_id: Uuid)
    -> Result<Vec<LifecycleRun>, DomainError>;
    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

// ─── Target Anchor Repositories ─────────────────────────────────────────────

#[async_trait::async_trait]
pub trait WorkflowGraphInstanceRepository: Send + Sync {
    async fn create(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError>;
    async fn get(&self, id: Uuid) -> Result<Option<WorkflowGraphInstance>, DomainError>;
    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<WorkflowGraphInstance>, DomainError>;
    async fn update(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait LifecycleAgentRepository: Send + Sync {
    async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError>;
    async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError>;
    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError>;
    async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait AgentFrameRepository: Send + Sync {
    async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError>;
    async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError>;
    async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError>;
    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError>;
    async fn attach_runtime_session_ref(
        &self,
        frame_id: Uuid,
        runtime_session_id: &str,
    ) -> Result<(), DomainError>;
    /// 通过 runtime session ref 反查 AgentFrame。
    /// Terminal callback 完整链路：RuntimeSession -> AgentFrame -> Agent -> Assignment。
    async fn find_by_runtime_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<AgentFrame>, DomainError>;
}

#[async_trait::async_trait]
pub trait AgentAssignmentRepository: Send + Sync {
    async fn create(&self, assignment: &AgentAssignment) -> Result<(), DomainError>;
    async fn find_for_attempt(
        &self,
        graph_instance_id: Uuid,
        activity_key: &str,
        attempt: i32,
    ) -> Result<Option<AgentAssignment>, DomainError>;
    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentAssignment>, DomainError>;
    async fn update(&self, assignment: &AgentAssignment) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait LifecycleSubjectAssociationRepository: Send + Sync {
    async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError>;
    async fn list_by_subject(
        &self,
        subject: &SubjectRef,
    ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError>;
    async fn list_by_anchor(
        &self,
        run_id: Uuid,
        agent_id: Option<Uuid>,
    ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait LifecycleGateRepository: Send + Sync {
    async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError>;
    async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError>;
    async fn list_open_for_agent(&self, agent_id: Uuid) -> Result<Vec<LifecycleGate>, DomainError>;
    async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait AgentLineageRepository: Send + Sync {
    async fn create(&self, lineage: &AgentLineage) -> Result<(), DomainError>;
    async fn list_children(&self, agent_id: Uuid) -> Result<Vec<AgentLineage>, DomainError>;
    async fn find_parent(&self, child_agent_id: Uuid) -> Result<Option<AgentLineage>, DomainError>;
}
