use std::sync::Arc;

use agentdash_agent_runtime_contract::PresentationThreadId;
use agentdash_application_ports::agent_run_message_submission::AgentRunMessageSubmissionReservation;
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget;
use agentdash_application_ports::launch::BackendSelectionInput;
use agentdash_application_ports::request_digest::canonical_request_digest;
use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLaunchDispatchResult, AgentLaunchIntent, AgentPolicy,
    CapabilityPolicy, ContextPolicy, ExecutionSource, LifecycleRunRepository, RunPolicy,
    RuntimePolicy, SubjectRef, WorkflowGraphRef,
};
use agentdash_spi::{AgentConfig, AuthIdentity};
use serde::Serialize;
use uuid::Uuid;

use super::{
    AgentRunMessageProductResultProjector, AgentRunMessageSubmissionOwnership,
    AgentRunMessageSubmissionResult, ConversationEffectiveExecutorConfigModel,
    ConversationModelConfigResolver, ProjectAgentInitialMessageSubmission,
    ProjectAgentInitialMessageSubmissionPort, ProjectAgentLifecycleLaunchPort,
    ProjectAgentRunStartReceiptPort, ProjectAgentRunStartReceiptRequest,
    ResolvedProjectAgentContext, build_project_agent_context, replay_agent_run_message_submission,
};
use crate::WorkflowApplicationError;

#[derive(Debug, Clone)]
pub struct ProjectAgentRunStartCommand {
    pub project_id: Uuid,
    pub project_agent_id: Uuid,
    pub input: Vec<agentdash_agent_protocol::UserInputBlock>,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub backend_selection: Option<BackendSelectionInput>,
    pub subject_ref: Option<SubjectRef>,
    pub identity: Option<AuthIdentity>,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectAgentRunStartSemanticRequest {
    kind: &'static str,
    project_id: Uuid,
    project_agent_id: Uuid,
    subject_ref: SubjectRef,
    input: Vec<agentdash_agent_protocol::UserInputBlock>,
    executor_config: Option<AgentConfig>,
    backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone)]
pub struct ProjectAgentRunStartProjectionContext {
    pub client_command_id: String,
    pub project_agent_context: ResolvedProjectAgentContext,
    pub effective_executor_config: ConversationEffectiveExecutorConfigModel,
    pub dispatch: AgentLaunchDispatchResult,
    pub frame_revision: i32,
    pub subject_ref: SubjectRef,
}

pub trait ProjectAgentRunStartProductProjectionPort: Send + Sync {
    fn projector(
        &self,
        context: ProjectAgentRunStartProjectionContext,
    ) -> Result<Arc<dyn AgentRunMessageProductResultProjector>, WorkflowApplicationError>;
}

impl<F> ProjectAgentRunStartProductProjectionPort for F
where
    F: Fn(
            ProjectAgentRunStartProjectionContext,
        )
            -> Result<Arc<dyn AgentRunMessageProductResultProjector>, WorkflowApplicationError>
        + Send
        + Sync,
{
    fn projector(
        &self,
        context: ProjectAgentRunStartProjectionContext,
    ) -> Result<Arc<dyn AgentRunMessageProductResultProjector>, WorkflowApplicationError> {
        self(context)
    }
}

pub trait ProjectAgentExecutionProfilePolicy: Send + Sync {
    fn is_known(&self, profile_id: &str) -> bool;
}

impl<F> ProjectAgentExecutionProfilePolicy for F
where
    F: Fn(&str) -> bool + Send + Sync,
{
    fn is_known(&self, profile_id: &str) -> bool {
        self(profile_id)
    }
}

#[derive(Clone)]
pub struct ProjectAgentRunStartDeps {
    pub project_agents: Arc<dyn ProjectAgentRepository>,
    pub lifecycle_runs: Arc<dyn LifecycleRunRepository>,
    pub frames: Arc<dyn AgentFrameRepository>,
    pub lifecycle_launch: Arc<dyn ProjectAgentLifecycleLaunchPort>,
    pub receipts: Arc<dyn ProjectAgentRunStartReceiptPort>,
    pub initial_submission: Arc<dyn ProjectAgentInitialMessageSubmissionPort>,
    pub execution_profiles: Arc<dyn ProjectAgentExecutionProfilePolicy>,
    pub projection: Arc<dyn ProjectAgentRunStartProductProjectionPort>,
}

