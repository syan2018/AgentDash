use agentdash_application_ports::agent_run_fork::{AgentRunForkGraph, AgentRunForkGraphStore};
use agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBinding;
use async_trait::async_trait;

use crate::{WorkflowApplicationError, agent_run::ForkAgentRunRuntime};

#[async_trait]
pub trait AgentRunForkRuntimePort: Send + Sync {
    async fn fork_runtime(
        &self,
        command: ForkAgentRunRuntime,
    ) -> Result<AgentRunRuntimeBinding, WorkflowApplicationError>;
}

pub struct AgentRunForkCommandService<'a> {
    graph_store: &'a dyn AgentRunForkGraphStore,
    runtime: &'a dyn AgentRunForkRuntimePort,
}

impl<'a> AgentRunForkCommandService<'a> {
    pub fn new(
        graph_store: &'a dyn AgentRunForkGraphStore,
        runtime: &'a dyn AgentRunForkRuntimePort,
    ) -> Self {
        Self {
            graph_store,
            runtime,
        }
    }

    pub async fn materialize(
        &self,
        graph: &AgentRunForkGraph,
        runtime_command: ForkAgentRunRuntime,
    ) -> Result<AgentRunRuntimeBinding, WorkflowApplicationError> {
        let result = async {
            self.graph_store
                .create_graph(graph)
                .await
                .map_err(WorkflowApplicationError::Internal)?;
            self.runtime.fork_runtime(runtime_command).await
        }
        .await;
        match result {
            Ok(binding) => Ok(binding),
            Err(error) => {
                let _ = self.graph_store.delete_graph(graph).await;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_application_ports::agent_run_runtime::{
        AgentRunRuntimeBinding, AgentRunRuntimeTarget,
    };
    use agentdash_domain::workflow::{
        AgentFrame, AgentRunLineage, AgentSource, LifecycleAgent, LifecycleRun,
    };
    use async_trait::async_trait;
    use uuid::Uuid;

    use super::*;

    struct FakeGraphStore {
        calls: Mutex<Vec<&'static str>>,
    }

    #[async_trait]
    impl AgentRunForkGraphStore for FakeGraphStore {
        async fn create_graph(&self, _graph: &AgentRunForkGraph) -> Result<(), String> {
            self.calls.lock().expect("calls").push("create");
            Ok(())
        }

        async fn delete_graph(&self, _graph: &AgentRunForkGraph) -> Result<(), String> {
            self.calls.lock().expect("calls").push("delete");
            Ok(())
        }
    }

    struct FailingRuntime;

    #[async_trait]
    impl AgentRunForkRuntimePort for FailingRuntime {
        async fn fork_runtime(
            &self,
            _command: ForkAgentRunRuntime,
        ) -> Result<AgentRunRuntimeBinding, WorkflowApplicationError> {
            Err(WorkflowApplicationError::Internal(
                "runtime fork failed".to_string(),
            ))
        }
    }

    #[tokio::test]
    async fn runtime_failure_compensates_the_complete_child_graph() {
        let graph_store = FakeGraphStore {
            calls: Mutex::new(Vec::new()),
        };
        let service = AgentRunForkCommandService::new(&graph_store, &FailingRuntime);
        let (graph, runtime_command) = fixture();

        service
            .materialize(&graph, runtime_command)
            .await
            .expect_err("runtime failure");

        assert_eq!(
            graph_store.calls.into_inner().expect("calls"),
            vec!["create", "delete"]
        );
    }

    fn fixture() -> (AgentRunForkGraph, ForkAgentRunRuntime) {
        let project_id = Uuid::new_v4();
        let parent_run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let child_run = LifecycleRun::new_plain_for_user(project_id, "user-1".to_string());
        let child_agent = LifecycleAgent::new_root_for_user(
            child_run.id,
            project_id,
            AgentSource::Unknown,
            "user-1".to_string(),
        );
        let child_frame = AgentFrame::new_initial(child_agent.id);
        let lineage = AgentRunLineage::new_fork(
            parent_run_id,
            parent_agent_id,
            child_run.id,
            child_agent.id,
            None,
            None,
            "user-1".to_string(),
            None,
        );
        let source_target = AgentRunRuntimeTarget {
            run_id: parent_run_id,
            agent_id: parent_agent_id,
        };
        let child_target = AgentRunRuntimeTarget {
            run_id: child_run.id,
            agent_id: child_agent.id,
        };
        (
            AgentRunForkGraph {
                child_run,
                child_agent,
                child_frame,
                lineage,
            },
            ForkAgentRunRuntime {
                source_target,
                child_target,
                child_presentation_thread_id:
                    agentdash_agent_runtime_contract::PresentationThreadId::new("child-thread")
                        .expect("thread id"),
                through_source_turn_id: None,
                identity: None,
                backend_selection: None,
            },
        )
    }
}
