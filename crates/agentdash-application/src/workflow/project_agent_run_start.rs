use uuid::Uuid;

use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, AgentRunDeliveryAcceptedRefs,
    AgentRunDeliveryCommandReceiptRepository, LifecycleAgentRepository, LifecycleGateRepository,
    LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    RuntimeSessionExecutionAnchorRepository, WorkflowGraphRepository,
};
use agentdash_domain::workflow::{
    AgentLaunchIntent, AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource, RunPolicy,
    RuntimePolicy, SubjectRef, WorkflowGraphRef,
};
use agentdash_spi::AgentConfig;
use agentdash_spi::platform::auth::AuthIdentity;
use async_trait::async_trait;

use crate::repository_set::RepositorySet;
use crate::session::{SessionCoreService, SessionMeta};
use crate::workflow::{
    AgentRunCommandReceiptView, AgentRunMessageCommand, AgentRunMessageDeliveryPort,
    AgentRunMessageService, LifecycleDispatchService, RuntimeSessionCreator,
    WorkflowApplicationError,
    command_receipt::{
        accepted_refs_from_record, claim_agent_run_command_receipt, digest_command_request,
        mark_command_terminal_failed,
    },
};

pub struct ProjectAgentRunStartCommand {
    pub project_id: Uuid,
    pub project_agent_id: Uuid,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub subject_ref: Option<SubjectRef>,
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone)]
pub struct ProjectAgentRunStartDispatch {
    pub project_agent: ProjectAgent,
    pub runtime_session_id: String,
    pub turn_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub subject_ref: Option<SubjectRef>,
    pub command_receipt: AgentRunCommandReceiptView,
}

pub struct ProjectAgentRunStartRepos<'a> {
    pub project_agent_repo: &'a dyn ProjectAgentRepository,
    pub lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    pub workflow_graph_repo: &'a dyn WorkflowGraphRepository,
    pub lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    pub agent_frame_repo: &'a dyn AgentFrameRepository,
    pub lifecycle_subject_association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    pub lifecycle_gate_repo: &'a dyn LifecycleGateRepository,
    pub agent_lineage_repo: &'a dyn AgentLineageRepository,
    pub execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    pub command_receipt_repo: &'a dyn AgentRunDeliveryCommandReceiptRepository,
    pub runtime_session_creator: &'a dyn RuntimeSessionCreator,
}

impl<'a> ProjectAgentRunStartRepos<'a> {
    pub fn from_repository_set(repos: &'a RepositorySet) -> Self {
        Self {
            project_agent_repo: repos.project_agent_repo.as_ref(),
            lifecycle_run_repo: repos.lifecycle_run_repo.as_ref(),
            workflow_graph_repo: repos.workflow_graph_repo.as_ref(),
            lifecycle_agent_repo: repos.lifecycle_agent_repo.as_ref(),
            agent_frame_repo: repos.agent_frame_repo.as_ref(),
            lifecycle_subject_association_repo: repos.lifecycle_subject_association_repo.as_ref(),
            lifecycle_gate_repo: repos.lifecycle_gate_repo.as_ref(),
            agent_lineage_repo: repos.agent_lineage_repo.as_ref(),
            execution_anchor_repo: repos.execution_anchor_repo.as_ref(),
            command_receipt_repo: repos.agent_run_delivery_command_receipt_repo.as_ref(),
            runtime_session_creator: repos.runtime_session_creator.as_ref(),
        }
    }
}

#[async_trait]
pub trait RuntimeSessionDraftCleanupPort: Send + Sync {
    async fn get_session_meta(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<SessionMeta>, WorkflowApplicationError>;
    async fn delete_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<(), WorkflowApplicationError>;
}

#[async_trait]
impl RuntimeSessionDraftCleanupPort for SessionCoreService {
    async fn get_session_meta(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<SessionMeta>, WorkflowApplicationError> {
        SessionCoreService::get_session_meta(self, runtime_session_id)
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "读取 RuntimeSession 清理状态失败: {error}"
                ))
            })
    }

    async fn delete_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<(), WorkflowApplicationError> {
        SessionCoreService::delete_session(self, runtime_session_id)
            .await
            .map_err(|error| {
                WorkflowApplicationError::Internal(format!("删除空 RuntimeSession 失败: {error}"))
            })
    }
}

