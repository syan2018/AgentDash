use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::agent_run_message_submission::{
    AgentRunAcceptedDeliveryKind, AgentRunMailboxAcceptedSettlement,
    AgentRunMailboxAcceptedSettlementResult, AgentRunMailboxDeliverySettlementPort,
    AgentRunMailboxDeliverySettlementResult, AgentRunMailboxFailedSettlement,
};
use agentdash_domain::DomainError;
use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxClaimRequest, AgentRunMailboxCreateOutcome, AgentRunMailboxMessage,
    AgentRunMailboxRepository, AgentRunMailboxState, ConsumptionBarrier,
    MAILBOX_CLAIM_LEASE_EXPIRED_RECONCILIATION, MailboxDelivery, MailboxDrainMode,
    MailboxMessageOrigin, MailboxMessageStatus, NewAgentRunMailboxMessage, SteeringStopEffect,
};
use agentdash_domain::backend::{
    ProjectBackendAccess, ProjectBackendAccessRepository, ProjectBackendAccessStatus,
};
use agentdash_domain::channel::{ChannelRegistryDocument, ChannelRegistryMutation};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentLineage, AgentLineageRepository, AgentProcedure,
    AgentProcedureRepository, AgentRunLineage, AgentRunLineageRepository, GateWaitPolicyEnvelope,
    LifecycleAgent, LifecycleAgentRepository, LifecycleGate, LifecycleGateRepository, LifecycleRun,
    LifecycleRunRepository, LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository,
    SubjectRef, WaitProducerRef, WorkflowGraph, WorkflowGraphRepository,
};
use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct MemoryLifecycleRunRepository {
    runs: Mutex<Vec<LifecycleRun>>,
}

#[async_trait::async_trait]
impl LifecycleRunRepository for MemoryLifecycleRunRepository {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        self.runs.lock().await.push(run.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
        Ok(self
            .runs
            .lock()
            .await
            .iter()
            .find(|run| run.id == id)
            .cloned())
    }

    async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
        Ok(self
            .runs
            .lock()
            .await
            .iter()
            .filter(|run| ids.contains(&run.id))
            .cloned()
            .collect())
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<LifecycleRun>, DomainError> {
        Ok(self
            .runs
            .lock()
            .await
            .iter()
            .filter(|run| run.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        let mut runs = self.runs.lock().await;
        if let Some(existing) = runs.iter_mut().find(|item| item.id == run.id) {
            let channel_registry = existing.channel_registry.clone();
            *existing = run.clone();
            existing.channel_registry = channel_registry;
        }
        Ok(())
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
        run_id: Uuid,
        mutation: ChannelRegistryMutation,
    ) -> Result<ChannelRegistryDocument, DomainError> {
        let mut runs = self.runs.lock().await;
        let Some(run) = runs.iter_mut().find(|item| item.id == run_id) else {
            return Err(DomainError::NotFound {
                entity: "lifecycle_run",
                id: run_id.to_string(),
            });
        };
        run.channel_registry.apply(mutation)?;
        Ok(run.channel_registry.clone())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.runs.lock().await.retain(|run| run.id != id);
        Ok(())
    }
}

impl MemoryLifecycleRunRepository {
    pub async fn debug_list(&self) -> Vec<LifecycleRun> {
        self.runs.lock().await.clone()
    }
}

#[derive(Default)]
pub struct MemoryAgentFrameRepository {
    frames: Mutex<Vec<AgentFrame>>,
}

#[derive(Default)]
pub struct MemoryAgentRunLineageRepository {
    lineages: Mutex<Vec<AgentRunLineage>>,
}

#[async_trait::async_trait]
impl AgentRunLineageRepository for MemoryAgentRunLineageRepository {
    async fn create(&self, lineage: &AgentRunLineage) -> Result<(), DomainError> {
        let mut lineages = self.lineages.lock().await;
        if lineages.iter().any(|existing| {
            existing.child_run_id == lineage.child_run_id
                && existing.child_agent_id == lineage.child_agent_id
        }) {
            return Err(DomainError::Conflict {
                entity: "agent_run_lineage",
                constraint: "unique_child",
                message: "child AgentRun already has a parent lineage".to_string(),
            });
        }
        lineages.push(lineage.clone());
        Ok(())
    }

    async fn find_parent(
        &self,
        child_run_id: Uuid,
        child_agent_id: Uuid,
    ) -> Result<Option<AgentRunLineage>, DomainError> {
        Ok(self
            .lineages
            .lock()
            .await
            .iter()
            .find(|lineage| {
                lineage.child_run_id == child_run_id && lineage.child_agent_id == child_agent_id
            })
            .cloned())
    }

    async fn list_children(
        &self,
        parent_run_id: Uuid,
        parent_agent_id: Uuid,
    ) -> Result<Vec<AgentRunLineage>, DomainError> {
        Ok(self
            .lineages
            .lock()
            .await
            .iter()
            .filter(|lineage| {
                lineage.parent_run_id == parent_run_id && lineage.parent_agent_id == parent_agent_id
            })
            .cloned()
            .collect())
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentRunLineage>, DomainError> {
        Ok(self
            .lineages
            .lock()
            .await
            .iter()
            .filter(|lineage| lineage.parent_run_id == run_id || lineage.child_run_id == run_id)
            .cloned()
            .collect())
    }
}

#[async_trait::async_trait]
impl AgentFrameRepository for MemoryAgentFrameRepository {
    async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
        self.frames.lock().await.push(frame.clone());
        Ok(())
    }

    async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(self
            .frames
            .lock()
            .await
            .iter()
            .find(|frame| frame.id == frame_id)
            .cloned())
    }

    async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        Ok(self
            .frames
            .lock()
            .await
            .iter()
            .filter(|frame| frame.agent_id == agent_id)
            .max_by_key(|frame| (frame.revision, frame.created_at))
            .cloned())
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
        Ok(self
            .frames
            .lock()
            .await
            .iter()
            .filter(|frame| frame.agent_id == agent_id)
            .cloned()
            .collect())
    }
}

