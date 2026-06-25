use agentdash_application_ports::agent_frame_materialization::AgentRunFrameConstructionPort;
use agentdash_application_ports::lifecycle_materialization::{
    LifecycleDispatchPort, LifecycleDispatchPortResult, LifecycleDispatchRequest,
    LifecycleMaterializationError,
};
use agentdash_application_ports::runtime_session_delivery::RuntimeSessionCreationPort;
use agentdash_application_ports::workflow_agent_frame_materialization::WorkflowAgentNodeFrameMaterializationPort;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLaunchDispatchResult, AgentLineageRepository,
    LifecycleAgentRepository, LifecycleGateRepository, LifecycleRunRepository,
    LifecycleSubjectAssociationRepository, RuntimeSessionExecutionAnchorRepository,
    WorkflowGraphRepository,
};
use async_trait::async_trait;

use super::{
    LifecycleDispatchService, WorkflowAgentNodeMaterializationRequest,
    WorkflowAgentNodeMaterializationResult, WorkflowApplicationError,
};

pub struct LifecycleDispatchFacade<'a> {
    run_repo: &'a dyn LifecycleRunRepository,
    workflow_graph_repo: &'a dyn WorkflowGraphRepository,
    agent_repo: &'a dyn LifecycleAgentRepository,
    frame_repo: &'a dyn AgentFrameRepository,
    association_repo: &'a dyn LifecycleSubjectAssociationRepository,
    gate_repo: &'a dyn LifecycleGateRepository,
    lineage_repo: &'a dyn AgentLineageRepository,
    anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
    runtime_session_creator: &'a dyn RuntimeSessionCreationPort,
    frame_construction: &'a dyn AgentRunFrameConstructionPort,
    workflow_agent_frame_materialization: Option<&'a dyn WorkflowAgentNodeFrameMaterializationPort>,
}

impl<'a> LifecycleDispatchFacade<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_repo: &'a dyn LifecycleRunRepository,
        workflow_graph_repo: &'a dyn WorkflowGraphRepository,
        agent_repo: &'a dyn LifecycleAgentRepository,
        frame_repo: &'a dyn AgentFrameRepository,
        association_repo: &'a dyn LifecycleSubjectAssociationRepository,
        gate_repo: &'a dyn LifecycleGateRepository,
        lineage_repo: &'a dyn AgentLineageRepository,
        anchor_repo: &'a dyn RuntimeSessionExecutionAnchorRepository,
        runtime_session_creator: &'a dyn RuntimeSessionCreationPort,
        frame_construction: &'a dyn AgentRunFrameConstructionPort,
    ) -> Self {
        Self {
            run_repo,
            workflow_graph_repo,
            agent_repo,
            frame_repo,
            association_repo,
            gate_repo,
            lineage_repo,
            anchor_repo,
            runtime_session_creator,
            frame_construction,
            workflow_agent_frame_materialization: None,
        }
    }

    pub fn with_workflow_agent_frame_materialization_port(
        mut self,
        port: &'a dyn WorkflowAgentNodeFrameMaterializationPort,
    ) -> Self {
        self.workflow_agent_frame_materialization = Some(port);
        self
    }

    fn service(&self) -> LifecycleDispatchService<'_> {
        let service = LifecycleDispatchService::new(
            self.run_repo,
            self.workflow_graph_repo,
            self.agent_repo,
            self.frame_repo,
            self.association_repo,
            self.gate_repo,
            self.lineage_repo,
        )
        .with_anchor_repo(self.anchor_repo)
        .with_runtime_session_creator(self.runtime_session_creator)
        .with_frame_construction_port(self.frame_construction);
        if let Some(port) = self.workflow_agent_frame_materialization {
            service.with_workflow_agent_frame_materialization_port(port)
        } else {
            service
        }
    }

    pub async fn launch_agent(
        &self,
        intent: &agentdash_domain::workflow::AgentLaunchIntent,
    ) -> Result<AgentLaunchDispatchResult, WorkflowApplicationError> {
        self.service().launch_agent(intent).await
    }

    pub async fn materialize_workflow_agent_node(
        &self,
        request: WorkflowAgentNodeMaterializationRequest,
    ) -> Result<WorkflowAgentNodeMaterializationResult, WorkflowApplicationError> {
        self.service()
            .materialize_workflow_agent_node(request)
            .await
    }
}

#[async_trait]
impl LifecycleDispatchPort for LifecycleDispatchFacade<'_> {
    async fn dispatch_lifecycle(
        &self,
        request: LifecycleDispatchRequest,
    ) -> Result<LifecycleDispatchPortResult, LifecycleMaterializationError> {
        let result = self
            .service()
            .dispatch(&request.intent)
            .await
            .map_err(lifecycle_materialization_error_from_workflow)?;
        Ok(LifecycleDispatchPortResult { result })
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
