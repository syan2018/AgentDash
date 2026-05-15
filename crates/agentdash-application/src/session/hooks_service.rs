use std::path::Path;

use agentdash_spi::ConnectorError;
use agentdash_spi::hooks::SharedHookSessionRuntime;

use super::hub::SessionHub;

#[derive(Clone)]
pub struct SessionHookService {
    hub: SessionHub,
}

impl SessionHookService {
    pub(super) fn new(hub: SessionHub) -> Self {
        Self { hub }
    }

    pub async fn ensure_hook_session_runtime(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        self.hub
            .ensure_hook_session_runtime(session_id, turn_id)
            .await
    }

    pub async fn get_hook_session_runtime(
        &self,
        session_id: &str,
    ) -> Option<SharedHookSessionRuntime> {
        self.hub.get_hook_session_runtime(session_id).await
    }

    pub async fn reload_session_hook_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        executor: &str,
        permission_policy: Option<&str>,
        working_directory: &Path,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        self.hub
            .reload_session_hook_runtime(
                session_id,
                turn_id,
                executor,
                permission_policy,
                working_directory,
            )
            .await
    }
}
