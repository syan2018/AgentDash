use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::{
    AgentRunProductLaunchPort, AgentRunProductLaunchRequest,
    AgentRunProductRuntimeProvisioningRequest, ConversationEffectiveExecutorConfigModel,
    ConversationModelConfigResolver, ConversationModelConfigSourceModel, ProductAgentFrameRef,
    ProductAgentSurfaceFacts, ProductCredentialScopeRef, ProductExecutionProfileRef,
    ResolvedProjectAgentContext, build_project_agent_context,
};
use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
use agentdash_domain::{
    agent::ProjectAgentRepository,
    agent_run_target::AgentRunTarget,
    common::AgentConfig,
    workflow::{
        AgentFrameRepository, AgentLaunchIntent, AgentLineageRepository, AgentPolicy,
        CapabilityPolicy, ContextPolicy, ExecutionSource, LifecycleAgentRepository,
        LifecycleGateRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
        RunPolicy, RuntimePolicy, SubjectRef, WorkflowGraphRef, WorkflowGraphRepository,
    },
};
use agentdash_platform_spi::AuthIdentity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{ApplicationError, lifecycle::LifecycleDispatchService};

#[derive(Debug, Clone)]
pub struct ProjectAgentRunStartCommand {
    pub project_id: Uuid,
    pub project_agent_id: Uuid,
    pub client_command_id: String,
    pub executor_config: Option<AgentConfig>,
    pub backend_selection: Option<serde_json::Value>,
    pub subject_ref: Option<SubjectRef>,
    pub identity: AuthIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgentRunStartEffectiveExecutor {
    pub executor: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub thinking_level: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgentRunStartAgentSummary {
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub preset_name: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgentRunStartOutcome {
    pub client_command_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i32,
    pub runtime_thread_id: String,
    pub subject_kind: String,
    pub subject_id: Uuid,
    pub effective_executor: ProjectAgentRunStartEffectiveExecutor,
    pub agent_summary: ProjectAgentRunStartAgentSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAgentRunStartResult {
    pub outcome: ProjectAgentRunStartOutcome,
    pub duplicate: bool,
}

pub struct ProjectAgentRunStartDeps {
    pub project_agents: Arc<dyn ProjectAgentRepository>,
    pub lifecycle_runs: Arc<dyn LifecycleRunRepository>,
    pub workflow_graphs: Arc<dyn WorkflowGraphRepository>,
    pub lifecycle_agents: Arc<dyn LifecycleAgentRepository>,
    pub frames: Arc<dyn AgentFrameRepository>,
    pub subject_associations: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub lifecycle_gates: Arc<dyn LifecycleGateRepository>,
    pub agent_lineage: Arc<dyn AgentLineageRepository>,
    pub frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    pub product_launch: Arc<dyn AgentRunProductLaunchPort>,
}

pub struct ProjectAgentRunStartService {
    deps: ProjectAgentRunStartDeps,
}

impl ProjectAgentRunStartService {
    pub fn new(deps: ProjectAgentRunStartDeps) -> Self {
        Self { deps }
    }

    pub async fn start(
        &self,
        command: ProjectAgentRunStartCommand,
    ) -> Result<ProjectAgentRunStartResult, ApplicationError> {
        validate_command(&command)?;
        let subject_ref = command
            .subject_ref
            .clone()
            .unwrap_or_else(|| SubjectRef::new("project", command.project_id));
        validate_subject(command.project_id, &subject_ref)?;

        let project_agent = self
            .deps
            .project_agents
            .get_by_project_and_id(command.project_id, command.project_agent_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::NotFound(format!(
                    "Project Agent {} 不存在",
                    command.project_agent_id
                ))
            })?;
        let project_agent_context = build_project_agent_context(&project_agent)
            .await
            .map_err(ApplicationError::BadRequest)?;
        let model_resolution = ConversationModelConfigResolver::resolve_project_agent_start(
            &project_agent,
            command.executor_config.as_ref(),
        )
        .map_err(agent_run_workflow_error)?;
        let effective_config = model_resolution.config;
        let effective_executor = model_resolution
            .view
            .effective_executor_config
            .unwrap_or_else(|| {
                ConversationModelConfigResolver::view_for_config(
                    &effective_config,
                    ConversationModelConfigSourceModel::ProjectAgentPreset,
                )
            });
        let identities = StableStartIdentities::derive(
            command.project_id,
            command.project_agent_id,
            command.client_command_id.trim(),
        )?;
        let duplicate = self
            .deps
            .lifecycle_runs
            .get_by_id(identities.run_id)
            .await?
            .is_some();
        let execution_profile_json = serde_json::to_value(&effective_config)
            .map_err(|error| ApplicationError::BadRequest(error.to_string()))?;
        let dispatch = LifecycleDispatchService::new(
            self.deps.lifecycle_runs.as_ref(),
            self.deps.workflow_graphs.as_ref(),
            self.deps.lifecycle_agents.as_ref(),
            self.deps.frames.as_ref(),
            self.deps.subject_associations.as_ref(),
            self.deps.lifecycle_gates.as_ref(),
            self.deps.agent_lineage.as_ref(),
        )
        .with_frame_construction_port(self.deps.frame_construction.as_ref())
        .launch_agent_with_stable_product_identities(
            &AgentLaunchIntent {
                project_id: command.project_id,
                source: ExecutionSource::ProjectAgent,
                created_by_user_id: Some(command.identity.user_id.clone()),
                subject_ref: Some(subject_ref.clone()),
                parent_run_id: None,
                parent_agent_id: None,
                project_agent_id: Some(command.project_agent_id),
                execution_profile_override: Some(execution_profile_json.clone()),
                workflow_graph_ref: workflow_graph_ref(&project_agent),
                run_policy: RunPolicy::CreateLinkedRun,
                agent_policy: AgentPolicy::Create,
                context_policy: ContextPolicy::Isolated,
                capability_policy: CapabilityPolicy::Baseline,
                runtime_policy: RuntimePolicy::ProvisionRuntimeThread,
            },
            identities.run_id,
            identities.agent_id,
            identities.frame_id,
            identities.runtime_id,
        )
        .await?;
        if dispatch.runtime_refs.run_ref != identities.run_id
            || dispatch.runtime_refs.agent_ref != identities.agent_id
            || dispatch.runtime_refs.frame_ref != identities.frame_id
            || dispatch.delivery_runtime_ref != identities.runtime_id
        {
            return Err(ApplicationError::Conflict(
                "Project AgentRun launch graph identity drifted".to_owned(),
            ));
        }
        let frame = self
            .deps
            .frames
            .get(identities.frame_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Internal(
                    "Project AgentRun launch 未持久化预分配 AgentFrame".to_owned(),
                )
            })?;
        let runtime_thread_id = RuntimeThreadId::new(identities.runtime_id.to_string())
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;

        let mut profile = ProductExecutionProfileRef {
            profile_key: effective_config.executor.clone(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: execution_profile_json,
            credential_scope: Some(ProductCredentialScopeRef {
                owner_kind: "user".to_owned(),
                owner_id: command.identity.user_id.clone(),
                credential_ref: effective_config
                    .provider_id
                    .as_deref()
                    .map(|provider| format!("llm-provider:{provider}"))
                    .unwrap_or_else(|| format!("complete-agent:{}", effective_config.executor)),
            }),
        };
        profile.refresh_digest();
        self.deps
            .product_launch
            .launch(AgentRunProductLaunchRequest {
                provisioning: AgentRunProductRuntimeProvisioningRequest {
                    target: AgentRunTarget {
                        run_id: identities.run_id,
                        agent_id: identities.agent_id,
                    },
                    runtime_thread_id: runtime_thread_id.clone(),
                    idempotency_key: format!("project-agent-run-start:{}", identities.run_id),
                    frame: ProductAgentFrameRef {
                        frame_id: frame.id,
                        agent_id: frame.agent_id,
                        revision: u64::try_from(frame.revision).map_err(|_| {
                            ApplicationError::Internal(
                                "Project AgentRun launch frame revision 无效".to_owned(),
                            )
                        })?,
                    },
                    execution_profile: profile,
                    surface_facts: ProductAgentSurfaceFacts::from_frame(&frame),
                },
                initial_context: None,
                initial_input: Vec::new(),
            })
            .await
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;

        let outcome = ProjectAgentRunStartOutcome {
            client_command_id: command.client_command_id.trim().to_owned(),
            run_id: identities.run_id,
            agent_id: identities.agent_id,
            frame_id: identities.frame_id,
            frame_revision: frame.revision,
            runtime_thread_id: runtime_thread_id.to_string(),
            subject_kind: subject_ref.kind,
            subject_id: subject_ref.id,
            effective_executor: effective_executor_snapshot(effective_executor),
            agent_summary: project_agent_summary(project_agent_context),
        };
        Ok(ProjectAgentRunStartResult { outcome, duplicate })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StableStartIdentities {
    run_id: Uuid,
    agent_id: Uuid,
    frame_id: Uuid,
    runtime_id: Uuid,
}

impl StableStartIdentities {
    fn derive(
        project_id: Uuid,
        project_agent_id: Uuid,
        client_command_id: &str,
    ) -> Result<Self, ApplicationError> {
        if client_command_id.is_empty() {
            return Err(ApplicationError::BadRequest(
                "client_command_id 不能为空".to_owned(),
            ));
        }
        Ok(Self {
            run_id: stable_uuid(project_id, project_agent_id, client_command_id, "run"),
            agent_id: stable_uuid(project_id, project_agent_id, client_command_id, "agent"),
            frame_id: stable_uuid(project_id, project_agent_id, client_command_id, "frame"),
            runtime_id: stable_uuid(project_id, project_agent_id, client_command_id, "runtime"),
        })
    }
}

fn stable_uuid(
    project_id: Uuid,
    project_agent_id: Uuid,
    client_command_id: &str,
    role: &str,
) -> Uuid {
    let digest = Sha256::digest(
        format!(
            "agentdash.project-agent-run-start/v1:{project_id}:{project_agent_id}:{client_command_id}:{role}"
        )
        .as_bytes(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn validate_command(command: &ProjectAgentRunStartCommand) -> Result<(), ApplicationError> {
    let client_command_id = command.client_command_id.trim();
    if client_command_id.is_empty() || client_command_id.len() > 256 {
        return Err(ApplicationError::BadRequest(
            "client_command_id 无效".to_owned(),
        ));
    }
    Ok(())
}

fn validate_subject(project_id: Uuid, subject: &SubjectRef) -> Result<(), ApplicationError> {
    match subject.kind.as_str() {
        "project" if subject.id == project_id => Ok(()),
        "project" => Err(ApplicationError::BadRequest(format!(
            "Project subject {} 不属于当前 Project {}",
            subject.id, project_id
        ))),
        "story" | "task" => Ok(()),
        kind => Err(ApplicationError::BadRequest(format!(
            "不支持的 ProjectAgent subject kind: {kind}"
        ))),
    }
}

fn workflow_graph_ref(
    project_agent: &agentdash_domain::agent::ProjectAgent,
) -> Option<WorkflowGraphRef> {
    project_agent
        .default_lifecycle_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| WorkflowGraphRef::ByKey {
            project_id: project_agent.project_id,
            key: key.to_owned(),
        })
}

fn effective_executor_snapshot(
    effective: ConversationEffectiveExecutorConfigModel,
) -> ProjectAgentRunStartEffectiveExecutor {
    ProjectAgentRunStartEffectiveExecutor {
        executor: effective.executor,
        provider_id: effective.provider_id,
        model_id: effective.model_id,
        agent_id: effective.agent_id,
        thinking_level: effective.thinking_level,
        source: model_source_name(effective.source).to_owned(),
    }
}

fn project_agent_summary(context: ResolvedProjectAgentContext) -> ProjectAgentRunStartAgentSummary {
    ProjectAgentRunStartAgentSummary {
        key: context.key,
        display_name: context.display_name,
        description: context.description,
        preset_name: context.preset_name,
        source: "project_agent".to_owned(),
    }
}

fn model_source_name(source: ConversationModelConfigSourceModel) -> &'static str {
    match source {
        ConversationModelConfigSourceModel::ProjectAgentPreset => "project_agent_preset",
        ConversationModelConfigSourceModel::FrameExecutionProfile => "frame_execution_profile",
        ConversationModelConfigSourceModel::UserOverride => "user_override",
        ConversationModelConfigSourceModel::ExecutorDiscoveryDefault => {
            "executor_discovery_default"
        }
        ConversationModelConfigSourceModel::Unspecified => "unspecified",
    }
}

fn agent_run_workflow_error(
    error: agentdash_application_agentrun::WorkflowApplicationError,
) -> ApplicationError {
    match error {
        agentdash_application_agentrun::WorkflowApplicationError::BadRequest(message)
        | agentdash_application_agentrun::WorkflowApplicationError::ModelRequired(message) => {
            ApplicationError::BadRequest(message)
        }
        agentdash_application_agentrun::WorkflowApplicationError::NotFound(message) => {
            ApplicationError::NotFound(message)
        }
        agentdash_application_agentrun::WorkflowApplicationError::Conflict(message) => {
            ApplicationError::Conflict(message)
        }
        agentdash_application_agentrun::WorkflowApplicationError::Unavailable(message) => {
            ApplicationError::Unavailable(message)
        }
        agentdash_application_agentrun::WorkflowApplicationError::Internal(message) => {
            ApplicationError::Internal(message)
        }
    }
}
