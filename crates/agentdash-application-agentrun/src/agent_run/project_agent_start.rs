use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use std::marker::PhantomData;
use uuid::Uuid;

#[cfg(test)]
use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
#[cfg(test)]
use agentdash_application_ports::runtime_session_delivery as runtime_session_delivery_port;
use agentdash_application_ports::runtime_session_delivery::RuntimeSessionCreationPort;
use agentdash_contracts::workflow::{
    ConversationEffectiveExecutorConfigView, ConversationModelConfigSource,
};
use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
use agentdash_domain::agent_run_mailbox::AgentRunMailboxRepository;
use agentdash_domain::backend::ProjectBackendAccessRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, AgentRunAcceptedRefs, AgentRunCommandKind,
    AgentRunCommandReceiptRepository, AgentRunDeliveryBindingRepository, LifecycleAgentRepository,
    LifecycleGateRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    RuntimeSessionExecutionAnchorRepository, WorkflowGraphRepository,
};
use agentdash_domain::workflow::{AgentLaunchDispatchResult, AgentRunCommandReceipt};
use agentdash_domain::workflow::{
    AgentLaunchIntent, AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource, RunPolicy,
    RuntimePolicy, SubjectRef, WorkflowGraphRef,
};
use agentdash_spi::AgentConfig;
use agentdash_spi::platform::auth::AuthIdentity;
use async_trait::async_trait;

use crate::agent_run::runtime_session_boundary::{SessionCoreService, SessionMeta};
use crate::agent_run::{
    AgentRunCommandReceiptView, AgentRunMailboxCommandOutcome, AgentRunMailboxCommandResult,
    AgentRunMailboxScheduleTrigger, AgentRunMailboxService, AgentRunMailboxUserMessageCommand,
    ConversationEffectiveExecutorConfigModel, ConversationModelConfigResolver,
    ConversationModelConfigSourceModel,
    command_receipt::{
        accepted_refs_from_record, claim_agent_run_command_receipt, digest_command_request,
        mark_command_terminal_failed,
    },
    mailbox::{outcome_from_message, outcome_from_result_json},
};
use crate::agent_run::{SessionControlService, SessionEventingService, SessionLaunchService};
use crate::agent_run_repository_set::RepositorySet;
use crate::error::WorkflowApplicationError;

pub struct ProjectAgentRunStartCommand {
    pub project_id: Uuid,
    pub project_agent_id: Uuid,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub backend_selection: Option<agentdash_application_ports::launch::BackendSelectionInput>,
    pub subject_ref: Option<SubjectRef>,
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone)]
pub struct ProjectAgentRunInitialMailboxCommand {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    #[cfg_attr(not(test), allow(dead_code))]
    pub runtime_session_id: String,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub backend_selection: Option<agentdash_application_ports::launch::BackendSelectionInput>,
    pub identity: Option<AuthIdentity>,
}

impl ProjectAgentRunInitialMailboxCommand {
    fn into_mailbox_command(self) -> AgentRunMailboxUserMessageCommand {
        AgentRunMailboxUserMessageCommand {
            run_id: self.run_id,
            agent_id: self.agent_id,
            frame_id: self.frame_id,
            source: agentdash_domain::agent_run_mailbox::MailboxSourceIdentity::draft_start(),
            schedule_on_submit: false,
            input: self.input,
            client_command_id: self.client_command_id,
            executor_config: self.executor_config,
            backend_selection: self.backend_selection,
            identity: self.identity,
            delivery_intent: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectAgentRunStartDispatch {
    pub project_agent: ProjectAgent,
    pub effective_executor_config: ConversationEffectiveExecutorConfigView,
    pub runtime_session_id: String,
    pub turn_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub subject_ref: Option<SubjectRef>,
    pub command_receipt: AgentRunCommandReceiptView,
    pub initial_message: AgentRunMailboxCommandResult,
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
    pub delivery_binding_repo: &'a dyn AgentRunDeliveryBindingRepository,
    pub project_backend_access_repo: &'a dyn ProjectBackendAccessRepository,
    pub command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
    pub mailbox_repo: &'a dyn AgentRunMailboxRepository,
    pub runtime_session_creator: &'a dyn RuntimeSessionCreationPort,
    pub agent_frame_construction: &'a dyn AgentRunFrameConstructionPort,
    pub project_agent_lifecycle_launch: &'a dyn ProjectAgentLifecycleLaunchPort,
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
            delivery_binding_repo: repos.agent_run_delivery_binding_repo.as_ref(),
            project_backend_access_repo: repos.project_backend_access_repo.as_ref(),
            command_receipt_repo: repos.agent_run_command_receipt_repo.as_ref(),
            mailbox_repo: repos.agent_run_mailbox_repo.as_ref(),
            runtime_session_creator: repos.runtime_session_creator.as_ref(),
            agent_frame_construction: repos.agent_frame_construction.as_ref(),
            project_agent_lifecycle_launch: repos.project_agent_lifecycle_launch.as_ref(),
        }
    }

    pub fn mailbox_service(
        &self,
        session_core: crate::agent_run::runtime_session_boundary::SessionCoreService,
        session_control: crate::agent_run::runtime_session_boundary::SessionControlService,
        session_eventing: crate::agent_run::runtime_session_boundary::SessionEventingService,
        session_launch: crate::agent_run::runtime_session_boundary::SessionLaunchService,
    ) -> AgentRunMailboxService<'a> {
        AgentRunMailboxService::new(
            self.lifecycle_run_repo,
            self.lifecycle_agent_repo,
            self.project_agent_repo,
            self.agent_frame_repo,
            self.execution_anchor_repo,
            self.delivery_binding_repo,
            self.project_backend_access_repo,
            self.command_receipt_repo,
            self.mailbox_repo,
            session_core,
            session_control,
            session_eventing,
            session_launch,
        )
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

#[async_trait]
trait ProjectAgentRunInitialMailboxPort: Send + Sync {
    async fn accept_initial_mailbox_message(
        &self,
        command: ProjectAgentRunInitialMailboxCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError>;

    async fn schedule_initial_mailbox_message(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
        identity: Option<AuthIdentity>,
    ) -> Result<(), WorkflowApplicationError>;
}

#[async_trait]
pub trait ProjectAgentLifecycleLaunchPort: Send + Sync {
    async fn launch_project_agent(
        &self,
        intent: &AgentLaunchIntent,
    ) -> Result<AgentLaunchDispatchResult, WorkflowApplicationError>;
}

#[async_trait]
impl ProjectAgentRunInitialMailboxPort for AgentRunMailboxService<'_> {
    async fn accept_initial_mailbox_message(
        &self,
        command: ProjectAgentRunInitialMailboxCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        self.accept_user_message(command.into_mailbox_command())
            .await
    }

    async fn schedule_initial_mailbox_message(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: &str,
        identity: Option<AuthIdentity>,
    ) -> Result<(), WorkflowApplicationError> {
        self.schedule(
            run_id,
            agent_id,
            runtime_session_id,
            AgentRunMailboxScheduleTrigger::UserMessageSubmitted,
            identity,
        )
        .await?;
        Ok(())
    }
}

enum ProjectAgentRunInitialMailboxDeps<'a> {
    Service {
        session_core: SessionCoreService,
        session_control: SessionControlService,
        session_eventing: SessionEventingService,
        session_launch: SessionLaunchService,
        _marker: PhantomData<&'a ()>,
    },
    #[cfg(test)]
    Port(&'a dyn ProjectAgentRunInitialMailboxPort),
}

pub struct ProjectAgentRunStartService<'a> {
    repos: ProjectAgentRunStartRepos<'a>,
    cleanup: &'a dyn RuntimeSessionDraftCleanupPort,
    lifecycle_launch: &'a dyn ProjectAgentLifecycleLaunchPort,
    initial_mailbox: ProjectAgentRunInitialMailboxDeps<'a>,
}

impl<'a> ProjectAgentRunStartService<'a> {
    pub fn new(
        repos: ProjectAgentRunStartRepos<'a>,
        cleanup: &'a dyn RuntimeSessionDraftCleanupPort,
        session_core: SessionCoreService,
        session_control: SessionControlService,
        session_eventing: SessionEventingService,
        session_launch: SessionLaunchService,
    ) -> Self {
        let lifecycle_launch = repos.project_agent_lifecycle_launch;
        Self {
            repos,
            cleanup,
            lifecycle_launch,
            initial_mailbox: ProjectAgentRunInitialMailboxDeps::Service {
                session_core,
                session_control,
                session_eventing,
                session_launch,
                _marker: PhantomData,
            },
        }
    }

