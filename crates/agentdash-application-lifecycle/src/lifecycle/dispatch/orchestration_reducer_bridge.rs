use agentdash_application_workflow::orchestration::{
    OrchestrationRuntimeEvent, apply_orchestration_event_to_run,
};
use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository, OrchestrationBindingRefs};

use crate::lifecycle::WorkflowApplicationError;

use super::plan::MaterializedAgentRuntime;

pub(crate) struct OrchestrationReducerBridge<'a> {
    run_repo: &'a dyn LifecycleRunRepository,
}

impl<'a> OrchestrationReducerBridge<'a> {
    pub(crate) fn new(run_repo: &'a dyn LifecycleRunRepository) -> Self {
        Self { run_repo }
    }

    pub(crate) async fn mark_node_claimed(
        &self,
        run: LifecycleRun,
        binding: &OrchestrationBindingRefs,
        materialized: &MaterializedAgentRuntime,
    ) -> Result<LifecycleRun, WorkflowApplicationError> {
        if materialized.runtime_refs.orchestration_binding.as_ref() != Some(binding) {
            return Err(WorkflowApplicationError::Internal(format!(
                "materialized runtime refs orchestration binding 与 reducer binding 不一致: expected {:?}, got {:?}",
                binding, materialized.runtime_refs.orchestration_binding
            )));
        }
        let (updated_run, _) = apply_orchestration_event_to_run(
            run,
            binding.orchestration_ref,
            OrchestrationRuntimeEvent::NodeClaimed {
                node_path: binding.node_path.clone(),
                attempt: binding.attempt,
                timestamp: chrono::Utc::now(),
            },
        )
        .map_err(|error| WorkflowApplicationError::Internal(error.to_string()))?;
        self.run_repo.update(&updated_run).await?;
        Ok(updated_run)
    }
}
