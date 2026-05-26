use agentdash_relay::{
    CommandExtensionActionInvokePayload, RelayError, RelayMessage,
    ResponseExtensionActionInvokePayload,
};
use serde_json::{Map, json};

use super::CommandHandler;

impl CommandHandler {
    pub(super) async fn handle_extension_action_invoke(
        &self,
        id: String,
        payload: CommandExtensionActionInvokePayload,
    ) -> RelayMessage {
        match self
            .extension_host
            .invoke_action(&payload.action_key, payload.input.clone())
            .await
        {
            Ok(output) => {
                let extension_key = payload.extension_key.clone();
                let extension_id = payload.extension_id.clone();
                let action_key = payload.action_key.clone();
                let mut metadata = Map::new();
                metadata.insert("extension_key".to_string(), json!(extension_key));
                metadata.insert("extension_id".to_string(), json!(extension_id));
                metadata.insert("action_key".to_string(), json!(action_key));
                metadata.insert("project_id".to_string(), json!(payload.project_id));
                metadata.insert("session_id".to_string(), json!(payload.session_id));
                metadata.insert("trace_id".to_string(), json!(payload.trace_id));
                metadata.insert("invocation_id".to_string(), json!(payload.invocation_id));
                RelayMessage::ResponseExtensionActionInvoke {
                    id,
                    payload: Some(ResponseExtensionActionInvokePayload {
                        extension_key,
                        extension_id,
                        action_key,
                        output,
                        metadata,
                    }),
                    error: None,
                }
            }
            Err(error) => RelayMessage::ResponseExtensionActionInvoke {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(error.to_string())),
            },
        }
    }
}
