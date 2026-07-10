use std::time::Duration;

use agentdash_application_ports::extension_runtime::{
    ExtensionActionInvokeRequest, ExtensionActionInvokeResponse,
    ExtensionBackendServiceHttpResponsePayload, ExtensionBackendServiceInvokeDiagnosticPayload,
    ExtensionBackendServiceInvokeMetadataPayload, ExtensionBackendServiceInvokeRequest,
    ExtensionBackendServiceInvokeResponse, ExtensionBackendServiceReadinessPayload,
    ExtensionBackendServiceTransport, ExtensionInvocationWorkspacePayload,
    ExtensionPackageArtifactPayload, ExtensionProtocolConsumerPayload,
    ExtensionProtocolInvokeRequest, ExtensionProtocolInvokeResponse,
    ExtensionRuntimeActionTransport, ExtensionRuntimeActionTransportError,
    ExtensionRuntimeHostPayload, ExtensionRuntimeProtocolTransport,
};
use agentdash_relay::{
    CommandExtensionActionInvokePayload, CommandExtensionBackendServiceInvokePayload,
    CommandExtensionProtocolInvokePayload, ExtensionBackendServiceHttpResponseRelay,
    ExtensionBackendServiceInvokeDiagnosticRelay, ExtensionBackendServiceInvokeMetadataRelay,
    ExtensionBackendServiceReadinessRelay, ExtensionInvocationWorkspaceRelay,
    ExtensionPackageArtifactRelay, ExtensionProtocolConsumerRelay, ExtensionRuntimeHostRelay,
    RelayMessage, ResponseExtensionActionInvokePayload,
    ResponseExtensionBackendServiceInvokePayload, ResponseExtensionProtocolInvokePayload,
};
use async_trait::async_trait;

use super::registry::{BackendCommandError, BackendRegistry};

