use agentdash_relay::{RelayError, RelayMessage, VfsMaterializePayload};

use super::CommandHandler;

impl CommandHandler {
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
