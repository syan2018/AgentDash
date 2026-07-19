use std::sync::Arc;

use agentdash_agent_runtime_contract::{
    ManagedRuntimeLifecycleStatus, ManagedRuntimeOperationStatus,
};
use agentdash_domain::workflow::{LifecycleAgentRepository, LifecycleRunRepository};
use thiserror::Error;
use uuid::Uuid;

use super::{
    AgentRunProductCommand, AgentRunProductCommandFacade, AgentRunProductCommandRequest,
    AgentRunProductProjectionQueryPort,
};
use agentdash_domain::agent_run_target::AgentRunTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunProductDeleteRequest {
    pub project_id: Uuid,
    pub run_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunProductDeleteOutcome {
    pub deleted: bool,
    pub project_id: Uuid,
    pub run_id: Uuid,
}

#[derive(Debug, Error)]
pub enum AgentRunProductDeleteError {
    #[error("AgentRun does not belong to the requested Project")]
    ProjectMismatch,
    #[error("AgentRun delete repository failed: {0}")]
    Repository(String),
    #[error("AgentRun Runtime close failed: {0}")]
    Runtime(String),
    #[error("AgentRun Runtime close did not reach terminal Closed state")]
    RuntimeNotClosed,
}

/// Product aggregate deletion boundary.
///
/// Every Complete Agent is first closed through the durable Product command claim. The stable
/// command identity makes a partially completed delete replay the same Runtime effects after a
/// process restart. The LifecycleRun cascade is only executed after every bound Runtime snapshot
/// proves terminal `Closed`.
pub struct AgentRunProductDeleteService {
    runs: Arc<dyn LifecycleRunRepository>,
    agents: Arc<dyn LifecycleAgentRepository>,
    projection: Arc<dyn AgentRunProductProjectionQueryPort>,
    commands: Arc<AgentRunProductCommandFacade>,
}

impl AgentRunProductDeleteService {
    pub fn new(
        runs: Arc<dyn LifecycleRunRepository>,
        agents: Arc<dyn LifecycleAgentRepository>,
        projection: Arc<dyn AgentRunProductProjectionQueryPort>,
        commands: Arc<AgentRunProductCommandFacade>,
    ) -> Self {
        Self {
            runs,
            agents,
            projection,
            commands,
        }
    }

    pub async fn delete(
        &self,
        request: AgentRunProductDeleteRequest,
    ) -> Result<AgentRunProductDeleteOutcome, AgentRunProductDeleteError> {
        let Some(run) = self
            .runs
            .get_by_id(request.run_id)
            .await
            .map_err(repository)?
        else {
            return Ok(AgentRunProductDeleteOutcome {
                deleted: false,
                project_id: request.project_id,
                run_id: request.run_id,
            });
        };
        if run.project_id != request.project_id {
            return Err(AgentRunProductDeleteError::ProjectMismatch);
        }
        let agents = self.agents.list_by_run(run.id).await.map_err(repository)?;

        for agent in &agents {
            let target = AgentRunTarget {
                run_id: run.id,
                agent_id: agent.id,
            };
            let snapshot = match self.projection.runtime_snapshot(&target).await {
                Ok(snapshot) => snapshot,
                Err(super::AgentRunProductProjectionError::TargetNotBound) => continue,
                Err(error) => {
                    return Err(AgentRunProductDeleteError::Runtime(error.to_string()));
                }
            };
            if snapshot.lifecycle == ManagedRuntimeLifecycleStatus::Closed {
                continue;
            }
            let receipt = self
                .commands
                .execute(AgentRunProductCommandRequest {
                    target,
                    client_command_id: format!("agent-run-delete:v1:{}:{}", run.id, agent.id),
                    expected_revision: snapshot.revision,
                    command: AgentRunProductCommand::Close,
                })
                .await
                .map_err(|error| AgentRunProductDeleteError::Runtime(error.to_string()))?;
            if receipt.status != ManagedRuntimeOperationStatus::Succeeded {
                return Err(AgentRunProductDeleteError::RuntimeNotClosed);
            }
        }

        for agent in &agents {
            let target = AgentRunTarget {
                run_id: run.id,
                agent_id: agent.id,
            };
            match self.projection.runtime_snapshot(&target).await {
                Ok(snapshot) if snapshot.lifecycle == ManagedRuntimeLifecycleStatus::Closed => {}
                Err(super::AgentRunProductProjectionError::TargetNotBound) => {}
                Ok(_) => return Err(AgentRunProductDeleteError::RuntimeNotClosed),
                Err(error) => {
                    return Err(AgentRunProductDeleteError::Runtime(error.to_string()));
                }
            }
        }

        self.runs.delete(run.id).await.map_err(repository)?;
        Ok(AgentRunProductDeleteOutcome {
            deleted: true,
            project_id: request.project_id,
            run_id: request.run_id,
        })
    }
}

fn repository(error: impl std::fmt::Display) -> AgentRunProductDeleteError {
    AgentRunProductDeleteError::Repository(error.to_string())
}