    #[cfg(test)]
    fn new_with_initial_mailbox_port(
        repos: ProjectAgentRunStartRepos<'a>,
        cleanup: &'a dyn RuntimeSessionDraftCleanupPort,
        initial_mailbox: &'a dyn ProjectAgentRunInitialMailboxPort,
    ) -> Self {
        let lifecycle_launch = repos.project_agent_lifecycle_launch;
        Self {
            repos,
            cleanup,
            lifecycle_launch,
            initial_mailbox: ProjectAgentRunInitialMailboxDeps::Port(initial_mailbox),
        }
    }

    pub async fn start_run(
        &self,
        mut command: ProjectAgentRunStartCommand,
    ) -> Result<ProjectAgentRunStartDispatch, WorkflowApplicationError> {
        diag!(Info, Subsystem::AgentRun,

            project_id = %command.project_id,
            project_agent_id = %command.project_agent_id,
            input_blocks = command.input.len(),
            has_executor_config = command.executor_config.is_some(),
            "ProjectAgent run start service entered"
        );
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
        diag!(Info, Subsystem::AgentRun,

            project_id = %command.project_id,
            project_agent_id = %command.project_agent_id,
            project_agent_name = %project_agent.name,
            "ProjectAgent run start project agent loaded"
        );

        let model_resolution = ConversationModelConfigResolver::resolve_project_agent_start(
            &project_agent,
            command.executor_config.as_ref(),
        )?;
        let effective_executor_config = model_resolution
            .view
            .effective_executor_config
            .clone()
            .unwrap_or_else(|| {
                ConversationModelConfigResolver::view_for_config(
                    &model_resolution.config,
                    ConversationModelConfigSourceModel::ProjectAgentPreset,
                )
            });
        let effective_executor_config =
            effective_executor_config_to_contract(effective_executor_config);
        command.executor_config = Some(model_resolution.config.clone());
        diag!(Info, Subsystem::AgentRun,

            project_id = %command.project_id,
            project_agent_id = %command.project_agent_id,
            provider_id = ?model_resolution.config.provider_id,
            model_id = ?model_resolution.config.model_id,
            "ProjectAgent run start model config resolved"
        );

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
            AgentRunCommandKind::ProjectAgentStart,
            command.client_command_id.clone(),
            request_digest,
        )
        .await?;
        diag!(Info, Subsystem::AgentRun,

            project_id = %command.project_id,
            project_agent_id = %command.project_agent_id,
            receipt_id = %claim.record.id,
            duplicate = claim.duplicate,
            "ProjectAgent run start command receipt claimed"
        );
        if claim.duplicate {
            return self
                .dispatch_from_project_agent_start_receipt(
                    project_agent,
                    Some(subject_ref),
                    &claim.record,
                    effective_executor_config,
                    true,
                )
                .await;
        }
        let intent = AgentLaunchIntent {
            project_id: command.project_id,
            source: ExecutionSource::ProjectAgent,
            created_by_user_id: command
                .identity
                .as_ref()
                .map(|identity| identity.user_id.clone()),
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

        diag!(Info, Subsystem::AgentRun,

            project_id = %command.project_id,
            project_agent_id = %command.project_agent_id,
            "ProjectAgent run start launching lifecycle agent"
        );
        let dispatch_result = match self.lifecycle_launch.launch_project_agent(&intent).await {
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
        diag!(Info, Subsystem::AgentRun,

            project_id = %command.project_id,
            project_agent_id = %command.project_agent_id,
            run_id = %dispatch_result.runtime_refs.run_ref,
            agent_id = %dispatch_result.runtime_refs.agent_ref,
            frame_id = %dispatch_result.runtime_refs.frame_ref,
            has_runtime_ref = dispatch_result.delivery_runtime_ref.is_some(),
            "ProjectAgent run start lifecycle launched"
        );
        let runtime_session_id = dispatch_result
            .delivery_runtime_ref
            .ok_or_else(|| {
                WorkflowApplicationError::Internal(
                    "ProjectAgent AgentRun materialize 未创建 RuntimeSession".to_string(),
                )
            })?
            .to_string();

        diag!(Info, Subsystem::AgentRun,

            runtime_session_id = %runtime_session_id,
            run_id = %dispatch_result.runtime_refs.run_ref,
            agent_id = %dispatch_result.runtime_refs.agent_ref,
            "ProjectAgent run start binding project agent"
        );
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
                let diagnostic_context = DiagnosticErrorContext::new(
                    "agent_run.project_agent_start",
                    "cleanup_after_bind_failure",
                );
                diag_error!(Warn, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &cleanup_error,
                    project_id = %command.project_id,
                    project_agent_id = %command.project_agent_id,
                    runtime_session_id = %runtime_session_id,
                    run_id = %dispatch_result.runtime_refs.run_ref,
                    agent_id = %dispatch_result.runtime_refs.agent_ref,
                    receipt_id = %claim.record.id,
                    "ProjectAgent cleanup after binding failure failed"
                );
            }
            mark_command_terminal_failed(self.repos.command_receipt_repo, claim.record.id, &error)
                .await;
            return Err(error);
        }

        diag!(Info, Subsystem::AgentRun,

            runtime_session_id = %runtime_session_id,
            run_id = %dispatch_result.runtime_refs.run_ref,
            agent_id = %dispatch_result.runtime_refs.agent_ref,
            "ProjectAgent run start accepting initial mailbox message"
        );
        let identity_for_initial_schedule = command.identity.clone();
        let initial_message_result = match self
            .accept_initial_mailbox_message(ProjectAgentRunInitialMailboxCommand {
                run_id: dispatch_result.runtime_refs.run_ref,
                agent_id: dispatch_result.runtime_refs.agent_ref,
                frame_id: dispatch_result.runtime_refs.frame_ref,
                runtime_session_id: runtime_session_id.clone(),
                input: command.input,
                client_command_id: format!("{}:initial-message", command.client_command_id),
                executor_config: command.executor_config,
                backend_selection: command.backend_selection,
                identity: command.identity,
            })
            .await
        {
            Ok(result) => result,
            Err(error) => {
                self.cleanup_after_initial_message_error(
                    &runtime_session_id,
                    dispatch_result.runtime_refs.run_ref,
                    claim.record.id,
                    &error,
                )
                .await;
                return Err(error);
            }
        };
        diag!(Info, Subsystem::AgentRun,

            runtime_session_id = %runtime_session_id,
            run_id = %dispatch_result.runtime_refs.run_ref,
            agent_id = %dispatch_result.runtime_refs.agent_ref,
            outcome = ?initial_message_result.outcome,
            has_mailbox_message = initial_message_result.mailbox_message.is_some(),
            "ProjectAgent run start initial mailbox accepted"
        );

        let accepted_refs = match self
            .accepted_refs_from_initial_mailbox_result(
                &initial_message_result,
                dispatch_result.runtime_refs.run_ref,
                dispatch_result.runtime_refs.agent_ref,
                dispatch_result.runtime_refs.frame_ref,
                &runtime_session_id,
            )
            .await
        {
            Ok(refs) => refs,
            Err(error) => {
                self.cleanup_after_initial_message_error(
                    &runtime_session_id,
                    dispatch_result.runtime_refs.run_ref,
                    claim.record.id,
                    &error,
                )
                .await;
                return Err(error);
            }
        };
        diag!(Info, Subsystem::AgentRun,

            runtime_session_id = %runtime_session_id,
            run_id = %dispatch_result.runtime_refs.run_ref,
            agent_id = %dispatch_result.runtime_refs.agent_ref,
            turn_id = ?accepted_refs.agent_run_turn_id,
            "ProjectAgent run start accepted refs resolved"
        );
        let turn_id = accepted_refs.agent_run_turn_id.clone().unwrap_or_default();
        let frame_id = accepted_refs
            .frame_id
            .unwrap_or(dispatch_result.runtime_refs.frame_ref);
        let frame_revision = accepted_refs.frame_revision.unwrap_or_default();
        let receipt = self
            .accept_project_agent_start_receipt(
                claim.record.id,
                initial_message_result.outcome,
                initial_message_result.mailbox_message.as_ref(),
                accepted_refs,
            )
            .await?;
        diag!(Info, Subsystem::AgentRun,

            runtime_session_id = %runtime_session_id,
            run_id = %dispatch_result.runtime_refs.run_ref,
            agent_id = %dispatch_result.runtime_refs.agent_ref,
            receipt_status = ?receipt.status,
            "ProjectAgent run start receipt accepted"
        );

