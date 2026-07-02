use std::collections::BTreeSet;

use agentdash_domain::workflow::{
    LifecycleAgentRepository, LifecycleRunRepository, LifecycleRunStatus,
    RuntimeSessionExecutionAnchorRepository,
};
use uuid::Uuid;

use crate::{AgentRunRepositorySet, WorkflowApplicationError};

use super::{SessionCoreService, SessionExecutionState};

pub struct AgentRunDeleteCommand {
    pub project_id: Uuid,
    pub run_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunDeleteOutcome {
    pub deleted: bool,
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub deleted_runtime_session_ids: Vec<String>,
}

#[derive(Clone, Copy)]
pub struct AgentRunDeleteRepos<'a> {
    pub lifecycle_runs: &'a dyn LifecycleRunRepository,
    pub lifecycle_agents: &'a dyn LifecycleAgentRepository,
    pub execution_anchors: &'a dyn RuntimeSessionExecutionAnchorRepository,
}

impl<'a> AgentRunDeleteRepos<'a> {
    pub fn from_repository_set(repos: &'a AgentRunRepositorySet) -> Self {
        Self {
            lifecycle_runs: repos.lifecycle_run_repo.as_ref(),
            lifecycle_agents: repos.lifecycle_agent_repo.as_ref(),
            execution_anchors: repos.execution_anchor_repo.as_ref(),
        }
    }
}

pub struct AgentRunDeleteCommandService<'a> {
    repos: AgentRunDeleteRepos<'a>,
    session_core: SessionCoreService,
}

impl<'a> AgentRunDeleteCommandService<'a> {
    pub fn new(repos: AgentRunDeleteRepos<'a>, session_core: SessionCoreService) -> Self {
        Self {
            repos,
            session_core,
        }
    }

