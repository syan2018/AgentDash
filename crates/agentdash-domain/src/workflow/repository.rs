use uuid::Uuid;

use super::agent_frame::AgentFrame;
use super::agent_lineage::AgentLineage;
use super::entity::{AgentProcedure, LifecycleRun, WorkflowGraph};
use super::lifecycle_agent::LifecycleAgent;
use super::lifecycle_gate::LifecycleGate;
use super::lifecycle_subject_association::{LifecycleSubjectAssociation, SubjectRef};
use super::runtime_session_anchor::RuntimeSessionExecutionAnchor;
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
pub trait LifecycleRunRepository: Send + Sync {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError>;
    async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<LifecycleRun>, DomainError>;
    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError>;
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
    async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError>;
    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError>;
    async fn append_visible_canvas_mount(
        &self,
        frame_id: Uuid,
        mount_id: &str,
    ) -> Result<(), DomainError>;
    async fn append_visible_workspace_module_ref(
        &self,
        _frame_id: Uuid,
        _module_ref: &str,
    ) -> Result<(), DomainError> {
        Ok(())
    }
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
    /// 一次取回某 run 下的全部 lineage 边，供 UI 在内存构建控制树 forest，
    /// 避免按 agent 逐个 `list_children` 的 N 次往返。
    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentLineage>, DomainError>;
}

/// RuntimeSession → 控制面锚点的 repository。
#[async_trait::async_trait]
pub trait RuntimeSessionExecutionAnchorRepository: Send + Sync {
    /// 写入或更新 runtime_session 到 lifecycle / agent / frame / orchestration node 的锚点。
    async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError>;
    /// 按 runtime_session_id 删除锚点。
    async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError>;
    /// 按 runtime_session_id 查找锚点。
    async fn find_by_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError>;
    /// 按 run 查询该控制面账本关联的 runtime sessions。
    async fn list_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError>;
    /// 按 agent 查询该 agent 关联的 runtime sessions。
    async fn list_by_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError>;
    /// 批量按 runtime_session_id 查询 anchors。
    async fn list_by_project_session_ids(
        &self,
        runtime_session_ids: &[String],
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError>;
    /// 按 `updated_at DESC` 查询 agent 最新写入的 raw anchor row。
    ///
    /// 该方法只表达 repository order，不表达 delivery/runtime selection policy。
    async fn latest_updated_anchor_for_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError>;
}
