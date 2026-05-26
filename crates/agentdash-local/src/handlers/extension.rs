use agentdash_relay::{
    CommandExtensionActionInvokePayload, RelayError, RelayMessage,
    ResponseExtensionActionInvokePayload,
};
use serde_json::{Map, json};

use super::CommandHandler;
use crate::{
    ExtensionArtifactDownloadRequest, LocalExtensionHostActivation,
    download_and_cache_extension_artifact,
};

impl CommandHandler {
    pub(super) async fn handle_extension_action_invoke(
        &self,
        id: String,
        payload: CommandExtensionActionInvokePayload,
    ) -> RelayMessage {
        let activation = self.ensure_extension_host_activation(&payload).await;
        if let Err(error) = activation {
            return RelayMessage::ResponseExtensionActionInvoke {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(error)),
            };
        }

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

    async fn ensure_extension_host_activation(
        &self,
        payload: &CommandExtensionActionInvokePayload,
    ) -> Result<(), String> {
        let Some(artifact) = payload.package_artifact.as_ref() else {
            return Ok(());
        };
        let cache_entry = download_and_cache_extension_artifact(ExtensionArtifactDownloadRequest {
            api_base_url: self.extension_artifact_api_base_url.clone(),
            access_token: self.extension_artifact_access_token.clone(),
            project_id: payload.project_id.clone(),
            artifact_id: artifact.artifact_id.clone(),
            archive_digest: artifact.archive_digest.clone(),
            cache_root: self.extension_artifact_cache_root.clone(),
        })
        .await
        .map_err(|error| error.to_string())?;

        self.extension_host
            .activate_cached_artifact(
                &cache_entry,
                LocalExtensionHostActivation {
                    extension_key: payload.extension_key.clone(),
                    backend_id: self.backend_id.clone(),
                    project_id: Some(payload.project_id.clone()),
                    session_id: Some(payload.session_id.clone()),
                    workspace_roots: self.workspace_roots.clone(),
                },
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
