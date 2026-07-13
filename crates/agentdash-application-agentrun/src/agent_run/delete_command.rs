use agentdash_application_ports::agent_run_delete::{AgentRunDeleteStore, DeleteAgentRunError};

use crate::WorkflowApplicationError;

pub struct AgentRunDeleteCommandService<'a> {
    store: &'a dyn AgentRunDeleteStore,
}

impl<'a> AgentRunDeleteCommandService<'a> {
    pub fn new(store: &'a dyn AgentRunDeleteStore) -> Self {
        Self { store }
    }

    pub async fn delete(
        &self,
        command: AgentRunDeleteCommand,
    ) -> Result<AgentRunDeleteOutcome, WorkflowApplicationError> {
        self.store
            .delete(command)
            .await
            .map_err(|error| match error {
                DeleteAgentRunError::NotFound { .. } => {
                    WorkflowApplicationError::NotFound(error.to_string())
                }
                DeleteAgentRunError::RuntimeActive { .. } => {
                    WorkflowApplicationError::Conflict(error.to_string())
                }
                DeleteAgentRunError::Persistence(_) => {
                    WorkflowApplicationError::Internal(error.to_string())
                }
            })
    }
}

pub use agentdash_application_ports::agent_run_delete::{
    DeleteAgentRunCommand as AgentRunDeleteCommand, DeleteAgentRunOutcome as AgentRunDeleteOutcome,
};

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use uuid::Uuid;

    use super::*;

    struct FixtureStore(Result<AgentRunDeleteOutcome, DeleteAgentRunError>);

    #[async_trait]
    impl AgentRunDeleteStore for FixtureStore {
        async fn delete(
            &self,
            _command: AgentRunDeleteCommand,
        ) -> Result<AgentRunDeleteOutcome, DeleteAgentRunError> {
            self.0.clone()
        }
    }

    #[tokio::test]
    async fn active_runtime_maps_to_command_conflict() {
        let run_id = Uuid::new_v4();
        let store = FixtureStore(Err(DeleteAgentRunError::RuntimeActive { run_id }));
        let service = AgentRunDeleteCommandService::new(&store);
        let error = service
            .delete(AgentRunDeleteCommand {
                project_id: Uuid::new_v4(),
                run_id,
            })
            .await
            .expect_err("active runtime must be rejected");
        assert!(matches!(error, WorkflowApplicationError::Conflict(_)));
    }
}
