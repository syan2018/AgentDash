use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::agent_run_fork_materialization::{
    AgentRunForkMaterializationError, AgentRunForkMaterializationInput,
    AgentRunForkMaterializationPort, AgentRunForkMaterializationResult,
};
use agentdash_domain::DomainError;
use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxClaimRequest, AgentRunMailboxMessage, AgentRunMailboxRepository,
    AgentRunMailboxState, ConsumptionBarrier, MailboxDelivery, MailboxDrainMode,
    MailboxMessageStatus, NewAgentRunMailboxMessage,
};
use agentdash_domain::backend::{
    ProjectBackendAccess, ProjectBackendAccessRepository, ProjectBackendAccessStatus,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentLineage, AgentLineageRepository, AgentProcedure,
    AgentProcedureRepository, AgentRunCommandClaim, AgentRunCommandReceipt,
    AgentRunCommandReceiptRepository, AgentRunCommandStatus, AgentRunDeliveryBinding,
    AgentRunDeliveryBindingRepository, AgentRunLineage, AgentRunLineageRepository,
    DeliveryBindingStatus, GateWaitPolicyEnvelope, LifecycleAgent, LifecycleAgentRepository,
    LifecycleGate, LifecycleGateRepository, LifecycleRun, LifecycleRunRepository,
    LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository,
    ManualContextCompactionRequest, ManualContextCompactionRequestRepository,
    ManualContextCompactionRequestStatus, NewAgentRunCommandReceipt,
    NewManualContextCompactionRequest, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository, SubjectRef, WaitProducerRef, WorkflowGraph,
    WorkflowGraphRepository,
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
            *existing = run.clone();
        }
        Ok(())
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

pub struct MemoryAgentRunForkMaterialization {
    runs: Arc<MemoryLifecycleRunRepository>,
    agents: Arc<MemoryLifecycleAgentRepository>,
    frames: Arc<MemoryAgentFrameRepository>,
    anchors: Arc<MemoryRuntimeSessionExecutionAnchorRepository>,
    delivery_bindings: Arc<MemoryAgentRunDeliveryBindingRepository>,
    lineages: Arc<MemoryAgentRunLineageRepository>,
    fail_message: Mutex<Option<String>>,
}

impl MemoryAgentRunForkMaterialization {
    pub fn new(
        runs: Arc<MemoryLifecycleRunRepository>,
        agents: Arc<MemoryLifecycleAgentRepository>,
        frames: Arc<MemoryAgentFrameRepository>,
        anchors: Arc<MemoryRuntimeSessionExecutionAnchorRepository>,
        delivery_bindings: Arc<MemoryAgentRunDeliveryBindingRepository>,
        lineages: Arc<MemoryAgentRunLineageRepository>,
    ) -> Self {
        Self {
            runs,
            agents,
            frames,
            anchors,
            delivery_bindings,
            lineages,
            fail_message: Mutex::new(None),
        }
    }

    pub async fn fail_next(&self, message: impl Into<String>) {
        *self.fail_message.lock().await = Some(message.into());
    }
}

#[async_trait::async_trait]
impl AgentRunForkMaterializationPort for MemoryAgentRunForkMaterialization {
    async fn materialize_forked_agent_run(
        &self,
        input: AgentRunForkMaterializationInput,
    ) -> Result<AgentRunForkMaterializationResult, AgentRunForkMaterializationError> {
        if let Some(message) = self.fail_message.lock().await.take() {
            return Err(AgentRunForkMaterializationError::Internal { message });
        }

        let child_runtime_session_id = input.child_runtime_session_id.clone();
        let mut child_run =
            LifecycleRun::new_plain_for_user(input.parent_run.project_id, &input.forked_by_user_id);
        child_run.topology = input.parent_run.topology;

        let mut child_agent = LifecycleAgent::new_root_for_user(
            child_run.id,
            child_run.project_id,
            input.parent_agent.source,
            &input.forked_by_user_id,
        )
        .with_bootstrap_status(&input.parent_agent.bootstrap_status);
        child_agent.project_agent_id = input.parent_agent.project_agent_id;

        let mut child_frame = AgentFrame::new_revision(
            child_agent.id,
            input.parent_frame.revision,
            "agent_run_fork",
        );
        child_frame.effective_capability_json =
            input.parent_frame.effective_capability_json.clone();
        child_frame.context_slice_json = input.parent_frame.context_slice_json.clone();
        child_frame.vfs_surface_json = input.parent_frame.vfs_surface_json.clone();
        child_frame.mcp_surface_json = input.parent_frame.mcp_surface_json.clone();
        child_frame.execution_profile_json = input.parent_frame.execution_profile_json.clone();
        child_frame.visible_canvas_mount_ids_json =
            input.parent_frame.visible_canvas_mount_ids_json.clone();
        child_frame.visible_workspace_module_refs_json = input
            .parent_frame
            .visible_workspace_module_refs_json
            .clone();
        child_frame.created_by_id = Some(input.forked_by_user_id.clone());

        let mut anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            child_runtime_session_id.clone(),
            child_run.id,
            child_frame.id,
            child_agent.id,
        );
        anchor.created_by_kind = "agent_run_fork".to_string();
        let delivery_binding = AgentRunDeliveryBinding::from_anchor(
            &anchor,
            DeliveryBindingStatus::Ready,
            anchor.updated_at,
        );

