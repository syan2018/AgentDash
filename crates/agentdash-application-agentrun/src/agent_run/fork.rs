use agentdash_agent_protocol::UserInputBlock;
use agentdash_agent_types::MessageRef;
use agentdash_application_ports::agent_run_fork_materialization::{
    AgentRunForkMaterializationError, AgentRunForkMaterializationInput,
};
use agentdash_application_ports::launch::BackendSelectionInput;
use agentdash_application_runtime_session::session::{SessionBranchingService, SessionForkRequest};
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_domain::agent_run_mailbox::{AgentRunMailboxMessage, MailboxSourceIdentity};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentRunAcceptedRefs, AgentRunCommandKind,
    AgentRunCommandReceipt, AgentRunCommandReceiptRepository, AgentRunCommandStatus,
    AgentRunDeliveryBindingRepository, AgentRunLineage, AgentRunLineageRepository, LifecycleAgent,
    LifecycleAgentRepository, LifecycleRun, LifecycleRunRepository,
    RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::AgentConfig;
use agentdash_spi::platform::auth::AuthIdentity;
use serde_json::Value;
use uuid::Uuid;

use crate::agent_run::command_receipt::{
    claim_agent_run_command_receipt, digest_command_request, mark_command_terminal_failed,
};
use crate::agent_run::mailbox::{outcome_from_message, outcome_from_result_json};
use crate::agent_run::runtime_session_boundary::SessionCoreService;
use crate::agent_run::{
    AgentRunCommandReceiptView, AgentRunMailboxCommandOutcome, AgentRunMailboxCommandTarget,
    AgentRunMailboxService, AgentRunMailboxUserMessageTargetCommand, DeliveryRuntimeSelection,
    DeliveryRuntimeSelectionError, DeliveryRuntimeSelectionRepositories,
    DeliveryRuntimeSelectionService,
};
use crate::agent_run_repository_set::RepositorySet;
use crate::error::WorkflowApplicationError;

pub struct AgentRunForkRepos<'a> {
    pub lifecycle_run_repo: &'a dyn LifecycleRunRepository,
    pub lifecycle_agent_repo: &'a dyn LifecycleAgentRepository,
    pub agent_frame_repo: &'a dyn AgentFrameRepository,
    pub execution_anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    pub delivery_binding_repo: &'a dyn AgentRunDeliveryBindingRepository,
    pub agent_run_command_receipt_repo: &'a dyn AgentRunCommandReceiptRepository,
    pub agent_run_mailbox_repo: &'a dyn agentdash_domain::agent_run_mailbox::AgentRunMailboxRepository,
    pub agent_run_lineage_repo: &'a dyn AgentRunLineageRepository,
    pub agent_run_fork_materialization: &'a dyn agentdash_application_ports::agent_run_fork_materialization::AgentRunForkMaterializationPort,
}

impl<'a> AgentRunForkRepos<'a> {
    pub fn from_repository_set(repos: &'a RepositorySet) -> Self {
        Self {
            lifecycle_run_repo: repos.lifecycle_run_repo.as_ref(),
            lifecycle_agent_repo: repos.lifecycle_agent_repo.as_ref(),
            agent_frame_repo: repos.agent_frame_repo.as_ref(),
            execution_anchor_repo: repos.execution_anchor_repo.as_ref(),
            delivery_binding_repo: repos.agent_run_delivery_binding_repo.as_ref(),
            agent_run_command_receipt_repo: repos.agent_run_command_receipt_repo.as_ref(),
            agent_run_mailbox_repo: repos.agent_run_mailbox_repo.as_ref(),
            agent_run_lineage_repo: repos.agent_run_lineage_repo.as_ref(),
            agent_run_fork_materialization: repos.agent_run_fork_materialization.as_ref(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentRunForkCommand {
    pub parent_run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub current_user_id: String,
    pub title: Option<String>,
    pub fork_point_ref: Option<MessageRef>,
    pub metadata_json: Option<Value>,
    pub client_command_id: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunForkSubmitCommand {
    pub parent_run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub current_user_id: String,
    pub title: Option<String>,
    pub fork_point_ref: Option<MessageRef>,
    pub metadata_json: Option<Value>,
    pub input: Vec<UserInputBlock>,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub backend_selection: Option<BackendSelectionInput>,
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone)]
pub struct AgentRunForkCommandResult {
    pub command_receipt: AgentRunCommandReceiptView,
    pub parent_refs: AgentRunAcceptedRefs,
    pub child_refs: AgentRunAcceptedRefs,
    pub lineage: AgentRunLineage,
    pub mailbox_outcome: Option<AgentRunMailboxCommandOutcome>,
    pub mailbox_message: Option<AgentRunMailboxMessage>,
}

pub struct AgentRunForkService<'a> {
    repos: AgentRunForkRepos<'a>,
    session_branching: SessionBranchingService,
    session_core: SessionCoreService,
    mailbox: AgentRunMailboxService<'a>,
}

impl<'a> AgentRunForkService<'a> {
    pub fn new(
        repos: &'a RepositorySet,
        session_branching: SessionBranchingService,
        session_core: SessionCoreService,
        mailbox: AgentRunMailboxService<'a>,
    ) -> Self {
        Self::from_repos(
            AgentRunForkRepos::from_repository_set(repos),
            session_branching,
            session_core,
            mailbox,
        )
    }

    pub fn from_repos(
        repos: AgentRunForkRepos<'a>,
        session_branching: SessionBranchingService,
        session_core: SessionCoreService,
        mailbox: AgentRunMailboxService<'a>,
    ) -> Self {
        Self {
            repos,
            session_branching,
            session_core,
            mailbox,
        }
    }

    pub(crate) async fn explicit_fork(
        &self,
        command: AgentRunForkCommand,
    ) -> Result<AgentRunForkCommandResult, WorkflowApplicationError> {
        self.execute(command.into_execution(None)).await
    }

    pub(crate) async fn fork_submit(
        &self,
        command: AgentRunForkSubmitCommand,
    ) -> Result<AgentRunForkCommandResult, WorkflowApplicationError> {
        if command.input.is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "input 不能为空".to_string(),
            ));
        }
        let AgentRunForkSubmitCommand {
            parent_run_id,
            parent_agent_id,
            current_user_id,
            title,
            fork_point_ref,
            metadata_json,
            input,
            client_command_id,
            executor_config,
            backend_selection,
            identity,
        } = command;
        let input = ForkSubmitInput {
            input,
            executor_config,
            backend_selection,
            identity,
        };
        self.execute(AgentRunForkExecutionCommand {
            parent_run_id,
            parent_agent_id,
            current_user_id,
            title,
            fork_point_ref,
            metadata_json,
            client_command_id,
            command_kind: AgentRunCommandKind::AgentRunForkSubmit,
            submit: Some(input),
        })
        .await
    }

