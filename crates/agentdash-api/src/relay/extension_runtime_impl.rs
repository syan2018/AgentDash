use std::time::Duration;

use agentdash_application_ports::extension_runtime::{
    ExtensionActionInvokeRequest, ExtensionActionInvokeResponse, ExtensionChannelConsumerPayload,
    ExtensionChannelInvokeRequest, ExtensionChannelInvokeResponse,
    ExtensionInvocationWorkspacePayload, ExtensionPackageArtifactPayload,
    ExtensionRuntimeActionTransport, ExtensionRuntimeActionTransportError,
    ExtensionRuntimeChannelTransport, ExtensionRuntimeHostPayload,
};
use agentdash_relay::{
    CommandExtensionActionInvokePayload, CommandExtensionChannelInvokePayload,
    ExtensionChannelConsumerRelay, ExtensionInvocationWorkspaceRelay,
    ExtensionPackageArtifactRelay, ExtensionRuntimeHostRelay, RelayMessage,
    ResponseExtensionActionInvokePayload, ResponseExtensionChannelInvokePayload,
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
impl ExtensionRuntimeChannelTransport for BackendRegistry {
    async fn invoke_extension_channel(
        &self,
        backend_id: &str,
        request: ExtensionChannelInvokeRequest,
    ) -> Result<ExtensionChannelInvokeResponse, ExtensionRuntimeActionTransportError> {
        let command = RelayMessage::CommandExtensionChannelInvoke {
            id: RelayMessage::new_id("ext-channel"),
            payload: channel_request_to_relay(request),
        };
        let response = self
            .send_command_with_timeout(backend_id, command, Duration::from_secs(30))
            .await
            .map_err(transport_error_from_backend)?;
        match response {
            RelayMessage::ResponseExtensionChannelInvoke {
                payload: Some(payload),
                error: None,
                ..
            } => Ok(channel_response_from_relay(payload)),
            RelayMessage::ResponseExtensionChannelInvoke {
                error: Some(error), ..
            } => Err(ExtensionRuntimeActionTransportError::Failed(
                error.to_string(),
            )),
            other => Err(ExtensionRuntimeActionTransportError::Failed(format!(
                "unexpected extension channel relay response: {}",
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
    request: ExtensionChannelInvokeRequest,
) -> CommandExtensionChannelInvokePayload {
    CommandExtensionChannelInvokePayload {
        provider_extension_key: request.provider_extension_key,
        provider_extension_id: request.provider_extension_id,
        channel_key: request.channel_key,
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
    response: ResponseExtensionChannelInvokePayload,
) -> ExtensionChannelInvokeResponse {
    ExtensionChannelInvokeResponse {
        provider_extension_key: response.provider_extension_key,
        provider_extension_id: response.provider_extension_id,
        channel_key: response.channel_key,
        method: response.method,
        output: response.output,
        metadata: response.metadata,
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

fn consumer_to_relay(consumer: ExtensionChannelConsumerPayload) -> ExtensionChannelConsumerRelay {
    ExtensionChannelConsumerRelay {
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
                    mcp_servers: Vec::new(),
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
}
