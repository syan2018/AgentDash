use std::sync::Arc;

use agentdash_application_workflow::{
    OrchestrationExecutorDrainResult, OrchestrationExecutorLauncher,
};
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentLineageRepository, ExecutionSource, LifecycleAgentRepository,
    LifecycleGateRepository, LifecycleRun, LifecycleRunRepository, LifecycleRunStartIntent,
    LifecycleSubjectAssociationRepository, WorkflowGraphRef, WorkflowGraphRepository,
};
use uuid::Uuid;

use crate::SharedPlatformConfig;

use super::{LifecycleDispatchService, WorkflowApplicationError};

#[derive(Debug, Clone)]
pub struct CreateLifecycleRunCommand {
    pub project_id: Uuid,
    pub source: ExecutionSource,
    pub workflow_graph_ref: WorkflowGraphRef,
}

#[derive(Debug, Clone)]
pub struct ContinueLifecycleRunResult {
    pub run: LifecycleRun,
    pub drain_result: OrchestrationExecutorDrainResult,
}

#[derive(Clone)]
pub struct LifecycleRunCommandService {
    deps: LifecycleRunCommandDeps,
}

#[derive(Clone)]
pub struct LifecycleRunCommandDeps {
    pub run_repo: Arc<dyn LifecycleRunRepository>,
    pub workflow_graph_repo: Arc<dyn WorkflowGraphRepository>,
    pub agent_repo: Arc<dyn LifecycleAgentRepository>,
    pub frame_repo: Arc<dyn AgentFrameRepository>,
    pub association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    pub gate_repo: Arc<dyn LifecycleGateRepository>,
    pub lineage_repo: Arc<dyn AgentLineageRepository>,
    pub orchestration_launcher: OrchestrationExecutorLauncher,
}

impl LifecycleRunCommandService {
    pub fn new(deps: LifecycleRunCommandDeps, _platform_config: SharedPlatformConfig) -> Self {
        Self { deps }
    }

    pub async fn create_lifecycle_run(
        &self,
        command: CreateLifecycleRunCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let dispatch_service = LifecycleDispatchService::new(
            self.deps.run_repo.as_ref(),
            self.deps.workflow_graph_repo.as_ref(),
            self.deps.agent_repo.as_ref(),
            self.deps.frame_repo.as_ref(),
            self.deps.association_repo.as_ref(),
            self.deps.gate_repo.as_ref(),
            self.deps.lineage_repo.as_ref(),
        );
        let dispatch_result = dispatch_service
            .start_lifecycle_run(&LifecycleRunStartIntent {
                project_id: command.project_id,
                source: command.source,
                workflow_graph_ref: command.workflow_graph_ref,
            })
            .await?;
        self.load_run(dispatch_result.run_ref).await
    }

    pub async fn continue_lifecycle_run(
        &self,
        run_id: Uuid,
    ) -> Result<ContinueLifecycleRunResult, WorkflowApplicationError> {
        let drain_result = self
            .deps
            .orchestration_launcher
            .drain_ready_nodes(run_id)
            .await?;
        let run = self.load_run(run_id).await?;
        Ok(ContinueLifecycleRunResult { run, drain_result })
    }

    pub async fn create_and_continue_lifecycle_run(
        &self,
        command: CreateLifecycleRunCommand,
    ) -> Result<ContinueLifecycleRunResult, WorkflowApplicationError> {
        let run = self.create_lifecycle_run(command).await?;
        self.continue_lifecycle_run(run.id).await
    }

    async fn load_run(&self, run_id: Uuid) -> Result<LifecycleRun, WorkflowApplicationError> {
        self.deps.run_repo.get_by_id(run_id).await?.ok_or_else(|| {
            WorkflowApplicationError::NotFound(format!("LifecycleRun 不存在: {run_id}"))
        })
    }
}