    async fn execute(
        &self,
        command: AgentRunForkExecutionCommand,
    ) -> Result<AgentRunForkCommandResult, WorkflowApplicationError> {
        if command.client_command_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "client_command_id 不能为空".to_string(),
            ));
        }
        if command.current_user_id.trim().is_empty() {
            return Err(WorkflowApplicationError::BadRequest(
                "current_user_id 不能为空".to_string(),
            ));
        }
        let log_context = AgentRunForkLogContext::from_command(&command);

        let parent = self
            .resolve_parent(command.parent_run_id, command.parent_agent_id)
            .await
            .inspect_err(|error| {
                log_agent_run_fork_stage_error("resolve_parent", &log_context, None, None, error);
            })?;
        let request_digest = digest_command_request(&serde_json::json!({
            "kind": command.command_kind.as_str(),
            "current_user_id": &command.current_user_id,
            "parent": {
                "run_id": parent.run.id,
                "agent_id": parent.agent.id,
                "frame_id": parent.frame.id,
                "runtime_session_id": parent.runtime_session_id,
            },
            "fork_point_ref": &command.fork_point_ref,
            "metadata_json": &command.metadata_json,
            "input": command.submit.as_ref().map(|submit| &submit.input),
            "executor_config": command.submit.as_ref().and_then(|submit| submit.executor_config.as_ref()),
            "backend_selection": command.submit.as_ref().and_then(|submit| submit.backend_selection.as_ref()),
        }))
        .inspect_err(|error| {
            log_agent_run_fork_stage_error(
                "request_digest",
                &log_context,
                Some(&parent),
                None,
                error,
            );
        })?;
        let claim = claim_agent_run_command_receipt(
            self.repos.agent_run_command_receipt_repo,
            "agent_run_fork",
            format!(
                "{}:{}:{}",
                command.current_user_id, parent.run.id, parent.agent.id
            ),
            command.command_kind,
            command.client_command_id.clone(),
            request_digest,
        )
        .await
        .inspect_err(|error| {
            log_agent_run_fork_stage_error(
                "receipt_claim",
                &log_context,
                Some(&parent),
                None,
                error,
            );
        })?;
        if claim.duplicate {
            return self.replay_duplicate(claim.record).await;
        }

        let fork_result = match self
            .session_branching
            .fork_session(SessionForkRequest {
                parent_session_id: parent.runtime_session_id.clone(),
                title: command.title.clone(),
                fork_point_ref: command.fork_point_ref.clone(),
                fork_point_compaction_id: None,
                metadata_json: command.metadata_json.clone().unwrap_or_else(|| {
                    serde_json::json!({
                        "parent_run_id": parent.run.id,
                        "parent_agent_id": parent.agent.id,
                        "forked_by_user_id": command.current_user_id,
                    })
                }),
            })
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let app_error = workflow_error_from_session_fork(error);
                log_agent_run_fork_stage_error(
                    "session_branching",
                    &log_context,
                    Some(&parent),
                    None,
                    &app_error,
                );
                mark_command_terminal_failed(
                    self.repos.agent_run_command_receipt_repo,
                    claim.record.id,
                    &app_error,
                )
                .await;
                return Err(app_error);
            }
        };

        let materialized = match self
            .repos
            .agent_run_fork_materialization
            .materialize_forked_agent_run(AgentRunForkMaterializationInput {
                parent_run: parent.run.clone(),
                parent_agent: parent.agent.clone(),
                parent_frame: parent.frame.clone(),
                parent_runtime_session_id: parent.runtime_session_id.clone(),
                child_runtime_session_id: fork_result.child_session.id.clone(),
                fork_point_event_seq: fork_result.lineage.fork_point_event_seq,
                fork_point_ref_json: command
                    .fork_point_ref
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?,
                forked_by_user_id: command.current_user_id.clone(),
                metadata_json: command.metadata_json.clone(),
            })
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let app_error = workflow_error_from_materialization(error);
                log_agent_run_fork_stage_error(
                    "materialization",
                    &log_context,
                    Some(&parent),
                    Some(&fork_result.child_session.id),
                    &app_error,
                );
                self.cleanup_child_runtime(&fork_result.child_session.id)
                    .await;
                mark_command_terminal_failed(
                    self.repos.agent_run_command_receipt_repo,
                    claim.record.id,
                    &app_error,
                )
                .await;
                return Err(app_error);
            }
        };

        let parent_refs = base_refs(
            &parent.run,
            &parent.agent,
            Some(&parent.frame),
            &parent.runtime_session_id,
        );
        let mut child_refs = materialized.accepted_refs();
        let mut mailbox_message = None;
        let mut mailbox_outcome = None;
        if let Some(submit) = command.submit {
            let mailbox_result = match self
                .mailbox
                .accept_user_message_for_target(AgentRunMailboxUserMessageTargetCommand {
                    target: AgentRunMailboxCommandTarget::from_runtime_session_adapter(
                        materialized.child_run.id,
                        materialized.child_agent.id,
                        materialized.child_frame.id,
                        materialized.child_runtime_session_id.clone(),
                    ),
                    source: MailboxSourceIdentity::composer(),
                    schedule_on_submit: true,
                    input: submit.input,
                    client_command_id: format!("{}:fork-submit-message", command.client_command_id),
                    executor_config: submit.executor_config,
                    backend_selection: submit.backend_selection,
                    identity: submit.identity,
                    delivery_intent: None,
                })
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    log_agent_run_fork_stage_error(
                        "child_mailbox_submit",
                        &log_context,
                        Some(&parent),
                        Some(&materialized.child_runtime_session_id),
                        &error,
                    );
                    mark_command_terminal_failed(
                        self.repos.agent_run_command_receipt_repo,
                        claim.record.id,
                        &error,
                    )
                    .await;
                    return Err(error);
                }
            };
            if let Some(refs) = mailbox_result.accepted_refs.clone() {
                child_refs.agent_run_turn_id = refs.agent_run_turn_id;
                child_refs.protocol_turn_id = refs.protocol_turn_id;
            }
            mailbox_outcome = Some(mailbox_result.outcome);
            mailbox_message = mailbox_result.mailbox_message;
        }

        if let Some(message) = mailbox_message.as_ref() {
            let _ = self
                .repos
                .agent_run_command_receipt_repo
                .attach_mailbox_message(claim.record.id, message.id)
                .await
                .inspect_err(|error| {
                    log_agent_run_fork_stage_error(
                        "receipt_attach_mailbox_message",
                        &log_context,
                        Some(&parent),
                        Some(&materialized.child_runtime_session_id),
                        error,
                    );
                })?;
        }
        let accepted = self
            .repos
            .agent_run_command_receipt_repo
            .mark_accepted(claim.record.id, child_refs.clone())
            .await
            .inspect_err(|error| {
                log_agent_run_fork_stage_error(
                    "receipt_mark_accepted",
                    &log_context,
                    Some(&parent),
                    Some(&materialized.child_runtime_session_id),
                    error,
                );
            })?;
        let result_json = fork_result_json(
            &parent_refs,
            &child_refs,
            mailbox_outcome,
            mailbox_message.as_ref().map(|message| message.id),
        );
        let stored = self
            .repos
            .agent_run_command_receipt_repo
            .store_result_json(claim.record.id, result_json)
            .await
            .inspect_err(|error| {
                log_agent_run_fork_stage_error(
                    "receipt_store_result",
                    &log_context,
                    Some(&parent),
                    Some(&materialized.child_runtime_session_id),
                    error,
                );
            })?;
        let receipt = if stored.updated_at >= accepted.updated_at {
            stored
        } else {
            accepted
        };

        Ok(AgentRunForkCommandResult {
            command_receipt: AgentRunCommandReceiptView::from_record(&receipt, false),
            parent_refs,
            child_refs,
            lineage: materialized.lineage,
            mailbox_outcome,
            mailbox_message,
        })
    }

    async fn replay_duplicate(
        &self,
        record: AgentRunCommandReceipt,
    ) -> Result<AgentRunForkCommandResult, WorkflowApplicationError> {
        match record.status {
            AgentRunCommandStatus::Pending => {
                return Err(WorkflowApplicationError::Conflict(
                    "AgentRun fork 命令仍在处理中，请稍后重试".to_string(),
                ));
            }
            AgentRunCommandStatus::TerminalFailed => {
                return Err(WorkflowApplicationError::Conflict(
                    record
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "AgentRun fork 命令已失败".to_string()),
                ));
            }
            AgentRunCommandStatus::Accepted => {}
        }

        let result = record.result_json.as_ref().ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "AgentRun fork receipt {} 缺少 result_json",
                record.id
            ))
        })?;
        let parent_refs = accepted_refs_from_result_json(result, "parent")?;
        let child_refs = record.accepted_refs.clone().ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "AgentRun fork receipt {} 缺少 accepted refs",
                record.id
            ))
        })?;
        let lineage = self
            .repos
            .agent_run_lineage_repo
            .find_parent(child_refs.run_id, child_refs.agent_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::Internal(format!(
                    "AgentRun fork canonical lineage missing for child {}:{}",
                    child_refs.run_id, child_refs.agent_id
                ))
            })?;
        let mailbox_message = match record.mailbox_message_id {
            Some(id) => self.repos.agent_run_mailbox_repo.get_message(id).await?,
            None => None,
        };
        let mailbox_outcome = mailbox_message
            .as_ref()
            .map(outcome_from_message)
            .or_else(|| result.get("mailbox").and_then(outcome_from_result_json));

        Ok(AgentRunForkCommandResult {
            command_receipt: AgentRunCommandReceiptView::from_record(&record, true),
            parent_refs,
            child_refs,
            lineage,
            mailbox_outcome,
            mailbox_message,
        })
    }

    async fn resolve_parent(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<ResolvedForkParent, WorkflowApplicationError> {
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("AgentRun {run_id} 不存在"))
            })?;
        let agent = self
            .repos
            .lifecycle_agent_repo
            .get(agent_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("LifecycleAgent {agent_id} 不存在"))
            })?;
        if agent.run_id != run.id {
            return Err(WorkflowApplicationError::Conflict(format!(
                "LifecycleAgent {} 不属于 AgentRun {}",
                agent.id, run.id
            )));
        }
        let selection =
            DeliveryRuntimeSelectionService::new(DeliveryRuntimeSelectionRepositories {
                lifecycle_runs: self.repos.lifecycle_run_repo,
                lifecycle_agents: self.repos.lifecycle_agent_repo,
                agent_frames: self.repos.agent_frame_repo,
                execution_anchors: self.repos.execution_anchor_repo,
                delivery_bindings: self.repos.delivery_binding_repo,
            })
            .select_current_delivery(run.id, agent.id)
            .await
            .map_err(workflow_error_from_selection_error)?;
        let frame = self
            .repos
            .agent_frame_repo
            .get(selection.current_frame_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "AgentFrame {} 不存在",
                    selection.current_frame_id
                ))
            })?;
        Ok(ResolvedForkParent {
            run,
            agent,
            frame,
            runtime_session_id: selection.runtime_session_id.clone(),
            _selection: selection,
        })
    }

    async fn cleanup_child_runtime(&self, runtime_session_id: &str) {
        let _ = self.session_core.delete_session(runtime_session_id).await;
    }
}