        let lineage = AgentRunLineage::new_fork(
            input.parent_run.id,
            input.parent_agent.id,
            child_run.id,
            child_agent.id,
            input.fork_point_event_seq,
            input.fork_point_ref_json,
            input.forked_by_user_id,
            input.metadata_json,
        )
        .with_frame_baseline(
            input.parent_frame.id,
            input.parent_frame.revision,
            child_frame.id,
            child_frame.revision,
        );

        self.runs
            .create(&child_run)
            .await
            .map_err(materialization_error)?;
        self.agents
            .create(&child_agent)
            .await
            .map_err(materialization_error)?;
        self.frames
            .create(&child_frame)
            .await
            .map_err(materialization_error)?;
        self.anchors
            .create_once(&anchor)
            .await
            .map_err(materialization_error)?;
        self.delivery_bindings
            .upsert(&delivery_binding)
            .await
            .map_err(materialization_error)?;
        self.lineages
            .create(&lineage)
            .await
            .map_err(materialization_error)?;

        Ok(AgentRunForkMaterializationResult {
            child_run,
            child_agent,
            child_frame,
            child_runtime_session_id,
            lineage,
        })
    }
}

fn materialization_error(error: DomainError) -> AgentRunForkMaterializationError {
    AgentRunForkMaterializationError::Internal {
        message: error.to_string(),
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
        outcome.runtime_session_id = Some(runtime_session_id);
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
pub struct MemoryRuntimeSessionExecutionAnchorRepository {
    anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
}

#[async_trait::async_trait]
impl RuntimeSessionExecutionAnchorRepository for MemoryRuntimeSessionExecutionAnchorRepository {
    async fn create_once(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
        let mut anchors = self.anchors.lock().await;
        if let Some(existing) = anchors
            .iter()
            .find(|item| item.runtime_session_id == anchor.runtime_session_id)
        {
            if existing.has_same_launch_coordinates_as(anchor) {
                return Ok(());
            }
            return Err(existing.immutable_conflict(anchor));
        }
        anchors.push(anchor.clone());
        Ok(())
    }

    async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
        self.anchors
            .lock()
            .await
            .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
        Ok(())
    }

    async fn find_by_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(self
            .anchors
            .lock()
            .await
            .iter()
            .find(|anchor| anchor.runtime_session_id == runtime_session_id)
            .cloned())
    }

    async fn list_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(self
            .anchors
            .lock()
            .await
            .iter()
            .filter(|anchor| anchor.run_id == run_id)
            .cloned()
            .collect())
    }

    async fn list_by_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(self
            .anchors
            .lock()
            .await
            .iter()
            .filter(|anchor| anchor.agent_id == agent_id)
            .cloned()
            .collect())
    }

    async fn list_by_project_session_ids(
        &self,
        runtime_session_ids: &[String],
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(self
            .anchors
            .lock()
            .await
            .iter()
            .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
            .cloned()
            .collect())
    }
}

impl MemoryRuntimeSessionExecutionAnchorRepository {
    pub async fn debug_list(&self) -> Vec<RuntimeSessionExecutionAnchor> {
        self.anchors.lock().await.clone()
    }
}

#[derive(Default)]
pub struct MemoryAgentRunDeliveryBindingRepository {
    bindings: Mutex<Vec<AgentRunDeliveryBinding>>,
}

