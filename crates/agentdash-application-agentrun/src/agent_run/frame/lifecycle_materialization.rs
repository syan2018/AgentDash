use std::sync::Arc;

use agentdash_application_ports::agent_frame_materialization as agent_frame_materialization_port;
#[cfg(test)]
use agentdash_domain::workflow::AgentFrame;
use agentdash_domain::workflow::AgentFrameRepository;
#[cfg(test)]
use agentdash_spi::Vfs;
#[cfg(test)]
use uuid::Uuid;

use crate::agent_run::frame::builder::AgentFrameBuilder;
#[cfg(test)]
use crate::error::WorkflowApplicationError;

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