#[derive(Debug, Clone)]
struct ForkSubmitInput {
    input: Vec<UserInputBlock>,
    executor_config: Option<AgentConfig>,
    backend_selection: Option<BackendSelectionInput>,
    identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone)]
struct AgentRunForkExecutionCommand {
    parent_run_id: Uuid,
    parent_agent_id: Uuid,
    current_user_id: String,
    title: Option<String>,
    fork_point_ref: Option<MessageRef>,
    metadata_json: Option<Value>,
    client_command_id: String,
    command_kind: AgentRunCommandKind,
    submit: Option<ForkSubmitInput>,
}

impl AgentRunForkCommand {
    fn into_execution(self, submit: Option<ForkSubmitInput>) -> AgentRunForkExecutionCommand {
        AgentRunForkExecutionCommand {
            parent_run_id: self.parent_run_id,
            parent_agent_id: self.parent_agent_id,
            current_user_id: self.current_user_id,
            title: self.title,
            fork_point_ref: self.fork_point_ref,
            metadata_json: self.metadata_json,
            client_command_id: self.client_command_id,
            command_kind: AgentRunCommandKind::AgentRunFork,
            submit,
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedForkParent {
    run: LifecycleRun,
    agent: LifecycleAgent,
    frame: AgentFrame,
    runtime_session_id: String,
    _selection: DeliveryRuntimeSelection,
}

#[derive(Debug, Clone)]
struct AgentRunForkLogContext {
    command_kind: &'static str,
    parent_run_id: Uuid,
    parent_agent_id: Uuid,
    current_user_id: String,
    client_command_id: String,
    fork_point: String,
}

impl AgentRunForkLogContext {
    fn from_command(command: &AgentRunForkExecutionCommand) -> Self {
        Self {
            command_kind: command.command_kind.as_str(),
            parent_run_id: command.parent_run_id,
            parent_agent_id: command.parent_agent_id,
            current_user_id: command.current_user_id.clone(),
            client_command_id: command.client_command_id.clone(),
            fork_point: message_ref_log_label(command.fork_point_ref.as_ref()),
        }
    }
}

fn log_agent_run_fork_stage_error<E>(
    stage: &'static str,
    context: &AgentRunForkLogContext,
    parent: Option<&ResolvedForkParent>,
    child_runtime_session_id: Option<&str>,
    error: &E,
) where
    E: std::fmt::Debug + std::fmt::Display,
{
    let parent_run_id = parent
        .map(|parent| parent.run.id)
        .unwrap_or(context.parent_run_id);
    let parent_agent_id = parent
        .map(|parent| parent.agent.id)
        .unwrap_or(context.parent_agent_id);
    let parent_frame_id = parent
        .map(|parent| parent.frame.id.to_string())
        .unwrap_or_else(|| "unresolved".to_string());
    let parent_runtime_session_id = parent
        .map(|parent| parent.runtime_session_id.as_str())
        .unwrap_or("unresolved");
    let child_runtime_session_id = child_runtime_session_id.unwrap_or("unavailable");
    let error_context =
        agent_run_fork_stage_error_context(stage, context, parent, Some(child_runtime_session_id));
    diag_error!(Error, Subsystem::AgentRun,
        context = &error_context,
        error = error,
        command_kind = context.command_kind,
        parent_run_id = %parent_run_id,
        parent_agent_id = %parent_agent_id,
        parent_frame_id = %parent_frame_id,
        parent_runtime_session_id = %parent_runtime_session_id,
        child_runtime_session_id = %child_runtime_session_id,
        current_user_id = %context.current_user_id,
        client_command_id = %context.client_command_id,
        fork_point = %context.fork_point,
        "AgentRun fork service stage failed"
    );
}

fn agent_run_fork_stage_error_context(
    stage: &str,
    _context: &AgentRunForkLogContext,
    _parent: Option<&ResolvedForkParent>,
    _child_runtime_session_id: Option<&str>,
) -> DiagnosticErrorContext {
    DiagnosticErrorContext::new("agent_run.fork", stage)
}

fn message_ref_log_label(value: Option<&MessageRef>) -> String {
    value
        .map(|message_ref| format!("{}:{}", message_ref.turn_id, message_ref.entry_index))
        .unwrap_or_else(|| "head".to_string())
}

fn base_refs(
    run: &LifecycleRun,
    agent: &LifecycleAgent,
    frame: Option<&AgentFrame>,
    runtime_session_id: &str,
) -> AgentRunAcceptedRefs {
    AgentRunAcceptedRefs {
        run_id: run.id,
        agent_id: agent.id,
        frame_id: frame.map(|frame| frame.id),
        frame_revision: frame.map(|frame| frame.revision),
        runtime_session_id: Some(runtime_session_id.to_string()),
        agent_run_turn_id: None,
        protocol_turn_id: None,
    }
}

fn fork_result_json(
    parent_refs: &AgentRunAcceptedRefs,
    child_refs: &AgentRunAcceptedRefs,
    mailbox_outcome: Option<AgentRunMailboxCommandOutcome>,
    mailbox_message_id: Option<Uuid>,
) -> Value {
    serde_json::json!({
        "outcome": "forked",
        "parent": refs_json(parent_refs),
        "child": refs_json(child_refs),
        "mailbox": {
            "outcome": mailbox_outcome.map(|outcome| outcome.as_str()),
            "mailbox_message_id": mailbox_message_id,
        },
        "redirect": {
            "run_id": child_refs.run_id,
            "agent_id": child_refs.agent_id,
        },
    })
}

fn refs_json(refs: &AgentRunAcceptedRefs) -> Value {
    serde_json::json!({
        "run_id": refs.run_id,
        "agent_id": refs.agent_id,
        "frame_id": refs.frame_id,
        "frame_revision": refs.frame_revision,
        "runtime_session_id": refs.runtime_session_id,
        "agent_run_turn_id": refs.agent_run_turn_id,
        "protocol_turn_id": refs.protocol_turn_id,
    })
}

fn accepted_refs_from_result_json(
    result: &Value,
    key: &str,
) -> Result<AgentRunAcceptedRefs, WorkflowApplicationError> {
    let value = result.get(key).ok_or_else(|| {
        WorkflowApplicationError::Internal(format!("AgentRun fork result_json 缺少 {key} refs"))
    })?;
    let run_id = uuid_from_json(value, "run_id")?;
    let agent_id = uuid_from_json(value, "agent_id")?;
    Ok(AgentRunAcceptedRefs {
        run_id,
        agent_id,
        frame_id: optional_uuid_from_json(value, "frame_id")?,
        frame_revision: value
            .get("frame_revision")
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        runtime_session_id: value
            .get("runtime_session_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        agent_run_turn_id: value
            .get("agent_run_turn_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        protocol_turn_id: value
            .get("protocol_turn_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn uuid_from_json(value: &Value, key: &str) -> Result<Uuid, WorkflowApplicationError> {
    let raw = value.get(key).and_then(Value::as_str).ok_or_else(|| {
        WorkflowApplicationError::Internal(format!("AgentRun fork result_json 缺少 {key}"))
    })?;
    Uuid::parse_str(raw).map_err(|error| {
        WorkflowApplicationError::Internal(format!("AgentRun fork result_json {key} 无效: {error}"))
    })
}

fn optional_uuid_from_json(
    value: &Value,
    key: &str,
) -> Result<Option<Uuid>, WorkflowApplicationError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|raw| {
            Uuid::parse_str(raw).map_err(|error| {
                WorkflowApplicationError::Internal(format!(
                    "AgentRun fork result_json {key} 无效: {error}"
                ))
            })
        })
        .transpose()
}

fn workflow_error_from_session_fork(error: std::io::Error) -> WorkflowApplicationError {
    match error.kind() {
        std::io::ErrorKind::NotFound => WorkflowApplicationError::NotFound(error.to_string()),
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => {
            WorkflowApplicationError::BadRequest(error.to_string())
        }
        _ => WorkflowApplicationError::Internal(error.to_string()),
    }
}

fn workflow_error_from_materialization(
    error: AgentRunForkMaterializationError,
) -> WorkflowApplicationError {
    match error {
        AgentRunForkMaterializationError::Rejected { message } => {
            WorkflowApplicationError::Conflict(message)
        }
        AgentRunForkMaterializationError::Internal { message } => {
            WorkflowApplicationError::Internal(message)
        }
    }
}

fn workflow_error_from_selection_error(
    error: DeliveryRuntimeSelectionError,
) -> WorkflowApplicationError {
    match error {
        DeliveryRuntimeSelectionError::RunNotFound { .. }
        | DeliveryRuntimeSelectionError::AgentNotFound { .. }
        | DeliveryRuntimeSelectionError::CurrentFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::LaunchFrameNotFound { .. }
        | DeliveryRuntimeSelectionError::SubjectNotFound { .. } => {
            WorkflowApplicationError::NotFound(error.to_string())
        }
        DeliveryRuntimeSelectionError::Repository(source) => WorkflowApplicationError::from(source),
        other => WorkflowApplicationError::Conflict(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, sync::Arc};

    use agentdash_agent_protocol::{
        BackboneEnvelope, SourceInfo, UserInputBlock, UserInputSubmissionKind,
        text_user_input_blocks,
    };
    use agentdash_application_ports::launch::{LaunchCommand, LaunchPlanningInput};
    use agentdash_application_runtime_session::session::{
        SessionRuntimeBuilder, TitleSource,
        persistence::{
            AgentFrameTransitionRecord, CompactionProjectionCommitResult,
            NewCompactionProjectionCommit, NewTerminalEffectRecord, PersistedSessionEvent,
            RuntimeCommandRecord, RuntimeCommandStatus, RuntimeDeliveryCommand,
            SessionCompactionRecord, SessionCompactionStore, SessionEventBacklog, SessionEventPage,
            SessionEventStore, SessionLineageRecord, SessionLineageRelationKind,
            SessionLineageStatus, SessionLineageStore, SessionMeta, SessionMetaStore,
            SessionProjectionHeadRecord, SessionProjectionSegmentRecord, SessionProjectionStore,
            SessionRuntimeCommandStore, SessionStoreError, SessionStoreResult, SessionStoreSet,
            SessionTerminalEffectStore, TerminalEffectRecord, TerminalEffectStatus,
        },
    };
    use agentdash_domain::agent_run_mailbox::MailboxMessageStatus;
    use agentdash_domain::workflow::{
        AgentFrame, AgentFrameRepository, AgentRunCommandKind, AgentRunCommandReceipt,
        AgentRunCommandReceiptRepository, AgentRunCommandStatus, AgentRunDeliveryBinding,
        AgentRunDeliveryBindingRepository, AgentRunLineageRepository, AgentSource,
        DeliveryBindingStatus, LifecycleAgent, LifecycleAgentRepository, LifecycleRun,
        LifecycleRunRepository, NewAgentRunCommandReceipt, RuntimeSessionExecutionAnchor,
        RuntimeSessionExecutionAnchorRepository,
    };
    use agentdash_spi::session_persistence::ExecutionStatus;
    use agentdash_spi::{
        AgentConnector, AgentInfo, ConnectorCapabilities, ConnectorError, ConnectorType,
        ExecutionContext, ExecutionStream, PromptPayload,
    };
    use tokio::sync::Mutex;
    use tokio_stream::wrappers::ReceiverStream;

    use super::*;
    use crate::agent_run::command_receipt::digest_command_request;
    use crate::agent_run::runtime_session_boundary::{
        RuntimeSessionControlPort, RuntimeSessionCorePort, RuntimeSessionEventingPort,
        RuntimeSessionLaunchPort, SessionControlService, SessionCoreService,
        SessionEventingService, SessionExecutionState, SessionLaunchService,
        SessionTurnSteerCommand,
    };
    use crate::test_support::{
        MemoryAgentFrameRepository, MemoryAgentRunCommandReceiptRepository,
        MemoryAgentRunDeliveryBindingRepository, MemoryAgentRunForkMaterialization,
        MemoryAgentRunLineageRepository, MemoryAgentRunMailboxRepository,
        MemoryLifecycleAgentRepository, MemoryLifecycleRunRepository, MemoryProjectAgentRepository,
        MemoryProjectBackendAccessRepository, MemoryRuntimeSessionExecutionAnchorRepository,
    };

    #[tokio::test]
    async fn explicit_fork_materializes_child_agent_run_and_cross_run_lineage() {
        let fixture = ForkFixture::new().await;

        let result = fixture
            .admission()
            .admit_explicit_fork(AgentRunForkCommand {
                parent_run_id: fixture.parent_run.id,
                parent_agent_id: fixture.parent_agent.id,
                current_user_id: "user-child".to_string(),
                title: Some("child fork".to_string()),
                fork_point_ref: None,
                metadata_json: Some(serde_json::json!({ "reason": "test" })),
                client_command_id: "fork-explicit".to_string(),
            })
            .await
            .expect("fork succeeds");

        assert_eq!(result.parent_refs.run_id, fixture.parent_run.id);
        assert_ne!(result.child_refs.run_id, fixture.parent_run.id);
        assert_eq!(result.lineage.parent_run_id, fixture.parent_run.id);
        assert_eq!(result.lineage.parent_agent_id, fixture.parent_agent.id);
        assert_eq!(result.lineage.forked_by_user_id, "user-child");

        let child_run = fixture
            .runs
            .get_by_id(result.child_refs.run_id)
            .await
            .expect("read child run")
            .expect("child run");
        assert_eq!(child_run.created_by_user_id, "user-child");

        let child_agent = fixture
            .agents
            .get(result.child_refs.agent_id)
            .await
            .expect("read child agent")
            .expect("child agent");
        assert_eq!(child_agent.created_by_user_id, "user-child");
        let child_binding = fixture
            .delivery_bindings
            .get_current(result.child_refs.run_id, result.child_refs.agent_id)
            .await
            .expect("read child binding")
            .expect("child binding");
        assert_eq!(
            Some(child_binding.runtime_session_id.as_str()),
            result.child_refs.runtime_session_id.as_deref()
        );

        let lineage = fixture
            .lineages
            .find_parent(result.child_refs.run_id, result.child_refs.agent_id)
            .await
            .expect("read lineage")
            .expect("lineage");
        assert_eq!(lineage.id, result.lineage.id);

        let child_runtime_session_id = result
            .child_refs
            .runtime_session_id
            .as_deref()
            .expect("child accepted refs should include runtime trace");
        let runtime_lineage = fixture
            .session_store
            .get_session_lineage(child_runtime_session_id)
            .await
            .expect("runtime lineage")
            .expect("runtime lineage");
        assert_eq!(
            runtime_lineage.parent_session_id,
            fixture.parent_runtime_session_id
        );
        assert_eq!(
            runtime_lineage.relation_kind,
            SessionLineageRelationKind::Fork
        );
        assert_eq!(result.command_receipt.status, "accepted");
        assert!(!result.command_receipt.duplicate);
    }

    #[tokio::test]
    async fn fork_submit_delivers_initial_input_to_child_mailbox_and_leaves_parent_unchanged() {
        let fixture = ForkFixture::new().await;

        let result = fixture
            .admission()
            .admit_fork_submit(AgentRunForkSubmitCommand {
                parent_run_id: fixture.parent_run.id,
                parent_agent_id: fixture.parent_agent.id,
                current_user_id: "fork-user".to_string(),
                title: None,
                fork_point_ref: None,
                metadata_json: None,
                input: text_user_input_blocks("hello child"),
                client_command_id: "fork-submit".to_string(),
                executor_config: None,
                backend_selection: None,
                identity: None,
            })
            .await
            .expect("fork-submit succeeds");

        let child_messages = fixture
            .mailbox
            .messages_for(result.child_refs.run_id, result.child_refs.agent_id)
            .await;
        assert_eq!(child_messages.len(), 1);
        assert_eq!(
            child_messages[0].runtime_session_id.as_deref(),
            result.child_refs.runtime_session_id.as_deref()
        );
        assert_eq!(child_messages[0].status, MailboxMessageStatus::Dispatched);
        assert_eq!(
            child_messages[0].accepted_agent_run_turn_id.as_deref(),
            Some("launched-turn")
        );

        assert!(
            fixture
                .mailbox
                .messages_for(fixture.parent_run.id, fixture.parent_agent.id)
                .await
                .is_empty()
        );
        let parent_binding = fixture
            .delivery_bindings
            .get_current(fixture.parent_run.id, fixture.parent_agent.id)
            .await
            .expect("read parent binding")
            .expect("parent binding");
        assert_eq!(
            Some(parent_binding.runtime_session_id.as_str()),
            Some(fixture.parent_runtime_session_id.as_str())
        );
        assert!(matches!(
            result.mailbox_outcome,
            Some(AgentRunMailboxCommandOutcome::Launched)
        ));
    }

    #[tokio::test]
    async fn duplicate_explicit_fork_replays_accepted_result() {
        let fixture = ForkFixture::new().await;

        let command = AgentRunForkCommand {
            parent_run_id: fixture.parent_run.id,
            parent_agent_id: fixture.parent_agent.id,
            current_user_id: "user-child".to_string(),
            title: None,
            fork_point_ref: None,
            metadata_json: None,
            client_command_id: "fork-replay".to_string(),
        };
        let first = fixture
            .admission()
            .admit_explicit_fork(command.clone())
            .await
            .expect("first fork");
        let receipts = fixture.receipts_for_client("fork-replay").await;
        let result_json = receipts[0]
            .result_json
            .as_ref()
            .expect("stored fork result");
        assert!(result_json.get("lineage").is_none());

        let replay = fixture
            .admission()
            .admit_explicit_fork(command)
            .await
            .expect("replay fork");

        assert!(replay.command_receipt.duplicate);
        assert_eq!(replay.child_refs.run_id, first.child_refs.run_id);
        assert_eq!(replay.child_refs.agent_id, first.child_refs.agent_id);
        assert_eq!(replay.lineage.id, first.lineage.id);
    }

    #[tokio::test]
    async fn accepted_duplicate_fork_errors_when_canonical_lineage_is_missing() {
        let fixture = ForkFixture::new().await;
        let command = AgentRunForkCommand {
            parent_run_id: fixture.parent_run.id,
            parent_agent_id: fixture.parent_agent.id,
            current_user_id: "user-child".to_string(),
            title: None,
            fork_point_ref: None,
            metadata_json: None,
            client_command_id: "fork-missing-lineage".to_string(),
        };
        let receipt = fixture
            .seed_pending_fork_receipt(
                &command.current_user_id,
                &command.client_command_id,
                AgentRunCommandKind::AgentRunFork,
                None,
                None,
            )
            .await;
        let parent_refs = base_refs(
            &fixture.parent_run,
            &fixture.parent_agent,
            Some(&fixture.parent_frame),
            &fixture.parent_runtime_session_id,
        );
        let child_refs = AgentRunAcceptedRefs {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            frame_id: None,
            frame_revision: None,
            runtime_session_id: Some("runtime-child-missing-lineage".to_string()),
            agent_run_turn_id: None,
            protocol_turn_id: None,
        };
        fixture
            .receipts
            .mark_accepted(receipt.id, child_refs.clone())
            .await
            .expect("mark accepted");
        fixture
            .receipts
            .store_result_json(
                receipt.id,
                fork_result_json(&parent_refs, &child_refs, None, None),
            )
            .await
            .expect("store result");

        let error = fixture
            .admission()
            .admit_explicit_fork(command)
            .await
            .expect_err("canonical lineage is required for replay");

        assert!(
            matches!(error, WorkflowApplicationError::Internal(message) if message.contains("canonical lineage missing"))
        );
        assert_eq!(fixture.session_store.created_child_count().await, 0);
    }

    #[tokio::test]
    async fn pending_duplicate_fork_conflicts_before_runtime_branch() {
        let fixture = ForkFixture::new().await;
        let command = AgentRunForkCommand {
            parent_run_id: fixture.parent_run.id,
            parent_agent_id: fixture.parent_agent.id,
            current_user_id: "user-child".to_string(),
            title: None,
            fork_point_ref: None,
            metadata_json: None,
            client_command_id: "fork-pending".to_string(),
        };
        fixture
            .seed_pending_fork_receipt(
                &command.current_user_id,
                &command.client_command_id,
                AgentRunCommandKind::AgentRunFork,
                None,
                None,
            )
            .await;

        let error = fixture
            .admission()
            .admit_explicit_fork(command)
            .await
            .expect_err("pending duplicate conflicts");

        assert!(matches!(error, WorkflowApplicationError::Conflict(_)));
        assert_eq!(fixture.session_store.created_child_count().await, 0);
    }

    #[tokio::test]
    async fn terminal_failed_duplicate_replays_failure_without_new_child() {
        let fixture = ForkFixture::new().await;
        let command = AgentRunForkCommand {
            parent_run_id: fixture.parent_run.id,
            parent_agent_id: fixture.parent_agent.id,
            current_user_id: "user-child".to_string(),
            title: None,
            fork_point_ref: None,
            metadata_json: None,
            client_command_id: "fork-terminal-failed".to_string(),
        };
        let receipt = fixture
            .seed_pending_fork_receipt(
                &command.current_user_id,
                &command.client_command_id,
                AgentRunCommandKind::AgentRunFork,
                None,
                None,
            )
            .await;
        fixture
            .receipts
            .mark_terminal_failed(receipt.id, "materialization failed".to_string())
            .await
            .expect("mark failed");

        let error = fixture
            .admission()
            .admit_explicit_fork(command)
            .await
            .expect_err("terminal failure replays");

        assert!(
            matches!(error, WorkflowApplicationError::Conflict(message) if message.contains("materialization failed"))
        );
        assert_eq!(fixture.session_store.created_child_count().await, 0);
    }

    #[tokio::test]
    async fn materialization_failure_cleans_up_child_runtime_and_marks_receipt_failed() {
        let fixture = ForkFixture::new().await;
        fixture
            .materialization
            .fail_next("boom after runtime fork")
            .await;

        let error = fixture
            .admission()
            .admit_explicit_fork(AgentRunForkCommand {
                parent_run_id: fixture.parent_run.id,
                parent_agent_id: fixture.parent_agent.id,
                current_user_id: "user-child".to_string(),
                title: None,
                fork_point_ref: None,
                metadata_json: None,
                client_command_id: "fork-cleanup".to_string(),
            })
            .await
            .expect_err("materialization fails");

        assert!(matches!(error, WorkflowApplicationError::Internal(_)));
        assert_eq!(fixture.core.deleted_count().await, 1);
        let receipts = fixture.receipts_for_client("fork-cleanup").await;
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].status, AgentRunCommandStatus::TerminalFailed);
    }

    struct ForkFixture {
        runs: Arc<MemoryLifecycleRunRepository>,
        agents: Arc<MemoryLifecycleAgentRepository>,
        project_agents: Arc<MemoryProjectAgentRepository>,
        frames: Arc<MemoryAgentFrameRepository>,
        anchors: Arc<MemoryRuntimeSessionExecutionAnchorRepository>,
        delivery_bindings: Arc<MemoryAgentRunDeliveryBindingRepository>,
        backend_access: Arc<MemoryProjectBackendAccessRepository>,
        receipts: Arc<MemoryAgentRunCommandReceiptRepository>,
        mailbox: Arc<MemoryAgentRunMailboxRepository>,
        lineages: Arc<MemoryAgentRunLineageRepository>,
        materialization: Arc<MemoryAgentRunForkMaterialization>,
        session_store: Arc<TestSessionStore>,
        core: Arc<TestCorePort>,
        control: Arc<TestControlPort>,
        eventing: Arc<TestEventingPort>,
        launch: Arc<TestLaunchPort>,
        parent_run: LifecycleRun,
        parent_agent: LifecycleAgent,
        parent_frame: AgentFrame,
        parent_runtime_session_id: String,
    }

    impl ForkFixture {
        async fn new() -> Self {
            let runs = Arc::new(MemoryLifecycleRunRepository::default());
            let agents = Arc::new(MemoryLifecycleAgentRepository::default());
            let project_agents = Arc::new(MemoryProjectAgentRepository::default());
            let frames = Arc::new(MemoryAgentFrameRepository::default());
            let anchors = Arc::new(MemoryRuntimeSessionExecutionAnchorRepository::default());
            let delivery_bindings = Arc::new(MemoryAgentRunDeliveryBindingRepository::default());
            let backend_access = Arc::new(MemoryProjectBackendAccessRepository::default());
            let receipts = Arc::new(MemoryAgentRunCommandReceiptRepository::default());
            let mailbox = Arc::new(MemoryAgentRunMailboxRepository::default());
            let lineages = Arc::new(MemoryAgentRunLineageRepository::default());
            let materialization = Arc::new(MemoryAgentRunForkMaterialization::new(
                runs.clone(),
                agents.clone(),
                frames.clone(),
                anchors.clone(),
                delivery_bindings.clone(),
                lineages.clone(),
            ));
            let session_store = Arc::new(TestSessionStore::default());
            let core = Arc::new(TestCorePort::default());
            let control = Arc::new(TestControlPort);
            let eventing = Arc::new(TestEventingPort);
            let launch = Arc::new(TestLaunchPort);

            let parent_runtime_session_id = "runtime-parent".to_string();
            session_store
                .create_session(&session_meta(&parent_runtime_session_id))
                .await
                .expect("parent runtime");

            let parent_run = LifecycleRun::new_plain_for_user(Uuid::new_v4(), "parent-owner");
            runs.create(&parent_run).await.expect("parent run");
            let parent_agent = LifecycleAgent::new_root_for_user(
                parent_run.id,
                parent_run.project_id,
                AgentSource::ProjectAgent,
                "parent-owner",
            );
            let launch_frame = AgentFrame::new_initial(parent_agent.id);
            let parent_frame = AgentFrame::new_revision(parent_agent.id, 2, "test");
            let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
                parent_runtime_session_id.clone(),
                parent_run.id,
                launch_frame.id,
                parent_agent.id,
            );
            let parent_binding = AgentRunDeliveryBinding::from_anchor(
                &anchor,
                DeliveryBindingStatus::Ready,
                anchor.updated_at,
            );
            frames.create(&launch_frame).await.expect("launch frame");
            frames.create(&parent_frame).await.expect("parent frame");
            anchors.create_once(&anchor).await.expect("anchor");
            delivery_bindings
                .upsert(&parent_binding)
                .await
                .expect("parent binding");
            agents.create(&parent_agent).await.expect("parent agent");

            Self {
                runs,
                agents,
                project_agents,
                frames,
                anchors,
                delivery_bindings,
                backend_access,
                receipts,
                mailbox,
                lineages,
                materialization,
                session_store,
                core,
                control,
                eventing,
                launch,
                parent_run,
                parent_agent,
                parent_frame,
                parent_runtime_session_id,
            }
        }

        fn service(&self) -> AgentRunForkService<'_> {
            let repos = AgentRunForkRepos {
                lifecycle_run_repo: self.runs.as_ref(),
                lifecycle_agent_repo: self.agents.as_ref(),
                agent_frame_repo: self.frames.as_ref(),
                execution_anchor_repo: self.anchors.as_ref(),
                delivery_binding_repo: self.delivery_bindings.as_ref(),
                agent_run_command_receipt_repo: self.receipts.as_ref(),
                agent_run_mailbox_repo: self.mailbox.as_ref(),
                agent_run_lineage_repo: self.lineages.as_ref(),
                agent_run_fork_materialization: self.materialization.as_ref(),
            };
            let mailbox = AgentRunMailboxService::new(
                self.runs.as_ref(),
                self.agents.as_ref(),
                self.project_agents.as_ref(),
                self.frames.as_ref(),
                self.anchors.as_ref(),
                self.delivery_bindings.as_ref(),
                self.backend_access.as_ref(),
                self.receipts.as_ref(),
                self.mailbox.as_ref(),
                SessionCoreService::new(self.core.clone()),
                SessionControlService::new(self.control.clone()),
                SessionEventingService::new(self.eventing.clone()),
                SessionLaunchService::new(self.launch.clone()),
            );
            let branching = SessionRuntimeBuilder::new_with_hooks_and_stores(
                Arc::new(NoopConnector),
                None,
                self.session_store.store_set(),
            )
            .branching_service();
            AgentRunForkService::from_repos(
                repos,
                branching,
                SessionCoreService::new(self.core.clone()),
                mailbox,
            )
        }

        fn admission(&self) -> crate::agent_run::AgentRunAdmissionService<'_> {
            crate::agent_run::AgentRunAdmissionService::for_fork(self.service())
        }

        async fn seed_pending_fork_receipt(
            &self,
            current_user_id: &str,
            client_command_id: &str,
            command_kind: AgentRunCommandKind,
            input: Option<&Vec<UserInputBlock>>,
            metadata_json: Option<&Value>,
        ) -> AgentRunCommandReceipt {
            let request_digest = digest_command_request(&serde_json::json!({
                "kind": command_kind.as_str(),
                "current_user_id": current_user_id,
                "parent": {
                    "run_id": self.parent_run.id,
                    "agent_id": self.parent_agent.id,
                    "frame_id": self.parent_frame.id,
                    "runtime_session_id": self.parent_runtime_session_id,
                },
                "fork_point_ref": Value::Null,
                "metadata_json": metadata_json,
                "input": input,
                "executor_config": Value::Null,
                "backend_selection": Value::Null,
            }))
            .expect("digest");
            self.receipts
                .claim(NewAgentRunCommandReceipt {
                    scope_kind: "agent_run_fork".to_string(),
                    scope_key: format!(
                        "{}:{}:{}",
                        current_user_id, self.parent_run.id, self.parent_agent.id
                    ),
                    command_kind,
                    client_command_id: client_command_id.to_string(),
                    request_digest,
                })
                .await
                .expect("claim")
                .receipt()
                .clone()
        }

        async fn receipts_for_client(
            &self,
            client_command_id: &str,
        ) -> Vec<AgentRunCommandReceipt> {
            self.receipts
                .debug_list()
                .await
                .into_iter()
                .filter(|receipt| receipt.client_command_id == client_command_id)
                .collect()
        }
    }

    struct NoopConnector;

    #[async_trait::async_trait]
    impl AgentConnector for NoopConnector {
        fn connector_id(&self) -> &'static str {
            "noop"
        }

        fn connector_type(&self) -> ConnectorType {
            ConnectorType::LocalExecutor
        }

        fn capabilities(&self) -> ConnectorCapabilities {
            ConnectorCapabilities::default()
        }

        fn list_executors(&self) -> Vec<AgentInfo> {
            Vec::new()
        }

        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: ExecutionContext,
        ) -> Result<ExecutionStream, ConnectorError> {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(Box::pin(ReceiverStream::new(rx)))
        }

        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestCorePort {
        deleted: Mutex<Vec<String>>,
    }

    impl TestCorePort {
        async fn deleted_count(&self) -> usize {
            self.deleted.lock().await.len()
        }
    }

    #[async_trait::async_trait]
    impl RuntimeSessionCorePort for TestCorePort {
        async fn inspect_session_execution_state(
            &self,
            _session_id: &str,
        ) -> Result<SessionExecutionState, WorkflowApplicationError> {
            Ok(SessionExecutionState::Idle)
        }

        async fn get_session_meta(
            &self,
            _session_id: &str,
        ) -> Result<
            Option<crate::agent_run::runtime_session_boundary::SessionMeta>,
            WorkflowApplicationError,
        > {
            Ok(None)
        }

        async fn delete_session(&self, session_id: &str) -> Result<(), WorkflowApplicationError> {
            self.deleted.lock().await.push(session_id.to_string());
            Ok(())
        }
    }

    struct TestControlPort;

    #[async_trait::async_trait]
    impl RuntimeSessionControlPort for TestControlPort {
        async fn supports_session_steering(&self, _session_id: &str) -> bool {
            false
        }

        async fn steer_session(
            &self,
            _command: SessionTurnSteerCommand,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    struct TestEventingPort;

    #[async_trait::async_trait]
    impl RuntimeSessionEventingPort for TestEventingPort {
        async fn list_event_page(
            &self,
            _session_id: &str,
            _after_seq: u64,
            _limit: u32,
        ) -> std::io::Result<crate::agent_run::runtime_session_boundary::SessionEventPage> {
            Ok(
                crate::agent_run::runtime_session_boundary::SessionEventPage {
                    snapshot_seq: 0,
                    events: Vec::new(),
                    has_more: false,
                    next_after_seq: 0,
                },
            )
        }

        async fn persist_notification(
            &self,
            _session_id: &str,
            _envelope: BackboneEnvelope,
        ) -> Result<(), WorkflowApplicationError> {
            Ok(())
        }

        async fn emit_user_input_submitted(
            &self,
            _session_id: &str,
            _turn_id: &str,
            _event_id: &str,
            _kind: UserInputSubmissionKind,
            _input: Vec<UserInputBlock>,
        ) -> Result<(), WorkflowApplicationError> {
            Ok(())
        }
    }

    struct TestLaunchPort;

    #[async_trait::async_trait]
    impl RuntimeSessionLaunchPort for TestLaunchPort {
        async fn launch_command_in_task(
            &self,
            _session_id: String,
            _command: LaunchCommand,
            _planning_input: LaunchPlanningInput,
        ) -> Result<String, WorkflowApplicationError> {
            Ok("launched-turn".to_string())
        }
    }

    #[derive(Default)]
    struct TestSessionStore {
        metas: Mutex<HashMap<String, SessionMeta>>,
        events: Mutex<HashMap<String, Vec<PersistedSessionEvent>>>,
        compactions: Mutex<HashMap<(String, String), SessionCompactionRecord>>,
        segments: Mutex<Vec<SessionProjectionSegmentRecord>>,
        heads: Mutex<HashMap<(String, String), SessionProjectionHeadRecord>>,
        lineages: Mutex<HashMap<String, SessionLineageRecord>>,
    }

    impl TestSessionStore {
        fn store_set(self: &Arc<Self>) -> SessionStoreSet {
            SessionStoreSet {
                meta: self.clone(),
                events: self.clone(),
                terminal_effects: self.clone(),
                runtime_commands: self.clone(),
                compactions: self.clone(),
                projections: self.clone(),
                lineage: self.clone(),
            }
        }

        async fn created_child_count(&self) -> usize {
            self.metas
                .lock()
                .await
                .keys()
                .filter(|id| id.as_str() != "runtime-parent")
                .count()
        }
    }

    #[async_trait::async_trait]
    impl SessionMetaStore for TestSessionStore {
        async fn create_session(&self, meta: &SessionMeta) -> SessionStoreResult<()> {
            self.metas
                .lock()
                .await
                .insert(meta.id.clone(), meta.clone());
            Ok(())
        }

        async fn get_session_meta(
            &self,
            session_id: &str,
        ) -> SessionStoreResult<Option<SessionMeta>> {
            Ok(self.metas.lock().await.get(session_id).cloned())
        }

        async fn list_sessions(&self) -> SessionStoreResult<Vec<SessionMeta>> {
            Ok(self.metas.lock().await.values().cloned().collect())
        }

        async fn save_session_meta(&self, meta: &SessionMeta) -> SessionStoreResult<()> {
            self.metas
                .lock()
                .await
                .insert(meta.id.clone(), meta.clone());
            Ok(())
        }

        async fn delete_session(&self, session_id: &str) -> SessionStoreResult<()> {
            self.metas.lock().await.remove(session_id);
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl SessionEventStore for TestSessionStore {
        async fn append_event(
            &self,
            session_id: &str,
            envelope: &BackboneEnvelope,
        ) -> SessionStoreResult<PersistedSessionEvent> {
            let mut events = self.events.lock().await;
            let list = events.entry(session_id.to_string()).or_default();
            let event = persisted_event(session_id, list.len() as u64 + 1, envelope.clone());
            list.push(event.clone());
            Ok(event)
        }

        async fn read_backlog(
            &self,
            session_id: &str,
            after_seq: u64,
        ) -> SessionStoreResult<SessionEventBacklog> {
            let events = self.list_events_from(session_id, after_seq + 1).await?;
            Ok(SessionEventBacklog {
                snapshot_seq: events
                    .last()
                    .map(|event| event.event_seq)
                    .unwrap_or(after_seq),
                events,
            })
        }

        async fn list_event_page(
            &self,
            session_id: &str,
            after_seq: u64,
            limit: u32,
        ) -> SessionStoreResult<SessionEventPage> {
            let events = self
                .list_events_from(session_id, after_seq + 1)
                .await?
                .into_iter()
                .take(limit as usize)
                .collect::<Vec<_>>();
            let next_after_seq = events
                .last()
                .map(|event| event.event_seq)
                .unwrap_or(after_seq);
            Ok(SessionEventPage {
                snapshot_seq: next_after_seq,
                has_more: false,
                next_after_seq,
                events,
            })
        }

        async fn list_all_events(
            &self,
            session_id: &str,
        ) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
            Ok(self
                .events
                .lock()
                .await
                .get(session_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn list_events_from(
            &self,
            session_id: &str,
            from_seq: u64,
        ) -> SessionStoreResult<Vec<PersistedSessionEvent>> {
            Ok(self
                .list_all_events(session_id)
                .await?
                .into_iter()
                .filter(|event| event.event_seq >= from_seq)
                .collect())
        }
    }

    #[async_trait::async_trait]
    impl SessionTerminalEffectStore for TestSessionStore {
        async fn insert_terminal_effect(
            &self,
            _effect: NewTerminalEffectRecord,
        ) -> SessionStoreResult<TerminalEffectRecord> {
            Err(SessionStoreError::Internal(
                "unused in fork tests".to_string(),
            ))
        }

        async fn mark_terminal_effect_running(&self, _effect_id: Uuid) -> SessionStoreResult<()> {
            Err(SessionStoreError::Internal(
                "unused in fork tests".to_string(),
            ))
        }

        async fn mark_terminal_effect_succeeded(&self, _effect_id: Uuid) -> SessionStoreResult<()> {
            Err(SessionStoreError::Internal(
                "unused in fork tests".to_string(),
            ))
        }

        async fn mark_terminal_effect_failed(
            &self,
            _effect_id: Uuid,
            _error: String,
        ) -> SessionStoreResult<()> {
            Err(SessionStoreError::Internal(
                "unused in fork tests".to_string(),
            ))
        }

        async fn mark_terminal_effect_dead_letter(
            &self,
            _effect_id: Uuid,
            _error: String,
        ) -> SessionStoreResult<()> {
            Err(SessionStoreError::Internal(
                "unused in fork tests".to_string(),
            ))
        }

        async fn list_terminal_effects_by_status(
            &self,
            _statuses: &[TerminalEffectStatus],
            _limit: u32,
        ) -> SessionStoreResult<Vec<TerminalEffectRecord>> {
            Ok(Vec::new())
        }
    }

    #[async_trait::async_trait]
    impl SessionRuntimeCommandStore for TestSessionStore {
        async fn upsert_runtime_delivery_command(
            &self,
            _delivery_runtime_session_id: &str,
            _delivery: RuntimeDeliveryCommand,
            _frame_transition: AgentFrameTransitionRecord,
        ) -> SessionStoreResult<RuntimeCommandRecord> {
            Err(SessionStoreError::Internal(
                "unused in fork tests".to_string(),
            ))
        }

        async fn list_requested_runtime_commands(
            &self,
            _session_id: &str,
        ) -> SessionStoreResult<Vec<RuntimeCommandRecord>> {
            Ok(Vec::new())
        }

        async fn mark_runtime_commands_applied(
            &self,
            _command_ids: &[Uuid],
        ) -> SessionStoreResult<()> {
            Ok(())
        }

        async fn mark_runtime_commands_failed(
            &self,
            _command_ids: &[Uuid],
            _error: String,
        ) -> SessionStoreResult<()> {
            Ok(())
        }

        async fn list_runtime_commands_by_status(
            &self,
            _statuses: &[RuntimeCommandStatus],
            _limit: u32,
        ) -> SessionStoreResult<Vec<RuntimeCommandRecord>> {
            Ok(Vec::new())
        }
    }

    #[async_trait::async_trait]
    impl SessionCompactionStore for TestSessionStore {
        async fn get_compaction(
            &self,
            session_id: &str,
            compaction_id: &str,
        ) -> SessionStoreResult<Option<SessionCompactionRecord>> {
            Ok(self
                .compactions
                .lock()
                .await
                .get(&(session_id.to_string(), compaction_id.to_string()))
                .cloned())
        }

        async fn list_compactions(
            &self,
            session_id: &str,
            projection_kind: &str,
        ) -> SessionStoreResult<Vec<SessionCompactionRecord>> {
            Ok(self
                .compactions
                .lock()
                .await
                .values()
                .filter(|record| {
                    record.session_id == session_id && record.projection_kind == projection_kind
                })
                .cloned()
                .collect())
        }
    }

    #[async_trait::async_trait]
    impl SessionProjectionStore for TestSessionStore {
        async fn list_projection_segments(
            &self,
            session_id: &str,
            projection_kind: &str,
            projection_version: u64,
        ) -> SessionStoreResult<Vec<SessionProjectionSegmentRecord>> {
            Ok(self
                .segments
                .lock()
                .await
                .iter()
                .filter(|segment| {
                    segment.session_id == session_id
                        && segment.projection_kind == projection_kind
                        && segment.projection_version == projection_version
                })
                .cloned()
                .collect())
        }

        async fn read_projection_head(
            &self,
            session_id: &str,
            projection_kind: &str,
        ) -> SessionStoreResult<Option<SessionProjectionHeadRecord>> {
            Ok(self
                .heads
                .lock()
                .await
                .get(&(session_id.to_string(), projection_kind.to_string()))
                .cloned())
        }

        async fn upsert_projection_head(
            &self,
            head: SessionProjectionHeadRecord,
        ) -> SessionStoreResult<()> {
            self.heads.lock().await.insert(
                (head.session_id.clone(), head.projection_kind.clone()),
                head,
            );
            Ok(())
        }

        async fn commit_compaction_projection(
            &self,
            session_id: &str,
            commit: NewCompactionProjectionCommit,
        ) -> SessionStoreResult<CompactionProjectionCommitResult> {
            let event = self
                .append_event(session_id, &commit.completed_event)
                .await?;
            let mut compaction = commit.compaction;
            compaction.completed_event_seq = Some(event.event_seq);
            compaction.completed_at_ms = Some(event.committed_at_ms);
            self.compactions.lock().await.insert(
                (session_id.to_string(), compaction.id.clone()),
                compaction.clone(),
            );
            self.segments.lock().await.extend(commit.segments.clone());
            let mut head = commit.head;
            head.updated_by_event_seq = Some(event.event_seq);
            head.updated_at_ms = event.committed_at_ms;
            self.upsert_projection_head(head.clone()).await?;
            Ok(CompactionProjectionCommitResult {
                event,
                compaction,
                segments: commit.segments,
                head,
            })
        }
    }

    #[async_trait::async_trait]
    impl SessionLineageStore for TestSessionStore {
        async fn upsert_session_lineage(
            &self,
            record: SessionLineageRecord,
        ) -> SessionStoreResult<()> {
            self.lineages
                .lock()
                .await
                .insert(record.child_session_id.clone(), record);
            Ok(())
        }

        async fn get_session_lineage(
            &self,
            child_session_id: &str,
        ) -> SessionStoreResult<Option<SessionLineageRecord>> {
            Ok(self.lineages.lock().await.get(child_session_id).cloned())
        }

        async fn list_session_children(
            &self,
            parent_session_id: &str,
            relation_kind: Option<SessionLineageRelationKind>,
            status: Option<SessionLineageStatus>,
        ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
            Ok(self
                .lineages
                .lock()
                .await
                .values()
                .filter(|lineage| {
                    lineage.parent_session_id == parent_session_id
                        && relation_kind.is_none_or(|kind| lineage.relation_kind == kind)
                        && status.is_none_or(|status| lineage.status == status)
                })
                .cloned()
                .collect())
        }

        async fn list_session_ancestors(
            &self,
            child_session_id: &str,
        ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
            let mut ancestors = Vec::new();
            let mut cursor = child_session_id.to_string();
            while let Some(lineage) = self.get_session_lineage(&cursor).await? {
                cursor = lineage.parent_session_id.clone();
                ancestors.push(lineage);
            }
            Ok(ancestors)
        }

        async fn list_session_descendants(
            &self,
            root_session_id: &str,
            relation_kind: Option<SessionLineageRelationKind>,
            status: Option<SessionLineageStatus>,
        ) -> SessionStoreResult<Vec<SessionLineageRecord>> {
            self.list_session_children(root_session_id, relation_kind, status)
                .await
        }

        async fn set_session_lineage_status(
            &self,
            child_session_id: &str,
            status: SessionLineageStatus,
            updated_at_ms: i64,
        ) -> SessionStoreResult<()> {
            if let Some(lineage) = self.lineages.lock().await.get_mut(child_session_id) {
                lineage.status = status;
                lineage.updated_at_ms = updated_at_ms;
            }
            Ok(())
        }
    }

    fn session_meta(id: &str) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
            title: id.to_string(),
            title_source: TitleSource::Auto,
            created_at: 1,
            updated_at: 1,
            last_event_seq: 0,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_session_id: None,
        }
    }

    fn persisted_event(
        session_id: &str,
        event_seq: u64,
        notification: BackboneEnvelope,
    ) -> PersistedSessionEvent {
        PersistedSessionEvent {
            session_id: session_id.to_string(),
            event_seq,
            occurred_at_ms: event_seq as i64,
            committed_at_ms: event_seq as i64,
            session_update_type: "test".to_string(),
            turn_id: notification.trace.turn_id.clone(),
            entry_index: notification.trace.entry_index,
            tool_call_id: None,
            ephemeral: false,
            notification,
        }
    }

    #[allow(dead_code)]
    fn platform_source() -> SourceInfo {
        SourceInfo {
            connector_id: "test".to_string(),
            connector_type: "test".to_string(),
            executor_id: None,
        }
    }
}
