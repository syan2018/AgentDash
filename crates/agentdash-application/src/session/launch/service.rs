use agentdash_spi::ConnectorError;

use super::{LaunchCommand, LaunchCommandOutcome};
use crate::session::hub::SessionRuntimeInner;
use crate::session::prompt_pipeline::{SessionLaunchDeps, SessionLaunchExecutor};

#[derive(Clone)]
pub struct SessionLaunchService {
    deps: SessionLaunchDeps,
}

impl SessionLaunchDeps {
    pub(in crate::session) fn from_inner(inner: &SessionRuntimeInner) -> Self {
        Self {
            connector: inner.connector.clone(),
            runtime_registry: inner.runtime_registry.clone(),
            turn_supervisor: inner.turn_supervisor.clone(),
            stores: inner.stores.clone(),
            title_generator: inner.title_generator.clone(),
            session_construction_provider: inner.session_construction_provider.clone(),
            hook_effect_handler_registry: inner.hook_effect_handler_registry.clone(),
            context_audit_bus: inner.context_audit_bus.clone(),
            base_system_prompt: inner.base_system_prompt.clone(),
            user_preferences: inner.user_preferences.clone(),
            runtime_tool_provider: inner.runtime_tool_provider.clone(),
            mcp_relay_provider: inner.mcp_relay_provider.clone(),
            eventing: inner.eventing_service(),
            core: inner.core_service(),
            hooks: inner.hook_service(),
            capability: inner.capability_service(),
            effects: inner.effects_service(),
        }
    }
}

impl SessionLaunchService {
    pub(in crate::session) fn new(inner: SessionRuntimeInner) -> Self {
        Self {
            deps: SessionLaunchDeps::from_inner(&inner),
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
