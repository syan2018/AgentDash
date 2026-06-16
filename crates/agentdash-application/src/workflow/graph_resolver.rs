use uuid::Uuid;

use agentdash_domain::workflow::{WorkflowGraph, WorkflowGraphRef, WorkflowGraphRepository};

use crate::lifecycle::WorkflowApplicationError;

pub struct ResolvedWorkflowGraph {
    pub graph: WorkflowGraph,
}

pub struct WorkflowGraphResolver<'a> {
    workflow_graph_repo: &'a dyn WorkflowGraphRepository,
}

impl<'a> WorkflowGraphResolver<'a> {
    pub fn new(workflow_graph_repo: &'a dyn WorkflowGraphRepository) -> Self {
        Self {
            workflow_graph_repo,
        }
    }

    pub async fn resolve(
        &self,
        project_id: Uuid,
        graph_ref: &WorkflowGraphRef,
    ) -> Result<ResolvedWorkflowGraph, WorkflowApplicationError> {
        let graph = match graph_ref {
            WorkflowGraphRef::ById(id) => {
                let graph = self
                    .workflow_graph_repo
                    .get_by_id(*id)
                    .await?
                    .ok_or_else(|| {
                        WorkflowApplicationError::NotFound(format!("workflow_graph 不存在: {id}"))
                    })?;
                if graph.project_id != project_id {
                    return Err(WorkflowApplicationError::NotFound(format!(
                        "workflow_graph 不存在: {id}"
                    )));
                }
                graph
            }
            WorkflowGraphRef::ByKey {
                project_id: ref_project_id,
                key,
            } => {
                if *ref_project_id != project_id {
                    return Err(WorkflowApplicationError::BadRequest(format!(
                        "WorkflowGraphRef project_id {} 与 intent project_id {} 不一致",
                        ref_project_id, project_id
                    )));
                }
                self.workflow_graph_repo
                    .get_by_project_and_key(*ref_project_id, key)
                    .await?
                    .ok_or_else(|| {
                        WorkflowApplicationError::NotFound(format!("workflow_graph 不存在: {key}"))
                    })?
            }
        };

        Ok(ResolvedWorkflowGraph { graph })
    }
}
