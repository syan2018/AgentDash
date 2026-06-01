use agentdash_domain::workflow::{
    ActivityAttemptStatus, ActivityDefinition, ActivityExecutionClaim, ActivityExecutorSpec,
    ActivityPortValue, AgentAssignment, AgentFrame, AgentProcedureRef, AgentSessionPolicy,
    ExecutionSource, ExecutorRunRef, FunctionActivityExecutorSpec, HumanActivityExecutorSpec,
    LifecycleAgent, WorkflowGraph,
};
use agentdash_spi::CapabilityScope;
use std::sync::Arc;

use agentdash_spi::{AgentConfig, FunctionRunner};
use serde_json::{Value, json};

use super::ActivityLifecycleRunState;
use super::scheduler::{
    ActivityExecutorLauncher, ActivityExecutorStartError, ActivityExecutorStartResult,
};
use crate::companion::skill_projection::project_companion_system_skill_to_activation;
use crate::platform_config::SharedPlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::capability_state::{
    CapabilityDimensionRegistry, CompanionCapabilityDimensionModule, McpCapabilityDimensionModule,
    ToolCapabilityDimensionModule, VfsCapabilityDimensionModule,
};
use crate::session::hub::PendingRuntimeContextTransitionInput;
use crate::session::{CapabilityArtifactSource, RuntimeCapabilityTransition, SetToolAccessEffect};
use crate::session::{
    LaunchCommand, SessionCapabilityService, SessionCoreService, SessionHookService,
    SessionLaunchService, UserPromptInput,
};
use crate::workflow::step_activation::apply_to_frame_runtime_target;
use crate::workflow::{
    AgentFrameBuilder, RuntimeSessionCreationRequest, activate_step_with_platform,
    agent_mcp_entries_from_servers, build_capability_state_for_activation, load_port_output_map,
};

#[derive(Debug, Clone)]
pub struct AgentActivityLaunchContext {
    pub project_id: uuid::Uuid,
    pub lifecycle_key: String,
    pub root_runtime_session_id: String,
}

pub struct AgentActivityExecutorLauncher<P> {
    context: AgentActivityLaunchContext,
    port: P,
}

impl<P> AgentActivityExecutorLauncher<P> {
    pub fn new(context: AgentActivityLaunchContext, port: P) -> Self {
        Self { context, port }
    }
}

#[derive(Debug, Clone)]
pub struct AgentActivityAssignmentContext {
    pub assignment: AgentAssignment,
    pub frame: AgentFrame,
}

