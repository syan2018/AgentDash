use std::sync::Arc;

use super::hub::SessionHub;
use super::title_generator::SessionTitleGenerator;
use super::types::SessionMeta;

#[derive(Clone)]
pub struct SessionTitleService {
    hub: SessionHub,
}

impl SessionTitleService {
    pub(super) fn new(hub: SessionHub) -> Self {
        Self { hub }
    }

    pub async fn set_user_title(
        &self,
        session_id: &str,
        title: &str,
    ) -> std::io::Result<Option<SessionMeta>> {
        self.hub.set_user_title(session_id, title).await
    }

    pub fn with_title_generator(self, _generator: Arc<dyn SessionTitleGenerator>) -> Self {
        self
    }
}
