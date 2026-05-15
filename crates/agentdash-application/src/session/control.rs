use std::sync::Arc;

use agentdash_spi::ConnectorError;

use super::companion_wait::CompanionWaitRegistry;
use super::eventing::SessionEventingService;
use super::persistence::SessionStoreSet;
use crate::companion::build_companion_human_response_notification;

#[derive(Clone)]
pub struct SessionControlService {
    stores: SessionStoreSet,
    eventing: SessionEventingService,
    companion_wait_registry: CompanionWaitRegistry,
    connector: Arc<dyn agentdash_spi::AgentConnector>,
}

impl SessionControlService {
    pub(super) fn new(
        stores: SessionStoreSet,
        eventing: SessionEventingService,
        companion_wait_registry: CompanionWaitRegistry,
        connector: Arc<dyn agentdash_spi::AgentConnector>,
    ) -> Self {
        Self {
            stores,
            eventing,
            companion_wait_registry,
            connector,
        }
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

    pub async fn respond_companion_request(
        &self,
        session_id: &str,
        request_id: &str,
        payload: serde_json::Value,
    ) -> Result<(), ConnectorError> {
        let resolved = self
            .companion_wait_registry
            .resolve(session_id, request_id, payload.clone())
            .await;

        let fallback_turn_id = self
            .stores
            .meta
            .get_session_meta(session_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?
            .and_then(|meta| meta.last_turn_id);
        let turn_id = resolved
            .as_ref()
            .map(|result| result.turn_id.as_str())
            .or(fallback_turn_id.as_deref());

        let request_type = resolved
            .as_ref()
            .and_then(|result| result.request_type.as_deref());

        let notification = build_companion_human_response_notification(
            session_id,
            turn_id,
            request_id,
            &payload,
            request_type,
            resolved.is_some(),
        );
        let _ = self
            .eventing
            .inject_notification(session_id, notification)
            .await;

        Ok(())
    }
}
