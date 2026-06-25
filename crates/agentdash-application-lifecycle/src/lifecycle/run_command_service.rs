use std::sync::Arc;

use agentdash_domain::workflow::{
    ExecutionSource, LifecycleRun, LifecycleRunStartIntent, WorkflowGraphRef,
};
use agentdash_spi::FunctionRunner;
use uuid::Uuid;

use crate::workflow::{OrchestrationExecutorDrainResult, OrchestrationExecutorLauncher};
use crate::{RepositorySet, SharedPlatformConfig};

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
    repos: RepositorySet,
    platform_config: SharedPlatformConfig,
    function_runner: Option<Arc<dyn FunctionRunner>>,
}

impl LifecycleRunCommandService {
    pub fn new(repos: RepositorySet, platform_config: SharedPlatformConfig) -> Self {
        Self {
            repos,
            platform_config,
            function_runner: None,
        }
    }

    pub fn with_function_runner(mut self, runner: Arc<dyn FunctionRunner>) -> Self {
        self.function_runner = Some(runner);
        self
    }

    pub async fn create_lifecycle_run(
        &self,
        command: CreateLifecycleRunCommand,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        let dispatch_service = LifecycleDispatchService::new(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.workflow_graph_repo.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.agent_frame_repo.as_ref(),
            self.repos.lifecycle_subject_association_repo.as_ref(),
            self.repos.lifecycle_gate_repo.as_ref(),
            self.repos.agent_lineage_repo.as_ref(),
        )
        .with_anchor_repo(self.repos.execution_anchor_repo.as_ref())
        .with_runtime_session_creator(self.repos.runtime_session_creator.as_ref())
        .with_frame_construction_port(self.repos.agent_frame_construction.as_ref());
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
        let mut launcher = OrchestrationExecutorLauncher::new_with_platform_config(
            self.repos.clone(),
            self.platform_config.clone(),
        );
        if let Some(function_runner) = &self.function_runner {
            launcher = launcher.with_function_runner(function_runner.clone());
        }
        let drain_result = launcher.drain_ready_nodes(run_id).await?;
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
        self.repos
            .lifecycle_run_repo
            .get_by_id(run_id)
            .await?
            .ok_or_else(|| {
                WorkflowApplicationError::NotFound(format!("LifecycleRun 不存在: {run_id}"))
            })
    }
}
