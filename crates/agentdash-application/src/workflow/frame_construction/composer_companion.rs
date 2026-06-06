//! Companion compose 路径 — companion parent slice + workflow activation。

use agentdash_domain::workflow::{AgentFrame, LifecycleAgent};
use agentdash_spi::ConnectorError;

use crate::session::construction_provider::{
    CompanionLaunchSource, SessionConstructionProviderInput,
};
use crate::session::{CompanionParentSpec, CompanionParentWorkflowSpec};
use crate::workflow::runtime_launch::FrameLaunchEnvelope;

use super::{FrameConstructionService, frame_builder_from_existing};

pub(super) async fn compose(
    svc: &FrameConstructionService,
    frame: &AgentFrame,
    mut agent: LifecycleAgent,
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
                        slice_mode: companion.slice_mode,
                        companion_executor_config: companion.companion_executor_config,
                        dispatch_prompt: companion.dispatch_prompt,
                    },
                    run: &workflow.run,
                    graph_instance_id: workflow.graph_instance_id,
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
                    slice_mode: companion.slice_mode,
                    companion_executor_config: companion.companion_executor_config,
                    dispatch_prompt: companion.dispatch_prompt,
                },
            )
            .await
    }
    .map_err(ConnectorError::InvalidConfig)?;

    svc.persist_composed_frame(
        builder,
        &mut agent,
        extras,
        command,
        input.session_id.as_str(),
        None,
    )
    .await
}