#[async_trait::async_trait]
pub trait AgentActivitySessionPort: Send + Sync {
    async fn get_executor_config(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<AgentConfig>, String>;
    async fn set_executor_config(
        &self,
        runtime_session_id: &str,
        executor_config: AgentConfig,
    ) -> Result<(), String>;
    async fn mark_owner_bootstrap_pending(&self, runtime_session_id: &str) -> Result<(), String>;
    async fn launch_workflow_prompt(
        &self,
        runtime_session_id: &str,
        executor_config: Option<AgentConfig>,
    ) -> Result<(), String>;
    async fn create_agent_activity_assignment(
        &self,
        _definition: &WorkflowGraph,
        _activity: &ActivityDefinition,
        _claim: &ActivityExecutionClaim,
        _runtime_session_id: Option<&str>,
        _executor_config: Option<&AgentConfig>,
    ) -> Result<AgentActivityAssignmentContext, String> {
        Err("Agent assignment port 未接入".to_string())
    }
    async fn create_runtime_session_for_agent_activity(
        &self,
        _definition: &WorkflowGraph,
        _activity: &ActivityDefinition,
        _claim: &ActivityExecutionClaim,
        _assignment: &AgentAssignment,
        _frame: &AgentFrame,
    ) -> Result<String, String> {
        Err("Agent activity runtime session port 未接入".to_string())
    }
    async fn apply_continue_root_activity(
        &self,
        _definition: &WorkflowGraph,
        _activity: &ActivityDefinition,
        _claim: &ActivityExecutionClaim,
        _procedure_key: &str,
        _root_runtime_session_id: &str,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn execute_function_activity(
        &self,
        _definition: &WorkflowGraph,
        _activity: &ActivityDefinition,
        _claim: &ActivityExecutionClaim,
        _spec: &FunctionActivityExecutorSpec,
        _state: &ActivityLifecycleRunState,
    ) -> Result<FunctionExecutionResult, String> {
        Err("Function executor port 未接入".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct FunctionExecutionResult {
    pub executor_run: ExecutorRunRef,
    pub completion_event: super::ActivityEvent,
}

#[derive(Clone)]
pub struct AgentActivityRuntimePort {
    session_core: SessionCoreService,
    session_launch: SessionLaunchService,
    session_hooks: Option<SessionHookService>,
    session_capability: Option<SessionCapabilityService>,
    repos: RepositorySet,
    platform_config: Option<SharedPlatformConfig>,
    function_runner: Arc<dyn FunctionRunner>,
}

impl AgentActivityRuntimePort {
    pub fn new(
        session_core: SessionCoreService,
        session_launch: SessionLaunchService,
        repos: RepositorySet,
        function_runner: Arc<dyn FunctionRunner>,
    ) -> Self {
        Self {
            session_core,
            session_launch,
            session_hooks: None,
            session_capability: None,
            repos,
            platform_config: None,
            function_runner,
        }
    }

    pub fn with_runtime_context(
        mut self,
        session_hooks: SessionHookService,
        session_capability: SessionCapabilityService,
        platform_config: SharedPlatformConfig,
    ) -> Self {
        self.session_hooks = Some(session_hooks);
        self.session_capability = Some(session_capability);
        self.platform_config = Some(platform_config);
        self
    }
}

#[async_trait::async_trait]
impl AgentActivitySessionPort for AgentActivityRuntimePort {
    async fn get_executor_config(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<AgentConfig>, String> {
        self.session_core
            .get_session_meta(runtime_session_id)
            .await
            .map_err(|error| format!("读取 runtime session meta 失败: {error}"))
            .map(|meta| meta.and_then(|meta| meta.executor_config))
    }

    async fn set_executor_config(
        &self,
        runtime_session_id: &str,
        executor_config: AgentConfig,
    ) -> Result<(), String> {
        self.session_core
            .update_session_meta(runtime_session_id, move |meta| {
                meta.executor_config = Some(executor_config.clone());
            })
            .await
            .map_err(|error| format!("继承 executor config 失败: {error}"))?;
        Ok(())
    }

    async fn mark_owner_bootstrap_pending(&self, runtime_session_id: &str) -> Result<(), String> {
        self.session_core
            .mark_owner_bootstrap_pending(runtime_session_id)
            .await
            .map_err(|error| format!("标记 owner bootstrap pending 失败: {error}"))
    }

    async fn launch_workflow_prompt(
        &self,
        runtime_session_id: &str,
        executor_config: Option<AgentConfig>,
    ) -> Result<(), String> {
        let mut user_input = UserPromptInput::from_text("");
        user_input.executor_config = executor_config;
        let command = LaunchCommand::workflow_orchestrator_input(user_input);
        self.session_launch
            .launch_command(runtime_session_id, command)
            .await
            .map(|_| ())
            .map_err(|error| format!("启动 activity runtime session prompt 失败: {error}"))
    }

    async fn create_agent_activity_assignment(
        &self,
        definition: &WorkflowGraph,
        activity: &ActivityDefinition,
        claim: &ActivityExecutionClaim,
        runtime_session_id: Option<&str>,
        executor_config: Option<&AgentConfig>,
    ) -> Result<AgentActivityAssignmentContext, String> {
        let ActivityExecutorSpec::Agent(spec) = &activity.executor else {
            return Err(format!("activity {} 不是 Agent executor", activity.key));
        };
        let procedure = self
            .repos
            .agent_procedure_repo
            .get_by_project_and_key(definition.project_id, &spec.procedure_key)
            .await
            .map_err(|error| format!("加载 Agent activity procedure 失败: {error}"))?
            .ok_or_else(|| format!("Agent activity procedure 不存在: {}", spec.procedure_key))?;

        let mut agent = LifecycleAgent::new_root(
            claim.run_id,
            definition.project_id,
            "workflow_activity_agent",
        );
        self.repos
            .lifecycle_agent_repo
            .create(&agent)
            .await
            .map_err(|error| format!("创建 LifecycleAgent 失败: {error}"))?;

        let mut builder = AgentFrameBuilder::new(agent.id)
            .with_graph_instance(claim.graph_instance_id, claim.activity_key.clone())
            .with_procedure(AgentProcedureRef::ById(procedure.id))
            .with_created_by("activity_executor", Some(claim.claim_id.to_string()));
        if let Some(runtime_session_id) = runtime_session_id {
            builder = builder.with_runtime_session(runtime_session_id.to_string());
        }
        if let Some(executor_config) = executor_config {
            builder = builder.with_execution_profile(executor_config);
        }
        let frame = builder
            .build(self.repos.agent_frame_repo.as_ref())
            .await
            .map_err(|error| format!("创建 AgentFrame 失败: {error}"))?;
        agent.set_current_frame(frame.id);
        self.repos
            .lifecycle_agent_repo
            .update(&agent)
            .await
            .map_err(|error| format!("更新 LifecycleAgent current_frame 失败: {error}"))?;

        let assignment = AgentAssignment::new(
            claim.run_id,
            claim.graph_instance_id,
            claim.activity_key.clone(),
            claim.attempt as i32,
            agent.id,
            frame.id,
        );
        self.repos
            .agent_assignment_repo
            .create(&assignment)
            .await
            .map_err(|error| format!("创建 AgentAssignment 失败: {error}"))?;
        Ok(AgentActivityAssignmentContext { assignment, frame })
    }

    async fn create_runtime_session_for_agent_activity(
        &self,
        definition: &WorkflowGraph,
        _activity: &ActivityDefinition,
        claim: &ActivityExecutionClaim,
        assignment: &AgentAssignment,
        frame: &AgentFrame,
    ) -> Result<String, String> {
        if assignment.frame_id != frame.id || assignment.agent_id != frame.agent_id {
            return Err(
                "AgentAssignment 与 AgentFrame 不匹配，拒绝创建 RuntimeSession".to_string(),
            );
        }
        let session_id = self
            .repos
            .runtime_session_creator
            .create_runtime_session(RuntimeSessionCreationRequest {
                project_id: definition.project_id,
                run_id: claim.run_id,
                agent_id: frame.agent_id,
                source: ExecutionSource::ParentAgent,
            })
            .await
            .map_err(|error| format!("创建 activity runtime session 失败: {error}"))?;
        let runtime_session_id = session_id.to_string();
        self.repos
            .agent_frame_repo
            .attach_runtime_session_ref(frame.id, &runtime_session_id)
            .await
            .map_err(|error| format!("写回 AgentFrame runtime session ref 失败: {error}"))?;
        Ok(runtime_session_id)
    }

    async fn apply_continue_root_activity(
        &self,
        definition: &WorkflowGraph,
        activity: &ActivityDefinition,
        claim: &ActivityExecutionClaim,
        procedure_key: &str,
        root_runtime_session_id: &str,
    ) -> Result<(), String> {
        let session_hooks = self
            .session_hooks
            .as_ref()
            .ok_or_else(|| "ContinueRoot 缺少 session hook service".to_string())?;
        let session_capability = self
            .session_capability
            .as_ref()
            .ok_or_else(|| "ContinueRoot 缺少 session capability service".to_string())?;
        let platform_config = self
            .platform_config
            .as_ref()
            .ok_or_else(|| "ContinueRoot 缺少 platform config".to_string())?;
        let workflow = self
            .repos
            .agent_procedure_repo
            .get_by_project_and_key(definition.project_id, procedure_key)
            .await
            .map_err(|error| format!("加载 ContinueRoot workflow 失败: {error}"))?
            .ok_or_else(|| format!("ContinueRoot workflow 不存在: {procedure_key}"))?;

        let available_presets =
            crate::session::load_available_presets(&self.repos, definition.project_id).await;
        let ready_port_keys =
            load_port_output_map(self.repos.inline_file_repo.as_ref(), claim.run_id)
                .await
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();

        if let Some(hook_runtime) = session_hooks
            .ensure_hook_runtime(root_runtime_session_id, None)
            .await
            .map_err(|error| format!("加载 root hook runtime 失败: {error}"))?
        {
            let snapshot = hook_runtime.snapshot();
            let owner_ctx = scope_from_run_context_or_project(
                snapshot.run_context.as_ref(),
                definition.project_id,
            );
            let runtime_mcp_servers = session_capability
                .get_runtime_mcp_servers(root_runtime_session_id)
                .await;
            let mut activation = activate_step_with_platform(
                &crate::workflow::StepActivationInput {
                    owner_ctx,
                    active_activity: activity,
                    workflow: Some(&workflow),
                    run_id: claim.run_id,
                    graph_instance_id: claim.graph_instance_id,
                    lifecycle_key: &definition.key,
                    agent_mcp_servers: agent_mcp_entries_from_servers(&runtime_mcp_servers),
                    available_presets,
                    companion_slice_mode: None,
                    baseline_override: None,
                    tool_directives: &[],
                    ready_port_keys,
                    available_companions: Vec::new(),
                },
                platform_config,
            );
            project_companion_system_skill_to_activation(
                &self.repos,
                definition.project_id,
                &mut activation,
            )
            .await
            .map_err(|error| error.to_string())?;
            let target = session_capability
                .resolve_runtime_session_target(root_runtime_session_id)
                .await?;
            let base_surface = session_capability
                .get_current_capability_state(&target.delivery_runtime_session_id)
                .await;
            apply_to_frame_runtime_target(
                &activation,
                &hook_runtime,
                session_capability,
                target,
                base_surface,
                None,
                &activity.key,
                Some(claim.run_id),
                Some(&definition.key),
            )
            .await
            .map(|_| ())
        } else {
            let owner_ctx = agentdash_spi::CapabilityScopeCtx::Project {
                project_id: definition.project_id,
            };
            let base_surface = session_capability
                .get_latest_capability_state(root_runtime_session_id)
                .await;
            let agent_mcp_servers = base_surface
                .as_ref()
                .map(|surface| agent_mcp_entries_from_servers(&surface.tool.mcp_servers))
                .unwrap_or_default();
            let mut activation = activate_step_with_platform(
                &crate::workflow::StepActivationInput {
                    owner_ctx,
                    active_activity: activity,
                    workflow: Some(&workflow),
                    run_id: claim.run_id,
                    graph_instance_id: claim.graph_instance_id,
                    lifecycle_key: &definition.key,
                    agent_mcp_servers,
                    available_presets,
                    companion_slice_mode: None,
                    baseline_override: None,
                    tool_directives: &[],
                    ready_port_keys,
                    available_companions: Vec::new(),
                },
                platform_config,
            );
            project_companion_system_skill_to_activation(
                &self.repos,
                definition.project_id,
                &mut activation,
            )
            .await
            .map_err(|error| error.to_string())?;
            let surface = build_capability_state_for_activation(&activation, base_surface.as_ref());
            let mut declarations =
                ToolCapabilityDimensionModule::capability_directive_declarations(
                    CapabilityArtifactSource::workflow(),
                    activation.tool_directives.clone(),
                )?;
            declarations.extend(VfsCapabilityDimensionModule::mount_operation_declarations(
                CapabilityArtifactSource::workflow(),
                activation.mount_directives.clone(),
            )?);
            let mut effects = vec![
                ToolCapabilityDimensionModule::set_tool_access_effect(SetToolAccessEffect {
                    capabilities: activation.capability_state.tool.capabilities.clone(),
                    enabled_clusters: activation.capability_state.tool.enabled_clusters.clone(),
                    tool_policy: activation.capability_state.tool.tool_policy.clone(),
                })?,
                McpCapabilityDimensionModule::set_server_set_effect(
                    activation.mcp_servers.clone(),
                )?,
                CompanionCapabilityDimensionModule::set_agent_roster_effect(
                    activation.capability_state.companion.agents.clone(),
                )?,
                VfsCapabilityDimensionModule::apply_vfs_overlay_effect(
                    activation.lifecycle_vfs.clone(),
                )?,
            ];
            if !activation.mount_directives.is_empty() {
                effects.push(VfsCapabilityDimensionModule::apply_mount_operations_effect(
                    activation.mount_directives.clone(),
                )?);
            }
            let transition = RuntimeCapabilityTransition::from_records(declarations, effects);
            CapabilityDimensionRegistry::built_in().validate_transition(&transition)?;
            let target = session_capability
                .resolve_runtime_session_target(root_runtime_session_id)
                .await?;
            session_capability
                .enqueue_pending_runtime_context_transition(PendingRuntimeContextTransitionInput {
                    target_frame_id: target.frame_id,
                    delivery_runtime_session_id: target.delivery_runtime_session_id,
                    turn_id: None,
                    frame_transition_id: format!(
                        "activity-{}-{}-{}",
                        activity.key,
                        claim.attempt,
                        uuid::Uuid::new_v4()
                    ),
                    phase_node: activity.key.clone(),
                    run_id: claim.run_id,
                    lifecycle_key: definition.key.clone(),
                    before_state: base_surface,
                    after_state: surface,
                    transition,
                    capability_keys: activation.capability_keys,
                    source_turn_id: None,
                    created_at: chrono::Utc::now().timestamp_millis(),
                })
                .await
        }
    }

    async fn execute_function_activity(
        &self,
        definition: &WorkflowGraph,
        activity: &ActivityDefinition,
        claim: &ActivityExecutionClaim,
        spec: &FunctionActivityExecutorSpec,
        state: &ActivityLifecycleRunState,
    ) -> Result<FunctionExecutionResult, String> {
        execute_function_activity(
            self.function_runner.as_ref(),
            definition,
            activity,
            claim,
            spec,
            state,
        )
        .await
    }
}

#[async_trait::async_trait]
impl<P> ActivityExecutorLauncher for AgentActivityExecutorLauncher<P>
where
    P: AgentActivitySessionPort,
{
    async fn start(
        &self,
        definition: &WorkflowGraph,
        state: &ActivityLifecycleRunState,
        claim: &ActivityExecutionClaim,
    ) -> Result<ActivityExecutorStartResult, ActivityExecutorStartError> {
        let Some(activity) = definition
            .activities
            .iter()
            .find(|activity| activity.key == claim.activity_key)
        else {
            return Err(ActivityExecutorStartError::terminal(format!(
                "activity 不存在: {}",
                claim.activity_key
            )));
        };
        match &activity.executor {
            ActivityExecutorSpec::Agent(spec) => match spec.session_policy {
                AgentSessionPolicy::SpawnChild => {
                    self.start_spawn_child(definition, activity, claim).await
                }
                AgentSessionPolicy::ContinueRoot => {
                    self.start_continue_root(
                        definition,
                        activity,
                        spec.procedure_key.as_str(),
                        state,
                        claim,
                    )
                    .await
                }
                AgentSessionPolicy::AttachExisting => {
                    Err(ActivityExecutorStartError::terminal(format!(
                        "Agent session policy `{}` 尚未接入 Activity executor",
                        serde_json::to_string(&spec.session_policy)
                            .unwrap_or_else(|_| "unknown".to_string())
                    )))
                }
            },
            ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(_spec)) => Ok(
                ActivityExecutorStartResult::started(ExecutorRunRef::HumanDecision {
                    decision_id: human_decision_id(claim),
                }),
            ),
            ActivityExecutorSpec::Function(spec) => {
                self.start_function(definition, activity, claim, spec, state)
                    .await
            }
        }
    }
}

impl<P> AgentActivityExecutorLauncher<P>
where
    P: AgentActivitySessionPort,
{
    async fn start_spawn_child(
        &self,
        definition: &WorkflowGraph,
        activity: &ActivityDefinition,
        claim: &ActivityExecutionClaim,
    ) -> Result<ActivityExecutorStartResult, ActivityExecutorStartError> {
        let title = format!(
            "[{}] {}#{}",
            definition.key, claim.activity_key, claim.attempt
        );
        let executor_config = if self.context.root_runtime_session_id.trim().is_empty() {
            None
        } else {
            self.port
                .get_executor_config(&self.context.root_runtime_session_id)
                .await
                .map_err(ActivityExecutorStartError::retryable)?
        };
        let assignment_context = self
            .port
            .create_agent_activity_assignment(
                definition,
                activity,
                claim,
                None,
                executor_config.as_ref(),
            )
            .await
            .map_err(ActivityExecutorStartError::terminal)?;
        let runtime_session_id = self
            .port
            .create_runtime_session_for_agent_activity(
                definition,
                activity,
                claim,
                &assignment_context.assignment,
                &assignment_context.frame,
            )
            .await
            .map_err(ActivityExecutorStartError::retryable)?;
        tracing::debug!(
            runtime_session_id = %runtime_session_id,
            title = %title,
            assignment_id = %assignment_context.assignment.id,
            frame_id = %assignment_context.frame.id,
            "Agent activity RuntimeSession 已由 AgentFrame 创建并回填"
        );
        if let Some(executor_config) = executor_config.clone() {
            self.port
                .set_executor_config(&runtime_session_id, executor_config)
                .await
                .map_err(ActivityExecutorStartError::retryable)?;
        }

        self.port
            .mark_owner_bootstrap_pending(&runtime_session_id)
            .await
            .map_err(ActivityExecutorStartError::retryable)?;
        self.port
            .launch_workflow_prompt(&runtime_session_id, executor_config)
            .await
            .map_err(ActivityExecutorStartError::retryable)?;

        Ok(
            ActivityExecutorStartResult::started(ExecutorRunRef::RuntimeSession {
                session_id: runtime_session_id,
            })
            .with_assignment(assignment_context.assignment),
        )
    }

    async fn start_continue_root(
        &self,
        definition: &WorkflowGraph,
        activity: &ActivityDefinition,
        procedure_key: &str,
        state: &ActivityLifecycleRunState,
        claim: &ActivityExecutionClaim,
    ) -> Result<ActivityExecutorStartResult, ActivityExecutorStartError> {
        let has_running_continue_root = state.attempts.iter().any(|attempt| {
            attempt.status == ActivityAttemptStatus::Running
                && attempt.activity_key != claim.activity_key
                && definition
                    .activities
                    .iter()
                    .find(|activity| activity.key == attempt.activity_key)
                    .and_then(|activity| match &activity.executor {
                        ActivityExecutorSpec::Agent(spec) => Some(spec.session_policy),
                        _ => None,
                    })
                    == Some(AgentSessionPolicy::ContinueRoot)
        });
        if has_running_continue_root {
            return Err(ActivityExecutorStartError::terminal(
                "root session 已存在 running ContinueRoot activity",
            ));
        }

        if self.context.root_runtime_session_id.trim().is_empty() {
            return Err(ActivityExecutorStartError::terminal(
                "ContinueRoot 缺少 root runtime session",
            ));
        }

        let executor_config = self
            .port
            .get_executor_config(&self.context.root_runtime_session_id)
            .await
            .map_err(ActivityExecutorStartError::retryable)?;
        let assignment = self
            .port
            .create_agent_activity_assignment(
                definition,
                activity,
                claim,
                Some(&self.context.root_runtime_session_id),
                executor_config.as_ref(),
            )
            .await
            .map_err(ActivityExecutorStartError::terminal)?;

        self.port
            .apply_continue_root_activity(
                definition,
                activity,
                claim,
                procedure_key,
                &self.context.root_runtime_session_id,
            )
            .await
            .map_err(ActivityExecutorStartError::retryable)?;
        Ok(
            ActivityExecutorStartResult::started(ExecutorRunRef::RuntimeSession {
                session_id: self.context.root_runtime_session_id.clone(),
            })
            .with_assignment(assignment.assignment),
        )
    }

    async fn start_function(
        &self,
        definition: &WorkflowGraph,
        activity: &ActivityDefinition,
        claim: &ActivityExecutionClaim,
        spec: &FunctionActivityExecutorSpec,
        state: &ActivityLifecycleRunState,
    ) -> Result<ActivityExecutorStartResult, ActivityExecutorStartError> {
        let result = self
            .port
            .execute_function_activity(definition, activity, claim, spec, state)
            .await
            .map_err(ActivityExecutorStartError::terminal)?;
        Ok(ActivityExecutorStartResult::with_events(
            result.executor_run,
            vec![result.completion_event],
        ))
    }
}

async fn execute_function_activity(
    function_runner: &dyn FunctionRunner,
    definition: &WorkflowGraph,
    activity: &ActivityDefinition,
    claim: &ActivityExecutionClaim,
    spec: &FunctionActivityExecutorSpec,
    state: &ActivityLifecycleRunState,
) -> Result<FunctionExecutionResult, String> {
    let function_run_id = uuid::Uuid::new_v4().to_string();
    let executor_run = ExecutorRunRef::FunctionRun {
        run_id: function_run_id,
    };
    let context = function_template_context(definition, activity, claim, state);
    let completion_event = match spec {
        FunctionActivityExecutorSpec::ApiRequest(spec) => {
            execute_api_request(function_runner, activity, claim, spec, &context).await
        }
        FunctionActivityExecutorSpec::BashExec(spec) => {
            execute_bash(function_runner, activity, claim, spec, &context).await
        }
    };
    Ok(FunctionExecutionResult {
        executor_run,
        completion_event,
    })
}

async fn execute_api_request(
    function_runner: &dyn FunctionRunner,
    activity: &ActivityDefinition,
    claim: &ActivityExecutionClaim,
    spec: &agentdash_domain::workflow::ApiRequestExecutorSpec,
    context: &Value,
) -> super::ActivityEvent {
    let outcome = match function_runner.run_api_request(spec, context).await {
        Ok(outcome) => outcome,
        Err(error) => return function_failed(claim, error),
    };
    let result = json!({
        "status": outcome.status,
        "body_text": outcome.body_text,
        "body_json": outcome.body_json,
    });
    if (200..300).contains(&outcome.status) {
        function_completed(
            activity,
            claim,
            result,
            Some(format!("API request {}", outcome.status)),
        )
    } else {
        function_failed(
            claim,
            format!("API request 返回非成功状态: {}", outcome.status),
        )
    }
}

async fn execute_bash(
    function_runner: &dyn FunctionRunner,
    activity: &ActivityDefinition,
    claim: &ActivityExecutionClaim,
    spec: &agentdash_domain::workflow::BashExecExecutorSpec,
    context: &Value,
) -> super::ActivityEvent {
    let outcome = match function_runner.run_bash(spec, context).await {
        Ok(outcome) => outcome,
        Err(error) => return function_failed(claim, error),
    };
    let result = json!({
        "exit_code": outcome.exit_code,
        "stdout": outcome.stdout,
        "stderr": outcome.stderr,
    });
    if outcome.success {
        function_completed(
            activity,
            claim,
            result,
            Some("Bash exec completed".to_string()),
        )
    } else {
        function_failed(
            claim,
            format!(
                "Bash exec failed with exit_code={}",
                outcome
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
        )
    }
}

fn function_template_context(
    definition: &WorkflowGraph,
    activity: &ActivityDefinition,
    claim: &ActivityExecutionClaim,
    state: &ActivityLifecycleRunState,
) -> Value {
    let inputs = state
        .inputs
        .iter()
        .filter(|input| input.activity_key == activity.key && input.attempt == claim.attempt)
        .map(|input| (input.port_key.clone(), input.value.clone()))
        .collect::<serde_json::Map<_, _>>();
    let outputs = state
        .outputs
        .iter()
        .map(|output| {
            (
                format!("{}.{}", output.activity_key, output.port_key),
                output.value.clone(),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "lifecycle": {
            "id": definition.id,
            "key": definition.key,
        },
        "activity": {
            "key": activity.key,
            "attempt": claim.attempt,
        },
        "run": {
            "id": claim.run_id,
        },
        "inputs": inputs,
        "outputs": outputs,
    })
}

fn function_completed(
    activity: &ActivityDefinition,
    claim: &ActivityExecutionClaim,
    result: Value,
    summary: Option<String>,
) -> super::ActivityEvent {
    super::ActivityEvent::ActivityCompleted {
        activity_key: claim.activity_key.clone(),
        attempt: claim.attempt,
        outputs: function_outputs(activity, result),
        summary,
    }
}

fn function_failed(
    claim: &ActivityExecutionClaim,
    error: impl Into<String>,
) -> super::ActivityEvent {
    super::ActivityEvent::ActivityFailed {
        activity_key: claim.activity_key.clone(),
        attempt: claim.attempt,
        error: error.into(),
    }
}

fn function_outputs(activity: &ActivityDefinition, value: Value) -> Vec<ActivityPortValue> {
    activity
        .output_ports
        .iter()
        .map(|port| ActivityPortValue {
            port_key: port.key.clone(),
            value: value.clone(),
        })
        .collect()
}

fn human_decision_id(claim: &ActivityExecutionClaim) -> String {
    format!("{}:{}#{}", claim.run_id, claim.activity_key, claim.attempt)
}

fn scope_from_run_context_or_project(
    run_context: Option<&agentdash_spi::hooks::SessionRunContext>,
    fallback_project_id: uuid::Uuid,
) -> agentdash_spi::CapabilityScopeCtx {
    match run_context {
        Some(ctx) => match ctx.scope {
            CapabilityScope::Task => agentdash_spi::CapabilityScopeCtx::Task {
                project_id: ctx.project_id,
                story_id: ctx.story_id.unwrap_or(ctx.project_id),
                task_id: ctx.task_id.unwrap_or(ctx.project_id),
            },
            CapabilityScope::Story => agentdash_spi::CapabilityScopeCtx::Story {
                project_id: ctx.project_id,
                story_id: ctx.story_id.unwrap_or(ctx.project_id),
            },
            CapabilityScope::Project => agentdash_spi::CapabilityScopeCtx::Project {
                project_id: ctx.project_id,
            },
        },
        None => agentdash_spi::CapabilityScopeCtx::Project {
            project_id: fallback_project_id,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::workflow::{
        ActivityAttemptState, ActivityAttemptStatus, ActivityCompletionPolicy, ActivityDefinition,
        ActivityExecutionClaim, ActivityExecutionClaimStatus, ActivityExecutorSpec,
        ActivityTransition, ActivityTransitionKind, AgentActivityExecutorSpec, AgentSessionPolicy,
        ApiRequestExecutorSpec, BashExecExecutorSpec, DefinitionSource,
        FunctionActivityExecutorSpec, HumanActivityExecutorSpec, HumanApprovalExecutorSpec,
        OutputPortDefinition, TransitionCondition,
    };

    use super::*;
    use crate::workflow::{ActivityEvent, ActivityLifecycleRunState, ActivityRunStatus};

    fn test_graph_instance_id() -> uuid::Uuid {
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    #[derive(Default)]
    struct FakePort {
        sessions: Mutex<Vec<String>>,
        launch_error: Mutex<Option<String>>,
        launches: Mutex<Vec<String>>,
        continue_root_applies: Mutex<Vec<String>>,
        assignments: Mutex<Vec<AgentAssignment>>,
    }

    #[async_trait::async_trait]
    impl AgentActivitySessionPort for FakePort {
        async fn get_executor_config(
            &self,
            _runtime_session_id: &str,
        ) -> Result<Option<AgentConfig>, String> {
            Ok(None)
        }

        async fn set_executor_config(
            &self,
            _runtime_session_id: &str,
            _executor_config: AgentConfig,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn mark_owner_bootstrap_pending(
            &self,
            _runtime_session_id: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn launch_workflow_prompt(
            &self,
            runtime_session_id: &str,
            _executor_config: Option<AgentConfig>,
        ) -> Result<(), String> {
            if let Some(error) = self.launch_error.lock().unwrap().clone() {
                return Err(error);
            }
            self.launches
                .lock()
                .unwrap()
                .push(runtime_session_id.to_string());
            Ok(())
        }

        async fn create_agent_activity_assignment(
            &self,
            _definition: &WorkflowGraph,
            _activity: &ActivityDefinition,
            claim: &ActivityExecutionClaim,
            runtime_session_id: Option<&str>,
            _executor_config: Option<&AgentConfig>,
        ) -> Result<AgentActivityAssignmentContext, String> {
            let agent_id = uuid::Uuid::new_v4();
            let runtime_refs =
                runtime_session_id.and_then(|id| AgentFrame::runtime_session_refs_json([id]));
            let mut frame = AgentFrame::new_initial(agent_id, runtime_refs);
            frame.graph_instance_id = Some(claim.graph_instance_id);
            frame.activity_key = Some(claim.activity_key.clone());
            let assignment = AgentAssignment::new(
                claim.run_id,
                claim.graph_instance_id,
                claim.activity_key.clone(),
                claim.attempt as i32,
                agent_id,
                frame.id,
            );
            self.assignments.lock().unwrap().push(assignment.clone());
            Ok(AgentActivityAssignmentContext { assignment, frame })
        }

        async fn create_runtime_session_for_agent_activity(
            &self,
            _definition: &WorkflowGraph,
            _activity: &ActivityDefinition,
            claim: &ActivityExecutionClaim,
            _assignment: &AgentAssignment,
            _frame: &AgentFrame,
        ) -> Result<String, String> {
            let runtime_session_id = format!("child-{}", self.sessions.lock().unwrap().len() + 1);
            self.sessions
                .lock()
                .unwrap()
                .push(format!("{}#{}", claim.activity_key, claim.attempt));
            Ok(runtime_session_id)
        }

        async fn apply_continue_root_activity(
            &self,
            _definition: &WorkflowGraph,
            _activity: &ActivityDefinition,
            claim: &ActivityExecutionClaim,
            _procedure_key: &str,
            _root_runtime_session_id: &str,
        ) -> Result<(), String> {
            self.continue_root_applies
                .lock()
                .unwrap()
                .push(format!("{}#{}", claim.activity_key, claim.attempt));
            Ok(())
        }

        async fn execute_function_activity(
            &self,
            definition: &WorkflowGraph,
            activity: &ActivityDefinition,
            claim: &ActivityExecutionClaim,
            spec: &FunctionActivityExecutorSpec,
            state: &ActivityLifecycleRunState,
        ) -> Result<FunctionExecutionResult, String> {
            let runner = agentdash_infrastructure::DefaultFunctionRunner::new();
            super::execute_function_activity(&runner, definition, activity, claim, spec, state)
                .await
        }
    }

    fn output_port(key: &str) -> OutputPortDefinition {
        OutputPortDefinition {
            key: key.to_string(),
            description: format!("{key} output"),
            gate_strategy: Default::default(),
            gate_params: None,
        }
    }

    fn definition(project_id: uuid::Uuid) -> WorkflowGraph {
        WorkflowGraph::new(
            project_id,
            "agent_flow",
            "Agent flow",
            "",
            DefinitionSource::UserAuthored,
            "plan",
            vec![ActivityDefinition {
                key: "plan".to_string(),
                description: "plan".to_string(),
                executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                    procedure_key: "wf_plan".to_string(),
                    session_policy: AgentSessionPolicy::SpawnChild,
                }),
                input_ports: vec![],
                output_ports: vec![output_port("proposal")],
                completion_policy: ActivityCompletionPolicy::OutputPorts {
                    required_ports: vec!["proposal".to_string()],
                },
                iteration_policy: Default::default(),
                join_policy: Default::default(),
            }],
            vec![],
        )
        .expect("definition")
    }

    fn continue_root_definition(project_id: uuid::Uuid) -> WorkflowGraph {
        continue_root_definition_with_activities(project_id, &["plan"])
    }

    fn human_approval_definition(project_id: uuid::Uuid) -> WorkflowGraph {
        WorkflowGraph::new(
            project_id,
            "approval_flow",
            "Approval flow",
            "",
            DefinitionSource::UserAuthored,
            "approval",
            vec![ActivityDefinition {
                key: "approval".to_string(),
                description: "approval".to_string(),
                executor: ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(
                    HumanApprovalExecutorSpec {
                        form_schema_key: "approval.plan_review".to_string(),
                        title: Some("Review plan".to_string()),
                    },
                )),
                input_ports: vec![],
                output_ports: vec![output_port("decision")],
                completion_policy: ActivityCompletionPolicy::HumanDecision {
                    decision_port: "decision".to_string(),
                },
                iteration_policy: Default::default(),
                join_policy: Default::default(),
            }],
            vec![],
        )
        .expect("definition")
    }

    fn function_definition(
        project_id: uuid::Uuid,
        spec: FunctionActivityExecutorSpec,
    ) -> WorkflowGraph {
        WorkflowGraph::new(
            project_id,
            "function_flow",
            "Function flow",
            "",
            DefinitionSource::UserAuthored,
            "collect",
            vec![ActivityDefinition {
                key: "collect".to_string(),
                description: "collect".to_string(),
                executor: ActivityExecutorSpec::Function(spec),
                input_ports: vec![],
                output_ports: vec![output_port("result")],
                completion_policy: ActivityCompletionPolicy::OutputPorts {
                    required_ports: vec!["result".to_string()],
                },
                iteration_policy: Default::default(),
                join_policy: Default::default(),
            }],
            vec![],
        )
        .expect("definition")
    }

    #[cfg(windows)]
    fn bash_spec(script: &str) -> FunctionActivityExecutorSpec {
        FunctionActivityExecutorSpec::BashExec(BashExecExecutorSpec {
            command: "cmd".to_string(),
            args: vec!["/C".to_string(), script.to_string()],
            working_directory: None,
        })
    }

    fn api_spec(url: String) -> FunctionActivityExecutorSpec {
        FunctionActivityExecutorSpec::ApiRequest(ApiRequestExecutorSpec {
            method: "GET".to_string(),
            url_template: url,
            body_template: None,
        })
    }

    async fn serve_once(response: &'static str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer).await;
            let _ = stream.write_all(response.as_bytes()).await;
        });
        format!("http://{addr}/function-test")
    }

    #[cfg(not(windows))]
    fn bash_spec(script: &str) -> FunctionActivityExecutorSpec {
        FunctionActivityExecutorSpec::BashExec(BashExecExecutorSpec {
            command: "sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            working_directory: None,
        })
    }

    fn continue_root_definition_with_activities(
        project_id: uuid::Uuid,
        activity_keys: &[&str],
    ) -> WorkflowGraph {
        WorkflowGraph::new(
            project_id,
            "agent_flow",
            "Agent flow",
            "",
            DefinitionSource::UserAuthored,
            "plan",
            activity_keys
                .iter()
                .map(|key| ActivityDefinition {
                    key: (*key).to_string(),
                    description: (*key).to_string(),
                    executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                        procedure_key: format!("wf_{key}"),
                        session_policy: AgentSessionPolicy::ContinueRoot,
                    }),
                    input_ports: vec![],
                    output_ports: vec![],
                    completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
                    iteration_policy: Default::default(),
                    join_policy: Default::default(),
                })
                .collect(),
            if activity_keys.len() > 1 {
                vec![ActivityTransition {
                    from: "plan".to_string(),
                    to: "review".to_string(),
                    kind: ActivityTransitionKind::Flow,
                    condition: TransitionCondition::Always,
                    artifact_bindings: vec![],
                    max_traversals: None,
                }]
            } else {
                vec![]
            },
        )
        .expect("definition")
    }

    fn state() -> ActivityLifecycleRunState {
        ActivityLifecycleRunState {
            graph_instance_id: test_graph_instance_id(),
            status: ActivityRunStatus::Ready,
            attempts: vec![ActivityAttemptState {
                activity_key: "plan".to_string(),
                attempt: 1,
                status: ActivityAttemptStatus::Claiming,
                executor_run: None,
                started_at: None,
                completed_at: None,
                summary: None,
            }],
            outputs: vec![],
            inputs: vec![],
        }
    }

    #[tokio::test]
    async fn spawn_child_creates_session_and_launches_prompt() {
        let project_id = uuid::Uuid::new_v4();
        let port = FakePort::default();
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "agent_flow".to_string(),
                root_runtime_session_id: "root-session".to_string(),
            },
            port,
        );
        let definition = definition(project_id);
        let claim = ActivityExecutionClaim {
            run_id: uuid::Uuid::new_v4(),
            graph_instance_id: test_graph_instance_id(),
            activity_key: "plan".to_string(),
            attempt: 1,
            claim_id: uuid::Uuid::new_v4(),
            executor_kind: "agent".to_string(),
            status: ActivityExecutionClaimStatus::Claiming,
            idempotency_key: "claim".to_string(),
            executor_run_ref: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let start_result = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        assert_eq!(
            start_result.executor_run,
            ExecutorRunRef::RuntimeSession {
                session_id: "child-1".to_string()
            }
        );
        assert!(start_result.immediate_events.is_empty());
        assert_eq!(
            launcher.port.launches.lock().unwrap().as_slice(),
            &["child-1".to_string()]
        );
    }

    #[tokio::test]
    async fn spawn_child_launch_failure_is_retryable() {
        let project_id = uuid::Uuid::new_v4();
        let port = FakePort {
            launch_error: Mutex::new(Some("prompt rejected".to_string())),
            ..Default::default()
        };
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "agent_flow".to_string(),
                root_runtime_session_id: "root-session".to_string(),
            },
            port,
        );
        let definition = definition(project_id);
        let claim = ActivityExecutionClaim::new(
            uuid::Uuid::new_v4(),
            test_graph_instance_id(),
            "plan",
            1,
            "agent",
        );

        let error = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect_err("launch should fail");

        assert!(error.retryable);
        assert_eq!(error.message, "prompt rejected");
    }

    #[tokio::test]
    async fn continue_root_applies_runtime_transition_and_uses_root_session() {
        let project_id = uuid::Uuid::new_v4();
        let port = FakePort::default();
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "agent_flow".to_string(),
                root_runtime_session_id: "root-session".to_string(),
            },
            port,
        );
        let definition = continue_root_definition(project_id);
        let claim = ActivityExecutionClaim::new(
            uuid::Uuid::new_v4(),
            test_graph_instance_id(),
            "plan",
            1,
            "agent",
        );

        let start_result = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        assert_eq!(
            start_result.executor_run,
            ExecutorRunRef::RuntimeSession {
                session_id: "root-session".to_string()
            }
        );
        assert_eq!(
            launcher
                .port
                .continue_root_applies
                .lock()
                .unwrap()
                .as_slice(),
            &["plan#1".to_string()]
        );
    }