#[derive(Clone)]
pub struct ProjectAgentRunStartService {
    deps: ProjectAgentRunStartDeps,
}

impl ProjectAgentRunStartService {
    pub fn new(deps: ProjectAgentRunStartDeps) -> Self {
        Self { deps }
    }

    pub async fn start_run(
        &self,
        mut command: ProjectAgentRunStartCommand,
    ) -> Result<AgentRunMessageSubmissionResult, WorkflowApplicationError> {
        validate_start_command(&command)?;
        let subject_ref = command
            .subject_ref
            .take()
            .unwrap_or_else(|| SubjectRef::new("project", command.project_id));
        validate_project_agent_subject_ref(command.project_id, &subject_ref)?;
        let semantic_request = ProjectAgentRunStartSemanticRequest {
            kind: "project_agent_start",
            project_id: command.project_id,
            project_agent_id: command.project_agent_id,
            subject_ref: subject_ref.clone(),
            input: command.input.clone(),
            executor_config: command.executor_config.clone(),
            backend_selection: command.backend_selection.clone(),
        };
        let request_digest = canonical_request_digest(&semantic_request)
            .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        let reserved_receipt_id = match self
            .deps
            .receipts
            .reserve_project_agent_start(ProjectAgentRunStartReceiptRequest {
                project_id: command.project_id,
                project_agent_id: command.project_agent_id,
                client_command_id: command.client_command_id.clone(),
                request_digest,
            })
            .await?
        {
            AgentRunMessageSubmissionReservation::Created { receipt_id } => receipt_id,
            AgentRunMessageSubmissionReservation::Replay { receipt } => {
                if receipt.status
                    == agentdash_domain::workflow::AgentRunCommandStatus::TerminalFailed
                {
                    return Err(WorkflowApplicationError::Internal(
                        receipt
                            .error_message
                            .unwrap_or_else(|| "Project Agent run start failed".to_string()),
                    ));
                }
                return replay_agent_run_message_submission(receipt, true);
            }
            AgentRunMessageSubmissionReservation::ReconcileRequired { receipt } => {
                return Err(WorkflowApplicationError::Conflict(format!(
                    "Project Agent run start {} is still in progress",
                    receipt.client_command_id
                )));
            }
        };

        let prepared = self.prepare_new_start(command, subject_ref.clone()).await;
        let PreparedProjectAgentStart {
            command,
            subject_ref,
            project_agent_context,
            effective_executor_config,
            effective_config,
            execution_profile_json,
        } = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                return Err(self
                    .abandon_invalid_reservation(reserved_receipt_id, error)
                    .await);
            }
        };

        let dispatch = match self
            .deps
            .lifecycle_launch
            .launch_project_agent(&AgentLaunchIntent {
                project_id: command.project_id,
                source: ExecutionSource::ProjectAgent,
                created_by_user_id: command
                    .identity
                    .as_ref()
                    .map(|identity| identity.user_id.clone()),
                subject_ref: Some(subject_ref.clone()),
                parent_run_id: None,
                parent_agent_id: None,
                project_agent_id: Some(command.project_agent_id),
                execution_profile_override: Some(execution_profile_json),
                workflow_graph_ref: workflow_graph_ref_for_project_agent(
                    &project_agent_context.project_agent,
                ),
                run_policy: RunPolicy::CreateLinkedRun,
                agent_policy: AgentPolicy::Create,
                context_policy: ContextPolicy::Isolated,
                capability_policy: CapabilityPolicy::Baseline,
                runtime_policy: RuntimePolicy::ProvisionRuntimeThread,
            })
            .await
        {
            Ok(dispatch) => dispatch,
            Err(error) => {
                return Err(self.freeze_launch_failure(reserved_receipt_id, error).await);
            }
        };

        self.finish_launched_start(
            reserved_receipt_id,
            command,
            subject_ref,
            project_agent_context,
            effective_executor_config,
            effective_config,
            dispatch,
        )
        .await
    }

    async fn prepare_new_start(
        &self,
        mut command: ProjectAgentRunStartCommand,
        subject_ref: SubjectRef,
    ) -> Result<PreparedProjectAgentStart, WorkflowApplicationError> {
        let project_agent = self
            .deps
            .project_agents
            .get_by_project_and_id(command.project_id, command.project_agent_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!(
                    "Project Agent {} 不存在",
                    command.project_agent_id
                ))
            })?;
        let project_agent_context = build_project_agent_context(&project_agent)
            .await
            .map_err(WorkflowApplicationError::BadRequest)?;
        let model_resolution = ConversationModelConfigResolver::resolve_project_agent_start(
            &project_agent,
            command.executor_config.as_ref(),
        )?;
        let profile_id = model_resolution.config.executor.trim();
        if !self.deps.execution_profiles.is_known(profile_id) {
            return Err(WorkflowApplicationError::BadRequest(format!(
                "未知 execution profile: {profile_id}"
            )));
        }
        let effective_executor_config = model_resolution
            .view
            .effective_executor_config
            .clone()
            .unwrap_or_else(|| {
                ConversationModelConfigResolver::view_for_config(
                    &model_resolution.config,
                    super::ConversationModelConfigSourceModel::ProjectAgentPreset,
                )
            });
        let execution_profile_json = serde_json::to_value(&model_resolution.config)
            .map_err(|error| WorkflowApplicationError::BadRequest(error.to_string()))?;
        command.executor_config = Some(model_resolution.config.clone());
        Ok(PreparedProjectAgentStart {
            command,
            subject_ref,
            project_agent_context,
            effective_executor_config,
            effective_config: model_resolution.config,
            execution_profile_json,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_launched_start(
        &self,
        reserved_receipt_id: Uuid,
        command: ProjectAgentRunStartCommand,
        subject_ref: SubjectRef,
        project_agent_context: ResolvedProjectAgentContext,
        effective_executor_config: ConversationEffectiveExecutorConfigModel,
        effective_config: AgentConfig,
        dispatch: AgentLaunchDispatchResult,
    ) -> Result<AgentRunMessageSubmissionResult, WorkflowApplicationError> {
        let run_id = dispatch.runtime_refs.run_ref;
        let prepared_submission = self.prepare_initial_submission(
            reserved_receipt_id,
            &command,
            &subject_ref,
            project_agent_context,
            effective_executor_config,
            effective_config,
            dispatch,
        );
        let (submission, projector) = match prepared_submission.await {
            Ok(prepared) => prepared,
            Err(error) => {
                return Err(self
                    .cleanup_and_freeze_unattached_failure(reserved_receipt_id, run_id, error)
                    .await);
            }
        };
        match self
            .deps
            .initial_submission
            .submit_initial_message(submission, projector)
            .await
        {
            Ok(result) => Ok(result),
            Err(failure) => match failure.ownership {
                AgentRunMessageSubmissionOwnership::Unattached { .. } => Err(self
                    .cleanup_and_freeze_unattached_failure(
                        reserved_receipt_id,
                        run_id,
                        failure.source,
                    )
                    .await),
                AgentRunMessageSubmissionOwnership::Attached { .. }
                | AgentRunMessageSubmissionOwnership::Unknown { .. } => Err(failure.source),
            },
        }
    }

    async fn prepare_initial_submission(
        &self,
        reserved_receipt_id: Uuid,
        command: &ProjectAgentRunStartCommand,
        subject_ref: &SubjectRef,
        project_agent_context: ResolvedProjectAgentContext,
        effective_executor_config: ConversationEffectiveExecutorConfigModel,
        effective_config: AgentConfig,
        dispatch: AgentLaunchDispatchResult,
    ) -> Result<
        (
            ProjectAgentInitialMessageSubmission,
            Arc<dyn AgentRunMessageProductResultProjector>,
        ),
        WorkflowApplicationError,
    > {
        let frame = self
            .deps
            .frames
            .get(dispatch.runtime_refs.frame_ref)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::Internal("Lifecycle launch 未产出 AgentFrame".to_string())
            })?;
        let projector = self
            .deps
            .projection
            .projector(ProjectAgentRunStartProjectionContext {
                client_command_id: command.client_command_id.clone(),
                project_agent_context,
                effective_executor_config,
                dispatch: dispatch.clone(),
                frame_revision: frame.revision,
                subject_ref: subject_ref.clone(),
            })?;
        let presentation_thread_id =
            PresentationThreadId::new(dispatch.delivery_runtime_ref.to_string())
                .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        Ok((
            ProjectAgentInitialMessageSubmission {
                reserved_receipt_id,
                target: AgentRunRuntimeTarget {
                    run_id: dispatch.runtime_refs.run_ref,
                    agent_id: dispatch.runtime_refs.agent_ref,
                },
                presentation_thread_id,
                client_command_id: command.client_command_id.clone(),
                input: command.input.clone(),
                identity: command.identity.clone(),
                execution_profile_override: effective_config,
                backend_selection: command.backend_selection.clone(),
            },
            projector,
        ))
    }

    async fn abandon_invalid_reservation(
        &self,
        receipt_id: Uuid,
        error: WorkflowApplicationError,
    ) -> WorkflowApplicationError {
        match self
            .deps
            .receipts
            .abandon_project_agent_start(receipt_id)
            .await
        {
            Ok(true) => error,
            Ok(false) => WorkflowApplicationError::Internal(format!(
                "{error}; Project Agent start validation reservation is no longer releasable"
            )),
            Err(abandon_error) => WorkflowApplicationError::Internal(format!(
                "{error}; 释放 Project Agent start reservation 失败: {abandon_error}"
            )),
        }
    }

    async fn freeze_launch_failure(
        &self,
        receipt_id: Uuid,
        error: WorkflowApplicationError,
    ) -> WorkflowApplicationError {
        let message = error.to_string();
        match self
            .deps
            .receipts
            .fail_project_agent_start(receipt_id, message.clone())
            .await
        {
            Ok(_) => WorkflowApplicationError::Internal(message),
            Err(failure_error) => WorkflowApplicationError::Internal(format!(
                "{message}; 固化 Project Agent launch 失败回执时失败: {failure_error}"
            )),
        }
    }

    async fn cleanup_and_freeze_unattached_failure(
        &self,
        receipt_id: Uuid,
        run_id: Uuid,
        error: WorkflowApplicationError,
    ) -> WorkflowApplicationError {
        let original = error.to_string();
        let cleanup = match self.deps.lifecycle_runs.get_by_id(run_id).await {
            Ok(Some(run)) if !run.execution_log.is_empty() => Err(format!(
                "refused to delete LifecycleRun {run_id} after observable execution events"
            )),
            Ok(Some(_)) => self
                .deps
                .lifecycle_runs
                .delete(run_id)
                .await
                .map_err(|error| error.to_string()),
            Ok(None) => Ok(()),
            Err(error) => Err(error.to_string()),
        };
        let stable_message = match cleanup {
            Ok(()) => original,
            Err(cleanup_error) => {
                format!("{original}; draft graph cleanup failed: {cleanup_error}")
            }
        };
        match self
            .deps
            .receipts
            .fail_project_agent_start(receipt_id, stable_message.clone())
            .await
        {
            Ok(_) => WorkflowApplicationError::Internal(stable_message),
            Err(failure_error) => WorkflowApplicationError::Internal(format!(
                "{stable_message}; 固化 Project Agent start 失败回执时失败: {failure_error}"
            )),
        }
    }
}

struct PreparedProjectAgentStart {
    command: ProjectAgentRunStartCommand,
    subject_ref: SubjectRef,
    project_agent_context: ResolvedProjectAgentContext,
    effective_executor_config: ConversationEffectiveExecutorConfigModel,
    effective_config: AgentConfig,
    execution_profile_json: serde_json::Value,
}

fn validate_start_command(
    command: &ProjectAgentRunStartCommand,
) -> Result<(), WorkflowApplicationError> {
    if command.client_command_id.trim().is_empty() {
        return Err(WorkflowApplicationError::BadRequest(
            "client_command_id 不能为空".to_string(),
        ));
    }
    if !command.input.iter().any(|block| match block {
        agentdash_agent_protocol::UserInputBlock::Text { text, .. } => !text.trim().is_empty(),
        _ => true,
    }) {
        return Err(WorkflowApplicationError::BadRequest(
            "input 不能为空".to_string(),
        ));
    }
    Ok(())
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

fn workflow_graph_ref_for_project_agent(
    project_agent: &agentdash_domain::agent::ProjectAgent,
) -> Option<WorkflowGraphRef> {
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
