use std::sync::Arc;

use agentdash_domain::mcp_preset::McpTransportConfig;
use agentdash_spi::platform::mcp_relay::McpRelayProvider;
use async_trait::async_trait;

use crate::mcp_preset::probe_transport;

use super::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeInvocationError,
    RuntimeInvocationOutput, RuntimeInvocationRequest, RuntimeProvider,
};

pub const MCP_PROBE_TRANSPORT_ACTION: &str = "mcp.probe_transport";

pub struct McpProbeTransportProvider {
    action_key: RuntimeActionKey,
    relay: Option<Arc<dyn McpRelayProvider>>,
}

impl McpProbeTransportProvider {
    pub fn new(relay: Option<Arc<dyn McpRelayProvider>>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(MCP_PROBE_TRANSPORT_ACTION)
                .expect("builtin runtime action key should be valid"),
            relay,
        }
    }
}

#[async_trait]
impl RuntimeProvider for McpProbeTransportProvider {
    fn action_key(&self) -> &RuntimeActionKey {
        &self.action_key
    }

    fn action_kind(&self) -> RuntimeActionKind {
        RuntimeActionKind::Setup
    }

    fn describe_action(&self) -> RuntimeActionDescriptor {
        RuntimeActionDescriptor {
            action_key: self.action_key.clone(),
            kind: RuntimeActionKind::Setup,
            description: Some("探测 MCP transport 连通性并发现工具列表".to_string()),
            input_schema: None,
            output_schema: None,
            default_policy: Default::default(),
        }
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
        let transport = serde_json::from_value::<McpTransportConfig>(request.input.clone())
            .map_err(|error| {
                RuntimeInvocationError::invalid_request(
                    format!("mcp.probe_transport 输入必须是 McpTransportConfig: {error}"),
                    Some(request.trace.clone()),
                )
            })?;

        let result = probe_transport(&transport, self.relay.as_deref()).await;
        let output = serde_json::to_value(result).map_err(|error| {
            RuntimeInvocationError::provider_failed(
                format!("序列化 mcp.probe_transport 结果失败: {error}"),
                Some(request.trace.clone()),
            )
        })?;

        Ok(RuntimeInvocationOutput::new(output))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use agentdash_domain::mcp_preset::McpTransportConfig;

    use super::*;
    use crate::runtime_gateway::{RuntimeActor, RuntimeContext, RuntimeGateway};

    #[tokio::test]
    async fn mcp_probe_provider_rejects_invalid_input_shape() {
        let gateway =
            RuntimeGateway::new().with_provider(Arc::new(McpProbeTransportProvider::new(None)));
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(MCP_PROBE_TRANSPORT_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: None,
                root_ref: None,
            },
            json!({ "type": "stdio" }),
        );

        let err = gateway
            .invoke(request)
            .await
            .expect_err("invalid transport should fail before provider work");

        assert_eq!(
            err.kind(),
            crate::runtime_gateway::RuntimeInvocationErrorKind::InvalidRequest
        );
    }

    #[tokio::test]
    async fn mcp_probe_provider_returns_probe_result_payload() {
        let gateway =
            RuntimeGateway::new().with_provider(Arc::new(McpProbeTransportProvider::new(None)));
        let input = serde_json::to_value(McpTransportConfig::Stdio {
            command: "npx".to_string(),
            args: vec![],
            env: vec![],
        })
        .expect("serialize transport");
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(MCP_PROBE_TRANSPORT_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: None,
                root_ref: None,
            },
            input,
        );

        let result = gateway
            .invoke(request)
            .await
            .expect("provider should return");

        assert_eq!(result.output.output["status"], "error");
        assert!(
            result.output.output["error"]
                .as_str()
                .unwrap_or_default()
                .contains("relay")
        );
    }
}
