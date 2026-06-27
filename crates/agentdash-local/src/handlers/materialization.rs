use agentdash_relay::{RelayError, RelayMessage, VfsMaterializePayload};
use std::sync::Arc;

use crate::materialization::MaterializationStore;

#[derive(Clone)]
pub(super) struct MaterializationCommandHandler {
    materialization_store: Arc<MaterializationStore>,
}

impl MaterializationCommandHandler {
    pub(super) fn new(materialization_store: Arc<MaterializationStore>) -> Self {
        Self {
            materialization_store,
        }
    }

    pub(super) async fn handle_vfs_materialize(
        &self,
        id: String,
        payload: VfsMaterializePayload,
    ) -> RelayMessage {
        match self.materialization_store.materialize(payload).await {
            Ok(response) => RelayMessage::ResponseVfsMaterialize {
                id,
                payload: Some(response),
                error: None,
            },
            Err(error) => RelayMessage::ResponseVfsMaterialize {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(error.to_string())),
            },
        }
    }
}