    #[tokio::test]
    async fn continue_root_rejects_parallel_running_attempt() {
        let project_id = uuid::Uuid::new_v4();
        let port = FakePort::default();
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "agent_flow".to_string(),
                root_runtime_session_id: "root-session".to_string(),
            },
            port,
        );
        let definition = continue_root_definition_with_activities(project_id, &["plan", "review"]);
        let mut state = state();
        state.attempts.push(ActivityAttemptState {
            activity_key: "review".to_string(),
            attempt: 1,
            status: ActivityAttemptStatus::Running,
            executor_run: Some(ExecutorRunRef::RuntimeSession {
                session_id: "root-session".to_string(),
            }),
            started_at: Some(chrono::Utc::now()),
            completed_at: None,
            summary: None,
        });
        let claim = ActivityExecutionClaim::new(
            uuid::Uuid::new_v4(),
            test_graph_instance_id(),
            "plan",
            1,
            "agent",
        );

        let error = launcher
            .start(&definition, &state, &claim)
            .await
            .expect_err("parallel ContinueRoot should be rejected");

        assert!(!error.retryable);
        assert_eq!(
            launcher
                .port
                .continue_root_applies
                .lock()
                .unwrap()
                .as_slice(),
            &[] as &[String]
        );
    }

    #[tokio::test]
    async fn human_approval_returns_pending_decision_ref() {
        let project_id = uuid::Uuid::new_v4();
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "approval_flow".to_string(),
                root_runtime_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = human_approval_definition(project_id);
        let claim = ActivityExecutionClaim::new(
            uuid::Uuid::new_v4(),
            test_graph_instance_id(),
            "approval",
            1,
            "human",
        );

        let start_result = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        assert_eq!(
            start_result.executor_run,
            ExecutorRunRef::HumanDecision {
                decision_id: format!("{}:approval#1", claim.run_id)
            }
        );
        assert!(start_result.immediate_events.is_empty());
    }

    #[tokio::test]
    async fn function_bash_success_returns_completed_event() {
        let project_id = uuid::Uuid::new_v4();
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "function_flow".to_string(),
                root_runtime_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = function_definition(project_id, bash_spec("echo hello"));
        let claim = ActivityExecutionClaim::new(
            uuid::Uuid::new_v4(),
            test_graph_instance_id(),
            "collect",
            1,
            "function",
        );

        let start_result = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        assert!(matches!(
            start_result.executor_run,
            ExecutorRunRef::FunctionRun { .. }
        ));
        assert_eq!(start_result.immediate_events.len(), 1);
        let ActivityEvent::ActivityCompleted {
            activity_key,
            attempt,
            outputs,
            ..
        } = &start_result.immediate_events[0]
        else {
            panic!("expected completed event");
        };
        assert_eq!(activity_key, "collect");
        assert_eq!(*attempt, 1);
        assert_eq!(outputs[0].port_key, "result");
        assert!(
            outputs[0].value["stdout"]
                .as_str()
                .unwrap_or_default()
                .contains("hello")
        );
    }

    #[tokio::test]
    async fn function_bash_failure_returns_failed_event() {
        let project_id = uuid::Uuid::new_v4();
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "function_flow".to_string(),
                root_runtime_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = function_definition(project_id, bash_spec("exit 7"));
        let claim = ActivityExecutionClaim::new(
            uuid::Uuid::new_v4(),
            test_graph_instance_id(),
            "collect",
            1,
            "function",
        );

        let start_result = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        assert!(matches!(
            start_result.executor_run,
            ExecutorRunRef::FunctionRun { .. }
        ));
        let ActivityEvent::ActivityFailed {
            activity_key,
            attempt,
            error,
        } = &start_result.immediate_events[0]
        else {
            panic!("expected failed event");
        };
        assert_eq!(activity_key, "collect");
        assert_eq!(*attempt, 1);
        assert!(error.contains("exit_code=7"));
    }

    #[tokio::test]
    async fn function_api_request_success_returns_completed_event() {
        let url = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
        )
        .await;
        let project_id = uuid::Uuid::new_v4();
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "function_flow".to_string(),
                root_runtime_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = function_definition(project_id, api_spec(url));
        let claim = ActivityExecutionClaim::new(
            uuid::Uuid::new_v4(),
            test_graph_instance_id(),
            "collect",
            1,
            "function",
        );

        let start_result = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        let ActivityEvent::ActivityCompleted { outputs, .. } = &start_result.immediate_events[0]
        else {
            panic!("expected completed event");
        };
        assert_eq!(outputs[0].value["status"], 200);
        assert_eq!(outputs[0].value["body_json"]["ok"], true);
    }

    #[tokio::test]
    async fn function_api_request_failure_returns_failed_event() {
        let url =
            serve_once("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 5\r\n\r\nerror")
                .await;
        let project_id = uuid::Uuid::new_v4();
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "function_flow".to_string(),
                root_runtime_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = function_definition(project_id, api_spec(url));
        let claim = ActivityExecutionClaim::new(
            uuid::Uuid::new_v4(),
            test_graph_instance_id(),
            "collect",
            1,
            "function",
        );

        let start_result = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        let ActivityEvent::ActivityFailed { error, .. } = &start_result.immediate_events[0] else {
            panic!("expected failed event");
        };
        assert!(error.contains("500"));
    }
}
