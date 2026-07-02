use std::sync::Arc;

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
    AgentFrame, AgentFrameRepository, AgentRunCommandClaim, AgentRunCommandReceipt,
    AgentRunCommandReceiptRepository, AgentRunCommandStatus, AgentRunLineage,
    AgentRunLineageRepository, DeliveryBindingStatus, LifecycleAgent, LifecycleAgentRepository,
    LifecycleRun, LifecycleRunRepository, NewAgentRunCommandReceipt, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository,
};
use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct MemoryLifecycleRunRepository {
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

#[derive(Default)]
pub(crate) struct MemoryAgentFrameRepository {
    frames: Mutex<Vec<AgentFrame>>,
}

#[derive(Default)]
pub(crate) struct MemoryAgentRunLineageRepository {
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

pub(crate) struct MemoryAgentRunForkMaterialization {
    runs: Arc<MemoryLifecycleRunRepository>,
    agents: Arc<MemoryLifecycleAgentRepository>,
    frames: Arc<MemoryAgentFrameRepository>,
    anchors: Arc<MemoryRuntimeSessionExecutionAnchorRepository>,
    lineages: Arc<MemoryAgentRunLineageRepository>,
    fail_message: Mutex<Option<String>>,
}

impl MemoryAgentRunForkMaterialization {
    pub(crate) fn new(
        runs: Arc<MemoryLifecycleRunRepository>,
        agents: Arc<MemoryLifecycleAgentRepository>,
        frames: Arc<MemoryAgentFrameRepository>,
        anchors: Arc<MemoryRuntimeSessionExecutionAnchorRepository>,
        lineages: Arc<MemoryAgentRunLineageRepository>,
    ) -> Self {
        Self {
            runs,
            agents,
            frames,
            anchors,
            lineages,
            fail_message: Mutex::new(None),
        }
    }

    pub(crate) async fn fail_next(&self, message: impl Into<String>) {
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

        let mut child_run =
            LifecycleRun::new_plain_for_user(input.parent_run.project_id, &input.forked_by_user_id);
        child_run.topology = input.parent_run.topology.clone();
        child_run.context = input.parent_run.context.clone();
        child_run.view_projection = input.parent_run.view_projection.clone();

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
            input.child_runtime_session_id.clone(),
            child_run.id,
            child_frame.id,
            child_agent.id,
        );
        anchor.created_by_kind = "agent_run_fork".to_string();
        child_agent.bind_current_delivery_from_anchor(
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
            input.parent_runtime_session_id,
            input.child_runtime_session_id,
            input.forked_by_user_id,
            input.metadata_json,
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
            .upsert(&anchor)
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

    async fn append_visible_canvas_mount(
        &self,
        frame_id: Uuid,
        mount_id: &str,
    ) -> Result<(), DomainError> {
        let mut frames = self.frames.lock().await;
        let frame = frames
            .iter_mut()
            .find(|frame| frame.id == frame_id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_frame",
                id: frame_id.to_string(),
            })?;
        frame.append_visible_canvas_mount(mount_id);
        Ok(())
    }

    async fn append_visible_workspace_module_ref(
        &self,
        frame_id: Uuid,
        module_ref: &str,
    ) -> Result<(), DomainError> {
        let mut frames = self.frames.lock().await;
        let frame = frames
            .iter_mut()
            .find(|frame| frame.id == frame_id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_frame",
                id: frame_id.to_string(),
            })?;
        frame.append_visible_workspace_module_ref(module_ref);
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct MemoryRuntimeSessionExecutionAnchorRepository {
    anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
}

#[async_trait::async_trait]
impl RuntimeSessionExecutionAnchorRepository for MemoryRuntimeSessionExecutionAnchorRepository {
    async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
        let mut anchors = self.anchors.lock().await;
        if let Some(existing) = anchors
            .iter_mut()
            .find(|item| item.runtime_session_id == anchor.runtime_session_id)
        {
            *existing = anchor.clone();
        } else {
            anchors.push(anchor.clone());
        }
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

    async fn latest_updated_anchor_for_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
        Ok(self
            .anchors
            .lock()
            .await
            .iter()
            .filter(|anchor| anchor.agent_id == agent_id)
            .max_by_key(|anchor| anchor.updated_at)
            .cloned())
    }
}

#[derive(Default)]
pub(crate) struct MemoryAgentRunCommandReceiptRepository {
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
    pub(crate) async fn debug_list(&self) -> Vec<AgentRunCommandReceipt> {
        self.receipts.lock().await.clone()
    }
}

#[derive(Default)]
pub(crate) struct MemoryLifecycleAgentRepository {
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

#[derive(Default)]
pub(crate) struct MemoryProjectAgentRepository {
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
pub(crate) struct MemoryProjectBackendAccessRepository {
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
pub(crate) struct MemoryAgentRunMailboxRepository {
    messages: Mutex<Vec<AgentRunMailboxMessage>>,
    states: Mutex<Vec<AgentRunMailboxState>>,
    cleaned: Mutex<Vec<Uuid>>,
}

impl MemoryAgentRunMailboxRepository {
    pub(crate) async fn messages_for(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Vec<AgentRunMailboxMessage> {
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
                || request.runtime_session_id.as_deref() != Some(&message.runtime_session_id)
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
        runtime_session_id: String,
        reason: String,
        message: Option<String>,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let state = AgentRunMailboxState {
            run_id,
            agent_id,
            runtime_session_id,
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
        runtime_session_id: String,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let state = AgentRunMailboxState {
            run_id,
            agent_id,
            runtime_session_id,
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
        runtime_session_id: String,
        preference: serde_json::Value,
    ) -> Result<AgentRunMailboxState, DomainError> {
        let mut state = self
            .get_state(run_id, agent_id)
            .await?
            .unwrap_or(AgentRunMailboxState {
                run_id,
                agent_id,
                runtime_session_id,
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
        runtime_session_id: message.runtime_session_id,
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
