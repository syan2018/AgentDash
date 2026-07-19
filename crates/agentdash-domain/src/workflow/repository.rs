use uuid::Uuid;

use super::agent_frame::AgentFrame;
use super::agent_lineage::AgentLineage;
use super::agent_run_lineage::AgentRunLineage;
use super::entity::{AgentProcedure, LifecycleRun, WorkflowGraph};
use super::gate_wait_policy::WaitProducerRef;
use super::lifecycle_agent::LifecycleAgent;
use super::lifecycle_gate::LifecycleGate;
use super::lifecycle_subject_association::{LifecycleSubjectAssociation, SubjectRef};
use crate::channel::{ChannelRegistryDocument, ChannelRegistryMutation};
use crate::common::error::DomainError;

#[derive(Debug, thiserror::Error)]
pub enum LifecycleRunWriteError {
    #[error(
        "LifecycleRun revision conflict: run_id={run_id}, expected={expected_revision}, actual={actual_revision}"
    )]
    RevisionConflict {
        run_id: Uuid,
        expected_revision: u64,
        actual_revision: u64,
    },
    #[error("LifecycleRun CAS persistence failed: {0}")]
    Persistence(#[from] DomainError),
    #[error("LifecycleRun repository does not implement the required revision CAS contract")]
    CasNotImplemented,
}

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
pub trait LifecycleRunRepository: Send + Sync {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError>;
    async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<LifecycleRun>, DomainError>;
    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError>;
    /// Atomically replaces the aggregate iff the stored revision equals
    /// `expected_revision`.
    ///
    /// Implementations must require `run.revision == expected_revision + 1`
    /// and persist the aggregate body plus revision in one transaction.
    /// Product Workflow execution must not compose the default implementation.
    /// The write set is the executor aggregate plus revision; it must preserve
    /// `channel_registry`, whose independent mutation method owns that column.
    async fn compare_and_swap(
        &self,
        _expected_revision: u64,
        _run: &LifecycleRun,
    ) -> Result<(), LifecycleRunWriteError> {
        Err(LifecycleRunWriteError::CasNotImplemented)
    }
    async fn load_channel_registry(
        &self,
        run_id: Uuid,
    ) -> Result<ChannelRegistryDocument, DomainError> {
        let Some(run) = self.get_by_id(run_id).await? else {
            return Err(DomainError::NotFound {
                entity: "lifecycle_run",
                id: run_id.to_string(),
            });
        };
        Ok(run.channel_registry)
    }
    async fn mutate_channel_registry(
        &self,
        _run_id: Uuid,
        _mutation: ChannelRegistryMutation,
    ) -> Result<ChannelRegistryDocument, DomainError> {
        Err(DomainError::InvalidConfig(
            "lifecycle_run.channel_registry mutation is not implemented for this repository"
                .to_string(),
        ))
    }
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
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
    async fn get_latest(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError>;
    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError>;
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
    async fn list_open_gate_wait_policies(
        &self,
        limit: usize,
    ) -> Result<Vec<LifecycleGate>, DomainError>;
    async fn list_by_wait_producer(
        &self,
        producer: &WaitProducerRef,
    ) -> Result<Vec<LifecycleGate>, DomainError>;
    /// Precise normal companion result writer lookup.
    ///
    /// Terminal/reconcile convergence must use wait producer declarations instead of
    /// agent/correlation lookup so producer terminal facts stay independent from gate kind.
    async fn find_by_agent_and_correlation(
        &self,
        agent_id: Uuid,
        correlation_id: &str,
    ) -> Result<Option<LifecycleGate>, DomainError>;
    async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError>;
}

#[async_trait::async_trait]
pub trait AgentLineageRepository: Send + Sync {
    async fn create(&self, lineage: &AgentLineage) -> Result<(), DomainError>;
    async fn list_children(&self, agent_id: Uuid) -> Result<Vec<AgentLineage>, DomainError>;
    async fn find_parent(&self, child_agent_id: Uuid) -> Result<Option<AgentLineage>, DomainError>;
    /// 一次取回某 run 下的全部 lineage 边，供 UI 在内存构建控制树 forest，
    /// 避免按 agent 逐个 `list_children` 的 N 次往返。
    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentLineage>, DomainError>;
}

#[async_trait::async_trait]
pub trait AgentRunLineageRepository: Send + Sync {
    async fn create(&self, lineage: &AgentRunLineage) -> Result<(), DomainError>;
    async fn find_parent(
        &self,
        child_run_id: Uuid,
        child_agent_id: Uuid,
    ) -> Result<Option<AgentRunLineage>, DomainError>;
    async fn list_children(
        &self,
        parent_run_id: Uuid,
        parent_agent_id: Uuid,
    ) -> Result<Vec<AgentRunLineage>, DomainError>;
    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentRunLineage>, DomainError>;
}