pub struct ProjectAgentRunStartService<'a> {
    repos: ProjectAgentRunStartRepos<'a>,
    cleanup: &'a dyn RuntimeSessionDraftCleanupPort,
}

impl<'a> ProjectAgentRunStartService<'a> {
    pub fn new(
        repos: ProjectAgentRunStartRepos<'a>,
        cleanup: &'a dyn RuntimeSessionDraftCleanupPort,
    ) -> Self {
        Self { repos, cleanup }
    }

    pub async fn start_run<D>(
        &self,
        command: ProjectAgentRunStartCommand,
        delivery: D,
    ) -> Result<ProjectAgentRunStartDispatch, WorkflowApplicationError>
    where
        D: AgentRunMessageDeliveryPort,
    {
        if command.input.is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "input 不能为空".to_string(),
            ));
        }
        if command.client_command_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "client_command_id 不能为空".to_string(),
            ));
        }

        let project_agent = self
            .repos
            .project_agent_repo
            .get_by_project_and_id(command.project_id, command.project_agent_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "Project Agent {} 不存在",
                    command.project_agent_id
                ))
            })?;

        let subject_ref = command
            .subject_ref
            .unwrap_or_else(|| SubjectRef::new("project", command.project_id));
        validate_project_agent_subject_ref(command.project_id, &subject_ref)?;
        let request_digest = digest_command_request(&serde_json::json!({
            "kind": "project_agent_start",
            "project_id": command.project_id,
            "project_agent_id": command.project_agent_id,
            "subject_ref": {
                "kind": subject_ref.kind,
                "id": subject_ref.id,
            },
            "input": command.input,
            "executor_config": command.executor_config,
        }))?;
        let claim = claim_agent_run_command_receipt(
            self.repos.command_receipt_repo,
            "project_agent_start",
            format!(
                "{}:{}:{}:{}",
                command.project_id, command.project_agent_id, subject_ref.kind, subject_ref.id
            ),
            command.client_command_id.clone(),
            request_digest,
        )
        .await?;
        if claim.duplicate {
            let accepted_refs = accepted_refs_from_record(&claim.record)?;
            return Ok(project_agent_start_dispatch_from_accepted_refs(
                project_agent,
                Some(subject_ref),
                accepted_refs,
                AgentRunCommandReceiptView::from_record(&claim.record, true),
            ));
        }
        let intent = AgentLaunchIntent {
            project_id: command.project_id,
            source: ExecutionSource::ProjectAgent,
            subject_ref: Some(subject_ref.clone()),
            parent_run_id: None,
            parent_agent_id: None,
            workflow_graph_ref: workflow_graph_ref_for_project_agent(&project_agent),
            run_policy: RunPolicy::CreateLinkedRun,
            agent_policy: AgentPolicy::Create,
            context_policy: ContextPolicy::Isolated,
            capability_policy: CapabilityPolicy::Baseline,
            runtime_policy: RuntimePolicy::CreateRuntimeSession,
        };

        let dispatch_service = LifecycleDispatchService::new(
            self.repos.lifecycle_run_repo,
            self.repos.workflow_graph_repo,
            self.repos.lifecycle_agent_repo,
            self.repos.agent_frame_repo,
            self.repos.lifecycle_subject_association_repo,
            self.repos.lifecycle_gate_repo,
            self.repos.agent_lineage_repo,
        )
        .with_anchor_repo(self.repos.execution_anchor_repo)
        .with_runtime_session_creator(self.repos.runtime_session_creator);

        let dispatch_result = match dispatch_service.launch_agent(&intent).await {
            Ok(dispatch_result) => dispatch_result,
            Err(error) => {
                mark_command_terminal_failed(
                    self.repos.command_receipt_repo,
                    claim.record.id,
                    &error,
                )
                .await;
                return Err(error);
            }
        };
        let runtime_session_id = dispatch_result
            .delivery_runtime_ref
            .ok_or_else(|| {
                WorkflowApplicationError::Internal(
                    "ProjectAgent AgentRun materialize 未创建 RuntimeSession".to_string(),
                )
            })?
            .to_string();

        if let Err(error) = self
            .bind_project_agent_to_lifecycle_agent(
                dispatch_result.runtime_refs.agent_ref,
                project_agent.id,
            )
            .await
        {
            if let Err(cleanup_error) = self
                .cleanup_if_session_has_no_events(
                    &runtime_session_id,
                    dispatch_result.runtime_refs.run_ref,
                )
                .await
            {
                tracing::warn!(
                    runtime_session_id = %runtime_session_id,
                    run_id = %dispatch_result.runtime_refs.run_ref,
                    error = %cleanup_error,
                    "ProjectAgent 绑定失败后的空 runtime/lifecycle 清理失败"
                );
            }
            mark_command_terminal_failed(self.repos.command_receipt_repo, claim.record.id, &error)
                .await;
            return Err(error);
        }

        let message_service = AgentRunMessageService::new(
            self.repos.lifecycle_run_repo,
            self.repos.lifecycle_agent_repo,
            self.repos.agent_frame_repo,
            self.repos.execution_anchor_repo,
            self.repos.command_receipt_repo,
            delivery,
        );

        let message_dispatch = match message_service
            .dispatch_user_message(AgentRunMessageCommand {
                delivery_runtime_session_id: runtime_session_id.clone(),
                input: command.input,
                client_command_id: format!("{}:initial-message", command.client_command_id),
                executor_config: command.executor_config,
                identity: command.identity,
            })
            .await
        {
            Ok(dispatch) => dispatch,
            Err(error) => {
                if let Err(cleanup_error) = self
                    .cleanup_if_session_has_no_events(
                        &runtime_session_id,
                        dispatch_result.runtime_refs.run_ref,
                    )
                    .await
                {
                    tracing::warn!(
                        runtime_session_id = %runtime_session_id,
                        run_id = %dispatch_result.runtime_refs.run_ref,
                        error = %cleanup_error,
                        "ProjectAgent 首条消息失败后的空 runtime/lifecycle 清理失败"
                    );
                }
                mark_command_terminal_failed(
                    self.repos.command_receipt_repo,
                    claim.record.id,
                    &error,
                )
                .await;
                return Err(error);
            }
        };
        let accepted_refs = AgentRunDeliveryAcceptedRefs {
            run_id: message_dispatch.run_id,
            agent_id: message_dispatch.agent_id,
            frame_id: Some(message_dispatch.frame_id),
            frame_revision: Some(message_dispatch.frame_revision),
            runtime_session_id: Some(runtime_session_id.clone()),
            turn_id: Some(message_dispatch.turn_id.clone()),
        };
        let receipt = self
            .repos
            .command_receipt_repo
            .mark_accepted(claim.record.id, accepted_refs)
            .await?;

        Ok(ProjectAgentRunStartDispatch {
            project_agent,
            runtime_session_id,
            turn_id: message_dispatch.turn_id,
            run_id: message_dispatch.run_id,
            agent_id: message_dispatch.agent_id,
            frame_id: message_dispatch.frame_id,
            frame_revision: message_dispatch.frame_revision,
            subject_ref: Some(subject_ref),
            command_receipt: AgentRunCommandReceiptView::from_record(&receipt, false),
        })
    }

    async fn bind_project_agent_to_lifecycle_agent(
        &self,
        lifecycle_agent_id: Uuid,
        project_agent_id: Uuid,
    ) -> Result<(), WorkflowApplicationError> {
        let Some(mut lifecycle_agent) = self
            .repos
            .lifecycle_agent_repo
            .get(lifecycle_agent_id)
            .await?
        else {
            return Err(WorkflowApplicationError::NotFound(format!(
                "LifecycleAgent 不存在: {lifecycle_agent_id}"
            )));
        };
        lifecycle_agent.project_agent_id = Some(project_agent_id);
        self.repos
            .lifecycle_agent_repo
            .update(&lifecycle_agent)
            .await?;
        Ok(())
    }

    async fn cleanup_if_session_has_no_events(
        &self,
        runtime_session_id: &str,
        run_id: Uuid,
    ) -> Result<(), WorkflowApplicationError> {
        let Some(meta) = self.cleanup.get_session_meta(runtime_session_id).await? else {
            return Ok(());
        };

        if meta.last_event_seq != 0 {
            return Ok(());
        }

        self.repos
            .execution_anchor_repo
            .delete_by_session(runtime_session_id)
            .await?;
        self.cleanup.delete_session(runtime_session_id).await?;
        self.repos.lifecycle_run_repo.delete(run_id).await?;
        Ok(())
    }
}

