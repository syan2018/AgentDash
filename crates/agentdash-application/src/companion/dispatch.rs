use uuid::Uuid;

use agentdash_domain::workflow::{
    AgentLaunchIntent, AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource, GatePolicy,
    GateWaitPolicyTemplate, InteractionDispatchIntent, LifecycleTaskPlanItemPatch, RunPolicy,
    RuntimePolicy, WaitExpectedResult, WaitTerminalOutcome, WaitTerminalPolicy, WaitWakeTarget,
};
use agentdash_spi::AgentConfig;
use agentdash_spi::action_type as at;

use super::tools::CompanionAdoptionMode;
use crate::lifecycle::LifecycleDispatchService;
use crate::repository_set::RepositorySet;
use crate::task::plan::update_run_task;
use agentdash_application_ports::launch::CompanionLaunchSource;

#[derive(Debug, Clone)]
pub(crate) struct CompanionChildDispatchRequest {
    pub project_id: Uuid,
    pub parent_run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub parent_frame_id: Uuid,
    pub wait: bool,
    pub slice_mode: agentdash_spi::CompanionSliceMode,
    pub adoption_mode: CompanionAdoptionMode,
    pub dispatch_id: String,
    pub companion_label: String,
    pub task_id: Option<Uuid>,
    pub selected_project_agent_id: Uuid,
    pub selected_agent_key: String,
    pub companion_executor_config: AgentConfig,
    pub parent_session_id: String,
    pub dispatch_prompt: String,
}

#[derive(Clone)]
pub(crate) struct CompanionChildDispatchOutcome {
    pub run_ref: Uuid,
    pub agent_ref: Uuid,
    pub frame_ref: Uuid,
    pub gate_ref: Option<Uuid>,
    pub delivery_runtime_session_id: String,
    pub launch_source: CompanionLaunchSource,
}

pub(crate) struct CompanionChildDispatchService<'a> {
    repos: &'a RepositorySet,
}

impl<'a> CompanionChildDispatchService<'a> {
    pub(crate) fn new(repos: &'a RepositorySet) -> Self {
        Self { repos }
    }

    pub(crate) async fn dispatch_child(
        &self,
        request: CompanionChildDispatchRequest,
    ) -> Result<CompanionChildDispatchOutcome, agentdash_spi::AgentToolError> {
        let context_policy = match request.slice_mode {
            agentdash_spi::CompanionSliceMode::Full => ContextPolicy::Inherit,
            _ => ContextPolicy::Slice,
        };
        let dispatch_service = self.lifecycle_dispatch_service();
        let outcome = if request.wait {
            let result = dispatch_service
                .open_interaction_gate(&InteractionDispatchIntent {
                    project_id: request.project_id,
                    source: ExecutionSource::ParentAgent,
                    parent_run_id: request.parent_run_id,
                    parent_agent_id: request.parent_agent_id,
                    workflow_graph_ref: None,
                    context_policy,
                    capability_policy: CapabilityPolicy::InheritedSlice,
                    runtime_policy: RuntimePolicy::CreateRuntimeSession,
                    gate_policy: GatePolicy {
                        gate_kind: gate_kind(request.adoption_mode).to_string(),
                        correlation_id: Some(request.dispatch_id.clone()),
                        payload: Some(serde_json::json!({
                            "parent_agent_id": request.parent_agent_id,
                            "parent_frame_id": request.parent_frame_id,
                            "companion_label": request.companion_label,
                            "adoption_mode": adoption_mode_key(request.adoption_mode),
                            "dispatch_id": request.dispatch_id,
                            "task_id": request.task_id.map(|id| id.to_string()),
                        })),
                        wait_policy: Some(companion_agent_run_delivery_wait_policy_template(
                            request.dispatch_id.clone(),
                            request.parent_run_id,
                            request.parent_agent_id,
                        )),
                    },
                })
                .await
                .map_err(|error| {
                    agentdash_spi::AgentToolError::ExecutionFailed(format!(
                        "dispatch 失败: {error}"
                    ))
                })?;
            let delivery_runtime_session_id =
                require_delivery_runtime_session(result.delivery_runtime_ref)?;
            CompanionChildDispatchOutcome {
                run_ref: result.runtime_refs.run_ref,
                agent_ref: result.runtime_refs.agent_ref,
                frame_ref: result.runtime_refs.frame_ref,
                gate_ref: Some(result.gate_ref),
                delivery_runtime_session_id,
                launch_source: build_launch_source(&request),
            }
        } else {
            let result = dispatch_service
                .launch_agent(&AgentLaunchIntent {
                    project_id: request.project_id,
                    source: ExecutionSource::ParentAgent,
                    created_by_user_id: None,
                    subject_ref: None,
                    parent_run_id: Some(request.parent_run_id),
                    parent_agent_id: Some(request.parent_agent_id),
                    workflow_graph_ref: None,
                    run_policy: RunPolicy::AppendGraph,
                    agent_policy: AgentPolicy::SpawnChild,
                    context_policy,
                    capability_policy: CapabilityPolicy::InheritedSlice,
                    runtime_policy: RuntimePolicy::CreateRuntimeSession,
                })
                .await
                .map_err(|error| {
                    agentdash_spi::AgentToolError::ExecutionFailed(format!(
                        "dispatch 失败: {error}"
                    ))
                })?;
            CompanionChildDispatchOutcome {
                run_ref: result.runtime_refs.run_ref,
                agent_ref: result.runtime_refs.agent_ref,
                frame_ref: result.runtime_refs.frame_ref,
                gate_ref: None,
                delivery_runtime_session_id: require_delivery_runtime_session(
                    result.delivery_runtime_ref,
                )?,
                launch_source: build_launch_source(&request),
            }
        };

        if let Some(task_id) = request.task_id {
            update_run_task(
                self.repos.lifecycle_run_repo.as_ref(),
                request.parent_run_id,
                task_id,
                LifecycleTaskPlanItemPatch {
                    assigned_agent_id: Some(Some(outcome.agent_ref)),
                    ..LifecycleTaskPlanItemPatch::default()
                },
            )
            .await
            .map_err(|error| {
                agentdash_spi::AgentToolError::ExecutionFailed(format!(
                    "Companion 已创建但 Task 指派关系写回失败: {error}"
                ))
            })?;
        }

        self.bind_selected_project_agent(outcome.agent_ref, request.selected_project_agent_id)
            .await?;

        Ok(outcome)
    }

