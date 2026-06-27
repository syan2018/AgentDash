use std::path::PathBuf;

use agentdash_relay::{
    CommandExtensionActionInvokePayload, CommandExtensionChannelInvokePayload,
    ExtensionInvocationWorkspaceRelay, ExtensionPackageArtifactRelay, ExtensionRuntimeHostRelay,
    RelayError, RelayMessage, ResponseExtensionActionInvokePayload,
    ResponseExtensionChannelInvokePayload,
};
use serde_json::{Map, json};

use crate::{
    ExtensionArtifactDownloadRequest, LocalExtensionHostActivation, LocalExtensionHostManager,
    download_and_cache_extension_artifact,
};

#[derive(Clone)]
pub(super) struct ExtensionCommandHandler {
    backend_id: String,
    workspace_roots: Vec<PathBuf>,
    extension_host: LocalExtensionHostManager,
    artifact_api_base_url: String,
    artifact_access_token: String,
    artifact_cache_root: PathBuf,
}

pub(super) struct ExtensionCommandHandlerConfig {
    pub backend_id: String,
    pub workspace_roots: Vec<PathBuf>,
    pub extension_host: LocalExtensionHostManager,
    pub artifact_api_base_url: String,
    pub artifact_access_token: String,
    pub artifact_cache_root: PathBuf,
}

impl ExtensionCommandHandler {
    pub(super) fn new(config: ExtensionCommandHandlerConfig) -> Self {
        Self {
            backend_id: config.backend_id,
            workspace_roots: config.workspace_roots,
            extension_host: config.extension_host,
            artifact_api_base_url: config.artifact_api_base_url,
            artifact_access_token: config.artifact_access_token,
            artifact_cache_root: config.artifact_cache_root,
        }
    }

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
        if let Err(error) = self
            .ensure_runtime_extension_hosts_activation(
                &payload.project_id,
                &payload.session_id,
                &payload.runtime_extensions,
                payload.workspace.as_ref(),
            )
            .await
        {
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

    pub(super) async fn handle_extension_channel_invoke(
        &self,
        id: String,
        payload: CommandExtensionChannelInvokePayload,
    ) -> RelayMessage {
        if let Err(error) = self
            .activate_extension_host_from_artifact(
                &payload.project_id,
                &payload.session_id,
                &payload.provider_extension_key,
                &payload.package_artifact,
                payload.workspace.as_ref(),
            )
            .await
        {
            return RelayMessage::ResponseExtensionChannelInvoke {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(error)),
            };
        }

        match self
            .extension_host
            .invoke_channel(&payload.channel_key, &payload.method, payload.input.clone())
            .await
        {
            Ok(output) => {
                let mut metadata = Map::new();
                metadata.insert(
                    "provider_extension_key".to_string(),
                    json!(payload.provider_extension_key),
                );
                metadata.insert(
                    "provider_extension_id".to_string(),
                    json!(payload.provider_extension_id),
                );
                metadata.insert("channel_key".to_string(), json!(payload.channel_key));
                metadata.insert("method".to_string(), json!(payload.method));
                metadata.insert("project_id".to_string(), json!(payload.project_id));
                metadata.insert("session_id".to_string(), json!(payload.session_id));
                metadata.insert("trace_id".to_string(), json!(payload.trace_id));
                metadata.insert("invocation_id".to_string(), json!(payload.invocation_id));
                metadata.insert("consumer_kind".to_string(), json!(payload.consumer.kind));
                if let Some(extension_key) = payload.consumer.extension_key {
                    metadata.insert("consumer_extension_key".to_string(), json!(extension_key));
                }
                if let Some(extension_id) = payload.consumer.extension_id {
                    metadata.insert("consumer_extension_id".to_string(), json!(extension_id));
                }
                if let Some(alias) = payload.consumer.dependency_alias {
                    metadata.insert("dependency_alias".to_string(), json!(alias));
                }
                RelayMessage::ResponseExtensionChannelInvoke {
                    id,
                    payload: Some(ResponseExtensionChannelInvokePayload {
                        provider_extension_key: payload.provider_extension_key,
                        provider_extension_id: payload.provider_extension_id,
                        channel_key: payload.channel_key,
                        method: payload.method,
                        output,
                        metadata,
                    }),
                    error: None,
                }
            }
            Err(error) => RelayMessage::ResponseExtensionChannelInvoke {
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
            api_base_url: self.artifact_api_base_url.clone(),
            access_token: self.artifact_access_token.clone(),
            project_id: payload.project_id.clone(),
            artifact_id: artifact.artifact_id.clone(),
            archive_digest: artifact.archive_digest.clone(),
            cache_root: self.artifact_cache_root.clone(),
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
                    default_workspace_root: workspace_root_from_relay(payload.workspace.as_ref()),
                    workspace_roots: self.workspace_roots.clone(),
                },
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    async fn ensure_runtime_extension_hosts_activation(
        &self,
        project_id: &str,
        session_id: &str,
        runtime_extensions: &[ExtensionRuntimeHostRelay],
        workspace: Option<&ExtensionInvocationWorkspaceRelay>,
    ) -> Result<(), String> {
        for extension in runtime_extensions {
            let Some(artifact) = extension.package_artifact.as_ref() else {
                continue;
            };
            self.activate_extension_host_from_artifact(
                project_id,
                session_id,
                &extension.extension_key,
                artifact,
                workspace,
            )
            .await?;
        }
        Ok(())
    }

    async fn activate_extension_host_from_artifact(
        &self,
        project_id: &str,
        session_id: &str,
        extension_key: &str,
        artifact: &ExtensionPackageArtifactRelay,
        workspace: Option<&ExtensionInvocationWorkspaceRelay>,
    ) -> Result<(), String> {
        let cache_entry = download_and_cache_extension_artifact(ExtensionArtifactDownloadRequest {
            api_base_url: self.artifact_api_base_url.clone(),
            access_token: self.artifact_access_token.clone(),
            project_id: project_id.to_string(),
            artifact_id: artifact.artifact_id.clone(),
            archive_digest: artifact.archive_digest.clone(),
            cache_root: self.artifact_cache_root.clone(),
        })
        .await
        .map_err(|error| error.to_string())?;

        self.extension_host
            .activate_cached_artifact(
                &cache_entry,
                LocalExtensionHostActivation {
                    extension_key: extension_key.to_string(),
                    backend_id: self.backend_id.clone(),
                    project_id: Some(project_id.to_string()),
                    session_id: Some(session_id.to_string()),
                    default_workspace_root: workspace_root_from_relay(workspace),
                    workspace_roots: self.workspace_roots.clone(),
                },
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn workspace_root_from_relay(
    workspace: Option<&ExtensionInvocationWorkspaceRelay>,
) -> Option<PathBuf> {
    workspace.and_then(|workspace| {
        let root_ref = workspace.root_ref.trim();
        if root_ref.is_empty() {
            None
        } else {
            Some(PathBuf::from(root_ref))
        }
    })
}