        let dispatch = ProjectAgentRunStartDispatch {
            project_agent,
            effective_executor_config,
            runtime_session_id,
            turn_id,
            run_id: dispatch_result.runtime_refs.run_ref,
            agent_id: dispatch_result.runtime_refs.agent_ref,
            frame_id,
            frame_revision,
            subject_ref: Some(subject_ref),
            command_receipt: receipt,
            initial_message: initial_message_result,
        };
        self.schedule_queued_initial_mailbox_message(&dispatch, identity_for_initial_schedule)
            .await;
        Ok(dispatch)
    }

    async fn accept_initial_mailbox_message(
        &self,
        command: ProjectAgentRunInitialMailboxCommand,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        match &self.initial_mailbox {
            ProjectAgentRunInitialMailboxDeps::Service {
                session_core,
                session_control,
                session_eventing,
                session_launch,
                ..
            } => {
                let mailbox = self.repos.mailbox_service(
                    session_core.clone(),
                    session_control.clone(),
                    session_eventing.clone(),
                    session_launch.clone(),
                );
                mailbox.accept_initial_mailbox_message(command).await
            }
            #[cfg(test)]
            ProjectAgentRunInitialMailboxDeps::Port(port) => {
                port.accept_initial_mailbox_message(command).await
            }
        }
    }

    async fn schedule_queued_initial_mailbox_message(
        &self,
        dispatch: &ProjectAgentRunStartDispatch,
        identity: Option<AuthIdentity>,
    ) {
        if dispatch.initial_message.outcome != AgentRunMailboxCommandOutcome::Queued
            || dispatch.initial_message.mailbox_message.is_none()
        {
            return;
        }

        diag!(Info, Subsystem::AgentRun,
            runtime_session_id = %dispatch.runtime_session_id,
            run_id = %dispatch.run_id,
            agent_id = %dispatch.agent_id,
            "ProjectAgent run start scheduling queued initial mailbox message"
        );
        let result = match &self.initial_mailbox {
            ProjectAgentRunInitialMailboxDeps::Service {
                session_core,
                session_control,
                session_eventing,
                session_launch,
                ..
            } => {
                let mailbox = self.repos.mailbox_service(
                    session_core.clone(),
                    session_control.clone(),
                    session_eventing.clone(),
                    session_launch.clone(),
                );
                mailbox
                    .schedule_initial_mailbox_message(
                        dispatch.run_id,
                        dispatch.agent_id,
                        &dispatch.runtime_session_id,
                        identity,
                    )
                    .await
            }
            #[cfg(test)]
            ProjectAgentRunInitialMailboxDeps::Port(port) => {
                port.schedule_initial_mailbox_message(
                    dispatch.run_id,
                    dispatch.agent_id,
                    &dispatch.runtime_session_id,
                    identity,
                )
                .await
            }
        };

        if let Err(error) = result {
            let diagnostic_context =
                DiagnosticErrorContext::new("agent_run.project_agent_start", "initial_schedule");
            diag_error!(Warn, Subsystem::AgentRun,
                context = &diagnostic_context,
                error = &error,
                runtime_session_id = %dispatch.runtime_session_id,
                run_id = %dispatch.run_id,
                agent_id = %dispatch.agent_id,
                "ProjectAgent 初始 mailbox 调度失败"
            );
        }
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
        self.repos
            .delivery_binding_repo
            .delete_by_session(runtime_session_id)
            .await?;
        self.cleanup.delete_session(runtime_session_id).await?;
        self.repos.lifecycle_run_repo.delete(run_id).await?;
        Ok(())
    }

    async fn cleanup_after_initial_message_error(
        &self,
        runtime_session_id: &str,
        run_id: Uuid,
        receipt_id: Uuid,
        error: &WorkflowApplicationError,
    ) {
        if let Err(cleanup_error) = self
            .cleanup_if_session_has_no_events(runtime_session_id, run_id)
            .await
        {
            let diagnostic_context = DiagnosticErrorContext::new(
                "agent_run.project_agent_start",
                "cleanup_after_initial_message_failure",
            );
            diag_error!(Warn, Subsystem::AgentRun,
                context = &diagnostic_context,
                error = &cleanup_error,
                runtime_session_id = %runtime_session_id,
                run_id = %run_id,
                receipt_id = %receipt_id,
                "ProjectAgent cleanup after initial mailbox failure failed"
            );
        }
        mark_command_terminal_failed(self.repos.command_receipt_repo, receipt_id, error).await;
    }

    async fn accepted_refs_from_initial_mailbox_result(
        &self,
        result: &AgentRunMailboxCommandResult,
        expected_run_id: Uuid,
        expected_agent_id: Uuid,
        launch_frame_id: Uuid,
        runtime_session_id: &str,
    ) -> Result<AgentRunAcceptedRefs, WorkflowApplicationError> {
        let mut refs = result
            .accepted_refs
            .clone()
            .unwrap_or_else(|| AgentRunAcceptedRefs {
                run_id: expected_run_id,
                agent_id: expected_agent_id,
                frame_id: Some(launch_frame_id),
                frame_revision: None,
                runtime_session_id: Some(runtime_session_id.to_string()),
                agent_run_turn_id: None,
                protocol_turn_id: None,
            });
        validate_project_agent_initial_mailbox_refs(
            &refs,
            expected_run_id,
            expected_agent_id,
            runtime_session_id,
        )?;

        let frame = match refs.frame_id {
            Some(frame_id) => self.repos.agent_frame_repo.get(frame_id).await?,
            None => self
                .repos
                .agent_frame_repo
                .get_current(expected_agent_id)
                .await?
                .or(self.repos.agent_frame_repo.get(launch_frame_id).await?),
        }
        .ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "ProjectAgent 首条 mailbox accepted refs 缺少可用 AgentFrame: {}",
                refs.frame_id.unwrap_or(launch_frame_id)
            ))
        })?;
        refs.frame_id = Some(frame.id);
        refs.frame_revision = Some(frame.revision);
        refs.runtime_session_id = Some(runtime_session_id.to_string());
        Ok(refs)
    }

    async fn accept_project_agent_start_receipt(
        &self,
        receipt_id: Uuid,
        outcome: AgentRunMailboxCommandOutcome,
        mailbox_message: Option<&agentdash_domain::agent_run_mailbox::AgentRunMailboxMessage>,
        accepted_refs: AgentRunAcceptedRefs,
    ) -> Result<AgentRunCommandReceiptView, WorkflowApplicationError> {
        if let Some(message) = mailbox_message {
            self.repos
                .command_receipt_repo
                .attach_mailbox_message(receipt_id, message.id)
                .await?;
        }
        let accepted = self
            .repos
            .command_receipt_repo
            .mark_accepted(receipt_id, accepted_refs)
            .await?;
        let result_json = serde_json::json!({
            "outcome": outcome.as_str(),
            "mailbox_message_id": mailbox_message.map(|message| message.id),
        });
        let stored = self
            .repos
            .command_receipt_repo
            .store_result_json(receipt_id, result_json)
            .await?;
        Ok(AgentRunCommandReceiptView::from_record(
            if stored.updated_at >= accepted.updated_at {
                &stored
            } else {
                &accepted
            },
            false,
        ))
    }

    async fn dispatch_from_project_agent_start_receipt(
        &self,
        project_agent: ProjectAgent,
        subject_ref: Option<SubjectRef>,
        record: &AgentRunCommandReceipt,
        effective_executor_config: ConversationEffectiveExecutorConfigView,
        duplicate: bool,
    ) -> Result<ProjectAgentRunStartDispatch, WorkflowApplicationError> {
        let refs = accepted_refs_from_record(record)?;
        let initial_message = self
            .initial_message_result_from_start_receipt(record, &refs, duplicate)
            .await?;
        Ok(project_agent_start_dispatch_from_refs(
            project_agent,
            subject_ref,
            refs,
            effective_executor_config,
            AgentRunCommandReceiptView::from_record(record, duplicate),
            initial_message,
        ))
    }

    async fn initial_message_result_from_start_receipt(
        &self,
        record: &AgentRunCommandReceipt,
        refs: &AgentRunAcceptedRefs,
        duplicate: bool,
    ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
        let mailbox_message = match record.mailbox_message_id {
            Some(message_id) => self.repos.mailbox_repo.get_message(message_id).await?,
            None => None,
        };
        let outcome = mailbox_message
            .as_ref()
            .map(outcome_from_message)
            .or_else(|| {
                record
                    .result_json
                    .as_ref()
                    .and_then(outcome_from_result_json)
            })
            .unwrap_or(AgentRunMailboxCommandOutcome::Queued);
        let command_receipt = match mailbox_message
            .as_ref()
            .and_then(|message| message.command_receipt_id)
        {
            Some(receipt_id) => self
                .repos
                .command_receipt_repo
                .get(receipt_id)
                .await?
                .map(|receipt| AgentRunCommandReceiptView::from_record(&receipt, duplicate))
                .unwrap_or_else(|| AgentRunCommandReceiptView::from_record(record, duplicate)),
            None => AgentRunCommandReceiptView::from_record(record, duplicate),
        };
        Ok(AgentRunMailboxCommandResult {
            command_receipt,
            outcome,
            mailbox_message,
            accepted_refs: Some(refs.clone()),
            runtime_state: None,
        })
    }
}

