use agentdash_domain::session_binding::{SessionBinding, SessionOwnerType};
use agentdash_domain::workflow::{
    ActivityExecutionClaim, ActivityExecutorSpec, ActivityLifecycleDefinition, AgentSessionPolicy,
    ExecutorRunRef,
};
use agentdash_spi::AgentConfig;

use super::ActivityLifecycleRunState;
use super::scheduler::{ActivityExecutorLauncher, ActivityExecutorStartError};
use super::session_association::{LIFECYCLE_ACTIVITY_LABEL_PREFIX, build_lifecycle_activity_label};
use crate::repository_set::RepositorySet;
use crate::session::{LaunchCommand, SessionCoreService, SessionLaunchService, UserPromptInput};

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
}

#[derive(Clone)]
pub struct AgentActivityRuntimePort {
    session_core: SessionCoreService,
    session_launch: SessionLaunchService,
    repos: RepositorySet,
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
            repos,
        }
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
}

#[async_trait::async_trait]
impl<P> ActivityExecutorLauncher for AgentActivityExecutorLauncher<P>
where
    P: AgentActivitySessionPort,
{
    async fn start(
        &self,
        definition: &ActivityLifecycleDefinition,
        _state: &ActivityLifecycleRunState,
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
        let ActivityExecutorSpec::Agent(spec) = &activity.executor else {
            return Err(ActivityExecutorStartError::terminal(format!(
                "activity `{}` 不是 Agent executor",
                activity.key
            )));
        };
        match spec.session_policy {
            AgentSessionPolicy::SpawnChild => self.start_spawn_child(definition, claim).await,
            AgentSessionPolicy::ContinueRoot | AgentSessionPolicy::AttachExisting => {
                Err(ActivityExecutorStartError::terminal(format!(
                    "Agent session policy `{}` 尚未接入 Activity executor",
                    serde_json::to_string(&spec.session_policy)
                        .unwrap_or_else(|_| "unknown".to_string())
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
                build_lifecycle_activity_label(&claim.activity_key, claim.attempt),
            )
        } else {
            SessionBinding::new(
                self.context.project_id,
                session_id.clone(),
                SessionOwnerType::Project,
                self.context.project_id,
                build_lifecycle_activity_label(&claim.activity_key, claim.attempt),
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
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::session_binding::SessionOwnerType;
    use agentdash_domain::workflow::{
        ActivityAttemptState, ActivityAttemptStatus, ActivityCompletionPolicy, ActivityDefinition,
        ActivityExecutionClaim, ActivityExecutionClaimStatus, ActivityExecutorSpec,
        AgentActivityExecutorSpec, AgentSessionPolicy, OutputPortDefinition, WorkflowBindingKind,
        WorkflowDefinitionSource,
    };

    use super::*;
    use crate::workflow::{ActivityLifecycleRunState, ActivityRunStatus};

    #[derive(Default)]
    struct FakePort {
        sessions: Mutex<Vec<String>>,
        bindings: Mutex<Vec<SessionBinding>>,
        launch_error: Mutex<Option<String>>,
        launches: Mutex<Vec<String>>,
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
        let claim = ActivityExecutionClaim {
            run_id: uuid::Uuid::new_v4(),
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
            .find(|binding| binding.label == "lifecycle_activity:plan#1")
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
}
