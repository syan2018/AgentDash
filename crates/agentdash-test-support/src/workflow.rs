use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_domain::DomainError;
use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
use agentdash_domain::backend::{
    ProjectBackendAccess, ProjectBackendAccessRepository, ProjectBackendAccessStatus,
};
use agentdash_domain::channel::{ChannelRegistryDocument, ChannelRegistryMutation};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentLineage, AgentLineageRepository, AgentProcedure,
    AgentProcedureRepository, AgentRunLineage, AgentRunLineageRepository, GateWaitPolicyEnvelope,
    LifecycleAgent, LifecycleAgentRepository, LifecycleGate, LifecycleGateRepository, LifecycleRun,
    LifecycleRunRepository, LifecycleRunWriteError, LifecycleSubjectAssociation,
    LifecycleSubjectAssociationRepository, SubjectRef, WaitProducerRef, WorkflowGraph,
    WorkflowGraphRepository,
};
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

    async fn compare_and_swap(
        &self,
        expected_revision: u64,
        run: &LifecycleRun,
    ) -> Result<(), LifecycleRunWriteError> {
        let mut runs = self.runs.lock().await;
        let Some(existing) = runs.iter_mut().find(|item| item.id == run.id) else {
            return Err(LifecycleRunWriteError::Persistence(DomainError::NotFound {
                entity: "lifecycle_run",
                id: run.id.to_string(),
            }));
        };
        if existing.revision != expected_revision || run.revision != expected_revision + 1 {
            return Err(LifecycleRunWriteError::RevisionConflict {
                run_id: run.id,
                expected_revision,
                actual_revision: existing.revision,
            });
        }
        let channel_registry = existing.channel_registry.clone();
        *existing = run.clone();
        existing.channel_registry = channel_registry;
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

    async fn get_latest(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
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
            runtime_thread_id,
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
        outcome.runtime_thread_id = runtime_thread_id;
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

        let current = repo.get_latest(agent_id).await.unwrap().unwrap();

        assert_eq!(current.id, latest_high_revision_id);
        assert_ne!(current.id, older_high_revision_id);
        assert_eq!(current.revision, 2);
    }
}
