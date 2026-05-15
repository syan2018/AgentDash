use agentdash_spi::ConnectorError;

use super::SessionHub;

impl SessionHub {
    pub async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError> {
        self.runtime_service().cancel(session_id).await
    }
}
