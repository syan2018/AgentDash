use std::io;

#[cfg(test)]
use super::hub::PendingRuntimeContextTransitionInput;
use super::hub::SessionRuntimeInner;
use super::hub::{ApplyPendingRuntimeContextTransitionInput, PendingRuntimeContextApplication};
use super::runtime_commands::{
    AgentFrameTransitionRecord, RuntimeCommandRecord, RuntimeDeliveryCommand,
};
use super::types::CapabilityState;
#[cfg(test)]
use crate::agent_run::runtime_capability_projection::{
    RuntimeCapabilityProjectionInput, derive_runtime_skill_baseline, merge_live_vfs_skill_entries,
};
#[cfg(test)]
use agentdash_spi::Vfs;

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

    #[cfg(test)]
    pub(crate) async fn enqueue_pending_runtime_context_transition(
        &self,
        mut input: PendingRuntimeContextTransitionInput,
    ) -> Result<(), String> {
        self.derive_skill_baseline_for_transition_state(
            input.before_state.as_ref(),
            &mut input.after_state,
        )
        .await;
        self.hub
            .enqueue_pending_runtime_context_transition(input)
            .await
    }

    #[cfg(test)]
    async fn derive_skill_baseline_for_transition_state(
        &self,
        before_state: Option<&CapabilityState>,
        after_state: &mut CapabilityState,
    ) {
        let Some(active_vfs) = after_state.vfs.active.as_ref() else {
            return;
        };
        let Some(skills) = self.derive_skill_entries_for_active_vfs(active_vfs).await else {
            return;
        };
        let existing = before_state
            .map(|state| state.skill.skills.as_slice())
            .unwrap_or_else(|| after_state.skill.skills.as_slice());
        after_state.skill.skills = merge_live_vfs_skill_entries(existing, skills);
    }

    #[cfg(test)]
    async fn derive_skill_entries_for_active_vfs(
        &self,
        active_vfs: &Vfs,
    ) -> Option<Vec<agentdash_spi::context::capability::SkillEntry>> {
        derive_runtime_skill_baseline(RuntimeCapabilityProjectionInput {
            vfs_service: self.hub.vfs_service.as_deref(),
            active_vfs: Some(active_vfs),
            identity: None,
            extra_skill_dirs: &self.hub.extra_skill_dirs,
            skill_discovery_providers: &self.hub.skill_discovery_providers,
            diagnostics_label: "runtime_context_transition",
        })
        .await
        .map(|caps| caps.skills)
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
