use agentdash_domain::workflow::{
    OrchestrationPlanSnapshot, ValidationSeverity, WorkflowGraph, WorkflowGraphRef,
};
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct WorkflowGraphPlanningRequest {
    pub project_id: Uuid,
    pub workflow_graph_ref: WorkflowGraphRef,
}

#[derive(Debug, Clone)]
pub struct PlannedWorkflowGraph {
    pub graph: WorkflowGraph,
    pub plan_snapshot: OrchestrationPlanSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGraphPlanningDiagnostic {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub source_path: String,
    pub related_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowGraphPlanningError {
    #[error("workflow graph ref rejected: {message}")]
    BadRequest { message: String },
    #[error("workflow graph was not found: {message}")]
    NotFound { message: String },
    #[error("workflow graph planning failed")]
    BlockingDiagnostics {
        workflow_graph_id: Uuid,
        diagnostics: Vec<WorkflowGraphPlanningDiagnostic>,
    },
    #[error("workflow graph planning failed: {message}")]
    Internal { message: String },
}

#[async_trait]
pub trait WorkflowGraphPlanningPort: Send + Sync {
    async fn plan_workflow_graph(
        &self,
        request: WorkflowGraphPlanningRequest,
    ) -> Result<PlannedWorkflowGraph, WorkflowGraphPlanningError>;
}
