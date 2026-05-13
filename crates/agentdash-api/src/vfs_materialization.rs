use std::sync::Arc;

use agentdash_application::vfs::VfsMaterializationTransport;
use agentdash_relay::{RelayMessage, VfsMaterializePayload, VfsMaterializeResponse};
use async_trait::async_trait;

use crate::relay::registry::BackendRegistry;

pub struct RelayVfsMaterializationTransport {
    backends: Arc<BackendRegistry>,
}

impl RelayVfsMaterializationTransport {
    pub fn new(backends: Arc<BackendRegistry>) -> Self {
        Self { backends }
    }
}

#[async_trait]
impl VfsMaterializationTransport for RelayVfsMaterializationTransport {
    async fn materialize(
        &self,
        backend_id: &str,
        payload: VfsMaterializePayload,
    ) -> Result<VfsMaterializeResponse, String> {
        let response = self
            .backends
            .send_command(
                backend_id,
                RelayMessage::CommandVfsMaterialize {
                    id: RelayMessage::new_id("vfs-materialize"),
                    payload: Box::new(payload),
                },
            )
            .await
            .map_err(|error| error.to_string())?;

        match response {
            RelayMessage::ResponseVfsMaterialize {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(payload),
            RelayMessage::ResponseVfsMaterialize {
                error: Some(error), ..
            } => Err(error.message),
            other => Err(format!("vfs.materialize 返回意外响应: {}", other.id())),
        }
    }
}
