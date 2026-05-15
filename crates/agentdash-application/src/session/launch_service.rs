use agentdash_spi::ConnectorError;

use super::hub::SessionHub;
use super::launch::{LaunchCommand, LaunchCommandOutcome};
use super::prompt_pipeline::{SessionLaunchDeps, SessionLaunchExecutor};

#[derive(Clone)]
pub struct SessionLaunchService {
    deps: SessionLaunchDeps,
}

impl SessionLaunchDeps {
    pub(in crate::session) fn from_hub(hub: &SessionHub) -> Self {
        Self {
            default_vfs: hub.default_vfs.clone(),
            connector: hub.connector.clone(),
            runtime_registry: hub.runtime_registry.clone(),
            turn_supervisor: hub.turn_supervisor.clone(),
            stores: hub.stores.clone(),
            vfs_service: hub.vfs_service.clone(),
            extra_skill_dirs: hub.extra_skill_dirs.clone(),
            title_generator: hub.title_generator.clone(),
            session_construction_provider: hub.session_construction_provider.clone(),
            hook_effect_handler_registry: hub.hook_effect_handler_registry.clone(),
            context_audit_bus: hub.context_audit_bus.clone(),
            base_system_prompt: hub.base_system_prompt.clone(),
            user_preferences: hub.user_preferences.clone(),
            runtime_tool_provider: hub.runtime_tool_provider.clone(),
            mcp_relay_provider: hub.mcp_relay_provider.clone(),
            eventing: hub.eventing_service(),
            core: hub.core_service(),
            hooks: hub.hook_service(),
            capability: hub.capability_service(),
            effects: hub.effects_service(),
        }
    }
}

impl SessionLaunchService {
    pub(super) fn new(hub: SessionHub) -> Self {
        Self {
            deps: SessionLaunchDeps::from_hub(&hub),
        }
    }

    pub async fn launch_command(
        &self,
        session_id: &str,
        command: LaunchCommand,
    ) -> Result<String, ConnectorError> {
        Ok(self
            .launch_command_with_outcome(session_id, command)
            .await?
            .turn_id)
    }

    pub async fn launch_command_with_outcome(
        &self,
        session_id: &str,
        command: LaunchCommand,
    ) -> Result<LaunchCommandOutcome, ConnectorError> {
        SessionLaunchExecutor::new(self.deps.clone())
            .execute_command(session_id, command)
            .await
    }
}