fn project_agent_start_dispatch_from_accepted_refs(
    project_agent: ProjectAgent,
    subject_ref: Option<SubjectRef>,
    refs: AgentRunDeliveryAcceptedRefs,
    command_receipt: AgentRunCommandReceiptView,
) -> ProjectAgentRunStartDispatch {
    ProjectAgentRunStartDispatch {
        project_agent,
        runtime_session_id: refs.runtime_session_id.unwrap_or_default(),
        turn_id: refs.turn_id.unwrap_or_default(),
        run_id: refs.run_id,
        agent_id: refs.agent_id,
        frame_id: refs.frame_id.unwrap_or_else(Uuid::nil),
        frame_revision: refs.frame_revision.unwrap_or_default(),
        subject_ref,
        command_receipt,
    }
}

fn workflow_graph_ref_for_project_agent(project_agent: &ProjectAgent) -> Option<WorkflowGraphRef> {
    project_agent
        .default_lifecycle_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| WorkflowGraphRef::ByKey {
            project_id: project_agent.project_id,
            key: key.to_string(),
        })
}

fn validate_project_agent_subject_ref(
    project_id: Uuid,
    subject_ref: &SubjectRef,
) -> Result<(), WorkflowApplicationError> {
    match subject_ref.kind.as_str() {
        "project" if subject_ref.id == project_id => Ok(()),
        "project" => Err(WorkflowApplicationError::BadRequest(format!(
            "Project subject {} 不属于当前 Project {}",
            subject_ref.id, project_id
        ))),
        "story" | "task" => Ok(()),
        kind => Err(WorkflowApplicationError::BadRequest(format!(
            "不支持的 ProjectAgent subject kind: {kind}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{ExecutionStatus, TitleSource};
    use crate::test_support::MemoryAgentRunDeliveryCommandReceiptRepository;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentFrame, AgentLineage, LifecycleAgent, LifecycleGate, LifecycleRun,
        LifecycleSubjectAssociation, RuntimeSessionExecutionAnchor, WorkflowGraph,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct ProjectAgentRepo {
        agent: Mutex<Option<ProjectAgent>>,
    }

    #[async_trait]
    impl ProjectAgentRepository for ProjectAgentRepo {
        async fn create(&self, agent: &ProjectAgent) -> Result<(), DomainError> {
            *self.agent.lock().unwrap() = Some(agent.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectAgent>, DomainError> {
            Ok(self
                .agent
                .lock()
                .unwrap()
                .as_ref()
                .filter(|agent| agent.id == id)
                .cloned())
        }

        async fn get_by_project_and_id(
            &self,
            project_id: Uuid,
            id: Uuid,
        ) -> Result<Option<ProjectAgent>, DomainError> {
            Ok(self
                .agent
                .lock()
                .unwrap()
                .as_ref()
                .filter(|agent| agent.project_id == project_id && agent.id == id)
                .cloned())
        }

        async fn get_by_project_and_name(
            &self,
            project_id: Uuid,
            name: &str,
        ) -> Result<Option<ProjectAgent>, DomainError> {
            Ok(self
                .agent
                .lock()
                .unwrap()
                .as_ref()
                .filter(|agent| agent.project_id == project_id && agent.name == name)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectAgent>, DomainError> {
            Ok(self
                .agent
                .lock()
                .unwrap()
                .iter()
                .filter(|agent| agent.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &ProjectAgent) -> Result<(), DomainError> {
            *self.agent.lock().unwrap() = Some(agent.clone());
            Ok(())
        }

        async fn delete(&self, _project_id: Uuid, id: Uuid) -> Result<(), DomainError> {
            let mut agent = self.agent.lock().unwrap();
            if agent.as_ref().is_some_and(|item| item.id == id) {
                *agent = None;
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct RunRepo {
        items: Mutex<Vec<LifecycleRun>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for RunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|existing| existing.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct WorkflowGraphRepo;

    #[async_trait]
    impl WorkflowGraphRepository for WorkflowGraphRepo {
        async fn create(&self, _lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, _id: Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(None)
        }

        async fn get_by_project_and_key(
            &self,
            _project_id: Uuid,
            _key: &str,
        ) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(None)
        }

        async fn list_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<WorkflowGraph>, DomainError> {
            Ok(Vec::new())
        }

        async fn update(&self, _lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct AgentRepo {
        items: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
    impl LifecycleAgentRepository for AgentRepo {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(agent.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|agent| agent.id == id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|existing| existing.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct FrameRepo {
        items: Mutex<Vec<AgentFrame>>,
    }

    #[async_trait]
    impl AgentFrameRepository for FrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn append_visible_canvas_mount(
            &self,
            _frame_id: Uuid,
            _mount_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct AssociationRepo {
        items: Mutex<Vec<LifecycleSubjectAssociation>>,
    }

    #[async_trait]
    impl LifecycleSubjectAssociationRepository for AssociationRepo {
        async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(assoc.clone());
            Ok(())
        }

        async fn list_by_subject(
            &self,
            _subject: &SubjectRef,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_by_anchor(
            &self,
            run_id: Uuid,
            agent_id: Option<Uuid>,
        ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|assoc| assoc.anchor_run_id == run_id && assoc.anchor_agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.items.lock().unwrap().retain(|assoc| assoc.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct GateRepo;

    #[async_trait]
    impl LifecycleGateRepository for GateRepo {
        async fn create(&self, _gate: &LifecycleGate) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, _id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(None)
        }

        async fn list_open_for_agent(
            &self,
            _agent_id: Uuid,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(Vec::new())
        }

        async fn update(&self, _gate: &LifecycleGate) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct LineageRepo;

    #[async_trait]
    impl AgentLineageRepository for LineageRepo {
        async fn create(&self, _lineage: &AgentLineage) -> Result<(), DomainError> {
            Ok(())
        }

        async fn list_children(&self, _agent_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
            Ok(Vec::new())
        }

        async fn find_parent(
            &self,
            _child_agent_id: Uuid,
        ) -> Result<Option<AgentLineage>, DomainError> {
            Ok(None)
        }
    }

    #[derive(Default)]
    struct AnchorRepo {
        items: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for AnchorRepo {
        async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.items
                .lock()
                .unwrap()
                .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
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
                .items
                .lock()
                .unwrap()
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
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
                .cloned()
                .collect())
        }

        async fn latest_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .max_by_key(|anchor| anchor.updated_at)
                .cloned())
        }
    }

    #[derive(Default)]
    struct RuntimeCreator {
        metas: Mutex<HashMap<String, SessionMeta>>,
    }

    #[async_trait]
    impl RuntimeSessionCreator for RuntimeCreator {
        async fn create_runtime_session(
            &self,
            _request: crate::workflow::RuntimeSessionCreationRequest,
        ) -> Result<Uuid, WorkflowApplicationError> {
            let id = Uuid::new_v4();
            self.metas.lock().unwrap().insert(
                id.to_string(),
                SessionMeta {
                    id: id.to_string(),
                    title: "test".to_string(),
                    title_source: TitleSource::Auto,
                    created_at: 1,
                    updated_at: 1,
                    last_event_seq: 0,
                    last_delivery_status: ExecutionStatus::Idle,
                    last_turn_id: None,
                    last_terminal_message: None,
                    executor_session_id: None,
                },
            );
            Ok(id)
        }
    }

    #[async_trait]
    impl RuntimeSessionDraftCleanupPort for RuntimeCreator {
        async fn get_session_meta(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<SessionMeta>, WorkflowApplicationError> {
            Ok(self.metas.lock().unwrap().get(runtime_session_id).cloned())
        }

        async fn delete_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<(), WorkflowApplicationError> {
            self.metas.lock().unwrap().remove(runtime_session_id);
            Ok(())
        }
    }

    struct FailingDelivery;

    #[async_trait]
    impl AgentRunMessageDeliveryPort for FailingDelivery {
        async fn deliver_user_message(
            &self,
            _delivery: crate::workflow::AgentRunMessageDelivery,
        ) -> Result<String, WorkflowApplicationError> {
            Err(WorkflowApplicationError::Internal(
                "connector setup failed".to_string(),
            ))
        }
    }

    struct SuccessfulDelivery;

    #[async_trait]
    impl AgentRunMessageDeliveryPort for SuccessfulDelivery {
        async fn deliver_user_message(
            &self,
            _delivery: crate::workflow::AgentRunMessageDelivery,
        ) -> Result<String, WorkflowApplicationError> {
            Ok("turn-1".to_string())
        }
    }

    #[tokio::test]
    async fn duplicate_start_command_reuses_accepted_run_without_materializing_again() {
        let project_id = Uuid::new_v4();
        let project_agent = ProjectAgent::new(project_id, "agent", "PI_AGENT");
        let project_agent_repo = ProjectAgentRepo {
            agent: Mutex::new(Some(project_agent.clone())),
        };
        let run_repo = RunRepo::default();
        let workflow_graph_repo = WorkflowGraphRepo;
        let agent_repo = AgentRepo::default();
        let frame_repo = FrameRepo::default();
        let association_repo = AssociationRepo::default();
        let gate_repo = GateRepo;
        let lineage_repo = LineageRepo;
        let anchor_repo = AnchorRepo::default();
        let command_receipt_repo = MemoryAgentRunDeliveryCommandReceiptRepository::default();
        let runtime_creator = RuntimeCreator::default();
        let service = ProjectAgentRunStartService::new(
            ProjectAgentRunStartRepos {
                project_agent_repo: &project_agent_repo,
                lifecycle_run_repo: &run_repo,
                workflow_graph_repo: &workflow_graph_repo,
                lifecycle_agent_repo: &agent_repo,
                agent_frame_repo: &frame_repo,
                lifecycle_subject_association_repo: &association_repo,
                lifecycle_gate_repo: &gate_repo,
                agent_lineage_repo: &lineage_repo,
                execution_anchor_repo: &anchor_repo,
                command_receipt_repo: &command_receipt_repo,
                runtime_session_creator: &runtime_creator,
            },
            &runtime_creator,
        );

        let command = || ProjectAgentRunStartCommand {
            project_id,
            project_agent_id: project_agent.id,
            input: agentdash_agent_protocol::text_user_input_blocks("hello"),
            client_command_id: "cmd-start-1".to_string(),
            executor_config: None,
            subject_ref: None,
            identity: None,
        };

        let first = service
            .start_run(command(), SuccessfulDelivery)
            .await
            .expect("first start");
        let second = service
            .start_run(command(), SuccessfulDelivery)
            .await
            .expect("duplicate start");

        assert_eq!(first.run_id, second.run_id);
        assert_eq!(first.agent_id, second.agent_id);
        assert_eq!(first.runtime_session_id, second.runtime_session_id);
        assert_eq!(first.turn_id, second.turn_id);
        assert!(!first.command_receipt.duplicate);
        assert!(second.command_receipt.duplicate);
        assert_eq!(run_repo.items.lock().unwrap().len(), 1);
        assert_eq!(runtime_creator.metas.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn delivery_failure_before_events_cleans_empty_runtime_and_run() {
        let project_id = Uuid::new_v4();
        let project_agent = ProjectAgent::new(project_id, "agent", "PI_AGENT");
        let project_agent_repo = ProjectAgentRepo {
            agent: Mutex::new(Some(project_agent.clone())),
        };
        let run_repo = RunRepo::default();
        let workflow_graph_repo = WorkflowGraphRepo;
        let agent_repo = AgentRepo::default();
        let frame_repo = FrameRepo::default();
        let association_repo = AssociationRepo::default();
        let gate_repo = GateRepo;
        let lineage_repo = LineageRepo;
        let anchor_repo = AnchorRepo::default();
        let command_receipt_repo = MemoryAgentRunDeliveryCommandReceiptRepository::default();
        let runtime_creator = RuntimeCreator::default();
        let service = ProjectAgentRunStartService::new(
            ProjectAgentRunStartRepos {
                project_agent_repo: &project_agent_repo,
                lifecycle_run_repo: &run_repo,
                workflow_graph_repo: &workflow_graph_repo,
                lifecycle_agent_repo: &agent_repo,
                agent_frame_repo: &frame_repo,
                lifecycle_subject_association_repo: &association_repo,
                lifecycle_gate_repo: &gate_repo,
                agent_lineage_repo: &lineage_repo,
                execution_anchor_repo: &anchor_repo,
                command_receipt_repo: &command_receipt_repo,
                runtime_session_creator: &runtime_creator,
            },
            &runtime_creator,
        );

        let error = service
            .start_run(
                ProjectAgentRunStartCommand {
                    project_id,
                    project_agent_id: project_agent.id,
                    input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                    client_command_id: "cmd-start-1".to_string(),
                    executor_config: None,
                    subject_ref: None,
                    identity: None,
                },
                FailingDelivery,
            )
            .await
            .expect_err("delivery failure should bubble");

        assert!(matches!(error, WorkflowApplicationError::Internal(_)));
        assert!(runtime_creator.metas.lock().unwrap().is_empty());
        assert!(run_repo.items.lock().unwrap().is_empty());
        assert!(anchor_repo.items.lock().unwrap().is_empty());
    }
}
