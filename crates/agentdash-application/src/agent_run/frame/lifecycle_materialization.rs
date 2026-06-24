use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository};
#[cfg(test)]
use agentdash_spi::Vfs;
use uuid::Uuid;

use crate::agent_run::frame::builder::AgentFrameBuilder;
use crate::agent_run::frame::construction::{
    LifecycleNodeSpec, compose_lifecycle_node_to_frame_with_audit,
};
use crate::lifecycle::{LifecycleLaunchFrameMaterializationRequest, WorkflowApplicationError};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;

pub(crate) async fn materialize_launch_anchor_frame(
    frame_repo: &dyn AgentFrameRepository,
    request: LifecycleLaunchFrameMaterializationRequest,
) -> Result<AgentFrame, WorkflowApplicationError> {
    let mut builder =
        AgentFrameBuilder::new_launch_anchor(request.agent_id, request.frame_created_by_id);
    if let Some(session_id) = request.runtime_session_ref {
        builder = builder.with_runtime_session(session_id.to_string());
    }
    Ok(builder.build(frame_repo).await?)
}

#[cfg(test)]
pub(crate) async fn materialize_launch_anchor_frame_with_vfs(
    frame_repo: &dyn AgentFrameRepository,
    request: LifecycleLaunchFrameMaterializationRequest,
    vfs: &Vfs,
) -> Result<AgentFrame, WorkflowApplicationError> {
    let mut builder =
        AgentFrameBuilder::new_launch_anchor(request.agent_id, request.frame_created_by_id);
    if let Some(session_id) = request.runtime_session_ref {
        builder = builder.with_runtime_session(session_id.to_string());
    }
    Ok(builder.with_vfs_typed(vfs).build(frame_repo).await?)
}

pub(crate) struct AgentRunWorkflowNodeFrameMaterializationRequest<'a> {
    pub frame_repo: &'a dyn AgentFrameRepository,
    pub repos: &'a RepositorySet,
    pub platform_config: &'a PlatformConfig,
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
        request.spec,
        None,
        Some(runtime_session_id.as_str()),
    )
    .await
    .map_err(WorkflowApplicationError::Internal)?;

    Ok(builder.build(request.frame_repo).await?)
}
