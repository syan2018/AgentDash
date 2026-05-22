use agentdash_domain::session_binding::{SessionBinding, SessionOwnerCtx, SessionOwnerType};
use agentdash_domain::workflow::{
    ActivityAttemptStatus, ActivityDefinition, ActivityExecutionClaim, ActivityExecutorSpec,
    ActivityLifecycleDefinition, ActivityPortValue, AgentSessionPolicy, ExecutorRunRef,
    FunctionActivityExecutorSpec, HumanActivityExecutorSpec, LifecycleNodeType,
    LifecycleStepDefinition,
};
use agentdash_spi::AgentConfig;
use serde_json::{Value, json};
use tokio::process::Command;

use super::ActivityLifecycleRunState;
use super::scheduler::{
    ActivityExecutorLauncher, ActivityExecutorStartError, ActivityExecutorStartResult,
};
use super::session_association::{LIFECYCLE_ACTIVITY_LABEL_PREFIX, build_lifecycle_activity_label};
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
use crate::workflow::step_activation::apply_to_running_session;
use crate::workflow::{
    activate_step_with_platform, agent_mcp_entries_from_servers,
    build_capability_state_for_activation, load_port_output_map,
};

#[derive(Debug, Clone)]
pub struct AgentActivityLaunchContext {
    pub project_id: uuid::Uuid,
    pub lifecycle_key: String,
    pub root_session_id: String,
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

#[async_trait::async_trait]
pub trait AgentActivitySessionPort: Send + Sync {
    async fn create_session(&self, title: &str) -> Result<String, String>;
    async fn list_session_bindings(&self, session_id: &str) -> Result<Vec<SessionBinding>, String>;
    async fn create_session_binding(&self, binding: SessionBinding) -> Result<(), String>;
    async fn get_executor_config(&self, session_id: &str) -> Result<Option<AgentConfig>, String>;
    async fn set_executor_config(
        &self,
        session_id: &str,
        executor_config: AgentConfig,
    ) -> Result<(), String>;
    async fn mark_owner_bootstrap_pending(&self, session_id: &str) -> Result<(), String>;
    async fn launch_workflow_prompt(
        &self,
        session_id: &str,
        executor_config: Option<AgentConfig>,
    ) -> Result<(), String>;
    async fn apply_continue_root_activity(
        &self,
        _definition: &ActivityLifecycleDefinition,
        _activity: &ActivityDefinition,
        _claim: &ActivityExecutionClaim,
        _workflow_key: &str,
        _root_session_id: &str,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn execute_function_activity(
        &self,
        _definition: &ActivityLifecycleDefinition,
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
}

impl AgentActivityRuntimePort {
    pub fn new(
        session_core: SessionCoreService,
        session_launch: SessionLaunchService,
        repos: RepositorySet,
    ) -> Self {
        Self {
            session_core,
            session_launch,
            session_hooks: None,
            session_capability: None,
            repos,
            platform_config: None,
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
    async fn create_session(&self, title: &str) -> Result<String, String> {
        self.session_core
            .create_session(title)
            .await
            .map(|meta| meta.id)
            .map_err(|error| format!("创建 activity child session 失败: {error}"))
    }

    async fn list_session_bindings(&self, session_id: &str) -> Result<Vec<SessionBinding>, String> {
        self.repos
            .session_binding_repo
            .list_by_session(session_id)
            .await
            .map_err(|error| format!("查询 root session binding 失败: {error}"))
    }

    async fn create_session_binding(&self, binding: SessionBinding) -> Result<(), String> {
        self.repos
            .session_binding_repo
            .create(&binding)
            .await
            .map_err(|error| format!("创建 activity session binding 失败: {error}"))
    }

    async fn get_executor_config(&self, session_id: &str) -> Result<Option<AgentConfig>, String> {
        self.session_core
            .get_session_meta(session_id)
            .await
            .map_err(|error| format!("读取 root session meta 失败: {error}"))
            .map(|meta| meta.and_then(|meta| meta.executor_config))
    }

    async fn set_executor_config(
        &self,
        session_id: &str,
        executor_config: AgentConfig,
    ) -> Result<(), String> {
        self.session_core
            .update_session_meta(session_id, move |meta| {
                meta.executor_config = Some(executor_config.clone());
            })
            .await
            .map_err(|error| format!("继承 executor config 失败: {error}"))?;
        Ok(())
    }

    async fn mark_owner_bootstrap_pending(&self, session_id: &str) -> Result<(), String> {
        self.session_core
            .mark_owner_bootstrap_pending(session_id)
            .await
            .map_err(|error| format!("标记 owner bootstrap pending 失败: {error}"))
    }

    async fn launch_workflow_prompt(
        &self,
        session_id: &str,
        executor_config: Option<AgentConfig>,
    ) -> Result<(), String> {
        let mut user_input = UserPromptInput::from_text("");
        user_input.executor_config = executor_config;
        let command = LaunchCommand::workflow_orchestrator_input(user_input);
        self.session_launch
            .launch_command(session_id, command)
            .await
            .map(|_| ())
            .map_err(|error| format!("启动 activity child session prompt 失败: {error}"))
    }

    async fn apply_continue_root_activity(
        &self,
        definition: &ActivityLifecycleDefinition,
        activity: &ActivityDefinition,
        claim: &ActivityExecutionClaim,
        workflow_key: &str,
        root_session_id: &str,
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
            .workflow_definition_repo
            .get_by_project_and_key(definition.project_id, workflow_key)
            .await
            .map_err(|error| format!("加载 ContinueRoot workflow 失败: {error}"))?
            .ok_or_else(|| format!("ContinueRoot workflow 不存在: {workflow_key}"))?;

        let active_step = activity_as_step(activity, workflow_key);
        let available_presets =
            crate::session::load_available_presets(&self.repos, definition.project_id).await;
        let ready_port_keys =
            load_port_output_map(self.repos.inline_file_repo.as_ref(), claim.run_id)
                .await
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();

        if let Some(hook_session) = session_hooks
            .ensure_hook_session_runtime(root_session_id, None)
            .await
            .map_err(|error| format!("加载 root hook runtime 失败: {error}"))?
        {
            let snapshot = hook_session.snapshot();
            let owner_ctx =
                owner_ctx_from_bindings_or_project(&snapshot.owners, definition.project_id);
            let runtime_mcp_servers = session_capability
                .get_runtime_mcp_servers(root_session_id)
                .await;
            let activation = activate_step_with_platform(
                &crate::workflow::StepActivationInput {
                    owner_ctx,
                    active_step: &active_step,
                    workflow: Some(&workflow),
                    run_id: claim.run_id,
                    lifecycle_key: &definition.key,
                    edges: &[],
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
            apply_to_running_session(
                &activation,
                &hook_session,
                session_capability,
                None,
                &activity.key,
                Some(claim.run_id),
                Some(&definition.key),
            )
            .await
            .map(|_| ())
        } else {
            let owner_ctx = SessionOwnerCtx::Project {
                project_id: definition.project_id,
            };
            let base_surface = session_capability
                .get_latest_capability_state(root_session_id)
                .await;
            let agent_mcp_servers = base_surface
                .as_ref()
                .map(|surface| agent_mcp_entries_from_servers(&surface.tool.mcp_servers))
                .unwrap_or_default();
            let activation = activate_step_with_platform(
                &crate::workflow::StepActivationInput {
                    owner_ctx,
                    active_step: &active_step,
                    workflow: Some(&workflow),
                    run_id: claim.run_id,
                    lifecycle_key: &definition.key,
                    edges: &[],
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
            session_capability
                .enqueue_pending_runtime_context_transition(PendingRuntimeContextTransitionInput {
                    session_id: root_session_id.to_string(),
                    turn_id: None,
                    transition_id: format!(
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
        definition: &ActivityLifecycleDefinition,
        activity: &ActivityDefinition,
        claim: &ActivityExecutionClaim,
        spec: &FunctionActivityExecutorSpec,
        state: &ActivityLifecycleRunState,
    ) -> Result<FunctionExecutionResult, String> {
        execute_function_activity(definition, activity, claim, spec, state).await
    }
}

#[async_trait::async_trait]
impl<P> ActivityExecutorLauncher for AgentActivityExecutorLauncher<P>
where
    P: AgentActivitySessionPort,
{
    async fn start(
        &self,
        definition: &ActivityLifecycleDefinition,
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
                AgentSessionPolicy::SpawnChild => self.start_spawn_child(definition, claim).await,
                AgentSessionPolicy::ContinueRoot => {
                    self.start_continue_root(
                        definition,
                        &activity,
                        spec.workflow_key.as_str(),
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
        definition: &ActivityLifecycleDefinition,
        claim: &ActivityExecutionClaim,
    ) -> Result<ActivityExecutorStartResult, ActivityExecutorStartError> {
        let title = format!(
            "[{}] {}#{}",
            definition.key, claim.activity_key, claim.attempt
        );
        let session_id = self
            .port
            .create_session(&title)
            .await
            .map_err(ActivityExecutorStartError::retryable)?;

        let bindings = self
            .port
            .list_session_bindings(&self.context.root_session_id)
            .await
            .map_err(ActivityExecutorStartError::retryable)?;
        let owner_binding = bindings
            .iter()
            .find(|binding| !binding.label.starts_with(LIFECYCLE_ACTIVITY_LABEL_PREFIX))
            .or_else(|| bindings.first());
        let binding = if let Some(owner_binding) = owner_binding {
            SessionBinding::new(
                self.context.project_id,
                session_id.clone(),
                owner_binding.owner_type,
                owner_binding.owner_id,
                build_lifecycle_activity_label(claim.run_id, &claim.activity_key, claim.attempt),
            )
        } else {
            SessionBinding::new(
                self.context.project_id,
                session_id.clone(),
                SessionOwnerType::Project,
                self.context.project_id,
                build_lifecycle_activity_label(claim.run_id, &claim.activity_key, claim.attempt),
            )
        };
        self.port
            .create_session_binding(binding)
            .await
            .map_err(ActivityExecutorStartError::retryable)?;

        let executor_config = self
            .port
            .get_executor_config(&self.context.root_session_id)
            .await
            .map_err(ActivityExecutorStartError::retryable)?;
        if let Some(executor_config) = executor_config.clone() {
            self.port
                .set_executor_config(&session_id, executor_config)
                .await
                .map_err(ActivityExecutorStartError::retryable)?;
        }

        self.port
            .mark_owner_bootstrap_pending(&session_id)
            .await
            .map_err(ActivityExecutorStartError::retryable)?;
        self.port
            .launch_workflow_prompt(&session_id, executor_config)
            .await
            .map_err(ActivityExecutorStartError::retryable)?;

        Ok(ActivityExecutorStartResult::started(
            ExecutorRunRef::AgentSession { session_id },
        ))
    }

    async fn start_continue_root(
        &self,
        definition: &ActivityLifecycleDefinition,
        activity: &ActivityDefinition,
        workflow_key: &str,
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

        self.port
            .apply_continue_root_activity(
                definition,
                activity,
                claim,
                workflow_key,
                &self.context.root_session_id,
            )
            .await
            .map_err(ActivityExecutorStartError::retryable)?;
        Ok(ActivityExecutorStartResult::started(
            ExecutorRunRef::AgentSession {
                session_id: self.context.root_session_id.clone(),
            },
        ))
    }

    async fn start_function(
        &self,
        definition: &ActivityLifecycleDefinition,
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

fn activity_as_step(activity: &ActivityDefinition, workflow_key: &str) -> LifecycleStepDefinition {
    LifecycleStepDefinition {
        key: activity.key.clone(),
        description: activity.description.clone(),
        workflow_key: Some(workflow_key.to_string()),
        node_type: LifecycleNodeType::PhaseNode,
        output_ports: activity.output_ports.clone(),
        input_ports: activity.input_ports.clone(),
        capability_config: Default::default(),
    }
}

async fn execute_function_activity(
    definition: &ActivityLifecycleDefinition,
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
            execute_api_request(activity, claim, spec, &context).await
        }
        FunctionActivityExecutorSpec::BashExec(spec) => {
            execute_bash(activity, claim, spec, &context).await
        }
    };
    Ok(FunctionExecutionResult {
        executor_run,
        completion_event,
    })
}

async fn execute_api_request(
    activity: &ActivityDefinition,
    claim: &ActivityExecutionClaim,
    spec: &agentdash_domain::workflow::ApiRequestExecutorSpec,
    context: &Value,
) -> super::ActivityEvent {
    let method_text = match render_template(&spec.method, context) {
        Ok(value) => value,
        Err(error) => return function_failed(claim, error),
    };
    let method = match reqwest::Method::from_bytes(method_text.as_bytes()) {
        Ok(method) => method,
        Err(error) => return function_failed(claim, format!("API method 非法: {error}")),
    };
    let url = match render_template(&spec.url_template, context) {
        Ok(value) => value,
        Err(error) => return function_failed(claim, error),
    };
    let client = reqwest::Client::new();
    let mut request = client.request(method, url);
    if let Some(body_template) = &spec.body_template {
        let body = match render_json_templates(body_template, context) {
            Ok(value) => value,
            Err(error) => return function_failed(claim, error),
        };
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => return function_failed(claim, format!("API request 失败: {error}")),
    };
    let status = response.status();
    let body_text = match response.text().await {
        Ok(text) => text,
        Err(error) => return function_failed(claim, format!("读取 API response 失败: {error}")),
    };
    let body_json = serde_json::from_str::<Value>(&body_text).ok();
    let result = json!({
        "status": status.as_u16(),
        "body_text": body_text,
        "body_json": body_json,
    });
    if status.is_success() {
        function_completed(
            activity,
            claim,
            result,
            Some(format!("API request {}", status.as_u16())),
        )
    } else {
        function_failed(
            claim,
            format!("API request 返回非成功状态: {}", status.as_u16()),
        )
    }
}

async fn execute_bash(
    activity: &ActivityDefinition,
    claim: &ActivityExecutionClaim,
    spec: &agentdash_domain::workflow::BashExecExecutorSpec,
    context: &Value,
) -> super::ActivityEvent {
    let command = match render_template(&spec.command, context) {
        Ok(value) => value,
        Err(error) => return function_failed(claim, error),
    };
    let args = match spec
        .args
        .iter()
        .map(|arg| render_template(arg, context))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(value) => value,
        Err(error) => return function_failed(claim, error),
    };
    let mut command_builder = Command::new(command);
    command_builder.args(args);
    if let Some(working_directory) = &spec.working_directory {
        match render_template(working_directory, context) {
            Ok(rendered) if !rendered.trim().is_empty() => {
                command_builder.current_dir(rendered);
            }
            Ok(_) => {}
            Err(error) => return function_failed(claim, error),
        }
    }
    let output = match command_builder.output().await {
        Ok(output) => output,
        Err(error) => return function_failed(claim, format!("Bash exec 启动失败: {error}")),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();
    let result = json!({
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    });
    if output.status.success() {
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
                exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
        )
    }
}

fn function_template_context(
    definition: &ActivityLifecycleDefinition,
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

fn render_template(template: &str, context: &Value) -> Result<String, String> {
    let context = tera::Context::from_serialize(context)
        .map_err(|error| format!("Function template context 非法: {error}"))?;
    tera::Tera::one_off(template, &context, false)
        .map_err(|error| format!("Function template 渲染失败: {error}"))
}

fn render_json_templates(value: &Value, context: &Value) -> Result<Value, String> {
    match value {
        Value::String(template) => render_template(template, context).map(Value::String),
        Value::Array(values) => values
            .iter()
            .map(|value| render_json_templates(value, context))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| Ok((key.clone(), render_json_templates(value, context)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(Value::Object),
        other => Ok(other.clone()),
    }
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

fn owner_ctx_from_bindings_or_project(
    owners: &[agentdash_spi::hooks::HookOwnerSummary],
    fallback_project_id: uuid::Uuid,
) -> SessionOwnerCtx {
    let Some(owner) = owners.first() else {
        return SessionOwnerCtx::Project {
            project_id: fallback_project_id,
        };
    };
    let project_id = owner
        .project_id
        .as_deref()
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .unwrap_or(fallback_project_id);
    let story_id = owner
        .story_id
        .as_deref()
        .and_then(|id| uuid::Uuid::parse_str(id).ok());
    let task_id = owner
        .task_id
        .as_deref()
        .and_then(|id| uuid::Uuid::parse_str(id).ok());

    match owner.owner_type {
        SessionOwnerType::Task => match (story_id, task_id) {
            (Some(story_id), Some(task_id)) => SessionOwnerCtx::Task {
                project_id,
                story_id,
                task_id,
            },
            _ => SessionOwnerCtx::Project { project_id },
        },
        SessionOwnerType::Story => match story_id {
            Some(story_id) => SessionOwnerCtx::Story {
                project_id,
                story_id,
            },
            None => SessionOwnerCtx::Project { project_id },
        },
        SessionOwnerType::Project => SessionOwnerCtx::Project { project_id },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::session_binding::SessionOwnerType;
    use agentdash_domain::workflow::{
        ActivityAttemptState, ActivityAttemptStatus, ActivityCompletionPolicy, ActivityDefinition,
        ActivityExecutionClaim, ActivityExecutionClaimStatus, ActivityExecutorSpec,
        ActivityTransition, ActivityTransitionKind, AgentActivityExecutorSpec, AgentSessionPolicy,
        ApiRequestExecutorSpec, BashExecExecutorSpec, FunctionActivityExecutorSpec,
        HumanActivityExecutorSpec, HumanApprovalExecutorSpec, OutputPortDefinition,
        TransitionCondition, WorkflowBindingKind, WorkflowDefinitionSource,
    };

    use super::*;
    use crate::workflow::{ActivityEvent, ActivityLifecycleRunState, ActivityRunStatus};

    #[derive(Default)]
    struct FakePort {
        sessions: Mutex<Vec<String>>,
        bindings: Mutex<Vec<SessionBinding>>,
        launch_error: Mutex<Option<String>>,
        launches: Mutex<Vec<String>>,
        continue_root_applies: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl AgentActivitySessionPort for FakePort {
        async fn create_session(&self, title: &str) -> Result<String, String> {
            let session_id = format!("child-{}", self.sessions.lock().unwrap().len() + 1);
            self.sessions.lock().unwrap().push(title.to_string());
            Ok(session_id)
        }

        async fn list_session_bindings(
            &self,
            _session_id: &str,
        ) -> Result<Vec<SessionBinding>, String> {
            Ok(self.bindings.lock().unwrap().clone())
        }

        async fn create_session_binding(&self, binding: SessionBinding) -> Result<(), String> {
            self.bindings.lock().unwrap().push(binding);
            Ok(())
        }

        async fn get_executor_config(
            &self,
            _session_id: &str,
        ) -> Result<Option<AgentConfig>, String> {
            Ok(None)
        }

        async fn set_executor_config(
            &self,
            _session_id: &str,
            _executor_config: AgentConfig,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn mark_owner_bootstrap_pending(&self, _session_id: &str) -> Result<(), String> {
            Ok(())
        }

        async fn launch_workflow_prompt(
            &self,
            session_id: &str,
            _executor_config: Option<AgentConfig>,
        ) -> Result<(), String> {
            if let Some(error) = self.launch_error.lock().unwrap().clone() {
                return Err(error);
            }
            self.launches.lock().unwrap().push(session_id.to_string());
            Ok(())
        }

        async fn apply_continue_root_activity(
            &self,
            _definition: &ActivityLifecycleDefinition,
            _activity: &ActivityDefinition,
            claim: &ActivityExecutionClaim,
            _workflow_key: &str,
            _root_session_id: &str,
        ) -> Result<(), String> {
            self.continue_root_applies
                .lock()
                .unwrap()
                .push(format!("{}#{}", claim.activity_key, claim.attempt));
            Ok(())
        }

        async fn execute_function_activity(
            &self,
            definition: &ActivityLifecycleDefinition,
            activity: &ActivityDefinition,
            claim: &ActivityExecutionClaim,
            spec: &FunctionActivityExecutorSpec,
            state: &ActivityLifecycleRunState,
        ) -> Result<FunctionExecutionResult, String> {
            super::execute_function_activity(definition, activity, claim, spec, state).await
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

    fn definition(project_id: uuid::Uuid) -> ActivityLifecycleDefinition {
        ActivityLifecycleDefinition::new(
            project_id,
            "agent_flow",
            "Agent flow",
            "",
            vec![WorkflowBindingKind::Story],
            WorkflowDefinitionSource::UserAuthored,
            "plan",
            vec![ActivityDefinition {
                key: "plan".to_string(),
                description: "plan".to_string(),
                executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                    workflow_key: "wf_plan".to_string(),
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

    fn continue_root_definition(project_id: uuid::Uuid) -> ActivityLifecycleDefinition {
        continue_root_definition_with_activities(project_id, &["plan"])
    }

    fn human_approval_definition(project_id: uuid::Uuid) -> ActivityLifecycleDefinition {
        ActivityLifecycleDefinition::new(
            project_id,
            "approval_flow",
            "Approval flow",
            "",
            vec![WorkflowBindingKind::Story],
            WorkflowDefinitionSource::UserAuthored,
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
    ) -> ActivityLifecycleDefinition {
        ActivityLifecycleDefinition::new(
            project_id,
            "function_flow",
            "Function flow",
            "",
            vec![WorkflowBindingKind::Story],
            WorkflowDefinitionSource::UserAuthored,
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
    ) -> ActivityLifecycleDefinition {
        ActivityLifecycleDefinition::new(
            project_id,
            "agent_flow",
            "Agent flow",
            "",
            vec![WorkflowBindingKind::Story],
            WorkflowDefinitionSource::UserAuthored,
            "plan",
            activity_keys
                .iter()
                .map(|key| ActivityDefinition {
                    key: (*key).to_string(),
                    description: (*key).to_string(),
                    executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                        workflow_key: format!("wf_{key}"),
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
    async fn spawn_child_creates_binding_and_launches_prompt() {
        let project_id = uuid::Uuid::new_v4();
        let root_owner_id = uuid::Uuid::new_v4();
        let root_binding = SessionBinding::new(
            project_id,
            "root-session".to_string(),
            SessionOwnerType::Story,
            root_owner_id,
            "execution",
        );
        let port = FakePort {
            bindings: Mutex::new(vec![root_binding]),
            ..Default::default()
        };
        let launcher = AgentActivityExecutorLauncher::new(
            AgentActivityLaunchContext {
                project_id,
                lifecycle_key: "agent_flow".to_string(),
                root_session_id: "root-session".to_string(),
            },
            port,
        );
        let definition = definition(project_id);
        let run_id = uuid::Uuid::new_v4();
        let claim = ActivityExecutionClaim {
            run_id,
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
            ExecutorRunRef::AgentSession {
                session_id: "child-1".to_string()
            }
        );
        assert!(start_result.immediate_events.is_empty());
        let bindings = launcher.port.bindings.lock().unwrap();
        let activity_binding = bindings
            .iter()
            .find(|binding| binding.label == format!("lifecycle_activity:{run_id}:plan#1"))
            .expect("activity binding");
        assert_eq!(activity_binding.owner_type, SessionOwnerType::Story);
        assert_eq!(activity_binding.owner_id, root_owner_id);
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
                root_session_id: "root-session".to_string(),
            },
            port,
        );
        let definition = definition(project_id);
        let claim = ActivityExecutionClaim::new(uuid::Uuid::new_v4(), "plan", 1, "agent");

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
                root_session_id: "root-session".to_string(),
            },
            port,
        );
        let definition = continue_root_definition(project_id);
        let claim = ActivityExecutionClaim::new(uuid::Uuid::new_v4(), "plan", 1, "agent");

        let start_result = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        assert_eq!(
            start_result.executor_run,
            ExecutorRunRef::AgentSession {
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
                root_session_id: "root-session".to_string(),
            },
            port,
        );
        let definition = continue_root_definition_with_activities(project_id, &["plan", "review"]);
        let mut state = state();
        state.attempts.push(ActivityAttemptState {
            activity_key: "review".to_string(),
            attempt: 1,
            status: ActivityAttemptStatus::Running,
            executor_run: Some(ExecutorRunRef::AgentSession {
                session_id: "root-session".to_string(),
            }),
            started_at: Some(chrono::Utc::now()),
            completed_at: None,
            summary: None,
        });
        let claim = ActivityExecutionClaim::new(uuid::Uuid::new_v4(), "plan", 1, "agent");

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
                root_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = human_approval_definition(project_id);
        let claim = ActivityExecutionClaim::new(uuid::Uuid::new_v4(), "approval", 1, "human");

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
                root_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = function_definition(project_id, bash_spec("echo hello"));
        let claim = ActivityExecutionClaim::new(uuid::Uuid::new_v4(), "collect", 1, "function");

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
                root_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = function_definition(project_id, bash_spec("exit 7"));
        let claim = ActivityExecutionClaim::new(uuid::Uuid::new_v4(), "collect", 1, "function");

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
                root_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = function_definition(project_id, api_spec(url));
        let claim = ActivityExecutionClaim::new(uuid::Uuid::new_v4(), "collect", 1, "function");

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
                root_session_id: "root-session".to_string(),
            },
            FakePort::default(),
        );
        let definition = function_definition(project_id, api_spec(url));
        let claim = ActivityExecutionClaim::new(uuid::Uuid::new_v4(), "collect", 1, "function");

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
