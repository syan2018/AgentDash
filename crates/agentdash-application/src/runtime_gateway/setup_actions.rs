use std::sync::Arc;

use agentdash_domain::mcp_preset::{McpRuntimeBindingConfig, McpTransportConfig};
use agentdash_spi::platform::mcp_probe::McpProbeTransport;
use agentdash_spi::platform::mcp_relay::McpRelayProvider;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::mcp_preset::probe_transport_without_runtime_context;
use crate::workspace::{WorkspaceDetectionError, detect_workspace_from_backend};

use super::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeInvocationError,
    RuntimeInvocationOutput, RuntimeInvocationRequest, RuntimeProvider,
};
use agentdash_application_ports::backend_transport::{BackendTransport, TransportError};

pub const MCP_PROBE_TRANSPORT_ACTION: &str = "mcp.probe_transport";
pub const WORKSPACE_BROWSE_DIRECTORY_ACTION: &str = "workspace.browse_directory";
pub const WORKSPACE_DETECT_ACTION: &str = "workspace.detect";
pub const WORKSPACE_DETECT_GIT_ACTION: &str = "workspace.detect_git";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceDetectInput {
    pub backend_id: String,
    pub root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceDetectGitInput {
    pub backend_id: String,
    pub root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceDetectGitOutput {
    pub resolved_root_ref: String,
    pub is_git_repo: bool,
    pub source_repo: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceBrowseDirectoryInput {
    pub backend_id: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceBrowseDirectoryOutput {
    pub current_path: String,
    pub entries: Vec<WorkspaceBrowseDirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceBrowseDirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpProbeTransportInput {
    pub transport: McpTransportConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_binding: Option<McpRuntimeBindingConfig>,
}

pub struct McpProbeTransportProvider {
    action_key: RuntimeActionKey,
    relay: Option<Arc<dyn McpRelayProvider>>,
    http_probe: Arc<dyn McpProbeTransport>,
}

impl McpProbeTransportProvider {
    pub fn new(
        relay: Option<Arc<dyn McpRelayProvider>>,
        http_probe: Arc<dyn McpProbeTransport>,
    ) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(MCP_PROBE_TRANSPORT_ACTION)
                .expect("builtin runtime action key should be valid"),
            relay,
            http_probe,
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
        let input = serde_json::from_value::<McpProbeTransportInput>(request.input.clone())
            .map_err(|error| {
                RuntimeInvocationError::invalid_request(
                    format!("mcp.probe_transport 输入必须是 McpProbeTransportInput: {error}"),
                    Some(request.trace.clone()),
                )
            })?;

        let result = probe_transport_without_runtime_context(
            &input.transport,
            input.runtime_binding.as_ref(),
            self.relay.as_deref(),
            self.http_probe.as_ref(),
        )
        .await;
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

pub struct WorkspaceDetectGitProvider {
    action_key: RuntimeActionKey,
    transport: Arc<dyn BackendTransport>,
}

impl WorkspaceDetectGitProvider {
    pub fn new(transport: Arc<dyn BackendTransport>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(WORKSPACE_DETECT_GIT_ACTION)
                .expect("builtin runtime action key should be valid"),
            transport,
        }
    }
}

#[async_trait]
impl RuntimeProvider for WorkspaceDetectGitProvider {
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
            description: Some("探测远程目录是否为 Git 仓库".to_string()),
            input_schema: None,
            output_schema: None,
            default_policy: Default::default(),
        }
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
        let input = serde_json::from_value::<WorkspaceDetectGitInput>(request.input.clone())
            .map_err(|error| {
                RuntimeInvocationError::invalid_request(
                    format!("workspace.detect_git 输入必须是 WorkspaceDetectGitInput: {error}"),
                    Some(request.trace.clone()),
                )
            })?;
        let backend_id = input.backend_id.trim();
        if backend_id.is_empty() {
            return Err(RuntimeInvocationError::invalid_request(
                "backend_id 不能为空",
                Some(request.trace.clone()),
            ));
        }
        let root_ref = input.root_ref.trim();
        if root_ref.is_empty() {
            return Err(RuntimeInvocationError::invalid_request(
                "root_ref 不能为空",
                Some(request.trace.clone()),
            ));
        }
        if !self.transport.is_online(backend_id).await {
            return Err(RuntimeInvocationError::conflict(
                format!("目标 Backend 当前不在线: {backend_id}"),
                Some(request.trace.clone()),
            ));
        }

        let info = self
            .transport
            .detect_git_repo(backend_id, root_ref)
            .await
            .map_err(|error| runtime_error_from_transport(error, &request))?;
        let output = WorkspaceDetectGitOutput {
            resolved_root_ref: root_ref.to_string(),
            is_git_repo: info.is_git_repo,
            source_repo: info.source_repo,
            branch: info.branch,
            commit_hash: info.commit_hash,
        };
        let output = serde_json::to_value(output).map_err(|error| {
            RuntimeInvocationError::provider_failed(
                format!("序列化 workspace.detect_git 结果失败: {error}"),
                Some(request.trace.clone()),
            )
        })?;

        Ok(RuntimeInvocationOutput::new(output))
    }
}

pub struct WorkspaceBrowseDirectoryProvider {
    action_key: RuntimeActionKey,
    transport: Arc<dyn BackendTransport>,
}

impl WorkspaceBrowseDirectoryProvider {
    pub fn new(transport: Arc<dyn BackendTransport>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(WORKSPACE_BROWSE_DIRECTORY_ACTION)
                .expect("builtin runtime action key should be valid"),
            transport,
        }
    }
}

#[async_trait]
impl RuntimeProvider for WorkspaceBrowseDirectoryProvider {
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
            description: Some("浏览远程后端上的目录入口".to_string()),
            input_schema: None,
            output_schema: None,
            default_policy: Default::default(),
        }
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
        let input = serde_json::from_value::<WorkspaceBrowseDirectoryInput>(request.input.clone())
            .map_err(|error| {
                RuntimeInvocationError::invalid_request(
                    format!(
                        "workspace.browse_directory 输入必须是 WorkspaceBrowseDirectoryInput: {error}"
                    ),
                    Some(request.trace.clone()),
                )
            })?;
        let backend_id = input.backend_id.trim();
        if backend_id.is_empty() {
            return Err(RuntimeInvocationError::invalid_request(
                "backend_id 不能为空",
                Some(request.trace.clone()),
            ));
        }
        if !self.transport.is_online(backend_id).await {
            return Err(RuntimeInvocationError::conflict(
                format!("目标 Backend 当前不在线: {backend_id}"),
                Some(request.trace.clone()),
            ));
        }

        let path = input.path.as_deref();
        let result = self
            .transport
            .browse_directory(backend_id, path)
            .await
            .map_err(|error| runtime_error_from_transport(error, &request))?;
        let output = WorkspaceBrowseDirectoryOutput {
            current_path: result.current_path,
            entries: result
                .entries
                .into_iter()
                .map(|entry| WorkspaceBrowseDirectoryEntry {
                    name: entry.name,
                    path: entry.path,
                    is_dir: entry.is_dir,
                })
                .collect(),
        };
        let output = serde_json::to_value(output).map_err(|error| {
            RuntimeInvocationError::provider_failed(
                format!("序列化 workspace.browse_directory 结果失败: {error}"),
                Some(request.trace.clone()),
            )
        })?;

        Ok(RuntimeInvocationOutput::new(output))
    }
}

