//! Companion compose 路径 — companion parent slice + workflow activation。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent};
use agentdash_spi::ConnectorError;

use crate::agent_run::frame::runtime_launch::FrameLaunchEnvelope;
use crate::session::construction_provider::{
    CompanionLaunchSource, SessionConstructionProviderInput,
};
use crate::session::{CompanionParentSpec, CompanionParentWorkflowSpec};

use super::{FrameConstructionService, frame_builder_from_existing};

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    _agent: LifecycleAgent,
    companion: CompanionLaunchSource,
    input: &SessionConstructionProviderInput,
) -> Result<FrameLaunchEnvelope, ConnectorError> {
    let command = &input.command;
    let builder =
        frame_builder_from_existing(frame, input.session_id.as_str(), command.reason_tag())?;
    let (builder, extras) = if let Some(workflow) = companion.workflow {
        svc.assembler()
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
    } else {
        svc.assembler()
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
                },
            )
            .await
    }
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
