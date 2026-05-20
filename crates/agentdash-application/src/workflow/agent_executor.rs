use agentdash_domain::session_binding::{SessionBinding, SessionOwnerCtx, SessionOwnerType};
use agentdash_domain::workflow::{
    ActivityAttemptStatus, ActivityDefinition, ActivityExecutionClaim, ActivityExecutorSpec,
    ActivityLifecycleDefinition, AgentSessionPolicy, ExecutorRunRef, HumanActivityExecutorSpec,
    LifecycleNodeType, LifecycleStepDefinition,
};
use agentdash_spi::AgentConfig;

use super::ActivityLifecycleRunState;
use super::scheduler::{ActivityExecutorLauncher, ActivityExecutorStartError};
use super::session_association::{LIFECYCLE_ACTIVITY_LABEL_PREFIX, build_lifecycle_activity_label};
use crate::platform_config::SharedPlatformConfig;
use crate::repository_set::RepositorySet;
use crate::session::hub::PendingRuntimeContextTransitionInput;
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
                    capability_keys: activation.capability_keys,
                    source_turn_id: None,
                    created_at: chrono::Utc::now().timestamp_millis(),
                })
                .await
        }
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
    ) -> Result<ExecutorRunRef, ActivityExecutorStartError> {
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
            ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(_spec)) => {
                Ok(ExecutorRunRef::HumanDecision {
                    decision_id: human_decision_id(claim),
                })
            }
            ActivityExecutorSpec::Function(_) => {
                Err(ActivityExecutorStartError::terminal(format!(
                    "activity `{}` 的 Function executor 尚未接入 Activity executor",
                    activity.key
                )))
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
    ) -> Result<ExecutorRunRef, ActivityExecutorStartError> {
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

        Ok(ExecutorRunRef::AgentSession { session_id })
    }

    async fn start_continue_root(
        &self,
        definition: &ActivityLifecycleDefinition,
        activity: &ActivityDefinition,
        workflow_key: &str,
        state: &ActivityLifecycleRunState,
        claim: &ActivityExecutionClaim,
    ) -> Result<ExecutorRunRef, ActivityExecutorStartError> {
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
        Ok(ExecutorRunRef::AgentSession {
            session_id: self.context.root_session_id.clone(),
        })
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
        HumanActivityExecutorSpec, HumanApprovalExecutorSpec, OutputPortDefinition,
        TransitionCondition, WorkflowBindingKind, WorkflowDefinitionSource,
    };

    use super::*;
    use crate::workflow::{ActivityLifecycleRunState, ActivityRunStatus};

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

        let executor_ref = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        assert_eq!(
            executor_ref,
            ExecutorRunRef::AgentSession {
                session_id: "child-1".to_string()
            }
        );
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

        let executor_ref = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        assert_eq!(
            executor_ref,
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

        let executor_ref = launcher
            .start(&definition, &state(), &claim)
            .await
            .expect("start");

        assert_eq!(
            executor_ref,
            ExecutorRunRef::HumanDecision {
                decision_id: format!("{}:approval#1", claim.run_id)
            }
        );
    }
}
