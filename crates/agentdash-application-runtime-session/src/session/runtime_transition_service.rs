use std::io;

use super::hub::SessionRuntimeInner;
use super::hub::{ApplyPendingRuntimeContextTransitionInput, PendingRuntimeContextApplication};
use super::runtime_commands::{
    AgentFrameTransitionRecord, RuntimeCommandRecord, RuntimeDeliveryCommand,
};
use super::types::CapabilityState;

/// Session live runtime transition 协调入口。
///
/// 这里保留 delivery outbox、turn 边界 pending transition 应用，以及 active runtime
/// snapshot 读取；AgentRun current surface query 和业务 surface update 不属于 session。
#[derive(Clone)]
pub struct SessionRuntimeTransitionService {
    hub: SessionRuntimeInner,
}

impl SessionRuntimeTransitionService {
    pub(super) fn new(hub: SessionRuntimeInner) -> Self {
        Self { hub }
    }

    pub async fn current_runtime_capability_state(
        &self,
        session_id: &str,
    ) -> Option<CapabilityState> {
        self.hub.get_current_capability_state(session_id).await
    }

    pub async fn latest_runtime_capability_state(
        &self,
        session_id: &str,
    ) -> Option<CapabilityState> {
        self.hub.get_latest_capability_state(session_id).await
    }

    pub async fn list_requested_runtime_commands(
        &self,
        session_id: &str,
    ) -> io::Result<Vec<RuntimeCommandRecord>> {
        self.hub
            .stores
            .runtime_commands
            .list_requested_runtime_commands(session_id)
            .await
            .map_err(Into::into)
    }

    pub async fn enqueue_runtime_delivery_command(
        &self,
        delivery_runtime_session_id: &str,
        delivery: RuntimeDeliveryCommand,
        frame_transition: AgentFrameTransitionRecord,
    ) -> std::io::Result<()> {
        self.hub
            .enqueue_runtime_delivery_command(
                delivery_runtime_session_id,
                delivery,
                frame_transition,
            )
            .await
    }

    pub(crate) async fn apply_pending_runtime_context_transitions_on_turn(
        &self,
        input: ApplyPendingRuntimeContextTransitionInput<'_>,
    ) -> PendingRuntimeContextApplication {
        self.hub
            .apply_pending_runtime_context_transitions_on_turn(input)
            .await
    }
}
