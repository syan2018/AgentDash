use std::sync::Arc;

use agentdash_domain::workflow::LifecycleGateRepository;
use agentdash_spi::ConnectorError;

use super::eventing::SessionEventingService;
use super::persistence::SessionStoreSet;
use crate::companion::{
    PayloadTypeRegistry, build_companion_human_response_notification, payload_types,
};

#[derive(Clone)]
pub struct SessionControlService {
    stores: SessionStoreSet,
    eventing: SessionEventingService,
    lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
    connector: Arc<dyn agentdash_spi::AgentConnector>,
}

impl SessionControlService {
    pub(super) fn new(
        stores: SessionStoreSet,
        eventing: SessionEventingService,
        lifecycle_gate_repo: Arc<dyn LifecycleGateRepository>,
        connector: Arc<dyn agentdash_spi::AgentConnector>,
    ) -> Self {
        Self {
            stores,
            eventing,
            lifecycle_gate_repo,
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
        if let Some(error) = payload_types::payload_object_error(&payload) {
            return Err(ConnectorError::Runtime(error));
        }

        // 从 LifecycleGate 获取请求元数据（request_type / turn_id）
        let gate_id = uuid::Uuid::parse_str(request_id).ok();
        let gate = if let Some(gid) = gate_id {
            self.lifecycle_gate_repo.get(gid).await.ok().flatten()
        } else {
            None
        };

        let gate_meta: Option<serde_json::Value> =
            gate.as_ref().and_then(|g| g.payload_json.clone());
        let wait_request_type = gate_meta
            .as_ref()
            .and_then(|m| m.get("request_type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let registry = PayloadTypeRegistry::with_builtins();
        if let Some(error) = registry.validate_response(&payload, wait_request_type.as_deref()) {
            return Err(ConnectorError::Runtime(error));
        }

        // Resolve gate（如果存在且仍处于 open 状态）
        let gate_resolved = if let (Some(gid), Some(mut g)) = (gate_id, gate) {
            if g.is_open() {
                g.payload_json = Some(payload.clone());
                g.resolve("companion_respond");
                let _ = self.lifecycle_gate_repo.update(&g).await;
                true
            } else {
                false
            }
        } else {
            false
        };

        let gate_turn_id = gate_meta
            .as_ref()
            .and_then(|m| m.get("turn_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let fallback_turn_id = self
            .stores
            .meta
            .get_session_meta(session_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?
            .and_then(|meta| meta.last_turn_id);
        let turn_id = gate_turn_id.as_deref().or(fallback_turn_id.as_deref());

        let notification = build_companion_human_response_notification(
            session_id,
            turn_id,
            request_id,
            &payload,
            wait_request_type.as_deref(),
            gate_resolved,
        );
        let _ = self
            .eventing
            .inject_notification(session_id, notification)
            .await;

        Ok(())
    }
}
