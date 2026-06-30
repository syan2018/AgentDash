use std::sync::Arc;

use agentdash_domain::workflow::{
    LifecycleAgentRepository, RuntimeSessionExecutionAnchorRepository,
};
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
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
}

impl AgentRunTaskScopeResolver {
    pub fn new(
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    ) -> Self {
        Self {
            execution_anchor_repo,
            lifecycle_agent_repo,
        }
    }

    pub fn from_repos(repos: &RepositorySet) -> Self {
        Self::new(
            repos.execution_anchor_repo.clone(),
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
        let anchor = self
            .execution_anchor_repo
            .find_by_session(&session_id)
            .await
            .map_err(|error| AgentRunTaskScopeResolutionError::AnchorLookup {
                session_id: session_id.clone(),
                message: error.to_string(),
            })?
            .ok_or_else(|| AgentRunTaskScopeResolutionError::AnchorMissing {
                session_id: session_id.clone(),
            })?;
        let agent = self
            .lifecycle_agent_repo
            .get(anchor.agent_id)
            .await
            .map_err(|error| AgentRunTaskScopeResolutionError::AgentLookup {
                agent_id: anchor.agent_id,
                message: error.to_string(),
            })?
            .ok_or(AgentRunTaskScopeResolutionError::AgentMissing {
                agent_id: anchor.agent_id,
            })?;
        if agent.run_id != anchor.run_id {
            return Err(AgentRunTaskScopeResolutionError::RunMismatch {
                anchor_run_id: anchor.run_id,
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
    #[error("查询 runtime session `{session_id}` 的执行锚点失败: {message}")]
    AnchorLookup { session_id: String, message: String },
    #[error("runtime session `{session_id}` 缺少执行锚点，无法定位 Task scope")]
    AnchorMissing { session_id: String },
    #[error("查询 LifecycleAgent `{agent_id}` 失败: {message}")]
    AgentLookup { agent_id: Uuid, message: String },
    #[error("LifecycleAgent `{agent_id}` 不存在，无法定位 Task scope")]
    AgentMissing { agent_id: Uuid },
    #[error("执行锚点 run_id `{anchor_run_id}` 与 LifecycleAgent run_id `{agent_run_id}` 不一致")]
    RunMismatch {
        anchor_run_id: Uuid,
        agent_run_id: Uuid,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{AgentSource, LifecycleAgent, RuntimeSessionExecutionAnchor};
    use async_trait::async_trait;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct InMemoryAnchorRepo {
        anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for InMemoryAnchorRepo {
        async fn upsert(&self, anchor: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
            self.anchors.lock().await.push(anchor.clone());
            Ok(())
        }

        async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
            self.anchors
                .lock()
                .await
                .retain(|anchor| anchor.runtime_session_id != runtime_session_id);
            Ok(())
        }

        async fn find_by_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .find(|anchor| anchor.runtime_session_id == runtime_session_id)
                .cloned())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
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
                .anchors
                .lock()
                .await
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
                .anchors
                .lock()
                .await
                .iter()
                .filter(|anchor| runtime_session_ids.contains(&anchor.runtime_session_id))
                .cloned()
                .collect())
        }

        async fn latest_updated_anchor_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
            Ok(self
                .anchors
                .lock()
                .await
                .iter()
                .filter(|anchor| anchor.agent_id == agent_id)
                .max_by_key(|anchor| anchor.updated_at)
                .cloned())
        }
    }

    #[derive(Default)]
    struct InMemoryAgentRepo {
        agents: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
    impl LifecycleAgentRepository for InMemoryAgentRepo {
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
        let anchor_repo = Arc::new(InMemoryAnchorRepo::default());
        let agent_repo = Arc::new(InMemoryAgentRepo::default());
        let resolver = AgentRunTaskScopeResolver::new(anchor_repo.clone(), agent_repo.clone());
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let frame_id = Uuid::new_v4();
        let agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        let agent_id = agent.id;
        agent_repo.create(&agent).await.expect("seed agent");
        anchor_repo
            .upsert(&RuntimeSessionExecutionAnchor::new_dispatch(
                "session-1",
                run_id,
                frame_id,
                agent_id,
            ))
            .await
            .expect("seed anchor");

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
