use std::sync::Arc;

use agentdash_spi::ConnectorError;

#[derive(Clone)]
pub struct SessionControlService {
    connector: Arc<dyn agentdash_spi::AgentConnector>,
}

impl SessionControlService {
    pub(super) fn new(connector: Arc<dyn agentdash_spi::AgentConnector>) -> Self {
        Self { connector }
    }

    pub async fn push_session_notification(
        &self,
        session_id: &str,
        message: String,
    ) -> Result<(), ConnectorError> {
        self.connector
            .push_session_notification(session_id, message)
            .await
    }

    pub async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        self.connector
            .approve_tool_call(session_id, tool_call_id)
            .await
    }

    pub async fn reject_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        self.connector
            .reject_tool_call(session_id, tool_call_id, reason)
            .await
    }
}