fn validate_project_agent_initial_mailbox_refs(
    refs: &AgentRunAcceptedRefs,
    expected_run_id: Uuid,
    expected_agent_id: Uuid,
    expected_runtime_session_id: &str,
) -> Result<(), WorkflowApplicationError> {
    if refs.run_id != expected_run_id || refs.agent_id != expected_agent_id {
        return Err(WorkflowApplicationError::Conflict(format!(
            "ProjectAgent 首条 mailbox accepted refs 指向 {} / {}，不匹配新建 AgentRun {} / {}",
            refs.run_id, refs.agent_id, expected_run_id, expected_agent_id
        )));
    }
    if refs.runtime_session_id.as_deref() != Some(expected_runtime_session_id) {
        return Err(WorkflowApplicationError::Conflict(format!(
            "ProjectAgent 首条 mailbox accepted runtime_session 不匹配: expected={expected_runtime_session_id}, actual={:?}",
            refs.runtime_session_id
        )));
    }
    Ok(())
}

fn project_agent_start_dispatch_from_refs(
    project_agent: ProjectAgent,
    subject_ref: Option<SubjectRef>,
    refs: AgentRunAcceptedRefs,
    effective_executor_config: ConversationEffectiveExecutorConfigView,
    command_receipt: AgentRunCommandReceiptView,
    initial_message: AgentRunMailboxCommandResult,
) -> ProjectAgentRunStartDispatch {
    ProjectAgentRunStartDispatch {
        project_agent,
        effective_executor_config,
        runtime_session_id: refs.runtime_session_id.unwrap_or_default(),
        turn_id: refs.agent_run_turn_id.unwrap_or_default(),
        run_id: refs.run_id,
        agent_id: refs.agent_id,
        frame_id: refs.frame_id.unwrap_or_else(Uuid::nil),
        frame_revision: refs.frame_revision.unwrap_or_default(),
        subject_ref,
        command_receipt,
        initial_message,
    }
}

