use agentdash_application_ports::workflow_graph_planning::{
    PlannedWorkflowGraph, WorkflowGraphPlanningDiagnostic, WorkflowGraphPlanningError,
    WorkflowGraphPlanningPort, WorkflowGraphPlanningRequest,
};
use agentdash_domain::workflow::WorkflowGraphRepository;
use async_trait::async_trait;

use crate::lifecycle::WorkflowApplicationError;

use super::{
    WorkflowGraphCompileDiagnostic, WorkflowGraphCompileInput, WorkflowGraphCompiler,
    WorkflowGraphResolver,
};

pub struct ApplicationWorkflowGraphPlanner<'a> {
    workflow_graph_repo: &'a dyn WorkflowGraphRepository,
}

impl<'a> ApplicationWorkflowGraphPlanner<'a> {
    pub fn new(workflow_graph_repo: &'a dyn WorkflowGraphRepository) -> Self {
        Self {
            workflow_graph_repo,
        }
    }
}

#[async_trait]
impl WorkflowGraphPlanningPort for ApplicationWorkflowGraphPlanner<'_> {
    async fn plan_workflow_graph(
        &self,
        request: WorkflowGraphPlanningRequest,
    ) -> Result<PlannedWorkflowGraph, WorkflowGraphPlanningError> {
        let graph = WorkflowGraphResolver::new(self.workflow_graph_repo)
            .resolve(request.project_id, &request.workflow_graph_ref)
            .await
            .map_err(workflow_graph_planning_error_from_workflow)?
            .graph;

        let output = WorkflowGraphCompiler::compile(WorkflowGraphCompileInput::strict(&graph));
        if output.has_blocking_diagnostics() {
            return Err(WorkflowGraphPlanningError::BlockingDiagnostics {
                workflow_graph_id: graph.id,
                diagnostics: output
                    .diagnostics
                    .into_iter()
                    .map(workflow_graph_planning_diagnostic_from_compile)
                    .collect(),
            });
        }

        Ok(PlannedWorkflowGraph {
            graph,
            plan_snapshot: output.plan_snapshot,
        })
    }
}

fn workflow_graph_planning_error_from_workflow(
    error: WorkflowApplicationError,
) -> WorkflowGraphPlanningError {
    match error {
        WorkflowApplicationError::BadRequest(message)
        | WorkflowApplicationError::ModelRequired(message)
        | WorkflowApplicationError::Conflict(message) => {
            WorkflowGraphPlanningError::BadRequest { message }
        }
        WorkflowApplicationError::NotFound(message) => {
            WorkflowGraphPlanningError::NotFound { message }
        }
        WorkflowApplicationError::Internal(message) => {
            WorkflowGraphPlanningError::Internal { message }
        }
    }
}

fn workflow_graph_planning_diagnostic_from_compile(
    diagnostic: WorkflowGraphCompileDiagnostic,
) -> WorkflowGraphPlanningDiagnostic {
    WorkflowGraphPlanningDiagnostic {
        code: diagnostic.code,
        severity: diagnostic.severity,
        message: diagnostic.message,
        source_path: diagnostic.source_path,
        related_paths: diagnostic.related_paths,
    }
}
