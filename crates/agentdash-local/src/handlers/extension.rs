use std::path::PathBuf;

use agentdash_relay::{
    CommandExtensionActionInvokePayload, CommandExtensionBackendServiceInvokePayload,
    CommandExtensionProtocolInvokePayload, ExtensionBackendServiceHttpResponseRelay,
    ExtensionBackendServiceInvokeDiagnosticRelay, ExtensionBackendServiceReadinessRelay,
    ExtensionInvocationWorkspaceRelay, ExtensionPackageArtifactRelay, ExtensionRuntimeHostRelay,
    RelayError, RelayMessage, ResponseExtensionActionInvokePayload,
    ResponseExtensionBackendServiceInvokePayload, ResponseExtensionProtocolInvokePayload,
};
use serde_json::{Map, json};

use super::CommandDispatchPlan;
use crate::{
    ExtensionArtifactDownloadRequest, ExtensionBackendServiceArtifact,
    ExtensionBackendServiceInstanceIdentity, ExtensionBackendServiceInvokeError,
    ExtensionBackendServiceInvokeRequest, ExtensionBackendServiceReadiness,
    ExtensionBackendServiceStartRequest, ExtensionBackendServiceStatus,
    LocalExtensionBackendServiceManager, LocalExtensionHostActivation, LocalExtensionHostManager,
    download_and_cache_extension_artifact,
};

#[derive(Clone)]
pub(super) struct ExtensionCommandHandler {
    backend_id: String,
    workspace_roots: Vec<PathBuf>,
    extension_host: LocalExtensionHostManager,
    backend_services: LocalExtensionBackendServiceManager,
    artifact_api_base_url: String,
    artifact_access_token: String,
    artifact_cache_root: PathBuf,
}

