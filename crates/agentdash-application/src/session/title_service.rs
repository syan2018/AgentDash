use std::sync::Arc;

use agentdash_agent_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo};

use super::core::SessionCoreService;
use super::eventing::SessionEventingService;
use super::title_generator::SessionTitleGenerator;
use super::types::{SessionMeta, TitleSource};

#[derive(Clone)]
pub struct SessionTitleService {
    core: SessionCoreService,
    eventing: SessionEventingService,
}

impl SessionTitleService {
    pub(super) fn new(core: SessionCoreService, eventing: SessionEventingService) -> Self {
        Self { core, eventing }
    }

    pub async fn set_user_title(
        &self,
        session_id: &str,
        title: &str,
    ) -> std::io::Result<Option<SessionMeta>> {
        let updated = self
            .core
            .update_session_meta(session_id, |meta| {
                meta.title = title.to_string();
                meta.title_source = TitleSource::User;
            })
            .await?;

        if let Some(ref meta) = updated {
            self.broadcast_session_meta_updated(session_id, meta).await;
        }
        Ok(updated)
    }

    pub fn with_title_generator(self, _generator: Arc<dyn SessionTitleGenerator>) -> Self {
        self
    }

    async fn broadcast_session_meta_updated(&self, session_id: &str, meta: &SessionMeta) {
        let source = SourceInfo {
            connector_id: "agentdash-server".to_string(),
            connector_type: "system".to_string(),
            executor_id: None,
        };

        let value = serde_json::json!({
            "title": meta.title,
            "title_source": meta.title_source,
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
