use agentdash_spi::ConnectorError;

use super::hub::SessionHub;
use super::launch::{LaunchCommand, LaunchCommandOutcome};
use super::prompt_pipeline::SessionLaunchExecutor;

#[derive(Clone)]
pub struct SessionLaunchService {
    hub: SessionHub,
}

impl SessionLaunchService {
    pub(super) fn new(hub: SessionHub) -> Self {
        Self { hub }
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
        SessionLaunchExecutor::new(&self.hub)
            .execute_command(session_id, command)
            .await
    }
}
