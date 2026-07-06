use std::sync::Arc;

use agentdash_agent_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo};
use agentdash_application_ports::workspace_title::WorkspaceTitlePort;

use super::eventing::SessionEventingService;

#[derive(Clone)]
pub struct SessionTitleService {
    eventing: SessionEventingService,
    workspace_title_port: Option<Arc<dyn WorkspaceTitlePort>>,
}

impl SessionTitleService {
    pub(super) fn new(
        eventing: SessionEventingService,
        workspace_title_port: Option<Arc<dyn WorkspaceTitlePort>>,
    ) -> Self {
        Self {
            eventing,
            workspace_title_port,
        }
    }

    pub async fn set_user_title(
        &self,
        session_id: &str,
        title: &str,
    ) -> std::io::Result<bool> {
        let updated = self.update_workspace_title(session_id, title, "user").await;

        if updated {
            self.broadcast_workspace_title_updated(session_id, title, "user")
                .await;
        }
        Ok(updated)
    }

    async fn update_workspace_title(
        &self,
        session_id: &str,
        title: &str,
        title_source: &str,
    ) -> bool {
        let Some(port) = self.workspace_title_port.as_ref() else {
            return false;
        };
        port.update_workspace_title(session_id, title.to_string(), title_source)
            .await
            .unwrap_or_default()
    }

    async fn broadcast_workspace_title_updated(
        &self,
        session_id: &str,
        title: &str,
        title_source: &str,
    ) {
        let source = SourceInfo {
            connector_id: "agentdash-server".to_string(),
            connector_type: "system".to_string(),
            executor_id: None,
        };

        let value = serde_json::json!({
            "title": title,
            "title_source": title_source,
        });

        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "session_meta_updated".to_string(),
                value,
            }),
            session_id,
            source,
        );
        let _ = self
            .eventing
            .persist_notification(session_id, envelope)
            .await;
    }
}