fn effective_executor_config_to_contract(
    config: ConversationEffectiveExecutorConfigModel,
) -> ConversationEffectiveExecutorConfigView {
    ConversationEffectiveExecutorConfigView {
        executor: config.executor,
        provider_id: config.provider_id,
        model_id: config.model_id,
        agent_id: config.agent_id,
        thinking_level: config.thinking_level,
        permission_policy: config.permission_policy,
        source: match config.source {
            ConversationModelConfigSourceModel::ProjectAgentPreset => {
                ConversationModelConfigSource::ProjectAgentPreset
            }
            ConversationModelConfigSourceModel::FrameExecutionProfile => {
                ConversationModelConfigSource::FrameExecutionProfile
            }
            ConversationModelConfigSourceModel::UserOverride => {
                ConversationModelConfigSource::UserOverride
            }
            ConversationModelConfigSourceModel::ExecutorDiscoveryDefault => {
                ConversationModelConfigSource::ExecutorDiscoveryDefault
            }
            ConversationModelConfigSourceModel::Unspecified => {
                ConversationModelConfigSource::Unspecified
            }
        },
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
    use crate::agent_run::runtime_session_boundary::{ExecutionStatus, TitleSource};
    use crate::test_support::{
        MemoryAgentRunCommandReceiptRepository, MemoryAgentRunDeliveryBindingRepository,
    };
    use agentdash_domain::DomainError;
    use agentdash_domain::agent_run_mailbox::{
        AgentRunMailboxClaimRequest, AgentRunMailboxMessage, AgentRunMailboxRepository,
        AgentRunMailboxState, ConsumptionBarrier, MailboxDelivery, MailboxDrainMode,
        MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity,
        NewAgentRunMailboxMessage,
    };
    use agentdash_domain::backend::{ProjectBackendAccess, ProjectBackendAccessStatus};
    use agentdash_domain::workflow::{
        AgentFrame, AgentLineage, AgentRunDeliveryBinding, AgentRuntimeRefs, AgentSource,
        DeliveryBindingStatus, LifecycleAgent, LifecycleGate, LifecycleRun,
        LifecycleSubjectAssociation, RuntimeSessionExecutionAnchor, WorkflowGraph,
    };
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

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
    }

    #[async_trait]
    impl AgentRunFrameConstructionPort for FrameRepo {
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
                        message: "test frame repo only supports DispatchLaunchAnchor".to_string(),
                    },
                );
            };
            let next_revision = self
                .items
                .lock()
                .unwrap()
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
            let mut outcome =
                agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome::new(
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

        async fn list_by_run(&self, _run_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct AnchorRepo {
        items: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for AnchorRepo {
        async fn create_once(
            &self,
            anchor: &RuntimeSessionExecutionAnchor,
        ) -> Result<(), DomainError> {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items
                .iter()
                .find(|item| item.runtime_session_id == anchor.runtime_session_id)
            {
                if existing.has_same_launch_coordinates_as(anchor) {
                    return Ok(());
                }
                return Err(existing.immutable_conflict(anchor));
            }
            items.push(anchor.clone());
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
    }

    #[derive(Default)]
    struct AccessRepo {
        items: Mutex<Vec<ProjectBackendAccess>>,
    }

    #[async_trait]
    impl ProjectBackendAccessRepository for AccessRepo {
        async fn create(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            self.items.lock().unwrap().push(access.clone());
            Ok(())
        }

        async fn update(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            if let Some(existing) = self
                .items
                .lock()
                .unwrap()
                .iter_mut()
                .find(|item| item.id == access.id)
            {
                *existing = access.clone();
            }
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|access| access.id == id)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
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
                .items
                .lock()
                .unwrap()
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
                .items
                .lock()
                .unwrap()
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
                .items
                .lock()
                .unwrap()
                .iter_mut()
                .find(|access| access.id == id)
            {
                access.status = status;
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct MailboxRepo {
        items: Mutex<Vec<AgentRunMailboxMessage>>,
        states: Mutex<Vec<AgentRunMailboxState>>,
    }

    #[async_trait]
    impl AgentRunMailboxRepository for MailboxRepo {
        async fn create_message(
            &self,
            message: NewAgentRunMailboxMessage,
        ) -> Result<AgentRunMailboxMessage, DomainError> {
            let now = Utc::now();
            let mut items = self.items.lock().unwrap();
            let order_key = i64::try_from(items.len()).unwrap_or_default() + 1;
            let item = AgentRunMailboxMessage {
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
                order_key,
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
            };
            items.push(item.clone());
            Ok(item)
        }

        async fn create_message_idempotent(
            &self,
            message: NewAgentRunMailboxMessage,
        ) -> Result<AgentRunMailboxMessage, DomainError> {
            if let Some(source_dedup_key) = message.source_dedup_key.as_deref()
                && let Some(existing) = self
                    .items
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|item| {
                        item.run_id == message.run_id
                            && item.agent_id == message.agent_id
                            && item.source_dedup_key.as_deref() == Some(source_dedup_key)
                    })
                    .cloned()
            {
                return Ok(existing);
            }
            self.create_message(message).await
        }

        async fn get_message(
            &self,
            id: Uuid,
        ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.id == id)
                .cloned())
        }

        async fn list_messages(
            &self,
            run_id: Uuid,
            agent_id: Uuid,
        ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
            Ok(self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.run_id == run_id && item.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn claim_next(
            &self,
            request: AgentRunMailboxClaimRequest,
        ) -> Result<Vec<AgentRunMailboxMessage>, DomainError> {
            let mut items = self.items.lock().unwrap();
            let limit = usize::try_from(request.limit).unwrap_or_default();
            let mut claimed = Vec::new();
            for item in items
                .iter_mut()
                .filter(|item| {
                    item.run_id == request.run_id
                        && item.agent_id == request.agent_id
                        && request.barriers.contains(&item.barrier)
                        && request
                            .drain_mode
                            .is_none_or(|mode| item.drain_mode == mode)
                        && matches!(
                            item.status,
                            MailboxMessageStatus::Queued | MailboxMessageStatus::ReadyToConsume
                        )
                })
                .take(limit)
            {
                if let Some(runtime_session_id) = request.runtime_session_id.clone() {
                    item.runtime_session_id = Some(runtime_session_id);
                }
                item.status = MailboxMessageStatus::Consuming;
                item.claim_token = Some(request.claim_token);
                item.claimed_at = Some(Utc::now());
                item.claim_expires_at = Some(request.claim_expires_at);
                item.attempt_count += 1;
                item.updated_at = Utc::now();
                claimed.push(item.clone());
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
            let mut items = self.items.lock().unwrap();
            let item = items.iter_mut().find(|item| item.id == id).ok_or_else(|| {
                DomainError::NotFound {
                    entity: "agent_run_mailbox_message",
                    id: id.to_string(),
                }
            })?;
            if claim_token.is_some() && item.claim_token != claim_token {
                return Err(DomainError::Conflict {
                    entity: "agent_run_mailbox_message",
                    constraint: "claim_token",
                    message: "claim token mismatch".to_string(),
                });
            }
            item.status = status;
            item.accepted_agent_run_turn_id = accepted_agent_run_turn_id;
            item.accepted_protocol_turn_id = accepted_protocol_turn_id;
            item.last_error = last_error;
            item.claim_token = None;
            item.claimed_at = None;
            item.claim_expires_at = None;
            if matches!(
                status,
                MailboxMessageStatus::Dispatched
                    | MailboxMessageStatus::Steered
                    | MailboxMessageStatus::Blocked
                    | MailboxMessageStatus::Failed
                    | MailboxMessageStatus::Deleted
            ) {
                item.consumed_at = Some(Utc::now());
            }
            item.updated_at = Utc::now();
            Ok(item.clone())
        }

        async fn update_message_policy(
            &self,
            id: Uuid,
            delivery: MailboxDelivery,
            barrier: ConsumptionBarrier,
            drain_mode: MailboxDrainMode,
            priority: i32,
        ) -> Result<AgentRunMailboxMessage, DomainError> {
            let mut items = self.items.lock().unwrap();
            let item = items.iter_mut().find(|item| item.id == id).ok_or_else(|| {
                DomainError::NotFound {
                    entity: "agent_run_mailbox_message",
                    id: id.to_string(),
                }
            })?;
            item.delivery = delivery;
            item.barrier = barrier;
            item.drain_mode = drain_mode;
            item.priority = priority;
            item.updated_at = Utc::now();
            Ok(item.clone())
        }

        async fn delete_message(
            &self,
            id: Uuid,
        ) -> Result<Option<AgentRunMailboxMessage>, DomainError> {
            let mut items = self.items.lock().unwrap();
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return Ok(None);
            };
            item.status = MailboxMessageStatus::Deleted;
            item.deleted_at = Some(Utc::now());
            item.updated_at = Utc::now();
            Ok(Some(item.clone()))
        }

        async fn cleanup_user_payload(&self, id: Uuid) -> Result<(), DomainError> {
            if let Some(item) = self
                .items
                .lock()
                .unwrap()
                .iter_mut()
                .find(|item| item.id == id && !item.retain_payload)
            {
                item.payload_json = None;
                item.updated_at = Utc::now();
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
                runtime_session_id,
                paused: true,
                pause_reason: Some(reason),
                pause_message: message,
                backend_selection_preference: self
                    .states
                    .lock()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|state| state.run_id == run_id && state.agent_id == agent_id)
                    .and_then(|state| state.backend_selection_preference.clone()),
                updated_at: Utc::now(),
            };
            self.states.lock().unwrap().push(state.clone());
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
                runtime_session_id,
                paused: false,
                pause_reason: None,
                pause_message: None,
                backend_selection_preference: self
                    .states
                    .lock()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|state| state.run_id == run_id && state.agent_id == agent_id)
                    .and_then(|state| state.backend_selection_preference.clone()),
                updated_at: Utc::now(),
            };
            self.states.lock().unwrap().push(state.clone());
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
                .unwrap()
                .iter()
                .rev()
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
            let state = AgentRunMailboxState {
                run_id,
                agent_id,
                runtime_session_id,
                paused: false,
                pause_reason: None,
                pause_message: None,
                backend_selection_preference: Some(preference),
                updated_at: Utc::now(),
            };
            self.states.lock().unwrap().push(state.clone());
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

    #[derive(Default)]
    struct RuntimeCreator {
        metas: Mutex<HashMap<String, SessionMeta>>,
    }

    #[async_trait]
    impl RuntimeSessionCreationPort for RuntimeCreator {
        async fn create_runtime_session(
            &self,
            _request: runtime_session_delivery_port::RuntimeSessionCreationRequest,
        ) -> Result<
            runtime_session_delivery_port::RuntimeSessionCreationResult,
            runtime_session_delivery_port::RuntimeSessionDeliveryError,
        > {
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
            Ok(
                runtime_session_delivery_port::RuntimeSessionCreationResult {
                    runtime_session_id: id,
                },
            )
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

    struct TestProjectAgentLifecycleLaunch<'a> {
        lifecycle_run_repo: &'a RunRepo,
        lifecycle_agent_repo: &'a AgentRepo,
        execution_anchor_repo: &'a AnchorRepo,
        delivery_binding_repo: &'a dyn AgentRunDeliveryBindingRepository,
        runtime_session_creator: &'a RuntimeCreator,
        agent_frame_construction: &'a FrameRepo,
    }

    impl<'a> TestProjectAgentLifecycleLaunch<'a> {
        fn new(
            lifecycle_run_repo: &'a RunRepo,
            lifecycle_agent_repo: &'a AgentRepo,
            execution_anchor_repo: &'a AnchorRepo,
            delivery_binding_repo: &'a dyn AgentRunDeliveryBindingRepository,
            runtime_session_creator: &'a RuntimeCreator,
            agent_frame_construction: &'a FrameRepo,
        ) -> Self {
            Self {
                lifecycle_run_repo,
                lifecycle_agent_repo,
                execution_anchor_repo,
                delivery_binding_repo,
                runtime_session_creator,
                agent_frame_construction,
            }
        }
    }

    #[async_trait]
    impl ProjectAgentLifecycleLaunchPort for TestProjectAgentLifecycleLaunch<'_> {
        async fn launch_project_agent(
            &self,
            intent: &AgentLaunchIntent,
        ) -> Result<AgentLaunchDispatchResult, WorkflowApplicationError> {
            let run = LifecycleRun::new_plain(intent.project_id);
            self.lifecycle_run_repo.create(&run).await?;

            let agent =
                LifecycleAgent::new_root(run.id, intent.project_id, AgentSource::ProjectAgent);
            self.lifecycle_agent_repo.create(&agent).await?;

            let runtime_session = self
                .runtime_session_creator
                .create_runtime_session(
                    runtime_session_delivery_port::RuntimeSessionCreationRequest {
                        project_id: intent.project_id,
                        run_id: run.id,
                        agent_id: agent.id,
                        source: intent.source.clone(),
                    },
                )
                .await
                .map_err(|error| {
                    WorkflowApplicationError::Internal(format!(
                        "测试 ProjectAgent lifecycle launch 创建 RuntimeSession 失败: {error}"
                    ))
                })?;
            let runtime_session_id = runtime_session.runtime_session_id.to_string();

            let frame = self
                .agent_frame_construction
                .execute_frame_construction_command(
                    agent_frame_materialization_port::FrameConstructionCommand::DispatchLaunchAnchor {
                        run_id: run.id,
                        agent_id: agent.id,
                        runtime_session_id: runtime_session_id.clone(),
                        created_by_id: Some("test_project_agent_lifecycle_launch".to_string()),
                    },
                )
                .await
                .map_err(|error| {
                    WorkflowApplicationError::Internal(format!(
                        "测试 ProjectAgent lifecycle launch 创建 AgentFrame 失败: {error}"
                    ))
                })?;
            let frame_id = frame.frame_id.ok_or_else(|| {
                WorkflowApplicationError::Internal(
                    "测试 ProjectAgent lifecycle launch 未返回 AgentFrame id".to_string(),
                )
            })?;

            let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
                runtime_session_id,
                run.id,
                frame_id,
                agent.id,
            );
            self.execution_anchor_repo.create_once(&anchor).await?;
            let binding = AgentRunDeliveryBinding::from_anchor(
                &anchor,
                DeliveryBindingStatus::Ready,
                anchor.updated_at,
            );
            self.delivery_binding_repo.upsert(&binding).await?;

            Ok(AgentLaunchDispatchResult {
                runtime_refs: AgentRuntimeRefs::new(run.id, agent.id, frame_id, None),
                delivery_runtime_ref: Some(runtime_session.runtime_session_id),
            })
        }
    }

    struct InitialMailboxPort<'a> {
        mailbox_repo: &'a MailboxRepo,
        behavior: InitialMailboxBehavior,
        captured: Arc<Mutex<Vec<AgentRunMailboxUserMessageCommand>>>,
        scheduled: Arc<Mutex<Vec<(Uuid, Uuid, String)>>>,
    }

    #[derive(Debug, Clone, Copy)]
    enum InitialMailboxBehavior {
        Launched,
        Failed,
        Queued,
        WrongRun,
        WrongAgent,
        WrongRuntime,
        MissingTurn,
    }

    impl<'a> InitialMailboxPort<'a> {
        fn with_behavior(mailbox_repo: &'a MailboxRepo, behavior: InitialMailboxBehavior) -> Self {
            Self {
                mailbox_repo,
                behavior,
                captured: Arc::new(Mutex::new(Vec::new())),
                scheduled: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn launched(mailbox_repo: &'a MailboxRepo) -> Self {
            Self::with_behavior(mailbox_repo, InitialMailboxBehavior::Launched)
        }

        fn failed(mailbox_repo: &'a MailboxRepo) -> Self {
            Self::with_behavior(mailbox_repo, InitialMailboxBehavior::Failed)
        }

        fn queued(mailbox_repo: &'a MailboxRepo) -> Self {
            Self::with_behavior(mailbox_repo, InitialMailboxBehavior::Queued)
        }

        fn missing_turn(mailbox_repo: &'a MailboxRepo) -> Self {
            Self::with_behavior(mailbox_repo, InitialMailboxBehavior::MissingTurn)
        }
    }

    #[async_trait]
    impl ProjectAgentRunInitialMailboxPort for InitialMailboxPort<'_> {
        async fn accept_initial_mailbox_message(
            &self,
            command: ProjectAgentRunInitialMailboxCommand,
        ) -> Result<AgentRunMailboxCommandResult, WorkflowApplicationError> {
            self.captured
                .lock()
                .unwrap()
                .push(command.clone().into_mailbox_command());
            let payload_json = serde_json::to_value(&command.input).map_err(|error| {
                WorkflowApplicationError::BadRequest(format!(
                    "test mailbox input 无法序列化: {error}"
                ))
            })?;
            let executor_config_json = command
                .executor_config
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|error| {
                    WorkflowApplicationError::BadRequest(format!(
                        "test mailbox executor_config 无法序列化: {error}"
                    ))
                })?;
            let message = self
                .mailbox_repo
                .create_message_idempotent(NewAgentRunMailboxMessage {
                    run_id: command.run_id,
                    agent_id: command.agent_id,
                    runtime_session_id: Some(command.runtime_session_id.clone()),
                    origin: MailboxMessageOrigin::User,
                    source: MailboxSourceIdentity::draft_start(),
                    delivery: MailboxDelivery::LaunchOrContinueTurn,
                    barrier: ConsumptionBarrier::ImmediateIfIdle,
                    drain_mode: MailboxDrainMode::One,
                    priority: 0,
                    source_dedup_key: Some(format!("initial:{}", command.client_command_id)),
                    queued_agent_run_turn_id: None,
                    expected_active_agent_run_turn_id: None,
                    command_receipt_id: None,
                    payload_json: Some(payload_json),
                    executor_config_json,
                    launch_planning_input: None,
                    preview: "hello".to_string(),
                    has_images: false,
                    retain_payload: false,
                })
                .await?;

            if matches!(self.behavior, InitialMailboxBehavior::Failed) {
                let failed = self
                    .mailbox_repo
                    .mark_message_status(
                        message.id,
                        None,
                        MailboxMessageStatus::Failed,
                        None,
                        None,
                        Some("connector setup failed".to_string()),
                    )
                    .await?;
                assert_eq!(failed.status, MailboxMessageStatus::Failed);
                return Ok(AgentRunMailboxCommandResult {
                    command_receipt: AgentRunCommandReceiptView {
                        client_command_id: command.client_command_id,
                        status: "accepted".to_string(),
                        duplicate: false,
                        message: None,
                    },
                    outcome: AgentRunMailboxCommandOutcome::Failed,
                    mailbox_message: Some(failed),
                    accepted_refs: None,
                    runtime_state: None,
                });
            }

            if matches!(self.behavior, InitialMailboxBehavior::Queued) {
                return Ok(AgentRunMailboxCommandResult {
                    command_receipt: AgentRunCommandReceiptView {
                        client_command_id: command.client_command_id,
                        status: "accepted".to_string(),
                        duplicate: false,
                        message: None,
                    },
                    outcome: AgentRunMailboxCommandOutcome::Queued,
                    mailbox_message: Some(message),
                    accepted_refs: Some(AgentRunAcceptedRefs {
                        run_id: command.run_id,
                        agent_id: command.agent_id,
                        frame_id: None,
                        frame_revision: None,
                        runtime_session_id: Some(command.runtime_session_id),
                        agent_run_turn_id: None,
                        protocol_turn_id: None,
                    }),
                    runtime_state: None,
                });
            }

            let claim_token = Uuid::new_v4();
            let claimed = self
                .mailbox_repo
                .claim_next(AgentRunMailboxClaimRequest {
                    run_id: command.run_id,
                    agent_id: command.agent_id,
                    runtime_session_id: Some(command.runtime_session_id.clone()),
                    barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
                    drain_mode: Some(MailboxDrainMode::One),
                    limit: 1,
                    claim_token,
                    claim_expires_at: Utc::now(),
                })
                .await?;
            assert_eq!(claimed.len(), 1);
            assert_eq!(claimed[0].id, message.id);
            let dispatched = self
                .mailbox_repo
                .mark_message_status(
                    message.id,
                    Some(claim_token),
                    MailboxMessageStatus::Dispatched,
                    match self.behavior {
                        InitialMailboxBehavior::MissingTurn => None,
                        _ => Some("turn-1".to_string()),
                    },
                    None,
                    None,
                )
                .await?;
            Ok(AgentRunMailboxCommandResult {
                command_receipt: AgentRunCommandReceiptView {
                    client_command_id: command.client_command_id,
                    status: "accepted".to_string(),
                    duplicate: false,
                    message: None,
                },
                outcome: AgentRunMailboxCommandOutcome::Launched,
                mailbox_message: Some(dispatched.clone()),
                accepted_refs: Some(AgentRunAcceptedRefs {
                    run_id: match self.behavior {
                        InitialMailboxBehavior::WrongRun => Uuid::new_v4(),
                        _ => command.run_id,
                    },
                    agent_id: match self.behavior {
                        InitialMailboxBehavior::WrongAgent => Uuid::new_v4(),
                        _ => command.agent_id,
                    },
                    frame_id: None,
                    frame_revision: None,
                    runtime_session_id: Some(match self.behavior {
                        InitialMailboxBehavior::WrongRuntime => "wrong-runtime".to_string(),
                        _ => command.runtime_session_id,
                    }),
                    agent_run_turn_id: dispatched.accepted_agent_run_turn_id,
                    protocol_turn_id: dispatched.accepted_protocol_turn_id,
                }),
                runtime_state: None,
            })
        }

        async fn schedule_initial_mailbox_message(
            &self,
            run_id: Uuid,
            agent_id: Uuid,
            runtime_session_id: &str,
            _identity: Option<AuthIdentity>,
        ) -> Result<(), WorkflowApplicationError> {
            self.scheduled
                .lock()
                .unwrap()
                .push((run_id, agent_id, runtime_session_id.to_string()));
            Ok(())
        }
    }

    fn runnable_project_agent(project_id: Uuid) -> ProjectAgent {
        let mut project_agent = ProjectAgent::new(project_id, "agent", "PI_AGENT");
        project_agent.config = serde_json::json!({
            "provider_id": "openai",
            "model_id": "gpt-5",
        });
        project_agent
    }

    struct StartHarness {
        project_id: Uuid,
        project_agent: ProjectAgent,
        project_agent_repo: ProjectAgentRepo,
        run_repo: RunRepo,
        workflow_graph_repo: WorkflowGraphRepo,
        agent_repo: AgentRepo,
        frame_repo: FrameRepo,
        association_repo: AssociationRepo,
        gate_repo: GateRepo,
        lineage_repo: LineageRepo,
        anchor_repo: AnchorRepo,
        access_repo: AccessRepo,
        command_receipt_repo: MemoryAgentRunCommandReceiptRepository,
        delivery_binding_repo: MemoryAgentRunDeliveryBindingRepository,
        mailbox_repo: MailboxRepo,
        runtime_creator: RuntimeCreator,
    }

    impl StartHarness {
        fn new() -> Self {
            let project_id = Uuid::new_v4();
            let project_agent = runnable_project_agent(project_id);
            Self {
                project_id,
                project_agent: project_agent.clone(),
                project_agent_repo: ProjectAgentRepo {
                    agent: Mutex::new(Some(project_agent)),
                },
                run_repo: RunRepo::default(),
                workflow_graph_repo: WorkflowGraphRepo,
                agent_repo: AgentRepo::default(),
                frame_repo: FrameRepo::default(),
                association_repo: AssociationRepo::default(),
                gate_repo: GateRepo,
                lineage_repo: LineageRepo,
                anchor_repo: AnchorRepo::default(),
                access_repo: AccessRepo::default(),
                command_receipt_repo: MemoryAgentRunCommandReceiptRepository::default(),
                delivery_binding_repo: MemoryAgentRunDeliveryBindingRepository::default(),
                mailbox_repo: MailboxRepo::default(),
                runtime_creator: RuntimeCreator::default(),
            }
        }

        fn service_with_initial_mailbox<'a>(
            &'a self,
            initial_mailbox: &'a dyn ProjectAgentRunInitialMailboxPort,
        ) -> ProjectAgentRunStartService<'a> {
            ProjectAgentRunStartService::new_with_initial_mailbox_port(
                ProjectAgentRunStartRepos {
                    project_agent_repo: &self.project_agent_repo,
                    lifecycle_run_repo: &self.run_repo,
                    workflow_graph_repo: &self.workflow_graph_repo,
                    lifecycle_agent_repo: &self.agent_repo,
                    agent_frame_repo: &self.frame_repo,
                    lifecycle_subject_association_repo: &self.association_repo,
                    lifecycle_gate_repo: &self.gate_repo,
                    agent_lineage_repo: &self.lineage_repo,
                    execution_anchor_repo: &self.anchor_repo,
                    project_backend_access_repo: &self.access_repo,
                    command_receipt_repo: &self.command_receipt_repo,
                    delivery_binding_repo: &self.delivery_binding_repo,
                    mailbox_repo: &self.mailbox_repo,
                    runtime_session_creator: &self.runtime_creator,
                    agent_frame_construction: &self.frame_repo,
                    project_agent_lifecycle_launch: self,
                },
                &self.runtime_creator,
                initial_mailbox,
            )
        }

        fn command(&self, client_command_id: &str) -> ProjectAgentRunStartCommand {
            ProjectAgentRunStartCommand {
                project_id: self.project_id,
                project_agent_id: self.project_agent.id,
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: client_command_id.to_string(),
                executor_config: None,
                backend_selection: None,
                subject_ref: None,
                identity: None,
            }
        }

        async fn start(
            &self,
            command: ProjectAgentRunStartCommand,
            initial_message: &dyn ProjectAgentRunInitialMailboxPort,
        ) -> Result<ProjectAgentRunStartDispatch, WorkflowApplicationError> {
            let service = self.service_with_initial_mailbox(initial_message);
            service.start_run(command).await
        }
    }

    #[async_trait]
    impl ProjectAgentLifecycleLaunchPort for StartHarness {
        async fn launch_project_agent(
            &self,
            intent: &AgentLaunchIntent,
        ) -> Result<AgentLaunchDispatchResult, WorkflowApplicationError> {
            TestProjectAgentLifecycleLaunch::new(
                &self.run_repo,
                &self.agent_repo,
                &self.anchor_repo,
                &self.delivery_binding_repo,
                &self.runtime_creator,
                &self.frame_repo,
            )
            .launch_project_agent(intent)
            .await
        }
    }

    #[tokio::test]
    async fn duplicate_start_command_reuses_accepted_run_without_materializing_again() {
        let project_id = Uuid::new_v4();
        let project_agent = runnable_project_agent(project_id);
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
        let access_repo = AccessRepo::default();
        let command_receipt_repo = MemoryAgentRunCommandReceiptRepository::default();
        let delivery_binding_repo = MemoryAgentRunDeliveryBindingRepository::default();
        let mailbox_repo = MailboxRepo::default();
        let runtime_creator = RuntimeCreator::default();
        let lifecycle_launch = TestProjectAgentLifecycleLaunch::new(
            &run_repo,
            &agent_repo,
            &anchor_repo,
            &delivery_binding_repo,
            &runtime_creator,
            &frame_repo,
        );
        let initial_message = InitialMailboxPort::launched(&mailbox_repo);
        let service = ProjectAgentRunStartService::new_with_initial_mailbox_port(
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
                delivery_binding_repo: &delivery_binding_repo,
                project_backend_access_repo: &access_repo,
                command_receipt_repo: &command_receipt_repo,
                mailbox_repo: &mailbox_repo,
                runtime_session_creator: &runtime_creator,
                agent_frame_construction: &frame_repo,
                project_agent_lifecycle_launch: &lifecycle_launch,
            },
            &runtime_creator,
            &initial_message,
        );

        let command = || ProjectAgentRunStartCommand {
            project_id,
            project_agent_id: project_agent.id,
            input: agentdash_agent_protocol::text_user_input_blocks("hello"),
            client_command_id: "cmd-start-1".to_string(),
            executor_config: None,
            backend_selection: None,
            subject_ref: None,
            identity: None,
        };

        let first = service.start_run(command()).await.expect("first start");
        assert_eq!(first.turn_id, "turn-1");
        {
            let mut messages = mailbox_repo.items.lock().unwrap();
            assert_eq!(messages.len(), 1);
            messages[0].accepted_agent_run_turn_id = Some("inner-replayed-turn".to_string());
        }
        let second = service.start_run(command()).await.expect("duplicate start");

        assert_eq!(first.run_id, second.run_id);
        assert_eq!(first.agent_id, second.agent_id);
        assert_eq!(first.runtime_session_id, second.runtime_session_id);
        assert_eq!(first.turn_id, second.turn_id);
        assert_eq!(
            first.initial_message.outcome,
            AgentRunMailboxCommandOutcome::Launched
        );
        assert_eq!(
            second.initial_message.outcome,
            AgentRunMailboxCommandOutcome::Launched
        );
        assert!(!first.command_receipt.duplicate);
        assert!(second.command_receipt.duplicate);
        assert_eq!(initial_message.captured.lock().unwrap().len(), 1);
        assert_eq!(run_repo.items.lock().unwrap().len(), 1);
        assert_eq!(runtime_creator.metas.lock().unwrap().len(), 1);
        let messages = mailbox_repo.items.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].status, MailboxMessageStatus::Dispatched);
        assert_eq!(messages[0].source.namespace, "core");
        assert_eq!(messages[0].source.kind, "draft_start");
        assert_eq!(messages[0].barrier, ConsumptionBarrier::ImmediateIfIdle);
        assert_eq!(
            messages[0].accepted_agent_run_turn_id.as_deref(),
            Some("inner-replayed-turn")
        );
    }

    #[tokio::test]
    async fn initial_mailbox_refs_mismatch_fails_outer_receipt() {
        for behavior in [
            InitialMailboxBehavior::WrongRun,
            InitialMailboxBehavior::WrongAgent,
            InitialMailboxBehavior::WrongRuntime,
        ] {
            let harness = StartHarness::new();
            let initial_message =
                InitialMailboxPort::with_behavior(&harness.mailbox_repo, behavior);

            let error = harness
                .start(harness.command("cmd-start-1"), &initial_message)
                .await
                .expect_err("mismatched initial launch refs should fail start");
            assert!(
                matches!(error, WorkflowApplicationError::Conflict(_)),
                "unexpected error for {behavior:?}: {error:?}"
            );
            assert!(harness.runtime_creator.metas.lock().unwrap().is_empty());
            assert!(harness.run_repo.items.lock().unwrap().is_empty());
            assert!(harness.anchor_repo.items.lock().unwrap().is_empty());
            assert_eq!(initial_message.captured.lock().unwrap().len(), 1);

            let duplicate_error = harness
                .start(harness.command("cmd-start-1"), &initial_message)
                .await
                .expect_err("failed outer receipt should replay failure");
            assert!(matches!(
                duplicate_error,
                WorkflowApplicationError::Conflict(_)
            ));
            assert_eq!(initial_message.captured.lock().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn initial_mailbox_missing_agent_run_turn_id_still_accepts_start_envelope() {
        let harness = StartHarness::new();
        let initial_message = InitialMailboxPort::missing_turn(&harness.mailbox_repo);

        let dispatch = harness
            .start(harness.command("cmd-start-1"), &initial_message)
            .await
            .expect("missing turn id should not fail start envelope");

        assert_eq!(dispatch.turn_id, "");
        assert_eq!(
            dispatch.initial_message.outcome,
            AgentRunMailboxCommandOutcome::Launched
        );
        assert_eq!(
            dispatch
                .initial_message
                .accepted_refs
                .as_ref()
                .and_then(|refs| refs.agent_run_turn_id.as_deref()),
            None
        );
        assert_eq!(harness.runtime_creator.metas.lock().unwrap().len(), 1);
        assert_eq!(harness.run_repo.items.lock().unwrap().len(), 1);
        assert_eq!(harness.anchor_repo.items.lock().unwrap().len(), 1);
        assert_eq!(initial_message.captured.lock().unwrap().len(), 1);

        let duplicate = harness
            .start(harness.command("cmd-start-1"), &initial_message)
            .await
            .expect("duplicate start should replay accepted start envelope");
        assert!(duplicate.command_receipt.duplicate);
        assert_eq!(duplicate.turn_id, "");
        assert_eq!(initial_message.captured.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn queued_initial_mailbox_schedule_is_owned_by_start_service() {
        let harness = StartHarness::new();
        let initial_message = InitialMailboxPort::queued(&harness.mailbox_repo);

        let dispatch = harness
            .start(harness.command("cmd-start-1"), &initial_message)
            .await
            .expect("queued initial message should still accept start envelope");

        assert_eq!(
            dispatch.initial_message.outcome,
            AgentRunMailboxCommandOutcome::Queued
        );
        assert_eq!(initial_message.captured.lock().unwrap().len(), 1);
        let scheduled = initial_message.scheduled.lock().unwrap();
        assert_eq!(scheduled.len(), 1);
        assert_eq!(scheduled[0].0, dispatch.run_id);
        assert_eq!(scheduled[0].1, dispatch.agent_id);
        assert_eq!(scheduled[0].2, dispatch.runtime_session_id);
    }

    #[tokio::test]
    async fn delivery_failure_records_initial_message_outcome_without_failing_start() {
        let project_id = Uuid::new_v4();
        let project_agent = runnable_project_agent(project_id);
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
        let access_repo = AccessRepo::default();
        let command_receipt_repo = MemoryAgentRunCommandReceiptRepository::default();
        let delivery_binding_repo = MemoryAgentRunDeliveryBindingRepository::default();
        let mailbox_repo = MailboxRepo::default();
        let runtime_creator = RuntimeCreator::default();
        let lifecycle_launch = TestProjectAgentLifecycleLaunch::new(
            &run_repo,
            &agent_repo,
            &anchor_repo,
            &delivery_binding_repo,
            &runtime_creator,
            &frame_repo,
        );
        let initial_message = InitialMailboxPort::failed(&mailbox_repo);
        let service = ProjectAgentRunStartService::new_with_initial_mailbox_port(
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
                delivery_binding_repo: &delivery_binding_repo,
                project_backend_access_repo: &access_repo,
                command_receipt_repo: &command_receipt_repo,
                mailbox_repo: &mailbox_repo,
                runtime_session_creator: &runtime_creator,
                agent_frame_construction: &frame_repo,
                project_agent_lifecycle_launch: &lifecycle_launch,
            },
            &runtime_creator,
            &initial_message,
        );

        let dispatch = service
            .start_run(ProjectAgentRunStartCommand {
                project_id,
                project_agent_id: project_agent.id,
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-start-1".to_string(),
                executor_config: None,
                backend_selection: None,
                subject_ref: None,
                identity: None,
            })
            .await
            .expect("delivery failure should still accept start envelope");

        assert_eq!(
            dispatch.initial_message.outcome,
            AgentRunMailboxCommandOutcome::Failed
        );
        assert_eq!(dispatch.command_receipt.status, "accepted");
        assert_eq!(runtime_creator.metas.lock().unwrap().len(), 1);
        assert_eq!(run_repo.items.lock().unwrap().len(), 1);
        assert_eq!(anchor_repo.items.lock().unwrap().len(), 1);
        let messages = mailbox_repo.items.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].status, MailboxMessageStatus::Failed);
    }

    #[tokio::test]
    async fn draft_start_delivery_receives_resolved_executor_provider_and_model() {
        let project_id = Uuid::new_v4();
        let project_agent = runnable_project_agent(project_id);
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
        let access_repo = AccessRepo::default();
        let command_receipt_repo = MemoryAgentRunCommandReceiptRepository::default();
        let delivery_binding_repo = MemoryAgentRunDeliveryBindingRepository::default();
        let mailbox_repo = MailboxRepo::default();
        let runtime_creator = RuntimeCreator::default();
        let lifecycle_launch = TestProjectAgentLifecycleLaunch::new(
            &run_repo,
            &agent_repo,
            &anchor_repo,
            &delivery_binding_repo,
            &runtime_creator,
            &frame_repo,
        );
        let initial_message = InitialMailboxPort::launched(&mailbox_repo);
        let service = ProjectAgentRunStartService::new_with_initial_mailbox_port(
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
                delivery_binding_repo: &delivery_binding_repo,
                project_backend_access_repo: &access_repo,
                command_receipt_repo: &command_receipt_repo,
                mailbox_repo: &mailbox_repo,
                runtime_session_creator: &runtime_creator,
                agent_frame_construction: &frame_repo,
                project_agent_lifecycle_launch: &lifecycle_launch,
            },
            &runtime_creator,
            &initial_message,
        );

        let dispatch = service
            .start_run(ProjectAgentRunStartCommand {
                project_id,
                project_agent_id: project_agent.id,
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-start-1".to_string(),
                executor_config: Some(AgentConfig::new("PI_AGENT")),
                backend_selection: None,
                subject_ref: None,
                identity: None,
            })
            .await
            .expect("resolved draft start");

        let captured = initial_message.captured.lock().unwrap();
        let command = captured.first().expect("captured initial mailbox command");
        assert_eq!(command.source.namespace, "core");
        assert_eq!(command.source.kind, "draft_start");
        assert!(
            !command.schedule_on_submit,
            "ProjectAgent start must create the durable initial mailbox envelope without synchronously scheduling the launch"
        );
        let config = command
            .executor_config
            .clone()
            .expect("mailbox executor config");
        assert_eq!(config.executor, "PI_AGENT");
        assert_eq!(config.provider_id.as_deref(), Some("openai"));
        assert_eq!(config.model_id.as_deref(), Some("gpt-5"));
        assert_eq!(
            dispatch.effective_executor_config.provider_id.as_deref(),
            Some("openai")
        );
        assert_eq!(
            dispatch.effective_executor_config.model_id.as_deref(),
            Some("gpt-5")
        );
    }

    #[tokio::test]
    async fn model_required_stops_before_materializing_runtime() {
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
        let access_repo = AccessRepo::default();
        let command_receipt_repo = MemoryAgentRunCommandReceiptRepository::default();
        let delivery_binding_repo = MemoryAgentRunDeliveryBindingRepository::default();
        let mailbox_repo = MailboxRepo::default();
        let runtime_creator = RuntimeCreator::default();
        let lifecycle_launch = TestProjectAgentLifecycleLaunch::new(
            &run_repo,
            &agent_repo,
            &anchor_repo,
            &delivery_binding_repo,
            &runtime_creator,
            &frame_repo,
        );
        let initial_message = InitialMailboxPort::launched(&mailbox_repo);
        let service = ProjectAgentRunStartService::new_with_initial_mailbox_port(
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
                delivery_binding_repo: &delivery_binding_repo,
                project_backend_access_repo: &access_repo,
                command_receipt_repo: &command_receipt_repo,
                mailbox_repo: &mailbox_repo,
                runtime_session_creator: &runtime_creator,
                agent_frame_construction: &frame_repo,
                project_agent_lifecycle_launch: &lifecycle_launch,
            },
            &runtime_creator,
            &initial_message,
        );

        let error = service
            .start_run(ProjectAgentRunStartCommand {
                project_id,
                project_agent_id: project_agent.id,
                input: agentdash_agent_protocol::text_user_input_blocks("hello"),
                client_command_id: "cmd-start-1".to_string(),
                executor_config: None,
                backend_selection: None,
                subject_ref: None,
                identity: None,
            })
            .await
            .expect_err("missing model should stop start");

        assert!(matches!(error, WorkflowApplicationError::ModelRequired(_)));
        assert!(run_repo.items.lock().unwrap().is_empty());
        assert!(agent_repo.items.lock().unwrap().is_empty());
        assert!(frame_repo.items.lock().unwrap().is_empty());
        assert!(runtime_creator.metas.lock().unwrap().is_empty());
        assert!(anchor_repo.items.lock().unwrap().is_empty());
        assert!(mailbox_repo.items.lock().unwrap().is_empty());
    }
}
