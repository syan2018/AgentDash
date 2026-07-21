use agentdash_application_ports::agent_frame_materialization::{
    AgentFrameWriteRole, AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError,
};
use agentdash_application_ports::workflow_agent_frame_materialization::{
    WorkflowAgentNodeFrameMaterializationInput, WorkflowAgentNodeFrameMaterializationPort,
};
use async_trait::async_trait;

use crate::agent_run::frame::AgentFrameBuilder;

use super::{
    AgentRunProjectOwnerFrameConstructionAdapter, LifecycleNodeSpec,
    compose_lifecycle_node_to_frame_with_audit,
    launch_anchor_materialization::materialize_frame_context_discovery,
};

#[async_trait]
impl WorkflowAgentNodeFrameMaterializationPort for AgentRunProjectOwnerFrameConstructionAdapter {
    async fn materialize_workflow_agent_node_frame(
        &self,
        input: WorkflowAgentNodeFrameMaterializationInput,
    ) -> Result<AgentRunFrameSurfaceCommandOutcome, AgentRunFrameSurfaceError> {
        let runtime_thread_id = input.runtime_thread_id.clone().ok_or_else(|| {
            AgentRunFrameSurfaceError::ConstructionRejected {
                message: "Workflow AgentCall frame materialization 缺少 RuntimeThreadId"
                    .to_string(),
            }
        })?;
        let run = self
            .repos
            .lifecycle_run_repo
            .get_by_id(input.run_id)
            .await
            .map_err(construction_error)?
            .ok_or_else(|| AgentRunFrameSurfaceError::ConstructionRejected {
                message: format!("Workflow AgentCall LifecycleRun {} 不存在", input.run_id),
            })?;
        if run.project_id != input.project_id {
            return Err(AgentRunFrameSurfaceError::ConstructionRejected {
                message: "Workflow AgentCall run/project authority drifted".to_string(),
            });
        }
        let builder = AgentFrameBuilder::new_launch_anchor(input.agent_id, input.created_by_id);
        let (builder, _) = compose_lifecycle_node_to_frame_with_audit(
            builder,
            &self.repos,
            self.platform_config.as_ref(),
            self.lifecycle_surface_projection.as_ref(),
            self.product_runtime_bindings.as_ref(),
            LifecycleNodeSpec {
                run: &run,
                orchestration_id: input.orchestration_id,
                node_path: &input.node_path,
                attempt: input.attempt,
                lifecycle_key: &input.lifecycle_key,
                activity: &input.activity,
                workflow_contract: input.workflow_contract.as_ref(),
                base_vfs: input.base_vfs.as_ref(),
                workflow_label: None,
                inherited_executor_config: input.inherited_executor_config,
            },
            Some(self.audit_bus.clone()),
            Some(&runtime_thread_id),
            Some(&run.id.to_string()),
            Some(&input.agent_id.to_string()),
        )
        .await
        .map_err(|message| AgentRunFrameSurfaceError::ConstructionRejected { message })?;
        let mut frame = builder
            .build_uncommitted(self.repos.agent_frame_repo.as_ref())
            .await
            .map_err(construction_error)?;
        materialize_frame_context_discovery(
            &mut frame,
            self.vfs_service.as_ref(),
            &self.extra_skill_dirs,
            &self.skill_discovery_providers,
            &self.memory_discovery_providers,
        )
        .await?;
        self.repos
            .agent_frame_repo
            .create(&frame)
            .await
            .map_err(construction_error)?;
        let mut outcome =
            AgentRunFrameSurfaceCommandOutcome::new(AgentFrameWriteRole::FrameConstruction);
        outcome.frame_id = Some(frame.id);
        outcome.agent_id = Some(frame.agent_id);
        outcome.runtime_thread_id = Some(runtime_thread_id);
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

fn construction_error(error: impl std::fmt::Display) -> AgentRunFrameSurfaceError {
    AgentRunFrameSurfaceError::ConstructionRejected {
        message: error.to_string(),
    }
}
