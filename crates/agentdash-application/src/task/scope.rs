use std::sync::Arc;

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository;
use agentdash_domain::workflow::LifecycleAgentRepository;
use agentdash_spi::ExecutionContext;
use uuid::Uuid;

use crate::repository_set::RepositorySet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPlanScope {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunTaskScopeInput {
    pub runtime_session_id: Option<String>,
}

impl AgentRunTaskScopeInput {
    pub fn from_execution_context(context: &ExecutionContext) -> Self {
        Self {
            runtime_session_id: context
                .turn
                .hook_runtime
                .as_ref()
                .map(|runtime| runtime.session_id().to_string()),
        }
    }
}

#[derive(Clone)]
pub struct AgentRunTaskScopeResolver {
    runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
}

impl AgentRunTaskScopeResolver {
    pub fn new(
        runtime_binding_repo: Arc<dyn AgentRunRuntimeBindingRepository>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    ) -> Self {
        Self {
            runtime_binding_repo,
            lifecycle_agent_repo,
        }
    }

    pub fn from_repos(repos: &RepositorySet) -> Self {
        Self::new(
            repos.agent_run_runtime_binding_repo.clone(),
            repos.lifecycle_agent_repo.clone(),
        )
    }

    pub async fn resolve(
        &self,
        input: &AgentRunTaskScopeInput,
    ) -> Result<TaskPlanScope, AgentRunTaskScopeResolutionError> {
        let session_id = input
            .runtime_session_id
            .clone()
            .ok_or(AgentRunTaskScopeResolutionError::MissingRuntimeSession)?;
        let thread_id = RuntimeThreadId::new(session_id.clone()).map_err(|error| {
            AgentRunTaskScopeResolutionError::BindingLookup {
                session_id: session_id.clone(),
                message: error.to_string(),
            }
        })?;
        let binding = self
            .runtime_binding_repo
            .load_by_thread_id(&thread_id)
            .await
            .map_err(|error| AgentRunTaskScopeResolutionError::BindingLookup {
                session_id: session_id.clone(),
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunTaskScopeResolutionError::BindingMissing {
                session_id: session_id.clone(),
            })?;
        let agent = self
            .lifecycle_agent_repo
            .get(binding.target.agent_id)
            .await
            .map_err(|error| AgentRunTaskScopeResolutionError::AgentLookup {
                agent_id: binding.target.agent_id,
                message: error.to_string(),
            })?
            .ok_or(AgentRunTaskScopeResolutionError::AgentMissing {
                agent_id: binding.target.agent_id,
            })?;
        if agent.run_id != binding.target.run_id {
            return Err(AgentRunTaskScopeResolutionError::RunMismatch {
                binding_run_id: binding.target.run_id,
                agent_run_id: agent.run_id,
            });
        }
        Ok(TaskPlanScope {
            project_id: agent.project_id,
            run_id: agent.run_id,
            agent_id: Some(agent.id),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRunTaskScopeResolutionError {
    #[error("当前 session 缺少 hook runtime，无法定位 Task scope")]
    MissingRuntimeSession,
    #[error("查询 runtime thread `{session_id}` 的 AgentRun binding 失败: {message}")]
    BindingLookup { session_id: String, message: String },
    #[error("runtime thread `{session_id}` 缺少 AgentRun binding，无法定位 Task scope")]
    BindingMissing { session_id: String },
    #[error("查询 LifecycleAgent `{agent_id}` 失败: {message}")]
    AgentLookup { agent_id: Uuid, message: String },
    #[error("LifecycleAgent `{agent_id}` 不存在，无法定位 Task scope")]
    AgentMissing { agent_id: Uuid },
    #[error(
        "Runtime binding run_id `{binding_run_id}` 与 LifecycleAgent run_id `{agent_run_id}` 不一致"
    )]
    RunMismatch {
        binding_run_id: Uuid,
        agent_run_id: Uuid,
    },
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::str::FromStr;

    use super::*;
    use agentdash_agent_runtime_contract::*;
    use agentdash_application_ports::agent_run_runtime::{
        AgentRunRuntimeBinding, AgentRunRuntimeBindingError, AgentRunRuntimeTarget,
    };
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{AgentSource, LifecycleAgent};
    use async_trait::async_trait;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct FixtureBindingRepo {
        bindings: Mutex<Vec<AgentRunRuntimeBinding>>,
    }

    #[async_trait]
    impl AgentRunRuntimeBindingRepository for FixtureBindingRepo {
        async fn load(
            &self,
            target: &AgentRunRuntimeTarget,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(self
                .bindings
                .lock()
                .await
                .iter()
                .find(|binding| &binding.target == target)
                .cloned())
        }

        async fn load_by_thread_id(
            &self,
            thread_id: &RuntimeThreadId,
        ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(self
                .bindings
                .lock()
                .await
                .iter()
                .find(|binding| &binding.thread_id == thread_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(self
                .bindings
                .lock()
                .await
                .iter()
                .filter(|binding| binding.target.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
            Ok(self
                .bindings
                .lock()
                .await
                .iter()
                .filter(|binding| binding.target.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn insert(
            &self,
            binding: AgentRunRuntimeBinding,
        ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
            self.bindings.lock().await.push(binding.clone());
            Ok(binding)
        }
    }

    fn runtime_id<T: FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("valid runtime id")
    }

    fn binding(run_id: Uuid, agent_id: Uuid) -> AgentRunRuntimeBinding {
        AgentRunRuntimeBinding {
            target: AgentRunRuntimeTarget { run_id, agent_id },
            presentation_thread_id: runtime_id("presentation-task-scope"),
            thread_id: runtime_id("session-1"),
            binding_id: runtime_id("binding-task-scope"),
            binding_epoch: agentdash_agent_runtime_contract::BindingEpoch(1),
            driver_generation: RuntimeDriverGeneration(1),
            source_thread_id: runtime_id("source-task-scope"),
            profile_digest: runtime_id("profile-task-scope"),
            profile_provenance: ProfileProvenance {
                service_digest: runtime_id("service-task-scope"),
                transport_digest: runtime_id("transport-task-scope"),
                host_policy_digest: runtime_id("policy-task-scope"),
            },
            bound_profile: RuntimeProfile {
                reference_class: ReferenceRuntimeClass::ManagedThread,
                input: InputProfile {
                    modalities: BTreeSet::new(),
                },
                instruction: InstructionProfile {
                    channels: BTreeSet::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                },
                tools: ToolProfile {
                    channels: BTreeSet::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                    cancellation: true,
                },
                workspace: WorkspaceProfile {
                    capabilities: BTreeSet::new(),
                    mechanism: DeliveryMechanism::Native,
                },
                interactions: InteractionProfile {
                    kinds: BTreeSet::new(),
                    durable_correlation: true,
                },
                lifecycle: BTreeSet::new(),
                hooks: HookProfile {
                    points: Vec::new(),
                    configuration_boundary: ConfigurationBoundary::Binding,
                },
                context: ContextProfile {
                    capabilities: BTreeSet::new(),
                    fidelity: ContextFidelity::Opaque,
                    activation_idempotent: false,
                },
                telemetry_config: BTreeSet::new(),
            },
            surface: agentdash_agent_runtime_contract::RuntimeSurfaceDescriptor {
                source_frame_id: "frame-task-scope".to_string(),
                surface_revision: agentdash_agent_runtime_contract::SurfaceRevision(1),
                surface_digest: runtime_id("surface-task-scope"),
                vfs_digest: "vfs-task-scope".to_string(),
                context_recipe_revision: agentdash_agent_runtime_contract::ContextRecipeRevision(1),
                context_digest: runtime_id("context-task-scope"),
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision: ToolSetRevision(0),
                tool_set_digest: "tools-task-scope".to_string(),
                hook_plan: BoundRuntimeHookPlan {
                    revision: HookPlanRevision(1),
                    digest: runtime_id("hook-task-scope"),
                    entries: Vec::new(),
                },
                terminal_hook_effect_binding: None,
            },
            settings_revision: ThreadSettingsRevision(0),
            context_delivery_target:
                agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget {
                    connector_id: "pi-agent".to_string(),
                    executor: "PI_AGENT".to_string(),
                },
        }
    }

    #[derive(Default)]
    struct FixtureAgentRepo {
        agents: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
    impl LifecycleAgentRepository for FixtureAgentRepo {
        async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            self.agents.lock().await.push(agent.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .await
                .iter()
                .find(|agent| agent.id == id)
                .cloned())
        }

        async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .await
                .iter()
                .filter(|agent| agent.run_id == run_id)
                .cloned()
                .collect())
        }

        async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
            let mut agents = self.agents.lock().await;
            if let Some(existing) = agents.iter_mut().find(|existing| existing.id == agent.id) {
                *existing = agent.clone();
                Ok(())
            } else {
                Err(DomainError::NotFound {
                    entity: "LifecycleAgent",
                    id: agent.id.to_string(),
                })
            }
        }
    }

    #[tokio::test]
    async fn resolver_maps_runtime_session_anchor_to_task_plan_scope() {
        let binding_repo = Arc::new(FixtureBindingRepo::default());
        let agent_repo = Arc::new(FixtureAgentRepo::default());
        let resolver = AgentRunTaskScopeResolver::new(binding_repo.clone(), agent_repo.clone());
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        let agent_id = agent.id;
        agent_repo.create(&agent).await.expect("seed agent");
        binding_repo
            .insert(binding(run_id, agent_id))
            .await
            .expect("seed binding");

        let scope = resolver
            .resolve(&AgentRunTaskScopeInput {
                runtime_session_id: Some("session-1".to_string()),
            })
            .await
            .expect("scope");

        assert_eq!(
            scope,
            TaskPlanScope {
                project_id,
                run_id,
                agent_id: Some(agent_id),
            }
        );
    }
}