impl MemoryAgentFrameRepository {
    pub async fn debug_list(&self) -> Vec<AgentFrame> {
        self.frames.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl agent_frame_materialization_port::AgentRunFrameConstructionPort
    for MemoryAgentFrameRepository
{
    async fn execute_frame_construction_command(
        &self,
        command: agent_frame_materialization_port::FrameConstructionCommand,
    ) -> Result<
        agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome,
        agent_frame_materialization_port::AgentRunFrameSurfaceError,
    > {
        let agent_frame_materialization_port::FrameConstructionCommand::DispatchLaunchAnchor {
            agent_id,
            runtime_session_id,
            created_by_id,
            ..
        } = command
        else {
            return Err(
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: "memory frame construction supports DispatchLaunchAnchor".to_string(),
                },
            );
        };

        let next_revision = self
            .frames
            .lock()
            .await
            .iter()
            .filter(|frame| frame.agent_id == agent_id)
            .map(|frame| frame.revision)
            .max()
            .unwrap_or(0)
            + 1;
        let mut frame = AgentFrame::new_revision(agent_id, next_revision, "frame_construction");
        frame.created_by_id = created_by_id;
        self.create(&frame).await.map_err(|error| {
            agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                message: error.to_string(),
            }
        })?;

        let mut outcome = agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome::new(
            agent_frame_materialization_port::AgentFrameWriteRole::FrameConstruction,
        );
        outcome.frame_id = Some(frame.id);
        outcome.agent_id = Some(frame.agent_id);
        outcome.runtime_session_id = runtime_session_id;
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

#[derive(Default)]
pub struct MemoryWorkflowGraphRepository {
    graphs: Mutex<Vec<WorkflowGraph>>,
}

#[async_trait::async_trait]
impl WorkflowGraphRepository for MemoryWorkflowGraphRepository {
    async fn create(&self, graph: &WorkflowGraph) -> Result<(), DomainError> {
        self.graphs.lock().await.push(graph.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
        Ok(self
            .graphs
            .lock()
            .await
            .iter()
            .find(|graph| graph.id == id)
            .cloned())
    }

    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<WorkflowGraph>, DomainError> {
        Ok(self
            .graphs
            .lock()
            .await
            .iter()
            .find(|graph| graph.project_id == project_id && graph.key == key)
            .cloned())
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<WorkflowGraph>, DomainError> {
        Ok(self
            .graphs
            .lock()
            .await
            .iter()
            .filter(|graph| graph.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn update(&self, graph: &WorkflowGraph) -> Result<(), DomainError> {
        let mut graphs = self.graphs.lock().await;
        if let Some(existing) = graphs.iter_mut().find(|item| item.id == graph.id) {
            *existing = graph.clone();
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.graphs.lock().await.retain(|graph| graph.id != id);
        Ok(())
    }
}

impl MemoryWorkflowGraphRepository {
    pub async fn debug_list(&self) -> Vec<WorkflowGraph> {
        self.graphs.lock().await.clone()
    }
}

#[derive(Default)]
pub struct MemoryAgentProcedureRepository {
    procedures: Mutex<Vec<AgentProcedure>>,
}

#[async_trait::async_trait]
impl AgentProcedureRepository for MemoryAgentProcedureRepository {
    async fn create(&self, procedure: &AgentProcedure) -> Result<(), DomainError> {
        self.procedures.lock().await.push(procedure.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<AgentProcedure>, DomainError> {
        Ok(self
            .procedures
            .lock()
            .await
            .iter()
            .find(|procedure| procedure.id == id)
            .cloned())
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<AgentProcedure>, DomainError> {
        Ok(self
            .procedures
            .lock()
            .await
            .iter()
            .find(|procedure| procedure.key == key)
            .cloned())
    }

    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<AgentProcedure>, DomainError> {
        Ok(self
            .procedures
            .lock()
            .await
            .iter()
            .find(|procedure| procedure.project_id == project_id && procedure.key == key)
            .cloned())
    }

    async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError> {
        Ok(self.procedures.lock().await.clone())
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<AgentProcedure>, DomainError> {
        Ok(self
            .procedures
            .lock()
            .await
            .iter()
            .filter(|procedure| procedure.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn update(&self, procedure: &AgentProcedure) -> Result<(), DomainError> {
        let mut procedures = self.procedures.lock().await;
        if let Some(existing) = procedures.iter_mut().find(|item| item.id == procedure.id) {
            *existing = procedure.clone();
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.procedures
            .lock()
            .await
            .retain(|procedure| procedure.id != id);
        Ok(())
    }
}

#[derive(Default)]
pub struct MemoryLifecycleSubjectAssociationRepository {
    associations: Mutex<Vec<LifecycleSubjectAssociation>>,
}

#[async_trait::async_trait]
impl LifecycleSubjectAssociationRepository for MemoryLifecycleSubjectAssociationRepository {
    async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError> {
        self.associations.lock().await.push(assoc.clone());
        Ok(())
    }

    async fn list_by_subject(
        &self,
        subject: &SubjectRef,
    ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
        Ok(self
            .associations
            .lock()
            .await
            .iter()
            .filter(|assoc| assoc.subject_kind == subject.kind && assoc.subject_id == subject.id)
            .cloned()
            .collect())
    }

    async fn list_by_anchor(
        &self,
        run_id: Uuid,
        agent_id: Option<Uuid>,
    ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
        Ok(self
            .associations
            .lock()
            .await
            .iter()
            .filter(|assoc| assoc.anchor_run_id == run_id && assoc.anchor_agent_id == agent_id)
            .cloned()
            .collect())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        self.associations
            .lock()
            .await
            .retain(|assoc| assoc.id != id);
        Ok(())
    }
}

impl MemoryLifecycleSubjectAssociationRepository {
    pub async fn debug_list(&self) -> Vec<LifecycleSubjectAssociation> {
        self.associations.lock().await.clone()
    }
}

#[derive(Default)]
pub struct MemoryAgentLineageRepository {
    lineages: Mutex<Vec<AgentLineage>>,
}

#[async_trait::async_trait]
impl AgentLineageRepository for MemoryAgentLineageRepository {
    async fn create(&self, lineage: &AgentLineage) -> Result<(), DomainError> {
        self.lineages.lock().await.push(lineage.clone());
        Ok(())
    }

    async fn list_children(&self, agent_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
        Ok(self
            .lineages
            .lock()
            .await
            .iter()
            .filter(|lineage| lineage.parent_agent_id == Some(agent_id))
            .cloned()
            .collect())
    }

    async fn find_parent(&self, child_agent_id: Uuid) -> Result<Option<AgentLineage>, DomainError> {
        Ok(self
            .lineages
            .lock()
            .await
            .iter()
            .find(|lineage| lineage.child_agent_id == child_agent_id)
            .cloned())
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
        Ok(self
            .lineages
            .lock()
            .await
            .iter()
            .filter(|lineage| lineage.run_id == run_id)
            .cloned()
            .collect())
    }
}

#[derive(Default)]
pub struct MemoryLifecycleAgentRepository {
    agents: Mutex<Vec<LifecycleAgent>>,
}

#[async_trait::async_trait]
impl LifecycleAgentRepository for MemoryLifecycleAgentRepository {
    async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
        self.agents.lock().await.push(agent.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
        Ok(self
            .agents
            .lock()
            .await
            .iter()
            .find(|agent| agent.id == id)
            .cloned())
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
        Ok(self
            .agents
            .lock()
            .await
            .iter()
            .filter(|agent| agent.run_id == run_id)
            .cloned()
            .collect())
    }

    async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
        let mut agents = self.agents.lock().await;
        if let Some(existing) = agents.iter_mut().find(|item| item.id == agent.id) {
            *existing = agent.clone();
        }
        Ok(())
    }
}

impl MemoryLifecycleAgentRepository {
    pub async fn debug_list(&self) -> Vec<LifecycleAgent> {
        self.agents.lock().await.clone()
    }
}

#[derive(Default)]
pub struct MemoryProjectAgentRepository {
    agents: Mutex<Vec<ProjectAgent>>,
}

#[async_trait::async_trait]
impl ProjectAgentRepository for MemoryProjectAgentRepository {
    async fn create(&self, agent: &ProjectAgent) -> Result<(), DomainError> {
        self.agents.lock().await.push(agent.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectAgent>, DomainError> {
        Ok(self
            .agents
            .lock()
            .await
            .iter()
            .find(|agent| agent.id == id)
            .cloned())
    }

    async fn get_by_project_and_id(
        &self,
        project_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ProjectAgent>, DomainError> {
        Ok(self
            .agents
            .lock()
            .await
            .iter()
            .find(|agent| agent.project_id == project_id && agent.id == id)
            .cloned())
    }

    async fn get_by_project_and_name(
        &self,
        project_id: Uuid,
        name: &str,
    ) -> Result<Option<ProjectAgent>, DomainError> {
        Ok(self
            .agents
            .lock()
            .await
            .iter()
            .find(|agent| agent.project_id == project_id && agent.name == name)
            .cloned())
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ProjectAgent>, DomainError> {
        Ok(self
            .agents
            .lock()
            .await
            .iter()
            .filter(|agent| agent.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn update(&self, agent: &ProjectAgent) -> Result<(), DomainError> {
        let mut agents = self.agents.lock().await;
        if let Some(existing) = agents.iter_mut().find(|item| item.id == agent.id) {
            *existing = agent.clone();
        }
        Ok(())
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> Result<(), DomainError> {
        self.agents
            .lock()
            .await
            .retain(|agent| agent.project_id != project_id || agent.id != id);
        Ok(())
    }
}

#[derive(Default)]
pub struct MemoryProjectBackendAccessRepository {
    accesses: Mutex<Vec<ProjectBackendAccess>>,
}

#[async_trait::async_trait]
impl ProjectBackendAccessRepository for MemoryProjectBackendAccessRepository {
    async fn create(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
        self.accesses.lock().await.push(access.clone());
        Ok(())
    }

    async fn update(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
        let mut accesses = self.accesses.lock().await;
        if let Some(existing) = accesses.iter_mut().find(|item| item.id == access.id) {
            *existing = access.clone();
        }
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectBackendAccess>, DomainError> {
        Ok(self
            .accesses
            .lock()
            .await
            .iter()
            .find(|access| access.id == id)
            .cloned())
    }

    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
        Ok(self
            .accesses
            .lock()
            .await
            .iter()
            .filter(|access| access.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn list_active_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
        Ok(self
            .list_by_project(project_id)
            .await?
            .into_iter()
            .filter(|access| access.status == ProjectBackendAccessStatus::Active)
            .collect())
    }

    async fn get_active_for_project_backend(
        &self,
        project_id: Uuid,
        backend_id: &str,
    ) -> Result<Option<ProjectBackendAccess>, DomainError> {
        Ok(self
            .list_active_by_project(project_id)
            .await?
            .into_iter()
            .find(|access| access.backend_id == backend_id.trim()))
    }

    async fn list_active_by_backend(
        &self,
        backend_id: &str,
    ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
        Ok(self
            .accesses
            .lock()
            .await
            .iter()
            .filter(|access| {
                access.backend_id == backend_id.trim()
                    && access.status == ProjectBackendAccessStatus::Active
            })
            .cloned()
            .collect())
    }

    async fn list_active_by_backends(
        &self,
        backend_ids: &[String],
    ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
        Ok(self
            .accesses
            .lock()
            .await
            .iter()
            .filter(|access| {
                backend_ids.contains(&access.backend_id)
                    && access.status == ProjectBackendAccessStatus::Active
            })
            .cloned()
            .collect())
    }

    async fn set_status(
        &self,
        id: Uuid,
        status: ProjectBackendAccessStatus,
    ) -> Result<(), DomainError> {
        if let Some(access) = self
            .accesses
            .lock()
            .await
            .iter_mut()
            .find(|access| access.id == id)
        {
            access.status = status;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct MemoryAgentRunMailboxRepository {
    messages: Mutex<Vec<AgentRunMailboxMessage>>,
    states: Mutex<Vec<AgentRunMailboxState>>,
    cleaned: Mutex<Vec<Uuid>>,
}

impl MemoryAgentRunMailboxRepository {
    pub async fn messages_for(&self, run_id: Uuid, agent_id: Uuid) -> Vec<AgentRunMailboxMessage> {
        self.list_messages(run_id, agent_id)
            .await
            .unwrap_or_default()
    }
}

pub struct MemoryAgentRunMessageSubmissionStore {
    mailbox: Arc<MemoryAgentRunMailboxRepository>,
}

impl MemoryAgentRunMessageSubmissionStore {
    pub fn new(mailbox: Arc<MemoryAgentRunMailboxRepository>) -> Self {
        Self { mailbox }
    }
}

#[async_trait::async_trait]
impl AgentRunMailboxDeliverySettlementPort for MemoryAgentRunMessageSubmissionStore {
    async fn settle_delivery_failed(
        &self,
        failure: AgentRunMailboxFailedSettlement,
    ) -> Result<AgentRunMailboxDeliverySettlementResult, DomainError> {
        let now = Utc::now();
        let mut messages = self.mailbox.messages.lock().await;
        let message = messages
            .iter_mut()
            .find(|message| {
                message.id == failure.mailbox_message_id
                    && message.claim_token == Some(failure.claim_token)
                    && message.status == MailboxMessageStatus::Consuming
            })
            .ok_or_else(|| DomainError::Conflict {
                entity: "agent_run_mailbox_message",
                constraint: "delivery_failure_claim",
                message: "mailbox claim no longer owns delivery failure settlement".to_string(),
            })?;
        message.status = MailboxMessageStatus::Failed;
        message.reconcile_required = false;
        message.last_error = Some(failure.error_message);
        message.claim_token = None;
        message.claimed_at = None;
        message.claim_expires_at = None;
        message.consumed_at.get_or_insert(now);
        message.updated_at = now;
        if message.origin == MailboxMessageOrigin::User && !message.retain_payload {
            message.payload_json = None;
            message.launch_planning_input = None;
        }
        Ok(AgentRunMailboxDeliverySettlementResult {
            message: message.clone(),
        })
    }

    async fn settle_delivery_accepted(
        &self,
        settlement: AgentRunMailboxAcceptedSettlement,
    ) -> Result<AgentRunMailboxAcceptedSettlementResult, DomainError> {
        let operation_id = settlement
            .accepted_refs
            .runtime_operation_id
            .ok_or_else(|| {
                DomainError::InvalidConfig(
                    "accepted mailbox settlement requires runtime_operation_id".to_string(),
                )
            })?;
        let now = Utc::now();
        let mut messages = self.mailbox.messages.lock().await;
        let message = messages
            .iter_mut()
            .find(|message| {
                message.id == settlement.mailbox_message_id
                    && message.claim_token == Some(settlement.claim_token)
                    && message.status == MailboxMessageStatus::Consuming
            })
            .ok_or_else(|| DomainError::Conflict {
                entity: "agent_run_mailbox_message",
                constraint: "runtime_operation_claim",
                message: "mailbox claim no longer owns runtime operation acceptance".to_string(),
            })?;
        message.status = match settlement.delivery_kind {
            AgentRunAcceptedDeliveryKind::Started => MailboxMessageStatus::Dispatched,
            AgentRunAcceptedDeliveryKind::Steered => MailboxMessageStatus::Steered,
        };
        message.accepted_runtime_operation_id = Some(operation_id);
        message.reconcile_required = false;
        message.last_error = None;
        message.claim_token = None;
        message.claimed_at = None;
        message.claim_expires_at = None;
        message.consumed_at.get_or_insert(now);
        message.updated_at = now;
        if message.origin == MailboxMessageOrigin::User && !message.retain_payload {
            message.payload_json = None;
            message.launch_planning_input = None;
        }
        Ok(AgentRunMailboxAcceptedSettlementResult {
            message: message.clone(),
        })
    }
}

#[async_trait::async_trait]
impl AgentRunMailboxRepository for MemoryAgentRunMailboxRepository {
    async fn list_pending_targets(&self) -> Result<Vec<(Uuid, Uuid)>, DomainError> {
        let mut targets = self
            .messages
            .lock()
            .await
            .iter()
            .filter(|message| {
                matches!(
                    message.status,
                    MailboxMessageStatus::Accepted
                        | MailboxMessageStatus::Queued
                        | MailboxMessageStatus::ReadyToConsume
                        | MailboxMessageStatus::Consuming
                )
            })
            .map(|message| (message.run_id, message.agent_id))
            .collect::<Vec<_>>();
        targets.sort_unstable();
        targets.dedup();
        Ok(targets)
    }

    async fn create_message(
        &self,
        message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let message = mailbox_message_from_new(message);
        self.messages.lock().await.push(message.clone());
        Ok(message)
    }

    async fn create_message_idempotent(
        &self,
        message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxCreateOutcome, DomainError> {
        if let Some(dedup_key) = message.source_dedup_key.as_deref()
            && let Some(existing) = self.messages.lock().await.iter().find(|existing| {
                existing.run_id == message.run_id
                    && existing.agent_id == message.agent_id
                    && existing.source_dedup_key.as_deref() == Some(dedup_key)
            })
        {
            if existing.delivery_request_digest != message.delivery_request_digest {
                return Err(DomainError::Conflict {
                    entity: "agent_run_mailbox_message",
                    constraint: "source_dedup_request_digest",
                    message:
                        "mailbox source_dedup_key is already bound to a different delivery request"
                            .to_string(),
                });
            }
            return Ok(AgentRunMailboxCreateOutcome::Existing(existing.clone()));
        }
        self.create_message(message)
            .await
            .map(AgentRunMailboxCreateOutcome::Created)
    }

    async fn get_message(&self, id: Uuid) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        Ok(self
            .messages
            .lock()
            .await
            .iter()
            .find(|message| message.id == id)
            .cloned())
    }

    async fn list_messages(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        Ok(self
            .messages
            .lock()
            .await
            .iter()
            .filter(|message| message.run_id == run_id && message.agent_id == agent_id)
            .cloned()
            .collect())
    }

    async fn claim_next(
        &self,
        request: AgentRunMailboxClaimRequest,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
        if self.states.lock().await.iter().any(|state| {
            state.run_id == request.run_id && state.agent_id == request.agent_id && state.paused
        }) {
            return Ok(Vec::new());
        }
        let mut messages = self.messages.lock().await;
        let mut claimed = Vec::new();
        for message in messages.iter_mut() {
            if claimed.len() >= request.limit as usize {
                break;
            }
            if message.run_id != request.run_id
                || message.agent_id != request.agent_id
                || !request.barriers.contains(&message.barrier)
                || request
                    .drain_mode
                    .is_some_and(|mode| mode != message.drain_mode)
                || message.reconcile_required
                || !matches!(
                    message.status,
                    MailboxMessageStatus::Queued | MailboxMessageStatus::ReadyToConsume
                )
            {
                continue;
            }
            message.status = MailboxMessageStatus::Consuming;
            message.claim_token = Some(request.claim_token);
            message.claimed_at = Some(Utc::now());
            message.claim_expires_at = Some(request.claim_expires_at);
            message.attempt_count += 1;
            claimed.push(message.clone());
        }
        Ok(claimed)
    }

    async fn claim_reconciliation(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        claim_token: Uuid,
        claim_expires_at: chrono::DateTime<Utc>,
    ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        if self
            .states
            .lock()
            .await
            .iter()
            .any(|state| state.run_id == run_id && state.agent_id == agent_id && state.paused)
        {
            return Ok(None);
        }
        let now = Utc::now();
        let mut messages = self.messages.lock().await;
        let Some(message) = messages
            .iter_mut()
            .filter(|message| {
                message.run_id == run_id
                    && message.agent_id == agent_id
                    && message.status == MailboxMessageStatus::Queued
                    && message.reconcile_required
            })
            .max_by_key(|message| (message.priority, -message.order_key))
        else {
            return Ok(None);
        };
        message.status = MailboxMessageStatus::Consuming;
        message.claim_token = Some(claim_token);
        message.claimed_at = Some(now);
        message.claim_expires_at = Some(claim_expires_at);
        message.attempt_count += 1;
        message.updated_at = now;
        Ok(Some(message.clone()))
    }

    async fn release_reconciliation_claim(
        &self,
        id: Uuid,
        claim_token: Uuid,
        last_error: String,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let mut messages = self.messages.lock().await;
        let message = messages
            .iter_mut()
            .find(|message| {
                message.id == id
                    && message.claim_token == Some(claim_token)
                    && message.status == MailboxMessageStatus::Consuming
                    && message.reconcile_required
            })
            .ok_or_else(|| DomainError::Conflict {
                entity: "agent_run_mailbox_message",
                constraint: "reconciliation_claim",
                message: "mailbox reconciliation claim is no longer owned".to_string(),
            })?;
        message.status = MailboxMessageStatus::Queued;
        message.claim_token = None;
        message.claimed_at = None;
        message.claim_expires_at = None;
        message.last_error = Some(last_error);
        message.updated_at = Utc::now();
        Ok(message.clone())
    }

    async fn recover_expired_consuming(
        &self,
        now: chrono::DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        let mut messages = self.messages.lock().await;
        let mut recovered = 0;
        for message in messages.iter_mut().filter(|message| {
            message.status == MailboxMessageStatus::Consuming
                && message
                    .claim_expires_at
                    .is_some_and(|expires_at| expires_at < now)
        }) {
            message.status = MailboxMessageStatus::Queued;
            message.reconcile_required = true;
            message.claim_token = None;
            message.claimed_at = None;
            message.claim_expires_at = None;
            message.last_error = Some(MAILBOX_CLAIM_LEASE_EXPIRED_RECONCILIATION.to_string());
            message.updated_at = now;
            recovered += 1;
        }
        Ok(recovered)
    }

    async fn mark_message_status(
        &self,
        id: Uuid,
        claim_token: Option<Uuid>,
        status: MailboxMessageStatus,
        last_error: Option<String>,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let mut messages = self.messages.lock().await;
        let message = messages
            .iter_mut()
            .find(|message| message.id == id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_mailbox_message",
                id: id.to_string(),
            })?;
        if message.claim_token != claim_token {
            return Err(DomainError::Conflict {
                entity: "agent_run_mailbox_message",
                constraint: "claim_token",
                message: "claim token mismatch".to_string(),
            });
        }
        message.status = status;
        message.last_error = last_error;
        message.reconcile_required = false;
        message.claim_token = None;
        message.claimed_at = None;
        message.claim_expires_at = None;
        let now = Utc::now();
        if matches!(
            status,
            MailboxMessageStatus::Dispatched
                | MailboxMessageStatus::Steered
                | MailboxMessageStatus::Failed
                | MailboxMessageStatus::Deleted
        ) {
            message.consumed_at.get_or_insert(now);
        }
        message.updated_at = now;
        Ok(message.clone())
    }

    async fn promote_message(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        id: Uuid,
        priority: i32,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let mut messages = self.messages.lock().await;
        let message = messages
            .iter_mut()
            .find(|message| {
                message.id == id && message.run_id == run_id && message.agent_id == agent_id
            })
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_mailbox_message",
                id: id.to_string(),
            })?;
        if message.origin != MailboxMessageOrigin::User
            || message.delivery != MailboxDelivery::LaunchOrContinueTurn
            || !matches!(
                message.status,
                MailboxMessageStatus::Accepted
                    | MailboxMessageStatus::Queued
                    | MailboxMessageStatus::ReadyToConsume
                    | MailboxMessageStatus::Paused
                    | MailboxMessageStatus::Blocked
            )
            || message.reconcile_required
        {
            return Err(DomainError::Conflict {
                entity: "agent_run_mailbox_message",
                constraint: "promotable_state",
                message: "mailbox message is not promotable".to_string(),
            });
        }
        message.delivery = MailboxDelivery::SteerActiveTurn {
            stop_effect: SteeringStopEffect::None,
        };
        message.barrier = ConsumptionBarrier::AgentLoopTurnBoundary;
        message.drain_mode = MailboxDrainMode::All;
        message.priority = priority;
        message.updated_at = Utc::now();
        Ok(message.clone())
    }

    async fn delete_message(
        &self,
        id: Uuid,
    ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
        let mut messages = self.messages.lock().await;
        if let Some(message) = messages.iter_mut().find(|message| message.id == id) {
            message.status = MailboxMessageStatus::Deleted;
            message.deleted_at = Some(Utc::now());
            return Ok(Some(message.clone()));
        }
        Ok(None)
    }

    async fn cleanup_user_payload(&self, id: Uuid) -> Result<(), DomainError> {
        self.cleaned.lock().await.push(id);
        if let Some(message) = self
            .messages
            .lock()
            .await
            .iter_mut()
            .find(|message| message.id == id)
        {
            message.payload_json = None;
            message.launch_planning_input = None;
        }
        Ok(())
    }

    async fn pause_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        reason: String,
        message: Option<String>,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let state = AgentRunMailboxState {
            run_id,
            agent_id,
            paused: true,
            pause_reason: Some(reason),
            pause_message: message,
            updated_at: Utc::now(),
        };
        self.upsert_state(state.clone()).await;
        Ok(state)
    }

    async fn resume_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let state = AgentRunMailboxState {
            run_id,
            agent_id,
            paused: false,
            pause_reason: None,
            pause_message: None,
            updated_at: Utc::now(),
        };
        self.upsert_state(state.clone()).await;
        Ok(state)
    }

    async fn get_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<AgentRunMailboxState>, DomainError> {
        Ok(self
            .states
            .lock()
            .await
            .iter()
            .find(|state| state.run_id == run_id && state.agent_id == agent_id)
            .cloned())
    }

    async fn move_message_after(
        &self,
        id: Uuid,
        _after_id: Option<Uuid>,
        _run_id: Uuid,
        _agent_id: Uuid,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        self.get_message(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_mailbox_message",
                id: id.to_string(),
            })
    }
}

impl MemoryAgentRunMailboxRepository {
    async fn upsert_state(&self, state: AgentRunMailboxState) {
        let mut states = self.states.lock().await;
        if let Some(existing) = states
            .iter_mut()
            .find(|item| item.run_id == state.run_id && item.agent_id == state.agent_id)
        {
            *existing = state;
        } else {
            states.push(state);
        }
    }
}

fn mailbox_message_from_new(message: NewAgentRunMailboxMessage) -> AgentRunMailboxMessage {
    let now = Utc::now();
    AgentRunMailboxMessage {
        id: message.id.unwrap_or_else(Uuid::new_v4),
        run_id: message.run_id,
        agent_id: message.agent_id,
        origin: message.origin,
        source: message.source,
        delivery: message.delivery,
        barrier: message.barrier,
        drain_mode: message.drain_mode,
        status: MailboxMessageStatus::Queued,
        priority: message.priority,
        order_key: now.timestamp_micros(),
        source_dedup_key: message.source_dedup_key,
        delivery_request_digest: message.delivery_request_digest,
        accepted_runtime_operation_id: None,
        reconcile_required: false,
        claim_token: None,
        claimed_at: None,
        claim_expires_at: None,
        payload_json: message.payload_json,
        launch_planning_input: message.launch_planning_input,
        preview: message.preview,
        has_images: message.has_images,
        retain_payload: message.retain_payload,
        attempt_count: 0,
        last_error: None,
        created_at: now,
        updated_at: now,
        consumed_at: None,
        deleted_at: None,
    }
}

#[derive(Default)]
pub struct MemoryLifecycleGateRepository {
    gates: Mutex<Vec<LifecycleGate>>,
}

#[async_trait::async_trait]
impl LifecycleGateRepository for MemoryLifecycleGateRepository {
    async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        self.gates.lock().await.push(gate.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .await
            .iter()
            .find(|gate| gate.id == id)
            .cloned())
    }

    async fn list_open_for_agent(&self, agent_id: Uuid) -> Result<Vec<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .await
            .iter()
            .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
            .cloned()
            .collect())
    }

    async fn list_open_gate_wait_policies(
        &self,
        limit: usize,
    ) -> Result<Vec<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .await
            .iter()
            .filter(|gate| {
                gate.is_open()
                    && gate
                        .payload_json
                        .as_ref()
                        .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                        .is_some()
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn list_by_wait_producer(
        &self,
        producer: &WaitProducerRef,
    ) -> Result<Vec<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .await
            .iter()
            .filter(|gate| {
                gate.payload_json
                    .as_ref()
                    .and_then(GateWaitPolicyEnvelope::from_payload_opt)
                    .is_some_and(|declaration| declaration.wait_policy.source == *producer)
            })
            .cloned()
            .collect())
    }

    async fn find_by_agent_and_correlation(
        &self,
        agent_id: Uuid,
        correlation_id: &str,
    ) -> Result<Option<LifecycleGate>, DomainError> {
        Ok(self
            .gates
            .lock()
            .await
            .iter()
            .find(|gate| gate.agent_id == Some(agent_id) && gate.correlation_id == correlation_id)
            .cloned())
    }

    async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        let mut gates = self.gates.lock().await;
        let existing = gates
            .iter_mut()
            .find(|existing| existing.id == gate.id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "lifecycle_gate",
                id: gate.id.to_string(),
            })?;
        *existing = gate.clone();
        Ok(())
    }
}

impl MemoryLifecycleGateRepository {
    pub async fn debug_list(&self) -> Vec<LifecycleGate> {
        self.gates.lock().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[tokio::test]
    async fn current_frame_uses_revision_then_created_at() {
        let repo = MemoryAgentFrameRepository::default();
        let agent_id = Uuid::new_v4();
        let other_agent_id = Uuid::new_v4();
        let base = Utc::now();

        let mut older_high_revision = AgentFrame::new_revision(agent_id, 2, "test");
        older_high_revision.created_at = base + TimeDelta::seconds(1);
        let older_high_revision_id = older_high_revision.id;

        let mut lower_revision_newer_time = AgentFrame::new_revision(agent_id, 1, "test");
        lower_revision_newer_time.created_at = base + TimeDelta::seconds(3);

        let mut latest_high_revision = AgentFrame::new_revision(agent_id, 2, "test");
        latest_high_revision.created_at = base + TimeDelta::seconds(4);
        let latest_high_revision_id = latest_high_revision.id;

        let mut other_agent_frame = AgentFrame::new_revision(other_agent_id, 9, "test");
        other_agent_frame.created_at = base + TimeDelta::seconds(9);

        repo.create(&older_high_revision).await.unwrap();
        repo.create(&lower_revision_newer_time).await.unwrap();
        repo.create(&latest_high_revision).await.unwrap();
        repo.create(&other_agent_frame).await.unwrap();

        let current = repo.get_current(agent_id).await.unwrap().unwrap();

        assert_eq!(current.id, latest_high_revision_id);
        assert_ne!(current.id, older_high_revision_id);
        assert_eq!(current.revision, 2);
    }
}
