use std::sync::Arc;

use agentdash_agent_protocol::UserInputBlock;
use agentdash_spi::ConnectorError;

#[derive(Clone)]
pub struct SessionControlService {
    connector: Arc<dyn agentdash_spi::AgentConnector>,
}

#[derive(Debug, Clone)]
pub struct SessionTurnSteerCommand {
    pub session_id: String,
    pub expected_turn_id: String,
    pub input: Vec<UserInputBlock>,
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

    pub async fn steer_session(
        &self,
        command: SessionTurnSteerCommand,
    ) -> Result<(), ConnectorError> {
        self.connector
            .steer_session(
                &command.session_id,
                &command.expected_turn_id,
                command.input,
            )
            .await
    }

    pub async fn supports_session_steering(&self, session_id: &str) -> bool {
        self.connector.supports_session_steering(session_id).await
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