pub(super) struct ExtensionCommandHandlerConfig {
    pub backend_id: String,
    pub workspace_roots: Vec<PathBuf>,
    pub extension_host: LocalExtensionHostManager,
    pub backend_services: LocalExtensionBackendServiceManager,
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
            backend_services: config.backend_services,
            artifact_api_base_url: config.artifact_api_base_url,
            artifact_access_token: config.artifact_access_token,
            artifact_cache_root: config.artifact_cache_root,
        }
    }

    pub(super) fn dispatch_plan(msg: &RelayMessage) -> Option<CommandDispatchPlan> {
        match msg {
            RelayMessage::CommandExtensionActionInvoke { .. }
            | RelayMessage::CommandExtensionProtocolInvoke { .. }
            | RelayMessage::CommandExtensionBackendServiceInvoke { .. } => {
                Some(CommandDispatchPlan::INLINE)
            }
            _ => None,
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
                &payload.execution_id,
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
                metadata.insert("execution_id".to_string(), json!(payload.execution_id));
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

    pub(super) async fn handle_extension_protocol_invoke(
        &self,
        id: String,
        payload: CommandExtensionProtocolInvokePayload,
    ) -> RelayMessage {
        if let Err(error) = self
            .activate_extension_host_from_artifact(
                &payload.project_id,
                &payload.execution_id,
                &payload.provider_extension_key,
                &payload.package_artifact,
                payload.workspace.as_ref(),
            )
            .await
        {
            return RelayMessage::ResponseExtensionProtocolInvoke {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(error)),
            };
        }

        match self
            .extension_host
            .invoke_protocol(
                &payload.protocol_key,
                &payload.method,
                payload.input.clone(),
            )
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
                metadata.insert("protocol_key".to_string(), json!(payload.protocol_key));
                metadata.insert(
                    "protocol_version".to_string(),
                    json!(payload.protocol_version),
                );
                metadata.insert("method".to_string(), json!(payload.method));
                metadata.insert("project_id".to_string(), json!(payload.project_id));
                metadata.insert("execution_id".to_string(), json!(payload.execution_id));
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
                RelayMessage::ResponseExtensionProtocolInvoke {
                    id,
                    payload: Some(ResponseExtensionProtocolInvokePayload {
                        provider_extension_key: payload.provider_extension_key,
                        provider_extension_id: payload.provider_extension_id,
                        protocol_key: payload.protocol_key,
                        protocol_version: payload.protocol_version,
                        method: payload.method,
                        output,
                        metadata,
                    }),
                    error: None,
                }
            }
            Err(error) => RelayMessage::ResponseExtensionProtocolInvoke {
                id,
                payload: None,
                error: Some(RelayError::runtime_error(error.to_string())),
            },
        }
    }

    pub(super) async fn handle_extension_backend_service_invoke(
        &self,
        id: String,
        payload: CommandExtensionBackendServiceInvokePayload,
    ) -> RelayMessage {
        let metadata = payload.metadata.clone();
        let cache_entry =
            match download_and_cache_extension_artifact(ExtensionArtifactDownloadRequest {
                api_base_url: self.artifact_api_base_url.clone(),
                access_token: self.artifact_access_token.clone(),
                project_id: metadata.project_id.clone(),
                artifact_id: payload.package_artifact.artifact_id.clone(),
                archive_digest: payload.package_artifact.archive_digest.clone(),
                cache_root: self.artifact_cache_root.clone(),
            })
            .await
            {
                Ok(cache_entry) => cache_entry,
                Err(error) => {
                    return RelayMessage::ResponseExtensionBackendServiceInvoke {
                        id,
                        payload: Some(ResponseExtensionBackendServiceInvokePayload {
                            metadata,
                            response: None,
                            diagnostic: Some(ExtensionBackendServiceInvokeDiagnosticRelay {
                                readiness: ExtensionBackendServiceReadinessRelay::MissingArtifact,
                                code: "artifact_unavailable".to_string(),
                                message: error.to_string(),
                                retryable: true,
                                details: None,
                            }),
                        }),
                        error: None,
                    };
                }
            };

        let identity = ExtensionBackendServiceInstanceIdentity {
            project_id: metadata.project_id.clone(),
            backend_id: metadata.backend_id.clone(),
            extension_key: metadata.extension_key.clone(),
            service_key: metadata.service_key.clone(),
            artifact_id: payload.package_artifact.artifact_id.clone(),
            archive_digest: payload.package_artifact.archive_digest.clone(),
        };
        let status = self
            .backend_services
            .start(ExtensionBackendServiceStartRequest {
                project_id: metadata.project_id.clone(),
                backend_id: metadata.backend_id.clone(),
                extension_key: metadata.extension_key.clone(),
                extension_id: metadata.extension_id.clone(),
                service_key: metadata.service_key.clone(),
                artifact: Some(ExtensionBackendServiceArtifact {
                    artifact_id: payload.package_artifact.artifact_id,
                    archive_digest: payload.package_artifact.archive_digest,
                }),
                cache_entry: Some(cache_entry),
            })
            .await;
        if status.readiness != ExtensionBackendServiceReadiness::Ready {
            return RelayMessage::ResponseExtensionBackendServiceInvoke {
                id,
                payload: Some(ResponseExtensionBackendServiceInvokePayload {
                    metadata,
                    response: None,
                    diagnostic: Some(diagnostic_from_status(&status)),
                }),
                error: None,
            };
        }

        match self
            .backend_services
            .invoke(ExtensionBackendServiceInvokeRequest {
                identity,
                extension_id: metadata.extension_id.clone(),
                route: metadata.route.clone(),
                method: payload.method,
                headers: payload.headers,
                body: payload.body,
                trace_id: metadata.trace_id.clone(),
            })
            .await
        {
            Ok(response) => RelayMessage::ResponseExtensionBackendServiceInvoke {
                id,
                payload: Some(ResponseExtensionBackendServiceInvokePayload {
                    metadata,
                    response: Some(ExtensionBackendServiceHttpResponseRelay {
                        status: response.status,
                        headers: response.headers,
                        body: response.body,
                    }),
                    diagnostic: None,
                }),
                error: None,
            },
            Err(error) => RelayMessage::ResponseExtensionBackendServiceInvoke {
                id,
                payload: Some(ResponseExtensionBackendServiceInvokePayload {
                    metadata,
                    response: None,
                    diagnostic: Some(diagnostic_from_invoke_error(error)),
                }),
                error: None,
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
                    execution_id: Some(payload.execution_id.clone()),
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
        execution_id: &str,
        runtime_extensions: &[ExtensionRuntimeHostRelay],
        workspace: Option<&ExtensionInvocationWorkspaceRelay>,
    ) -> Result<(), String> {
        for extension in runtime_extensions {
            let Some(artifact) = extension.package_artifact.as_ref() else {
                continue;
            };
            self.activate_extension_host_from_artifact(
                project_id,
                execution_id,
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
        execution_id: &str,
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
                    execution_id: Some(execution_id.to_string()),
                    default_workspace_root: workspace_root_from_relay(workspace),
                    workspace_roots: self.workspace_roots.clone(),
                },
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn diagnostic_from_invoke_error(
    error: ExtensionBackendServiceInvokeError,
) -> ExtensionBackendServiceInvokeDiagnosticRelay {
    match error {
        ExtensionBackendServiceInvokeError::Unavailable { status } => {
            diagnostic_from_status(&status)
        }
        ExtensionBackendServiceInvokeError::InvalidRequest(message) => {
            ExtensionBackendServiceInvokeDiagnosticRelay {
                readiness: ExtensionBackendServiceReadinessRelay::ServiceUnavailable,
                code: "invalid_request".to_string(),
                message,
                retryable: false,
                details: None,
            }
        }
        ExtensionBackendServiceInvokeError::Http(message) => {
            ExtensionBackendServiceInvokeDiagnosticRelay {
                readiness: ExtensionBackendServiceReadinessRelay::HealthFailed,
                code: "backend_service_http_failed".to_string(),
                message,
                retryable: true,
                details: None,
            }
        }
    }
}

fn diagnostic_from_status(
    status: &ExtensionBackendServiceStatus,
) -> ExtensionBackendServiceInvokeDiagnosticRelay {
    ExtensionBackendServiceInvokeDiagnosticRelay {
        readiness: readiness_to_relay(status.readiness.clone()),
        code: readiness_code(&status.readiness).to_string(),
        message: status
            .message
            .clone()
            .unwrap_or_else(|| "backend service is not ready".to_string()),
        retryable: matches!(
            status.readiness,
            ExtensionBackendServiceReadiness::MissingArtifact
                | ExtensionBackendServiceReadiness::MaterializeFailed
                | ExtensionBackendServiceReadiness::Starting
                | ExtensionBackendServiceReadiness::HealthFailed
        ),
        details: Some(json!({
            "project_id": status.identity.project_id,
            "backend_id": status.identity.backend_id,
            "extension_key": status.identity.extension_key,
            "service_key": status.identity.service_key,
            "endpoint": status.endpoint,
            "pid": status.pid,
            "updated_at": status.updated_at,
        })),
    }
}

fn readiness_to_relay(
    readiness: ExtensionBackendServiceReadiness,
) -> ExtensionBackendServiceReadinessRelay {
    match readiness {
        ExtensionBackendServiceReadiness::MissingArtifact => {
            ExtensionBackendServiceReadinessRelay::MissingArtifact
        }
        ExtensionBackendServiceReadiness::MaterializeFailed => {
            ExtensionBackendServiceReadinessRelay::MaterializeFailed
        }
        ExtensionBackendServiceReadiness::Starting => {
            ExtensionBackendServiceReadinessRelay::Starting
        }
        ExtensionBackendServiceReadiness::HealthFailed => {
            ExtensionBackendServiceReadinessRelay::HealthFailed
        }
        ExtensionBackendServiceReadiness::Ready => ExtensionBackendServiceReadinessRelay::Ready,
        ExtensionBackendServiceReadiness::ProcessExited => {
            ExtensionBackendServiceReadinessRelay::ProcessExited
        }
        ExtensionBackendServiceReadiness::UnsupportedRuntime => {
            ExtensionBackendServiceReadinessRelay::UnsupportedRuntime
        }
    }
}

fn readiness_code(readiness: &ExtensionBackendServiceReadiness) -> &'static str {
    match readiness {
        ExtensionBackendServiceReadiness::MissingArtifact => "missing_artifact",
        ExtensionBackendServiceReadiness::MaterializeFailed => "materialize_failed",
        ExtensionBackendServiceReadiness::Starting => "starting",
        ExtensionBackendServiceReadiness::HealthFailed => "health_failed",
        ExtensionBackendServiceReadiness::Ready => "ready",
        ExtensionBackendServiceReadiness::ProcessExited => "process_exited",
        ExtensionBackendServiceReadiness::UnsupportedRuntime => "unsupported_runtime",
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
