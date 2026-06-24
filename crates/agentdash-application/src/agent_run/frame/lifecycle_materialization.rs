use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
use agentdash_application_ports::lifecycle_surface_projection::LifecycleSurfaceProjectionPort;
use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository};
#[cfg(test)]
use agentdash_spi::Vfs;
use uuid::Uuid;

use crate::agent_run::frame::builder::AgentFrameBuilder;
use crate::agent_run::frame::construction::{
    LifecycleNodeSpec, compose_lifecycle_node_to_frame_with_audit,
};
use crate::lifecycle::WorkflowApplicationError;
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;

#[derive(Clone)]
pub struct AgentRunLaunchAnchorFrameConstructionAdapter {
    frame_repo: Arc<dyn AgentFrameRepository>,
}

impl AgentRunLaunchAnchorFrameConstructionAdapter {
    pub fn new(frame_repo: Arc<dyn AgentFrameRepository>) -> Self {
        Self { frame_repo }
    }
}

#[async_trait::async_trait]
impl agent_frame_materialization_port::AgentRunFrameConstructionPort
    for AgentRunLaunchAnchorFrameConstructionAdapter
{
    async fn execute_frame_construction_command(
        &self,
        command: agent_frame_materialization_port::FrameConstructionCommand,
    ) -> Result<
        agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome,
        agent_frame_materialization_port::AgentRunFrameSurfaceError,
    > {
        let agent_frame_materialization_port::FrameConstructionCommand::DispatchLaunchAnchor {
            agent_id,
            runtime_session_id,
            created_by_id,
            ..
        } = command
        else {
            return Err(
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: "launch anchor adapter only supports DispatchLaunchAnchor commands"
                        .to_string(),
                },
            );
        };

        let frame = AgentFrameBuilder::new_launch_anchor(agent_id, created_by_id)
            .with_runtime_session(runtime_session_id.clone())
            .build(self.frame_repo.as_ref())
            .await
            .map_err(|error| {
                agent_frame_materialization_port::AgentRunFrameSurfaceError::ConstructionRejected {
                    message: error.to_string(),
                }
            })?;
        let mut outcome = agent_frame_materialization_port::AgentRunFrameSurfaceCommandOutcome::new(
            agent_frame_materialization_port::AgentFrameWriteRole::FrameConstruction,
        );
        outcome.frame_id = Some(frame.id);
        outcome.agent_id = Some(frame.agent_id);
        outcome.runtime_session_id = Some(runtime_session_id);
        outcome.wrote_frame_revision = true;
        Ok(outcome)
    }
}

#[cfg(test)]
pub(crate) async fn construct_launch_anchor_frame_with_vfs(
    frame_repo: &dyn AgentFrameRepository,
    agent_id: Uuid,
    runtime_session_ref: Option<Uuid>,
    frame_created_by_id: Option<String>,
    vfs: &Vfs,
) -> Result<AgentFrame, WorkflowApplicationError> {
    let mut builder = AgentFrameBuilder::new_launch_anchor(agent_id, frame_created_by_id);
    if let Some(session_id) = runtime_session_ref {
        builder = builder.with_runtime_session(session_id.to_string());
    }
    Ok(builder.with_vfs_typed(vfs).build(frame_repo).await?)
}

pub(crate) struct AgentRunWorkflowNodeFrameMaterializationRequest<'a> {
    pub frame_repo: &'a dyn AgentFrameRepository,
    pub repos: &'a RepositorySet,
    pub platform_config: &'a PlatformConfig,
    pub lifecycle_surface_projection: &'a dyn LifecycleSurfaceProjectionPort,
    pub agent_id: Uuid,
    pub runtime_session_ref: Uuid,
    pub frame_created_by_id: Option<String>,
    pub spec: LifecycleNodeSpec<'a>,
}

pub(crate) async fn materialize_workflow_agent_node_frame(
    request: AgentRunWorkflowNodeFrameMaterializationRequest<'_>,
) -> Result<AgentFrame, WorkflowApplicationError> {
    let runtime_session_id = request.runtime_session_ref.to_string();
    let builder =
        AgentFrameBuilder::new_launch_anchor(request.agent_id, request.frame_created_by_id)
            .with_runtime_session(runtime_session_id.clone());
    let (builder, _extras) = compose_lifecycle_node_to_frame_with_audit(
        builder,
        request.repos,
        request.platform_config,
        request.lifecycle_surface_projection,
        request.spec,
        None,
        Some(runtime_session_id.as_str()),
    )
    .await
    .map_err(WorkflowApplicationError::Internal)?;

    Ok(builder.build(request.frame_repo).await?)
}