    pub async fn delete(
        &self,
        command: AgentRunDeleteCommand,
    ) -> Result<AgentRunDeleteOutcome, WorkflowApplicationError> {
        let run = self
            .repos
            .lifecycle_runs
            .get_by_id(command.run_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("AgentRun 不存在: {}", command.run_id))
            })?;

        if run.project_id != command.project_id {
            return Err(WorkflowApplicationError::NotFound(format!(
                "AgentRun 不存在或不属于当前 Project: {}",
                command.run_id
            )));
        }

        if run.status == LifecycleRunStatus::Running {
            return Err(WorkflowApplicationError::Conflict(format!(
                "AgentRun {} 正在运行，不能删除",
                run.id
            )));
        }

        let agents = self.repos.lifecycle_agents.list_by_run(run.id).await?;
        let anchors = self.repos.execution_anchors.list_by_run(run.id).await?;
        let mut runtime_session_ids = BTreeSet::new();
        for anchor in anchors {
            runtime_session_ids.insert(anchor.runtime_session_id);
        }
        for agent in agents {
            if let Some(delivery) = agent.current_delivery {
                runtime_session_ids.insert(delivery.runtime_session_id);
            }
        }

        let runtime_session_ids = runtime_session_ids.into_iter().collect::<Vec<_>>();
        self.ensure_runtime_sessions_not_active(&runtime_session_ids, run.id)
            .await?;

        let mut deleted_runtime_session_ids = Vec::new();
        for session_id in &runtime_session_ids {
            match self.session_core.delete_session(session_id).await {
                Ok(()) => deleted_runtime_session_ids.push(session_id.clone()),
                Err(error) if is_session_not_found(&error) => {}
                Err(error) => return Err(error),
            }
        }

        self.repos.lifecycle_runs.delete(run.id).await?;

        Ok(AgentRunDeleteOutcome {
            deleted: true,
            project_id: command.project_id,
            run_id: command.run_id,
            deleted_runtime_session_ids,
        })
    }

    async fn ensure_runtime_sessions_not_active(
        &self,
        runtime_session_ids: &[String],
        run_id: Uuid,
    ) -> Result<(), WorkflowApplicationError> {
        for session_id in runtime_session_ids {
            match self
                .session_core
                .inspect_session_execution_state(session_id)
                .await
            {
                Ok(SessionExecutionState::Running { .. }) => {
                    return Err(WorkflowApplicationError::Conflict(format!(
                        "AgentRun {run_id} 的 RuntimeSession {session_id} 正在运行，不能删除"
                    )));
                }
                Ok(SessionExecutionState::Cancelling { .. }) => {
                    return Err(WorkflowApplicationError::Conflict(format!(
                        "AgentRun {run_id} 的 RuntimeSession {session_id} 正在取消，不能删除"
                    )));
                }
                Ok(_) => {}
                Err(error) if is_session_not_found(&error) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

fn is_session_not_found(error: &WorkflowApplicationError) -> bool {
    matches!(error, WorkflowApplicationError::NotFound(_))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentSource, LifecycleAgent, LifecycleRun, RuntimeSessionExecutionAnchor,
    };
    use async_trait::async_trait;
    use tokio::sync::Mutex;

    use super::*;
    use crate::agent_run::{RuntimeSessionCorePort, SessionMeta};

    #[derive(Default)]
    struct MemoryRunRepo {
        runs: Mutex<Vec<LifecycleRun>>,
        deleted: Mutex<Vec<Uuid>>,
    }

    #[async_trait]
    impl LifecycleRunRepository for MemoryRunRepo {
        async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            self.runs.lock().await.push(run.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .find(|run| run.id == id)
                .cloned())
        }

        async fn list_by_ids(&self, ids: &[Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .filter(|run| ids.contains(&run.id))
                .cloned()
                .collect())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleRun>, DomainError> {
            Ok(self
                .runs
                .lock()
                .await
                .iter()
                .filter(|run| run.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
            let mut runs = self.runs.lock().await;
            if let Some(existing) = runs.iter_mut().find(|item| item.id == run.id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.deleted.lock().await.push(id);
            self.runs.lock().await.retain(|run| run.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryAgentRepo {
        agents: Mutex<Vec<LifecycleAgent>>,
    }

    #[async_trait]
    impl LifecycleAgentRepository for MemoryAgentRepo {
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
            if let Some(existing) = agents.iter_mut().find(|item| item.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryAnchorRepo {
        anchors: Mutex<Vec<RuntimeSessionExecutionAnchor>>,
    }

    #[async_trait]
    impl RuntimeSessionExecutionAnchorRepository for MemoryAnchorRepo {
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
    struct TestCorePort {
        states: Mutex<HashMap<String, SessionExecutionState>>,
        deleted: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl RuntimeSessionCorePort for TestCorePort {
        async fn inspect_session_execution_state(
            &self,
            session_id: &str,
        ) -> Result<SessionExecutionState, WorkflowApplicationError> {
            self.states
                .lock()
                .await
                .get(session_id)
                .cloned()
                .ok_or_else(|| {
                    WorkflowApplicationError::NotFound(format!("session {session_id} 不存在"))
                })
        }

        async fn get_session_meta(
            &self,
            _session_id: &str,
        ) -> Result<Option<SessionMeta>, WorkflowApplicationError> {
            Ok(None)
        }

        async fn delete_session(&self, session_id: &str) -> Result<(), WorkflowApplicationError> {
            if self.states.lock().await.contains_key(session_id) {
                self.deleted.lock().await.push(session_id.to_string());
                Ok(())
            } else {
                Err(WorkflowApplicationError::NotFound(format!(
                    "session {session_id} 不存在"
                )))
            }
        }
    }

    struct Fixture {
        runs: MemoryRunRepo,
        agents: MemoryAgentRepo,
        anchors: MemoryAnchorRepo,
        core: Arc<TestCorePort>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                runs: MemoryRunRepo::default(),
                agents: MemoryAgentRepo::default(),
                anchors: MemoryAnchorRepo::default(),
                core: Arc::new(TestCorePort::default()),
            }
        }

        fn service(&self) -> AgentRunDeleteCommandService<'_> {
            AgentRunDeleteCommandService::new(
                AgentRunDeleteRepos {
                    lifecycle_runs: &self.runs,
                    lifecycle_agents: &self.agents,
                    execution_anchors: &self.anchors,
                },
                SessionCoreService::new(self.core.clone()),
            )
        }
    }

    async fn seed_run(fixture: &Fixture) -> (LifecycleRun, LifecycleAgent) {
        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        fixture.runs.create(&run).await.expect("run");
        fixture.agents.create(&agent).await.expect("agent");
        (run, agent)
    }

    async fn seed_anchor(
        fixture: &Fixture,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        session_id: &str,
        state: SessionExecutionState,
    ) {
        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            session_id,
            run.id,
            Uuid::new_v4(),
            agent.id,
        );
        fixture.anchors.upsert(&anchor).await.expect("anchor");
        fixture
            .core
            .states
            .lock()
            .await
            .insert(session_id.to_string(), state);
    }

    #[tokio::test]
    async fn deletes_terminal_sessions_then_lifecycle_run() {
        let fixture = Fixture::new();
        let (run, agent) = seed_run(&fixture).await;
        seed_anchor(
            &fixture,
            &run,
            &agent,
            "sess-a",
            SessionExecutionState::Completed {
                turn_id: "turn-a".to_string(),
            },
        )
        .await;
        seed_anchor(
            &fixture,
            &run,
            &agent,
            "sess-b",
            SessionExecutionState::Idle,
        )
        .await;

        let outcome = fixture
            .service()
            .delete(AgentRunDeleteCommand {
                project_id: run.project_id,
                run_id: run.id,
            })
            .await
            .expect("delete");

        assert!(outcome.deleted);
        assert_eq!(
            outcome.deleted_runtime_session_ids,
            vec!["sess-a", "sess-b"]
        );
        assert!(fixture.runs.get_by_id(run.id).await.expect("get").is_none());
    }

    #[tokio::test]
    async fn rejects_cross_project_without_side_effects() {
        let fixture = Fixture::new();
        let (run, agent) = seed_run(&fixture).await;
        seed_anchor(
            &fixture,
            &run,
            &agent,
            "sess-a",
            SessionExecutionState::Idle,
        )
        .await;

        let error = fixture
            .service()
            .delete(AgentRunDeleteCommand {
                project_id: Uuid::new_v4(),
                run_id: run.id,
            })
            .await
            .expect_err("cross project");

        assert!(matches!(error, WorkflowApplicationError::NotFound(_)));
        assert!(fixture.core.deleted.lock().await.is_empty());
        assert!(fixture.runs.get_by_id(run.id).await.expect("get").is_some());
    }

    #[tokio::test]
    async fn rejects_running_session_before_any_delete() {
        let fixture = Fixture::new();
        let (run, agent) = seed_run(&fixture).await;
        seed_anchor(
            &fixture,
            &run,
            &agent,
            "sess-running",
            SessionExecutionState::Running {
                turn_id: Some("turn-1".to_string()),
            },
        )
        .await;

        let error = fixture
            .service()
            .delete(AgentRunDeleteCommand {
                project_id: run.project_id,
                run_id: run.id,
            })
            .await
            .expect_err("running rejected");

        assert!(matches!(error, WorkflowApplicationError::Conflict(_)));
        assert!(fixture.core.deleted.lock().await.is_empty());
        assert_eq!(fixture.runs.deleted.lock().await.len(), 0);
    }

    #[tokio::test]
    async fn rejects_cancelling_session_before_any_delete() {
        let fixture = Fixture::new();
        let (run, agent) = seed_run(&fixture).await;
        seed_anchor(
            &fixture,
            &run,
            &agent,
            "sess-cancelling",
            SessionExecutionState::Cancelling {
                turn_id: Some("turn-1".to_string()),
            },
        )
        .await;

        let error = fixture
            .service()
            .delete(AgentRunDeleteCommand {
                project_id: run.project_id,
                run_id: run.id,
            })
            .await
            .expect_err("cancelling rejected");

        assert!(matches!(error, WorkflowApplicationError::Conflict(_)));
        assert!(fixture.core.deleted.lock().await.is_empty());
        assert_eq!(fixture.runs.deleted.lock().await.len(), 0);
    }
}
