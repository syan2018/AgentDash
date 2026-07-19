use std::sync::Arc;

use agentdash_application_agentrun::agent_run::AgentRunProductRuntimeBindingRepository;
use agentdash_application_ports::{
    hook_workflow_projection::{
        HookActiveWorkflowFacts, HookExecutionLogAppendCommand, HookWorkflowProjection,
        HookWorkflowProjectionError, HookWorkflowProjectionPort, HookWorkflowProjectionQuery,
    },
    lifecycle_surface_projection as lifecycle_surface,
};
use async_trait::async_trait;

use crate::repository_set::RepositorySet;

#[derive(Clone)]
pub struct ProductHookWorkflowProjection {
    repos: RepositorySet,
    product_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
}

impl ProductHookWorkflowProjection {
    pub fn new(
        repos: RepositorySet,
        product_bindings: Arc<dyn AgentRunProductRuntimeBindingRepository>,
    ) -> Self {
        Self {
            repos,
            product_bindings,
        }
    }
}

#[async_trait]
impl HookWorkflowProjectionPort for ProductHookWorkflowProjection {
    async fn load_hook_workflow_projection(
        &self,
        query: HookWorkflowProjectionQuery,
    ) -> Result<HookWorkflowProjection, HookWorkflowProjectionError> {
        let workflow =
            agentdash_application_lifecycle::resolve_active_workflow_projection_for_target(
                &query.target,
                self.repos.agent_procedure_repo.as_ref(),
                self.repos.agent_frame_repo.as_ref(),
                self.repos.lifecycle_run_repo.as_ref(),
            )
            .await
            .map_err(|message| HookWorkflowProjectionError::Projection { message })?;

        let Some(workflow) = workflow else {
            return Ok(HookWorkflowProjection {
                run_context: None,
                active_workflow: None,
            });
        };

        let run_context = agentdash_application_lifecycle::SubjectRunContextResolver::new(
            self.repos.lifecycle_run_repo.as_ref(),
            self.repos.lifecycle_subject_association_repo.as_ref(),
            self.product_bindings.as_ref(),
            self.repos.lifecycle_agent_repo.as_ref(),
            self.repos.story_repo.as_ref(),
        )
        .resolve_for_run(&workflow.run)
        .await
        .map_err(|error| HookWorkflowProjectionError::Projection {
            message: error.to_string(),
        })?;
        let artifact_scope = agentdash_application_lifecycle::RuntimeNodeArtifactScope {
            run_id: workflow.run.id,
            orchestration_id: workflow.orchestration_id,
            node_path: workflow.node_path.clone(),
            attempt: workflow.active_attempt.attempt,
        };
        let fulfilled_output_ports =
            agentdash_application_lifecycle::load_scoped_port_output_map(
                self.repos.inline_file_repo.as_ref(),
                &artifact_scope,
            )
            .await;

        Ok(HookWorkflowProjection {
            run_context: Some(run_context),
            active_workflow: Some(HookActiveWorkflowFacts {
                projection: to_port_projection(workflow),
                fulfilled_output_ports,
            }),
        })
    }

    async fn append_execution_log(
        &self,
        command: HookExecutionLogAppendCommand,
    ) -> Result<(), HookWorkflowProjectionError> {
        agentdash_application_lifecycle::lifecycle::execution_log::flush_execution_log_entries(
            self.repos.lifecycle_run_repo.as_ref(),
            command.entries,
        )
        .await
        .map_err(|error| HookWorkflowProjectionError::Effect {
            message: error.to_string(),
        })
    }
}

fn to_port_projection(
    workflow: agentdash_application_lifecycle::ActiveWorkflowProjection,
) -> lifecycle_surface::ActiveWorkflowProjection {
    lifecycle_surface::ActiveWorkflowProjection {
        run: workflow.run,
        orchestration_id: workflow.orchestration_id,
        node_path: workflow.node_path,
        lifecycle_graph_id: workflow.lifecycle_graph_id,
        lifecycle_key: workflow.lifecycle_key,
        lifecycle_name: workflow.lifecycle_name,
        active_activity: workflow.active_activity,
        active_attempt: workflow.active_attempt,
        active_node_type: workflow.active_node_type,
        active_procedure_key: workflow.active_procedure_key,
        snapshot_contract: workflow.snapshot_contract,
        primary_workflow: workflow.primary_workflow,
    }
}
