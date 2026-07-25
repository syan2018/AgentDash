use std::sync::Arc;

use agentdash_domain::workflow::LifecycleRunRepository;
use thiserror::Error;
use uuid::Uuid;

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
}

/// Deletes the Product-owned AgentRun aggregate.
///
/// A concrete Agent source has its own owner and lifecycle. Product deletion therefore never
/// depends on Agent availability or a presentation snapshot reaching a derived `Closed` state.
pub struct AgentRunProductDeleteService {
    runs: Arc<dyn LifecycleRunRepository>,
}

impl AgentRunProductDeleteService {
    pub fn new(runs: Arc<dyn LifecycleRunRepository>) -> Self {
        Self { runs }
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