#[async_trait::async_trait]
impl AgentRunDeliveryBindingRepository for MemoryAgentRunDeliveryBindingRepository {
    async fn upsert(&self, binding: &AgentRunDeliveryBinding) -> Result<(), DomainError> {
        let mut bindings = self.bindings.lock().await;
        if let Some(existing) = bindings
            .iter_mut()
            .find(|item| item.run_id == binding.run_id && item.agent_id == binding.agent_id)
        {
            *existing = binding.clone();
        } else {
            bindings.push(binding.clone());
        }
        Ok(())
    }

    async fn get_current(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<AgentRunDeliveryBinding>, DomainError> {
        Ok(self
            .bindings
            .lock()
            .await
            .iter()
            .find(|binding| binding.run_id == run_id && binding.agent_id == agent_id)
            .cloned())
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentRunDeliveryBinding>, DomainError> {
        Ok(self
            .bindings
            .lock()
            .await
            .iter()
            .filter(|binding| binding.run_id == run_id)
            .cloned()
            .collect())
    }

    async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
        self.bindings
            .lock()
            .await
            .retain(|binding| binding.runtime_session_id != runtime_session_id);
        Ok(())
    }
}

impl MemoryAgentRunDeliveryBindingRepository {
    pub async fn debug_list(&self) -> Vec<AgentRunDeliveryBinding> {
        self.bindings.lock().await.clone()
    }
}

#[derive(Default)]
pub struct MemoryAgentRunCommandReceiptRepository {
    receipts: Mutex<Vec<AgentRunCommandReceipt>>,
}

#[async_trait::async_trait]
impl AgentRunCommandReceiptRepository for MemoryAgentRunCommandReceiptRepository {
    async fn claim(
        &self,
        receipt: NewAgentRunCommandReceipt,
    ) -> Result<AgentRunCommandClaim, DomainError> {
        let mut receipts = self.receipts.lock().await;
        if let Some(existing) = receipts.iter().find(|item| {
            item.scope_kind == receipt.scope_kind
                && item.scope_key == receipt.scope_key
                && item.client_command_id == receipt.client_command_id
        }) {
            if existing.request_digest != receipt.request_digest {
                return Err(DomainError::Conflict {
                    entity: "agent_run_command_receipt",
                    constraint: "request_digest",
                    message: format!(
                        "client_command_id `{}` 已用于不同请求",
                        receipt.client_command_id
                    ),
                });
            }
            return Ok(AgentRunCommandClaim::Duplicate(existing.clone()));
        }

        let now = Utc::now();
        let record = AgentRunCommandReceipt {
            id: Uuid::new_v4(),
            scope_kind: receipt.scope_kind,
            scope_key: receipt.scope_key,
            command_kind: receipt.command_kind,
            client_command_id: receipt.client_command_id,
            request_digest: receipt.request_digest,
            status: AgentRunCommandStatus::Pending,
            mailbox_message_id: None,
            accepted_refs: None,
            result_json: None,
            error_message: None,
            created_at: now,
            updated_at: now,
            accepted_at: None,
            failed_at: None,
        };
        receipts.push(record.clone());
        Ok(AgentRunCommandClaim::Created(record))
    }

    async fn mark_accepted(
        &self,
        id: Uuid,
        accepted_refs: agentdash_domain::workflow::AgentRunAcceptedRefs,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let mut receipts = self.receipts.lock().await;
        let record = receipts
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_command_receipt",
                id: id.to_string(),
            })?;
        record.status = AgentRunCommandStatus::Accepted;
        record.accepted_refs = Some(accepted_refs);
        record.error_message = None;
        record.updated_at = Utc::now();
        record.accepted_at = Some(record.updated_at);
        record.failed_at = None;
        Ok(record.clone())
    }

    async fn attach_mailbox_message(
        &self,
        id: Uuid,
        mailbox_message_id: Uuid,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let mut receipts = self.receipts.lock().await;
        let record = receipts
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_command_receipt",
                id: id.to_string(),
            })?;
        record.mailbox_message_id = Some(mailbox_message_id);
        record.updated_at = Utc::now();
        Ok(record.clone())
    }

    async fn store_result_json(
        &self,
        id: Uuid,
        result_json: serde_json::Value,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let mut receipts = self.receipts.lock().await;
        let record = receipts
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_command_receipt",
                id: id.to_string(),
            })?;
        record.result_json = Some(result_json);
        record.updated_at = Utc::now();
        Ok(record.clone())
    }

    async fn mark_terminal_failed(
        &self,
        id: Uuid,
        error_message: String,
    ) -> Result<AgentRunCommandReceipt, DomainError> {
        let mut receipts = self.receipts.lock().await;
        let record = receipts
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_command_receipt",
                id: id.to_string(),
            })?;
        record.status = AgentRunCommandStatus::TerminalFailed;
        record.error_message = Some(error_message);
        record.updated_at = Utc::now();
        record.failed_at = Some(record.updated_at);
        Ok(record.clone())
    }

    async fn get(&self, id: Uuid) -> Result<Option<AgentRunCommandReceipt>, DomainError> {
        Ok(self
            .receipts
            .lock()
            .await
            .iter()
            .find(|item| item.id == id)
            .cloned())
    }
}

