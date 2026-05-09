use std::sync::Arc;

use agentdash_domain::mcp_preset::McpTransportConfig;
use agentdash_spi::platform::mcp_relay::McpRelayProvider;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::mcp_preset::probe_transport;
use crate::workspace::{WorkspaceDetectionError, detect_workspace_from_backend};

use super::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeInvocationError,
    RuntimeInvocationOutput, RuntimeInvocationRequest, RuntimeProvider,
};
use crate::backend_transport::BackendTransport;

pub const MCP_PROBE_TRANSPORT_ACTION: &str = "mcp.probe_transport";
pub const WORKSPACE_DETECT_ACTION: &str = "workspace.detect";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceDetectInput {
    pub backend_id: String,
    pub root_ref: String,
}

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

pub struct WorkspaceDetectProvider {
    action_key: RuntimeActionKey,
    transport: Arc<dyn BackendTransport>,
}

impl WorkspaceDetectProvider {
    pub fn new(transport: Arc<dyn BackendTransport>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(WORKSPACE_DETECT_ACTION)
                .expect("builtin runtime action key should be valid"),
            transport,
        }
    }
}

#[async_trait]
impl RuntimeProvider for WorkspaceDetectProvider {
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
            description: Some("探测远程目录并推断 Workspace identity 与 binding".to_string()),
            input_schema: None,
            output_schema: None,
            default_policy: Default::default(),
        }
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
        let input = serde_json::from_value::<WorkspaceDetectInput>(request.input.clone()).map_err(
            |error| {
                RuntimeInvocationError::invalid_request(
                    format!("workspace.detect 输入必须是 WorkspaceDetectInput: {error}"),
                    Some(request.trace.clone()),
                )
            },
        )?;

        let result = detect_workspace_from_backend(
            self.transport.as_ref(),
            &input.backend_id,
            &input.root_ref,
        )
        .await
        .map_err(|error| match error {
            WorkspaceDetectionError::BadRequest(message) => {
                RuntimeInvocationError::invalid_request(message, Some(request.trace.clone()))
            }
            WorkspaceDetectionError::BackendOffline(message) => {
                RuntimeInvocationError::conflict(message, Some(request.trace.clone()))
            }
            WorkspaceDetectionError::TransportFailed(message) => {
                RuntimeInvocationError::provider_failed(message, Some(request.trace.clone()))
            }
        })?;
        let output = serde_json::to_value(result).map_err(|error| {
            RuntimeInvocationError::provider_failed(
                format!("序列化 workspace.detect 结果失败: {error}"),
                Some(request.trace.clone()),
            )
        })?;

        Ok(RuntimeInvocationOutput::new(output))
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::workspace::WorkspaceIdentityKind;
    use serde_json::json;

    use agentdash_domain::mcp_preset::McpTransportConfig;

    use super::*;
    use crate::backend_transport::{GitRepoInfo, TransportError, WorkspaceProbeInfo};
    use crate::runtime_gateway::{RuntimeActor, RuntimeContext, RuntimeGateway};

    struct FakeBackendTransport {
        online: bool,
        probe: WorkspaceProbeInfo,
    }

    #[async_trait]
    impl BackendTransport for FakeBackendTransport {
        async fn is_online(&self, _backend_id: &str) -> bool {
            self.online
        }

        async fn list_online_backend_ids(&self) -> Vec<String> {
            if self.online {
                vec!["backend-1".to_string()]
            } else {
                Vec::new()
            }
        }

        async fn detect_workspace(
            &self,
            _backend_id: &str,
            _root: &str,
        ) -> Result<WorkspaceProbeInfo, TransportError> {
            Ok(self.probe.clone())
        }
    }

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

    #[tokio::test]
    async fn workspace_detect_provider_rejects_missing_root_ref() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(WorkspaceDetectProvider::new(
            Arc::new(FakeBackendTransport {
                online: true,
                probe: WorkspaceProbeInfo::default(),
            }),
        )));
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(WORKSPACE_DETECT_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: Some("backend-1".to_string()),
                root_ref: None,
            },
            serde_json::to_value(WorkspaceDetectInput {
                backend_id: "backend-1".to_string(),
                root_ref: String::new(),
            })
            .expect("serialize input"),
        );

        let err = gateway
            .invoke(request)
            .await
            .expect_err("empty root_ref should fail validation");

        assert_eq!(
            err.kind(),
            crate::runtime_gateway::RuntimeInvocationErrorKind::InvalidRequest
        );
    }

    #[tokio::test]
    async fn workspace_detect_provider_maps_offline_backend_to_conflict() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(WorkspaceDetectProvider::new(
            Arc::new(FakeBackendTransport {
                online: false,
                probe: WorkspaceProbeInfo::default(),
            }),
        )));
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(WORKSPACE_DETECT_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: Some("backend-1".to_string()),
                root_ref: Some("C:/repo".to_string()),
            },
            serde_json::to_value(WorkspaceDetectInput {
                backend_id: "backend-1".to_string(),
                root_ref: "C:/repo".to_string(),
            })
            .expect("serialize input"),
        );

        let err = gateway
            .invoke(request)
            .await
            .expect_err("offline backend should be a conflict");

        assert_eq!(
            err.kind(),
            crate::runtime_gateway::RuntimeInvocationErrorKind::Conflict
        );
    }

    #[tokio::test]
    async fn workspace_detect_provider_returns_detection_result_payload() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(WorkspaceDetectProvider::new(
            Arc::new(FakeBackendTransport {
                online: true,
                probe: WorkspaceProbeInfo {
                    git: Some(GitRepoInfo {
                        is_git_repo: true,
                        repo_root: Some("C:/repo".to_string()),
                        source_repo: Some("https://github.com/openai/agentdash.git".to_string()),
                        default_branch: Some("main".to_string()),
                        branch: Some("main".to_string()),
                        commit_hash: Some("abc".to_string()),
                    }),
                    p4: None,
                    warnings: Vec::new(),
                },
            }),
        )));
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(WORKSPACE_DETECT_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: Some("backend-1".to_string()),
                root_ref: Some("C:/repo".to_string()),
            },
            serde_json::to_value(WorkspaceDetectInput {
                backend_id: "backend-1".to_string(),
                root_ref: "C:/repo".to_string(),
            })
            .expect("serialize input"),
        );

        let result = gateway
            .invoke(request)
            .await
            .expect("detect should succeed");

        assert_eq!(
            result.output.output["identity_kind"],
            serde_json::to_value(WorkspaceIdentityKind::GitRepo).expect("serialize kind")
        );
        assert_eq!(result.output.output["confidence"], "high");
    }
}
