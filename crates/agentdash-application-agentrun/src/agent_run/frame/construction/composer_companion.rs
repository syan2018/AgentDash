//! Companion launch modifier — 在已判定的 owner surface 上叠加 parent slice / workflow facts。

use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::ConnectorError;

use crate::agent_run::frame::launch_envelope_provider::{
    CompanionLaunchSource, FrameLaunchEnvelopeConstructionInput,
};
use crate::agent_run::frame::runtime_launch::FrameLaunchEnvelope;

use super::{
    CompanionParentSpec, CompanionParentWorkflowSpec, FrameConstructionService,
    frame_builder_from_existing,
};

pub(super) async fn compose_project_agent_owner_modifier(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    companion: CompanionLaunchSource,
    input: &FrameLaunchEnvelopeConstructionInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let command = &input.command;
    let identity = command.identity();
    let builder =
        frame_builder_from_existing(frame, input.session_id.as_str(), command.reason_tag())?;
    let (builder, extras) = svc
        .assembler()
        .compose_companion_to_frame(
            builder,
            CompanionParentSpec {
                parent_session_id: &companion.parent_session_id,
                child_session_id: input.session_id.as_str(),
                slice_mode: companion.slice_mode,
                companion_executor_config: companion.companion_executor_config,
                dispatch_prompt: companion.dispatch_prompt,
                selected_project_agent_id: companion.selected_project_agent_id,
                selected_agent_key: companion.selected_agent_key.clone(),
                identity,
            },
        )
        .await
        .map_err(ConnectorError::InvalidConfig)?;

    svc.compose_pending_frame(
        builder,
        extras,
        command,
        input.session_id.as_str(),
        None,
        &input.requested_runtime_commands,
    )
    .await
}

pub(super) async fn compose_lifecycle_node_owner_modifier(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    companion: CompanionLaunchSource,
    input: &FrameLaunchEnvelopeConstructionInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let command = &input.command;
    let identity = command.identity();
    let workflow = companion.workflow.ok_or_else(|| {
        ConnectorError::InvalidConfig(format!(
            "RuntimeSession {} 的 LifecycleNode companion modifier 缺少 workflow facts",
            input.session_id
        ))
    })?;
    let builder =
        frame_builder_from_existing(frame, input.session_id.as_str(), command.reason_tag())?;
    let (builder, extras) = svc
        .assembler()
        .compose_companion_with_workflow_to_frame(
            builder,
            CompanionParentWorkflowSpec {
                companion: CompanionParentSpec {
                    parent_session_id: &companion.parent_session_id,
                    child_session_id: input.session_id.as_str(),
                    slice_mode: companion.slice_mode,
                    companion_executor_config: companion.companion_executor_config,
                    dispatch_prompt: companion.dispatch_prompt,
                    selected_project_agent_id: companion.selected_project_agent_id,
                    selected_agent_key: companion.selected_agent_key.clone(),
                    identity,
                },
                run: &workflow.run,
                orchestration_id: workflow.orchestration_id,
                node_path: &workflow.node_path,
                attempt: workflow.attempt,
                lifecycle: &workflow.lifecycle,
                activity: &workflow.activity,
                workflow: workflow.workflow.as_ref(),
            },
        )
        .await
        .map_err(ConnectorError::InvalidConfig)?;

    svc.compose_pending_frame(
        builder,
        extras,
        command,
        input.session_id.as_str(),
        None,
        &input.requested_runtime_commands,
    )
    .await
}
