//! Companion launch modifier — 在已判定的 owner surface 上叠加 parent slice / workflow facts。

use agentdash_domain::workflow::AgentFrame;
use agentdash_platform_spi::PlatformRuntimeError;

use crate::agent_run::frame::FrameLaunchEnvelope;
use agentdash_application_ports::launch::CompanionLaunchSource;

use crate::agent_run::frame::FrameLaunchEnvelopeConstructionInput;

use super::{
    CompanionParentSpec, CompanionParentWorkflowSpec, FrameConstructionService,
    frame_builder_from_existing,
};

pub(super) async fn compose_project_agent_owner_modifier(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    companion: CompanionLaunchSource,
    input: &FrameLaunchEnvelopeConstructionInput,
) -> Result<FrameLaunchEnvelope, PlatformRuntimeError> {
    let command = &input.command;
    let builder = frame_builder_from_existing(frame, command.reason_tag())?;
    let (builder, extras) = svc
        .assembler()
        .compose_companion_to_frame(
            builder,
            CompanionParentSpec {
                parent_session_id: &companion.parent_session_id,
                child_session_id: input.runtime_thread_id.as_str(),
                slice_mode: companion.slice_mode,
                companion_executor_config: companion.companion_executor_config,
                dispatch_prompt: companion.dispatch_prompt,
                selected_project_agent_id: companion.selected_project_agent_id,
                selected_agent_key: companion.selected_agent_key.clone(),
            },
        )
        .await
        .map_err(PlatformRuntimeError::InvalidConfig)?;

    svc.compose_pending_frame(
        builder,
        extras,
        command,
        input.runtime_thread_id.as_str(),
        None,
    )
    .await
}

pub(super) async fn compose_lifecycle_node_owner_modifier(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    companion: CompanionLaunchSource,
    input: &FrameLaunchEnvelopeConstructionInput,
) -> Result<FrameLaunchEnvelope, PlatformRuntimeError> {
    let command = &input.command;
    let workflow = companion.workflow.ok_or_else(|| {
        PlatformRuntimeError::InvalidConfig(format!(
            "RuntimeThread {} 的 LifecycleNode companion modifier 缺少 workflow facts",
            input.runtime_thread_id
        ))
    })?;
    let builder = frame_builder_from_existing(frame, command.reason_tag())?;
    let (builder, extras) = svc
        .assembler()
        .compose_companion_with_workflow_to_frame(
            builder,
            CompanionParentWorkflowSpec {
                companion: CompanionParentSpec {
                    parent_session_id: &companion.parent_session_id,
                    child_session_id: input.runtime_thread_id.as_str(),
                    slice_mode: companion.slice_mode,
                    companion_executor_config: companion.companion_executor_config,
                    dispatch_prompt: companion.dispatch_prompt,
                    selected_project_agent_id: companion.selected_project_agent_id,
                    selected_agent_key: companion.selected_agent_key.clone(),
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
        .map_err(PlatformRuntimeError::InvalidConfig)?;

    svc.compose_pending_frame(
        builder,
        extras,
        command,
        input.runtime_thread_id.as_str(),
        None,
    )
    .await
}