#[async_trait]
impl ExtensionRuntimeActionTransport for BackendRegistry {
    async fn invoke_extension_action(
        &self,
        backend_id: &str,
        request: ExtensionActionInvokeRequest,
    ) -> Result<ExtensionActionInvokeResponse, ExtensionRuntimeActionTransportError> {
        let command = RelayMessage::CommandExtensionActionInvoke {
            id: RelayMessage::new_id("ext-action"),
            payload: action_request_to_relay(request),
        };
        let response = self
            .send_command_with_timeout(backend_id, command, Duration::from_secs(30))
            .await
            .map_err(transport_error_from_backend)?;
        match response {
            RelayMessage::ResponseExtensionActionInvoke {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(action_response_from_relay(payload)),
            RelayMessage::ResponseExtensionActionInvoke {
                error: Some(error), ..
            } => Err(ExtensionRuntimeActionTransportError::Failed(
                error.to_string(),
            )),
            other => Err(ExtensionRuntimeActionTransportError::Failed(format!(
                "unexpected extension action relay response: {}",
                other.id()
            ))),
        }
    }
}

#[async_trait]
impl ExtensionRuntimeProtocolTransport for BackendRegistry {
    async fn invoke_extension_protocol(
        &self,
        backend_id: &str,
        request: ExtensionProtocolInvokeRequest,
    ) -> Result<ExtensionProtocolInvokeResponse, ExtensionRuntimeActionTransportError> {
        let command = RelayMessage::CommandExtensionProtocolInvoke {
            id: RelayMessage::new_id("ext-channel"),
            payload: channel_request_to_relay(request),
        };
        let response = self
            .send_command_with_timeout(backend_id, command, Duration::from_secs(30))
            .await
            .map_err(transport_error_from_backend)?;
        match response {
            RelayMessage::ResponseExtensionProtocolInvoke {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(channel_response_from_relay(payload)),
            RelayMessage::ResponseExtensionProtocolInvoke {
                error: Some(error), ..
            } => Err(ExtensionRuntimeActionTransportError::Failed(
                error.to_string(),
            )),
            other => Err(ExtensionRuntimeActionTransportError::Failed(format!(
                "unexpected extension protocol relay response: {}",
                other.id()
            ))),
        }
    }
}

#[async_trait]
impl ExtensionBackendServiceTransport for BackendRegistry {
    async fn invoke_extension_backend_service(
        &self,
        backend_id: &str,
        request: ExtensionBackendServiceInvokeRequest,
    ) -> Result<ExtensionBackendServiceInvokeResponse, ExtensionRuntimeActionTransportError> {
        let command = RelayMessage::CommandExtensionBackendServiceInvoke {
            id: RelayMessage::new_id("ext-backend-service"),
            payload: backend_service_request_to_relay(backend_id, request),
        };
        let response = self
            .send_command_with_timeout(backend_id, command, Duration::from_secs(30))
            .await
            .map_err(transport_error_from_backend)?;
        match response {
            RelayMessage::ResponseExtensionBackendServiceInvoke {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(backend_service_response_from_relay(payload)),
            RelayMessage::ResponseExtensionBackendServiceInvoke {
                error: Some(error), ..
            } => Err(ExtensionRuntimeActionTransportError::Failed(
                error.to_string(),
            )),
            other => Err(ExtensionRuntimeActionTransportError::Failed(format!(
                "unexpected extension backend service relay response: {}",
                other.id()
            ))),
        }
    }
}

fn action_request_to_relay(
    request: ExtensionActionInvokeRequest,
) -> CommandExtensionActionInvokePayload {
    CommandExtensionActionInvokePayload {
        extension_key: request.extension_key,
        extension_id: request.extension_id,
        action_key: request.action_key,
        project_id: request.project_id,
        session_id: request.session_id,
        input: request.input,
        package_artifact: request.package_artifact.map(package_artifact_to_relay),
        runtime_extensions: request
            .runtime_extensions
            .into_iter()
            .map(runtime_host_to_relay)
            .collect(),
        workspace: request.workspace.map(workspace_to_relay),
        trace_id: request.trace_id,
        invocation_id: request.invocation_id,
    }
}

fn action_response_from_relay(
    response: ResponseExtensionActionInvokePayload,
) -> ExtensionActionInvokeResponse {
    ExtensionActionInvokeResponse {
        extension_key: response.extension_key,
        extension_id: response.extension_id,
        action_key: response.action_key,
        output: response.output,
        metadata: response.metadata,
    }
}

fn channel_request_to_relay(
    request: ExtensionProtocolInvokeRequest,
) -> CommandExtensionProtocolInvokePayload {
    CommandExtensionProtocolInvokePayload {
        provider_extension_key: request.provider_extension_key,
        provider_extension_id: request.provider_extension_id,
        protocol_key: request.protocol_key,
        protocol_version: request.protocol_version,
        method: request.method,
        project_id: request.project_id,
        session_id: request.session_id,
        input: request.input,
        package_artifact: package_artifact_to_relay(request.package_artifact),
        consumer: consumer_to_relay(request.consumer),
        workspace: request.workspace.map(workspace_to_relay),
        trace_id: request.trace_id,
        invocation_id: request.invocation_id,
    }
}

fn channel_response_from_relay(
    response: ResponseExtensionProtocolInvokePayload,
) -> ExtensionProtocolInvokeResponse {
    ExtensionProtocolInvokeResponse {
        provider_extension_key: response.provider_extension_key,
        provider_extension_id: response.provider_extension_id,
        protocol_key: response.protocol_key,
        protocol_version: response.protocol_version,
        method: response.method,
        output: response.output,
        metadata: response.metadata,
    }
}

fn backend_service_request_to_relay(
    backend_id: &str,
    request: ExtensionBackendServiceInvokeRequest,
) -> CommandExtensionBackendServiceInvokePayload {
    CommandExtensionBackendServiceInvokePayload {
        metadata: ExtensionBackendServiceInvokeMetadataRelay {
            project_id: request.project_id,
            backend_id: backend_id.to_string(),
            extension_key: request.extension_key,
            extension_id: request.extension_id,
            service_key: request.service_key,
            route: request.route,
            trace_id: request.trace_id,
            invocation_id: request.invocation_id,
        },
        session_id: request.session_id,
        method: request.method,
        headers: request.headers,
        body: request.body,
        package_artifact: package_artifact_to_relay(request.package_artifact),
        workspace: request.workspace.map(workspace_to_relay),
    }
}

fn backend_service_response_from_relay(
    response: ResponseExtensionBackendServiceInvokePayload,
) -> ExtensionBackendServiceInvokeResponse {
    ExtensionBackendServiceInvokeResponse {
        metadata: backend_service_metadata_from_relay(response.metadata),
        response: response
            .response
            .map(backend_service_http_response_from_relay),
        diagnostic: response
            .diagnostic
            .map(backend_service_diagnostic_from_relay),
    }
}

fn backend_service_metadata_from_relay(
    metadata: ExtensionBackendServiceInvokeMetadataRelay,
) -> ExtensionBackendServiceInvokeMetadataPayload {
    ExtensionBackendServiceInvokeMetadataPayload {
        project_id: metadata.project_id,
        backend_id: metadata.backend_id,
        extension_key: metadata.extension_key,
        extension_id: metadata.extension_id,
        service_key: metadata.service_key,
        route: metadata.route,
        trace_id: metadata.trace_id,
        invocation_id: metadata.invocation_id,
    }
}

fn backend_service_http_response_from_relay(
    response: ExtensionBackendServiceHttpResponseRelay,
) -> ExtensionBackendServiceHttpResponsePayload {
    ExtensionBackendServiceHttpResponsePayload {
        status: response.status,
        headers: response.headers,
        body: response.body,
    }
}

fn backend_service_diagnostic_from_relay(
    diagnostic: ExtensionBackendServiceInvokeDiagnosticRelay,
) -> ExtensionBackendServiceInvokeDiagnosticPayload {
    ExtensionBackendServiceInvokeDiagnosticPayload {
        readiness: backend_service_readiness_from_relay(diagnostic.readiness),
        code: diagnostic.code,
        message: diagnostic.message,
        retryable: diagnostic.retryable,
        details: diagnostic.details,
    }
}

fn backend_service_readiness_from_relay(
    readiness: ExtensionBackendServiceReadinessRelay,
) -> ExtensionBackendServiceReadinessPayload {
    match readiness {
        ExtensionBackendServiceReadinessRelay::Ready => {
            ExtensionBackendServiceReadinessPayload::Ready
        }
        ExtensionBackendServiceReadinessRelay::MissingArtifact => {
            ExtensionBackendServiceReadinessPayload::MissingArtifact
        }
        ExtensionBackendServiceReadinessRelay::MaterializeFailed => {
            ExtensionBackendServiceReadinessPayload::MaterializeFailed
        }
        ExtensionBackendServiceReadinessRelay::Starting => {
            ExtensionBackendServiceReadinessPayload::Starting
        }
        ExtensionBackendServiceReadinessRelay::HealthFailed => {
            ExtensionBackendServiceReadinessPayload::HealthFailed
        }
        ExtensionBackendServiceReadinessRelay::ProcessExited => {
            ExtensionBackendServiceReadinessPayload::ProcessExited
        }
        ExtensionBackendServiceReadinessRelay::UnsupportedRuntime => {
            ExtensionBackendServiceReadinessPayload::UnsupportedRuntime
        }
        ExtensionBackendServiceReadinessRelay::ServiceUnavailable => {
            ExtensionBackendServiceReadinessPayload::ServiceUnavailable
        }
    }
}

fn package_artifact_to_relay(
    artifact: ExtensionPackageArtifactPayload,
) -> ExtensionPackageArtifactRelay {
    ExtensionPackageArtifactRelay {
        artifact_id: artifact.artifact_id,
        archive_digest: artifact.archive_digest,
    }
}

fn runtime_host_to_relay(host: ExtensionRuntimeHostPayload) -> ExtensionRuntimeHostRelay {
    ExtensionRuntimeHostRelay {
        extension_key: host.extension_key,
        extension_id: host.extension_id,
        package_artifact: host.package_artifact.map(package_artifact_to_relay),
    }
}

fn consumer_to_relay(consumer: ExtensionProtocolConsumerPayload) -> ExtensionProtocolConsumerRelay {
    ExtensionProtocolConsumerRelay {
        kind: consumer.kind,
        extension_key: consumer.extension_key,
        extension_id: consumer.extension_id,
        dependency_alias: consumer.dependency_alias,
    }
}

fn workspace_to_relay(
    workspace: ExtensionInvocationWorkspacePayload,
) -> ExtensionInvocationWorkspaceRelay {
    ExtensionInvocationWorkspaceRelay {
        mount_id: workspace.mount_id,
        root_ref: workspace.root_ref,
    }
}

fn transport_error_from_backend(
    error: BackendCommandError,
) -> ExtensionRuntimeActionTransportError {
    match error {
        BackendCommandError::Offline { backend_id } => {
            ExtensionRuntimeActionTransportError::Offline { backend_id }
        }
        BackendCommandError::Timeout { backend_id } => {
            ExtensionRuntimeActionTransportError::Timeout { backend_id }
        }
        BackendCommandError::ResponseDropped { backend_id } => {
            ExtensionRuntimeActionTransportError::ResponseDropped { backend_id }
        }
        BackendCommandError::SendFailed { backend_id } => {
            ExtensionRuntimeActionTransportError::Failed(format!(
                "send extension action command failed: {backend_id}"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::registry::ConnectedBackend;
    use agentdash_relay::{CapabilitiesPayload, RelayError};
    use chrono::Utc;
    use serde_json::json;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn extension_action_transport_maps_relay_response() {
        let registry = BackendRegistry::new();
        let (sender, mut rx) = mpsc::unbounded_channel();
        registry
            .try_register(ConnectedBackend {
                backend_id: "backend-1".to_string(),
                name: "local".to_string(),
                version: "0.1.0".to_string(),
                capabilities: CapabilitiesPayload {
                    executors: Vec::new(),
                    supports_cancel: true,
                    supports_discover_options: true,
                    ..Default::default()
                },
                sender,
                connected_at: Utc::now(),
            })
            .await
            .expect("register backend");

        let pending = {
            let registry = registry.clone();
            tokio::spawn(async move {
                registry
                    .invoke_extension_action("backend-1", command_payload())
                    .await
            })
        };
        let command = rx.recv().await.expect("command should be sent");
        assert!(matches!(
            command,
            RelayMessage::CommandExtensionActionInvoke { .. }
        ));
        let response = RelayMessage::ResponseExtensionActionInvoke {
            id: command.id().to_string(),
            payload: Some(ResponseExtensionActionInvokePayload {
                extension_key: "local-hello".to_string(),
                extension_id: "local-hello".to_string(),
                action_key: "local-hello.profile".to_string(),
                output: json!({ "backend_id": "backend-1" }),
                metadata: Default::default(),
            }),
            error: None,
        };
        assert!(registry.resolve_response(&response).await);

        let output = pending
            .await
            .expect("join")
            .expect("transport should succeed");
        assert_eq!(output.output["backend_id"], "backend-1");
    }

    #[tokio::test]
    async fn extension_action_transport_maps_relay_error() {
        let registry = BackendRegistry::new();
        let err = registry
            .invoke_extension_action("missing", command_payload())
            .await
            .expect_err("offline backend");
        assert!(matches!(
            err,
            ExtensionRuntimeActionTransportError::Offline { .. }
        ));

        let relay_err = ExtensionRuntimeActionTransportError::Failed(
            RelayError::runtime_error("boom").to_string(),
        );
        assert!(relay_err.to_string().contains("boom"));
    }

    #[tokio::test]
    async fn extension_backend_service_transport_carries_http_intent_and_metadata() {
        let registry = BackendRegistry::new();
        let (sender, mut rx) = mpsc::unbounded_channel();
        registry
            .try_register(ConnectedBackend {
                backend_id: "backend-1".to_string(),
                name: "local".to_string(),
                version: "0.1.0".to_string(),
                capabilities: CapabilitiesPayload {
                    executors: Vec::new(),
                    supports_cancel: true,
                    supports_discover_options: true,
                    ..Default::default()
                },
                sender,
                connected_at: Utc::now(),
            })
            .await
            .expect("register backend");

        let pending = {
            let registry = registry.clone();
            tokio::spawn(async move {
                registry
                    .invoke_extension_backend_service("backend-1", backend_service_payload())
                    .await
            })
        };
        let command = rx.recv().await.expect("command should be sent");
        let RelayMessage::CommandExtensionBackendServiceInvoke { payload, .. } = &command else {
            panic!("expected extension backend service command");
        };
        assert_eq!(payload.metadata.project_id, "project-1");
        assert_eq!(payload.metadata.backend_id, "backend-1");
        assert_eq!(payload.metadata.extension_key, "local-webapp");
        assert_eq!(payload.metadata.extension_id, "local-webapp");
        assert_eq!(payload.metadata.service_key, "local-webapp.api");
        assert_eq!(payload.metadata.route, "/api/search");
        assert_eq!(payload.metadata.trace_id, "trace-1");
        assert_eq!(payload.method, "POST");
        assert_eq!(
            payload.body.as_deref(),
            Some(br#"{"query":"demo"}"#.as_ref())
        );

        let response = RelayMessage::ResponseExtensionBackendServiceInvoke {
            id: command.id().to_string(),
            payload: Some(ResponseExtensionBackendServiceInvokePayload {
                metadata: payload.metadata.clone(),
                response: Some(ExtensionBackendServiceHttpResponseRelay {
                    status: 200,
                    headers: std::collections::BTreeMap::from([(
                        "content-type".to_string(),
                        "application/json".to_string(),
                    )]),
                    body: Some(br#"{"ok":true}"#.to_vec()),
                }),
                diagnostic: None,
            }),
            error: None,
        };
        assert!(registry.resolve_response(&response).await);

        let output = pending
            .await
            .expect("join")
            .expect("transport should succeed");
        let http_response = output.response.expect("http response");
        assert_eq!(http_response.status, 200);
        assert_eq!(
            http_response.body.as_deref(),
            Some(br#"{"ok":true}"#.as_ref())
        );
        assert_eq!(output.metadata.backend_id, "backend-1");
        assert!(output.diagnostic.is_none());
    }

    #[tokio::test]
    async fn extension_backend_service_transport_maps_unavailable_diagnostic() {
        let registry = BackendRegistry::new();
        let (sender, mut rx) = mpsc::unbounded_channel();
        registry
            .try_register(ConnectedBackend {
                backend_id: "backend-1".to_string(),
                name: "local".to_string(),
                version: "0.1.0".to_string(),
                capabilities: CapabilitiesPayload {
                    executors: Vec::new(),
                    supports_cancel: true,
                    supports_discover_options: true,
                    ..Default::default()
                },
                sender,
                connected_at: Utc::now(),
            })
            .await
            .expect("register backend");

        let pending = {
            let registry = registry.clone();
            tokio::spawn(async move {
                registry
                    .invoke_extension_backend_service("backend-1", backend_service_payload())
                    .await
            })
        };
        let command = rx.recv().await.expect("command should be sent");
        let RelayMessage::CommandExtensionBackendServiceInvoke { payload, .. } = &command else {
            panic!("expected extension backend service command");
        };
        let response = RelayMessage::ResponseExtensionBackendServiceInvoke {
            id: command.id().to_string(),
            payload: Some(ResponseExtensionBackendServiceInvokePayload {
                metadata: payload.metadata.clone(),
                response: None,
                diagnostic: Some(ExtensionBackendServiceInvokeDiagnosticRelay {
                    readiness: ExtensionBackendServiceReadinessRelay::ServiceUnavailable,
                    code: "service_unavailable".to_string(),
                    message: "backend service is not ready".to_string(),
                    retryable: true,
                    details: Some(json!({ "state": "starting" })),
                }),
            }),
            error: None,
        };
        assert!(registry.resolve_response(&response).await);

        let output = pending
            .await
            .expect("join")
            .expect("transport should succeed");
        let diagnostic = output.diagnostic.expect("diagnostic");
        assert_eq!(
            diagnostic.readiness,
            ExtensionBackendServiceReadinessPayload::ServiceUnavailable
        );
        assert_eq!(diagnostic.code, "service_unavailable");
        assert!(diagnostic.retryable);
        assert!(output.response.is_none());
    }

    fn command_payload() -> ExtensionActionInvokeRequest {
        ExtensionActionInvokeRequest {
            extension_key: "local-hello".to_string(),
            extension_id: "local-hello".to_string(),
            action_key: "local-hello.profile".to_string(),
            project_id: "project-1".to_string(),
            session_id: "session-1".to_string(),
            input: json!({}),
            package_artifact: None,
            runtime_extensions: vec![],
            workspace: None,
            trace_id: "trace-1".to_string(),
            invocation_id: "rtinv-1".to_string(),
        }
    }

    fn backend_service_payload() -> ExtensionBackendServiceInvokeRequest {
        ExtensionBackendServiceInvokeRequest {
            extension_key: "local-webapp".to_string(),
            extension_id: "local-webapp".to_string(),
            service_key: "local-webapp.api".to_string(),
            route: "/api/search".to_string(),
            project_id: "project-1".to_string(),
            session_id: "session-1".to_string(),
            method: "POST".to_string(),
            headers: std::collections::BTreeMap::from([(
                "content-type".to_string(),
                "application/json".to_string(),
            )]),
            body: Some(br#"{"query":"demo"}"#.to_vec()),
            package_artifact: ExtensionPackageArtifactPayload {
                artifact_id: "artifact-1".to_string(),
                archive_digest:
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
            },
            workspace: Some(ExtensionInvocationWorkspacePayload {
                mount_id: "main".to_string(),
                root_ref: "D:/Workspaces/demo".to_string(),
            }),
            trace_id: "trace-1".to_string(),
            invocation_id: "bsinv-1".to_string(),
        }
    }
}