fn runtime_error_from_transport(
    error: TransportError,
    request: &RuntimeInvocationRequest,
) -> RuntimeInvocationError {
    match error {
        TransportError::BackendOffline(message) => {
            RuntimeInvocationError::conflict(message, Some(request.trace.clone()))
        }
        TransportError::OperationFailed(message) => {
            RuntimeInvocationError::provider_failed(message, Some(request.trace.clone()))
        }
        TransportError::Timeout => RuntimeInvocationError::timeout(
            format!("{} 执行超时", request.action_key),
            Some(request.trace.clone()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::workspace::WorkspaceIdentityKind;
    use agentdash_infrastructure::RmcpProbeTransport;
    use serde_json::json;

    use agentdash_domain::mcp_preset::{
        McpRuntimeBindingConfig, McpRuntimeBindingRule, McpRuntimeBindingSource,
        McpRuntimeBindingTarget, McpTransportConfig,
    };

    use super::*;
    use crate::runtime_gateway::{RuntimeActor, RuntimeContext, RuntimeGateway};
    use agentdash_application_ports::backend_transport::{
        DirectoryBrowseInfo, DirectoryEntryInfo, GitRepoInfo, TransportError, WorkspaceProbeInfo,
    };

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

        async fn browse_directory(
            &self,
            backend_id: &str,
            path: Option<&str>,
        ) -> Result<DirectoryBrowseInfo, TransportError> {
            if !self.online {
                return Err(TransportError::BackendOffline(backend_id.to_string()));
            }
            Ok(DirectoryBrowseInfo {
                current_path: path.unwrap_or("C:/").to_string(),
                entries: vec![DirectoryEntryInfo {
                    name: "repo".to_string(),
                    path: "C:/repo".to_string(),
                    is_dir: true,
                }],
            })
        }
    }

    #[tokio::test]
    async fn mcp_probe_provider_rejects_invalid_input_shape() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            McpProbeTransportProvider::new(None, Arc::new(RmcpProbeTransport::new())),
        ));
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
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            McpProbeTransportProvider::new(None, Arc::new(RmcpProbeTransport::new())),
        ));
        let input = serde_json::to_value(McpProbeTransportInput {
            transport: McpTransportConfig::Stdio {
                command: "npx".to_string(),
                args: vec![],
                env: vec![],
                cwd: None,
            },
            runtime_binding: None,
        })
        .expect("serialize input");
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
    async fn mcp_probe_provider_returns_unsupported_for_required_runtime_binding() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            McpProbeTransportProvider::new(None, Arc::new(RmcpProbeTransport::new())),
        ));
        let input = json!({
            "transport": {
                "type": "http",
                "url": "http://127.0.0.1:1/mcp"
            },
            "runtime_binding": McpRuntimeBindingConfig {
                mount_id: Some("main".to_string()),
                bindings: vec![McpRuntimeBindingRule {
                    source: McpRuntimeBindingSource::WorkspaceDetectedFact {
                        path: vec!["p4".to_string(), "client_name".to_string()],
                    },
                    target: McpRuntimeBindingTarget::HttpQuery {
                        name: "p4_client".to_string(),
                    },
                    required: true,
                }],
            }
        });
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

        assert_eq!(result.output.output["status"], "unsupported");
        assert!(
            result.output.output["reason"]
                .as_str()
                .unwrap_or_default()
                .contains("workspace.detected_facts.p4.client_name")
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

    #[tokio::test]
    async fn workspace_detect_git_provider_rejects_missing_root_ref() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            WorkspaceDetectGitProvider::new(Arc::new(FakeBackendTransport {
                online: true,
                probe: WorkspaceProbeInfo::default(),
            })),
        ));
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(WORKSPACE_DETECT_GIT_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: Some("backend-1".to_string()),
                root_ref: None,
            },
            serde_json::to_value(WorkspaceDetectGitInput {
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
    async fn workspace_detect_git_provider_returns_git_payload() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            WorkspaceDetectGitProvider::new(Arc::new(FakeBackendTransport {
                online: true,
                probe: WorkspaceProbeInfo {
                    git: Some(GitRepoInfo {
                        is_git_repo: true,
                        repo_root: Some("C:/repo".to_string()),
                        source_repo: Some("https://github.com/openai/agentdash.git".to_string()),
                        default_branch: Some("main".to_string()),
                        branch: Some("feature".to_string()),
                        commit_hash: Some("abc".to_string()),
                    }),
                    p4: None,
                    warnings: Vec::new(),
                },
            })),
        ));
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(WORKSPACE_DETECT_GIT_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: Some("backend-1".to_string()),
                root_ref: Some("C:/repo".to_string()),
            },
            serde_json::to_value(WorkspaceDetectGitInput {
                backend_id: "backend-1".to_string(),
                root_ref: "C:/repo".to_string(),
            })
            .expect("serialize input"),
        );

        let result = gateway
            .invoke(request)
            .await
            .expect("detect git should succeed");

        assert_eq!(result.output.output["resolved_root_ref"], "C:/repo");
        assert_eq!(result.output.output["is_git_repo"], true);
        assert_eq!(
            result.output.output["source_repo"],
            "https://github.com/openai/agentdash.git"
        );
        assert_eq!(result.output.output["branch"], "feature");
        assert_eq!(result.output.output["commit_hash"], "abc");
    }

    #[tokio::test]
    async fn workspace_detect_git_provider_maps_offline_backend_to_conflict() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            WorkspaceDetectGitProvider::new(Arc::new(FakeBackendTransport {
                online: false,
                probe: WorkspaceProbeInfo::default(),
            })),
        ));
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(WORKSPACE_DETECT_GIT_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: Some("backend-1".to_string()),
                root_ref: Some("C:/repo".to_string()),
            },
            serde_json::to_value(WorkspaceDetectGitInput {
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
    async fn workspace_browse_directory_provider_maps_offline_backend_to_conflict() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            WorkspaceBrowseDirectoryProvider::new(Arc::new(FakeBackendTransport {
                online: false,
                probe: WorkspaceProbeInfo::default(),
            })),
        ));
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(WORKSPACE_BROWSE_DIRECTORY_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: Some("backend-1".to_string()),
                root_ref: None,
            },
            serde_json::to_value(WorkspaceBrowseDirectoryInput {
                backend_id: "backend-1".to_string(),
                path: None,
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
    async fn workspace_browse_directory_provider_returns_directory_payload() {
        let gateway = RuntimeGateway::new().with_provider(Arc::new(
            WorkspaceBrowseDirectoryProvider::new(Arc::new(FakeBackendTransport {
                online: true,
                probe: WorkspaceProbeInfo::default(),
            })),
        ));
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(WORKSPACE_BROWSE_DIRECTORY_ACTION).expect("valid action key"),
            RuntimeActor::EnvironmentSetup { request_id: None },
            RuntimeContext::Setup {
                project_id: None,
                workspace_id: None,
                backend_id: Some("backend-1".to_string()),
                root_ref: None,
            },
            serde_json::to_value(WorkspaceBrowseDirectoryInput {
                backend_id: "backend-1".to_string(),
                path: Some("C:/".to_string()),
            })
            .expect("serialize input"),
        );

        let result = gateway
            .invoke(request)
            .await
            .expect("browse should succeed");

        assert_eq!(result.output.output["current_path"], "C:/");
        assert_eq!(result.output.output["entries"][0]["name"], "repo");
        assert_eq!(result.output.output["entries"][0]["is_dir"], true);
    }
}