impl MemoryAgentRunCommandReceiptRepository {
    pub async fn debug_list(&self) -> Vec<AgentRunCommandReceipt> {
        self.receipts.lock().await.clone()
    }
}

#[derive(Default)]
pub struct MemoryManualContextCompactionRequestRepository {
    requests: Mutex<Vec<ManualContextCompactionRequest>>,
}

#[async_trait::async_trait]
impl ManualContextCompactionRequestRepository for MemoryManualContextCompactionRequestRepository {
    async fn create_requested(
        &self,
        request: NewManualContextCompactionRequest,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        let mut requests = self.requests.lock().await;
        if let Some(existing) = requests
            .iter()
            .find(|item| item.command_receipt_id == request.command_receipt_id)
        {
            return Ok(existing.clone());
        }
        if requests.iter().any(|item| {
            item.session_id == request.session_id
                && item.status == ManualContextCompactionRequestStatus::Requested
        }) {
            return Err(DomainError::Conflict {
                entity: "runtime_session_compaction_request",
                constraint: "requested_session",
                message: "runtime session already has a pending context compact request"
                    .to_string(),
            });
        }

        let now = Utc::now();
        let record = ManualContextCompactionRequest {
            id: Uuid::new_v4(),
            session_id: request.session_id,
            run_id: request.run_id,
            agent_id: request.agent_id,
            command_receipt_id: request.command_receipt_id,
            status: ManualContextCompactionRequestStatus::Requested,
            requested_mode: request.requested_mode,
            keep_last_n: request.keep_last_n,
            reserve_tokens: request.reserve_tokens,
            request_metadata: request.request_metadata,
            result_metadata: None,
            requested_at: now,
            updated_at: now,
            consumed_turn_id: None,
            completed_compaction_id: None,
            compacted_until_ref: None,
            first_kept_ref: None,
        };
        requests.push(record.clone());
        Ok(record)
    }

    async fn get_by_command_receipt(
        &self,
        command_receipt_id: Uuid,
    ) -> Result<Option<ManualContextCompactionRequest>, DomainError> {
        Ok(self
            .requests
            .lock()
            .await
            .iter()
            .find(|item| item.command_receipt_id == command_receipt_id)
            .cloned())
    }

    async fn get_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<ManualContextCompactionRequest>, DomainError> {
        Ok(self
            .requests
            .lock()
            .await
            .iter()
            .find(|item| item.id == id)
            .cloned())
    }

    async fn find_requested_by_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ManualContextCompactionRequest>, DomainError> {
        Ok(self
            .requests
            .lock()
            .await
            .iter()
            .find(|item| {
                item.session_id == session_id
                    && item.status == ManualContextCompactionRequestStatus::Requested
            })
            .cloned())
    }

    async fn mark_consumed(
        &self,
        id: Uuid,
        turn_id: String,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        self.update(id, |request| {
            request.status = ManualContextCompactionRequestStatus::Consumed;
            request.consumed_turn_id = Some(turn_id);
        })
        .await
    }

    async fn mark_completed(
        &self,
        id: Uuid,
        compaction_id: String,
        compacted_until_ref: Option<serde_json::Value>,
        first_kept_ref: Option<serde_json::Value>,
        result_metadata: Option<serde_json::Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        self.update(id, |request| {
            request.status = ManualContextCompactionRequestStatus::Completed;
            request.completed_compaction_id = Some(compaction_id);
            request.compacted_until_ref = compacted_until_ref;
            request.first_kept_ref = first_kept_ref;
            request.result_metadata = result_metadata;
        })
        .await
    }

    async fn mark_noop(
        &self,
        id: Uuid,
        result_metadata: Option<serde_json::Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        self.update(id, |request| {
            request.status = ManualContextCompactionRequestStatus::Noop;
            request.result_metadata = result_metadata;
        })
        .await
    }

    async fn mark_failed(
        &self,
        id: Uuid,
        result_metadata: Option<serde_json::Value>,
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        self.update(id, |request| {
            request.status = ManualContextCompactionRequestStatus::Failed;
            request.result_metadata = result_metadata;
        })
        .await
    }
}

