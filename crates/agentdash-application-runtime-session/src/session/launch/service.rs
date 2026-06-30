use agentdash_application_ports::launch::{LaunchCommand, LaunchPlanningInput};
use agentdash_spi::ConnectorError;

use super::{LaunchCommandOutcome, SessionLaunchDeps, SessionLaunchOrchestrator};
use crate::session::hub::SessionRuntimeInner;

#[derive(Clone)]
pub struct SessionLaunchService {
    deps: SessionLaunchDeps,
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
        planning_input: LaunchPlanningInput,
    ) -> Result<String, ConnectorError> {
        Ok(self
            .launch_command_with_outcome(session_id, command, planning_input)
            .await?
            .turn_id)
    }

    pub async fn launch_command_in_task(
        &self,
        session_id: String,
        command: LaunchCommand,
        planning_input: LaunchPlanningInput,
    ) -> Result<String, ConnectorError> {
        let service = self.clone();
        tokio::spawn(async move {
            service
                .launch_command(&session_id, command, planning_input)
                .await
        })
        .await
        .map_err(|error| {
            ConnectorError::Runtime(format!("session launch task join failed: {error}"))
        })?
    }

    pub async fn launch_command_with_outcome(
        &self,
        session_id: &str,
        command: LaunchCommand,
        planning_input: LaunchPlanningInput,
    ) -> Result<LaunchCommandOutcome, ConnectorError> {
        SessionLaunchOrchestrator::new(self.deps.clone())
            .launch(session_id, command, planning_input)
            .await
    }
}
