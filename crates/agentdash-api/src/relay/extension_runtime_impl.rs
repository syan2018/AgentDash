use std::time::Duration;

use agentdash_application::runtime_gateway::{
    ExtensionRuntimeActionTransport, ExtensionRuntimeActionTransportError,
};
use agentdash_relay::{
    CommandExtensionActionInvokePayload, RelayMessage, ResponseExtensionActionInvokePayload,
};
use async_trait::async_trait;

use super::registry::{BackendCommandError, BackendRegistry};

#[async_trait]
impl ExtensionRuntimeActionTransport for BackendRegistry {
    async fn invoke_extension_action(
        &self,
        backend_id: &str,
        payload: CommandExtensionActionInvokePayload,
    ) -> Result<ResponseExtensionActionInvokePayload, ExtensionRuntimeActionTransportError> {
        let command = RelayMessage::CommandExtensionActionInvoke {
            id: RelayMessage::new_id("ext-action"),
            payload,
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
            } => Ok(payload),
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
                workspace_roots: Vec::new(),
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

    fn command_payload() -> CommandExtensionActionInvokePayload {
        CommandExtensionActionInvokePayload {
            extension_key: "local-hello".to_string(),
            extension_id: "local-hello".to_string(),
            action_key: "local-hello.profile".to_string(),
            project_id: "project-1".to_string(),
            session_id: "session-1".to_string(),
            input: json!({}),
            package_artifact: None,
            trace_id: "trace-1".to_string(),
            invocation_id: "rtinv-1".to_string(),
        }
    }
}