impl MemoryManualContextCompactionRequestRepository {
    async fn update(
        &self,
        id: Uuid,
        apply: impl FnOnce(&mut ManualContextCompactionRequest),
    ) -> Result<ManualContextCompactionRequest, DomainError> {
        let mut requests = self.requests.lock().await;
        let request = requests
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "runtime_session_compaction_request",
                id: id.to_string(),
            })?;
        apply(request);
        request.updated_at = Utc::now();
        Ok(request.clone())
    }

    pub async fn debug_list(&self) -> Vec<ManualContextCompactionRequest> {
        self.requests.lock().await.clone()
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

#[async_trait::async_trait]
impl AgentRunMailboxRepository for MemoryAgentRunMailboxRepository {
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
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        if let Some(dedup_key) = message.source_dedup_key.as_deref()
            && let Some(existing) = self.messages.lock().await.iter().find(|existing| {
                existing.run_id == message.run_id
                    && existing.agent_id == message.agent_id
                    && existing.source_dedup_key.as_deref() == Some(dedup_key)
            })
        {
            return Ok(existing.clone());
        }
        self.create_message(message).await
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
                || !matches!(
                    message.status,
                    MailboxMessageStatus::Queued | MailboxMessageStatus::ReadyToConsume
                )
            {
                continue;
            }
            if let Some(runtime_session_id) = request.delivery_runtime_session_id.clone() {
                message.delivery_runtime_session_id = Some(runtime_session_id);
            }
            message.status = MailboxMessageStatus::Consuming;
            message.claim_token = Some(request.claim_token);
            message.claim_expires_at = Some(request.claim_expires_at);
            message.attempt_count += 1;
            claimed.push(message.clone());
        }
        Ok(claimed)
    }

    async fn recover_expired_consuming(
        &self,
        _now: chrono::DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        Ok(0)
    }

    async fn mark_message_status(
        &self,
        id: Uuid,
        claim_token: Option<Uuid>,
        status: MailboxMessageStatus,
        accepted_agent_run_turn_id: Option<String>,
        accepted_protocol_turn_id: Option<String>,
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
        message.accepted_agent_run_turn_id = accepted_agent_run_turn_id;
        message.accepted_protocol_turn_id = accepted_protocol_turn_id;
        message.last_error = last_error;
        message.claim_token = None;
        message.claim_expires_at = None;
        message.consumed_at = Some(Utc::now());
        message.updated_at = Utc::now();
        Ok(message.clone())
    }

    async fn update_message_policy(
        &self,
        id: Uuid,
        delivery: MailboxDelivery,
        barrier: ConsumptionBarrier,
        drain_mode: MailboxDrainMode,
        priority: i32,
    ) -> Result<AgentRunMailboxMessage, DomainError> {
        let mut messages = self.messages.lock().await;
        let message = messages
            .iter_mut()
            .find(|message| message.id == id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_run_mailbox_message",
                id: id.to_string(),
            })?;
        message.delivery = delivery;
        message.barrier = barrier;
        message.drain_mode = drain_mode;
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
        }
        Ok(())
    }

    async fn pause_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: Option<String>,
        reason: String,
        message: Option<String>,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let state = AgentRunMailboxState {
            run_id,
            agent_id,
            delivery_runtime_session_id: runtime_session_id,
            paused: true,
            pause_reason: Some(reason),
            pause_message: message,
            backend_selection_preference: None,
            updated_at: Utc::now(),
        };
        self.upsert_state(state.clone()).await;
        Ok(state)
    }

    async fn resume_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: Option<String>,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let state = AgentRunMailboxState {
            run_id,
            agent_id,
            delivery_runtime_session_id: runtime_session_id,
            paused: false,
            pause_reason: None,
            pause_message: None,
            backend_selection_preference: None,
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

    async fn set_backend_selection_preference(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: Option<String>,
        preference: serde_json::Value,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let mut state = self
            .get_state(run_id, agent_id)
            .await?
            .unwrap_or(AgentRunMailboxState {
                run_id,
                agent_id,
                delivery_runtime_session_id: runtime_session_id,
                paused: false,
                pause_reason: None,
                pause_message: None,
                backend_selection_preference: None,
                updated_at: Utc::now(),
            });
        state.backend_selection_preference = Some(preference);
        state.updated_at = Utc::now();
        self.upsert_state(state.clone()).await;
        Ok(state)
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
        id: Uuid::new_v4(),
        run_id: message.run_id,
        agent_id: message.agent_id,
        delivery_runtime_session_id: message.delivery_runtime_session_id,
        origin: message.origin,
        source: message.source,
        delivery: message.delivery,
        barrier: message.barrier,
        drain_mode: message.drain_mode,
        status: MailboxMessageStatus::Queued,
        priority: message.priority,
        order_key: now.timestamp_micros(),
        source_dedup_key: message.source_dedup_key,
        queued_agent_run_turn_id: message.queued_agent_run_turn_id,
        consuming_agent_run_turn_id: None,
        expected_active_agent_run_turn_id: message.expected_active_agent_run_turn_id,
        accepted_agent_run_turn_id: None,
        accepted_protocol_turn_id: None,
        claim_token: None,
        claimed_at: None,
        claim_expires_at: None,
        command_receipt_id: message.command_receipt_id,
        payload_json: message.payload_json,
        executor_config_json: message.executor_config_json,
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
    use agentdash_domain::workflow::AgentRunCommandKind;
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

    #[tokio::test]
    async fn anchor_create_once_is_idempotent_and_rejects_immutable_conflict() {
        let repo = MemoryRuntimeSessionExecutionAnchorRepository::default();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let frame_id = Uuid::new_v4();
        let anchor =
            RuntimeSessionExecutionAnchor::new_dispatch("runtime-a", run_id, frame_id, agent_id);

        repo.create_once(&anchor).await.unwrap();
        repo.create_once(&anchor).await.unwrap();

        let anchors = repo.list_by_run(run_id).await.unwrap();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].runtime_session_id, "runtime-a");

        let conflicting = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-a",
            run_id,
            Uuid::new_v4(),
            agent_id,
        );

        let error = repo.create_once(&conflicting).await.unwrap_err();
        assert!(matches!(
            error,
            DomainError::Conflict {
                entity: "runtime_session_execution_anchor",
                constraint: "runtime_session_id_immutable",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn delivery_binding_upsert_replaces_current_binding_for_run_agent() {
        let repo = MemoryAgentRunDeliveryBindingRepository::default();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let first_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-a",
            run_id,
            Uuid::new_v4(),
            agent_id,
        );
        let second_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            "runtime-b",
            run_id,
            Uuid::new_v4(),
            agent_id,
        );

        let first = AgentRunDeliveryBinding::from_anchor(
            &first_anchor,
            DeliveryBindingStatus::Ready,
            Utc::now(),
        );
        let second = AgentRunDeliveryBinding::from_anchor(
            &second_anchor,
            DeliveryBindingStatus::Running,
            Utc::now() + TimeDelta::seconds(1),
        );

        repo.upsert(&first).await.unwrap();
        repo.upsert(&second).await.unwrap();

        let current = repo.get_current(run_id, agent_id).await.unwrap().unwrap();
        let bindings = repo.list_by_run(run_id).await.unwrap();

        assert_eq!(current.runtime_session_id, "runtime-b");
        assert_eq!(current.status, DeliveryBindingStatus::Running);
        assert_eq!(bindings.len(), 1);
    }

    #[tokio::test]
    async fn command_receipt_claim_detects_duplicate_and_digest_conflict() {
        let repo = MemoryAgentRunCommandReceiptRepository::default();
        let receipt = new_receipt("command-a", "digest-a");

        let created = repo.claim(receipt.clone()).await.unwrap();
        let duplicate = repo.claim(receipt).await.unwrap();
        let conflict = repo
            .claim(new_receipt("command-a", "digest-b"))
            .await
            .unwrap_err();

        assert!(matches!(created, AgentRunCommandClaim::Created(_)));
        assert!(matches!(duplicate, AgentRunCommandClaim::Duplicate(_)));
        assert!(matches!(
            conflict,
            DomainError::Conflict {
                entity: "agent_run_command_receipt",
                constraint: "request_digest",
                ..
            }
        ));
        assert_eq!(repo.debug_list().await.len(), 1);
    }

    fn new_receipt(client_command_id: &str, request_digest: &str) -> NewAgentRunCommandReceipt {
        NewAgentRunCommandReceipt {
            scope_kind: "agent_run".to_string(),
            scope_key: "run-a:agent-a".to_string(),
            command_kind: AgentRunCommandKind::MessageSubmit,
            client_command_id: client_command_id.to_string(),
            request_digest: request_digest.to_string(),
        }
    }
}
