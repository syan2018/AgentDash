use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
use agentdash_application_ports::lifecycle_materialization::{
    LifecycleMaterializationError, WorkflowAgentNodeMaterializationPort,
    WorkflowAgentNodeMaterializationRequest, WorkflowAgentNodeMaterializationResult,
};
use agentdash_application_ports::workflow_agent_frame_materialization::WorkflowAgentNodeFrameMaterializationPort;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, LifecycleAgentRepository,
    LifecycleGateRepository, LifecycleRunRepository, LifecycleSubjectAssociationRepository,
    WorkflowGraphRepository,
};
use async_trait::async_trait;

use crate::lifecycle::{LifecycleDispatchService, WorkflowApplicationError};

#[derive(Clone)]
pub struct LifecycleWorkflowAgentNodeMaterializationDeps {
    pub run_repo: Arc<dyn LifecycleRunRepository>,
    pub workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub gate_repo: Arc<dyn LifecycleGateRepository>,
    pub lineage_repo: Arc<dyn AgentLineageRepository>,
    pub frame_construction: Arc<dyn AgentRunFrameConstructionPort>,
    pub workflow_agent_frame_materialization: Arc<dyn WorkflowAgentNodeFrameMaterializationPort>,
}

#[derive(Clone)]
pub struct LifecycleWorkflowAgentNodeMaterializationAdapter {
    deps: LifecycleWorkflowAgentNodeMaterializationDeps,
}

impl LifecycleWorkflowAgentNodeMaterializationAdapter {
    pub fn new(deps: LifecycleWorkflowAgentNodeMaterializationDeps) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl WorkflowAgentNodeMaterializationPort for LifecycleWorkflowAgentNodeMaterializationAdapter {
    async fn materialize_workflow_agent_node(
        &self,
        request: WorkflowAgentNodeMaterializationRequest,
    ) -> Result<WorkflowAgentNodeMaterializationResult, LifecycleMaterializationError> {
        let service = LifecycleDispatchService::new(
            self.deps.run_repo.as_ref(),
            self.deps.workflow_graph_repo.as_ref(),
            self.deps.agent_repo.as_ref(),
            self.deps.frame_repo.as_ref(),
            self.deps.association_repo.as_ref(),
            self.deps.gate_repo.as_ref(),
            self.deps.lineage_repo.as_ref(),
        )
        .with_frame_construction_port(self.deps.frame_construction.as_ref())
        .with_workflow_agent_frame_materialization_port(
            self.deps.workflow_agent_frame_materialization.as_ref(),
        );

        service
            .materialize_workflow_agent_node(request)
            .await
            .map_err(lifecycle_materialization_error_from_workflow)
    }
}

fn lifecycle_materialization_error_from_workflow(
    error: WorkflowApplicationError,
) -> LifecycleMaterializationError {
    match error {
        WorkflowApplicationError::BadRequest(message)
        | WorkflowApplicationError::ModelRequired(message)
        | WorkflowApplicationError::NotFound(message)
        | WorkflowApplicationError::Conflict(message) => {
            LifecycleMaterializationError::Rejected { message }
        }
        WorkflowApplicationError::Internal(message) => {
            LifecycleMaterializationError::Internal { message }
        }
    }
}