    fn lifecycle_dispatch_service(&self) -> LifecycleDispatchService<'_> {
        LifecycleDispatchService::new(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.workflow_graph_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
            self.repos.lifecycle_subject_association_repo.as_ref(),
            self.repos.lifecycle_gate_repo.as_ref(),
            self.repos.agent_lineage_repo.as_ref(),
        )
        .with_anchor_repo(self.repos.execution_anchor_repo.as_ref())
        .with_delivery_binding_repo(self.repos.agent_run_delivery_binding_repo.as_ref())
        .with_runtime_session_creator(self.repos.runtime_session_creator.as_ref())
        .with_frame_construction_port(self.repos.agent_frame_construction.as_ref())
    }

    async fn bind_selected_project_agent(
        &self,
        lifecycle_agent_id: Uuid,
        project_agent_id: Uuid,
    ) -> Result<(), agentdash_spi::AgentToolError> {
        let Some(mut lifecycle_agent) = self
            .repos
            .lifecycle_agent_repo
            .get(lifecycle_agent_id)
            .await
            .map_err(|error| agentdash_spi::AgentToolError::ExecutionFailed(error.to_string()))?
        else {
            return Err(agentdash_spi::AgentToolError::ExecutionFailed(format!(
                "LifecycleAgent {lifecycle_agent_id} 不存在，无法绑定 selected companion ProjectAgent"
            )));
        };
        lifecycle_agent.project_agent_id = Some(project_agent_id);
        self.repos
            .lifecycle_agent_repo
            .update(&lifecycle_agent)
            .await
            .map_err(|error| agentdash_spi::AgentToolError::ExecutionFailed(error.to_string()))?;
        Ok(())
    }
}

fn companion_agent_run_delivery_wait_policy_template(
    correlation_ref: String,
    target_run_id: Uuid,
    target_agent_id: Uuid,
) -> GateWaitPolicyTemplate {
    GateWaitPolicyTemplate {
        expected_result: WaitExpectedResult {
            kind: "companion_result".to_string(),
            correlation_ref: Some(correlation_ref),
        },
        terminal_policy: WaitTerminalPolicy {
            failed: WaitTerminalOutcome {
                status: "failed".to_string(),
                failure_kind: "runtime_terminal_failed".to_string(),
            },
            interrupted: WaitTerminalOutcome {
                status: "cancelled".to_string(),
                failure_kind: "runtime_terminal_cancelled".to_string(),
            },
            completed: WaitTerminalOutcome {
                status: "failed".to_string(),
                failure_kind: "missing_companion_respond".to_string(),
            },
        },
        wake_target: WaitWakeTarget {
            namespace: "companion".to_string(),
            target_run_id,
            target_agent_id,
            client_command_id: "companion-result:{gate_id}".to_string(),
        },
    }
}

fn require_delivery_runtime_session(
    delivery_runtime_ref: Option<Uuid>,
) -> Result<String, agentdash_spi::AgentToolError> {
    delivery_runtime_ref
        .ok_or_else(|| {
            agentdash_spi::AgentToolError::ExecutionFailed(
                "dispatch 未创建 child delivery runtime session".to_string(),
            )
        })
        .map(|id| id.to_string())
}

fn build_launch_source(request: &CompanionChildDispatchRequest) -> CompanionLaunchSource {
    CompanionLaunchSource {
        parent_session_id: request.parent_session_id.clone(),
        selected_project_agent_id: Some(request.selected_project_agent_id),
        selected_agent_key: Some(request.selected_agent_key.clone()),
        slice_mode: request.slice_mode,
        companion_executor_config: request.companion_executor_config.clone(),
        dispatch_prompt: request.dispatch_prompt.clone(),
        workflow: None,
    }
}

fn gate_kind(mode: CompanionAdoptionMode) -> &'static str {
    match mode {
        CompanionAdoptionMode::BlockingReview => "companion_wait_blocking",
        CompanionAdoptionMode::FollowUpRequired => "companion_wait_follow_up",
        CompanionAdoptionMode::Suggestion => "companion_wait",
    }
}

fn adoption_mode_key(mode: CompanionAdoptionMode) -> &'static str {
    match mode {
        CompanionAdoptionMode::Suggestion => at::SUGGESTION,
        CompanionAdoptionMode::FollowUpRequired => at::FOLLOW_UP_REQUIRED,
        CompanionAdoptionMode::BlockingReview => at::BLOCKING_REVIEW,
    }
}
