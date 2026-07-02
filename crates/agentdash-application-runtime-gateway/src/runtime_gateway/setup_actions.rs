use std::sync::Arc;

use async_trait::async_trait;

use super::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeInvocationError,
    RuntimeInvocationOutput, RuntimeInvocationRequest, RuntimeProvider,
};
use agentdash_application_ports::runtime_gateway_setup::{
    MCP_PROBE_TRANSPORT_ACTION, McpProbeSetupPort, McpProbeTransportInput,
    RuntimeGatewaySetupError, WORKSPACE_BROWSE_DIRECTORY_ACTION, WORKSPACE_DETECT_ACTION,
    WORKSPACE_DETECT_GIT_ACTION, WORKSPACE_DISCOVER_BY_IDENTITY_ACTION,
    WorkspaceBrowseDirectoryInput, WorkspaceBrowseDirectorySetupPort, WorkspaceDetectGitInput,
    WorkspaceDetectGitSetupPort, WorkspaceDetectInput, WorkspaceDetectSetupPort,
    WorkspaceDiscoverByIdentityInput, WorkspaceDiscoverByIdentitySetupPort,
};

pub struct McpProbeTransportProvider {
    action_key: RuntimeActionKey,
    probe: Arc<dyn McpProbeSetupPort>,
}

impl McpProbeTransportProvider {
    pub fn new(probe: Arc<dyn McpProbeSetupPort>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(MCP_PROBE_TRANSPORT_ACTION)
                .expect("builtin runtime action key should be valid"),
            probe,
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
            metadata: Default::default(),
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

        let output = self
            .probe
            .probe_transport(input)
            .await
            .map_err(|error| runtime_error_from_setup(error, &request))?;
        let output = serde_json::to_value(output).map_err(|error| {
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
    detector: Arc<dyn WorkspaceDetectSetupPort>,
}

impl WorkspaceDetectProvider {
    pub fn new(detector: Arc<dyn WorkspaceDetectSetupPort>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(WORKSPACE_DETECT_ACTION)
                .expect("builtin runtime action key should be valid"),
            detector,
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
            metadata: Default::default(),
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

        let output = self
            .detector
            .detect_workspace(input)
            .await
            .map_err(|error| runtime_error_from_setup(error, &request))?;
        let output = serde_json::to_value(output).map_err(|error| {
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
    detector: Arc<dyn WorkspaceDetectGitSetupPort>,
}

impl WorkspaceDetectGitProvider {
    pub fn new(detector: Arc<dyn WorkspaceDetectGitSetupPort>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(WORKSPACE_DETECT_GIT_ACTION)
                .expect("builtin runtime action key should be valid"),
            detector,
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
            metadata: Default::default(),
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
        let output = self
            .detector
            .detect_git(input)
            .await
            .map_err(|error| runtime_error_from_setup(error, &request))?;
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
    browser: Arc<dyn WorkspaceBrowseDirectorySetupPort>,
}

impl WorkspaceBrowseDirectoryProvider {
    pub fn new(browser: Arc<dyn WorkspaceBrowseDirectorySetupPort>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(WORKSPACE_BROWSE_DIRECTORY_ACTION)
                .expect("builtin runtime action key should be valid"),
            browser,
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
            metadata: Default::default(),
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
        let output = self
            .browser
            .browse_directory(input)
            .await
            .map_err(|error| runtime_error_from_setup(error, &request))?;
        let output = serde_json::to_value(output).map_err(|error| {
            RuntimeInvocationError::provider_failed(
                format!("序列化 workspace.browse_directory 结果失败: {error}"),
                Some(request.trace.clone()),
            )
        })?;

        Ok(RuntimeInvocationOutput::new(output))
    }
}

pub struct WorkspaceDiscoverByIdentityProvider {
    action_key: RuntimeActionKey,
    discovery: Arc<dyn WorkspaceDiscoverByIdentitySetupPort>,
}

impl WorkspaceDiscoverByIdentityProvider {
    pub fn new(discovery: Arc<dyn WorkspaceDiscoverByIdentitySetupPort>) -> Self {
        Self {
            action_key: RuntimeActionKey::parse(WORKSPACE_DISCOVER_BY_IDENTITY_ACTION)
                .expect("builtin runtime action key should be valid"),
            discovery,
        }
    }
}

#[async_trait]
impl RuntimeProvider for WorkspaceDiscoverByIdentityProvider {
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
            description: Some(
                "按 Workspace identity 发现目标本机 backend 上的候选目录".to_string(),
            ),
            input_schema: None,
            output_schema: None,
            default_policy: Default::default(),
            metadata: Default::default(),
        }
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
        let input =
            serde_json::from_value::<WorkspaceDiscoverByIdentityInput>(request.input.clone())
                .map_err(|error| {
                    RuntimeInvocationError::invalid_request(
                        format!(
                            "workspace.discover_by_identity 输入必须是 WorkspaceDiscoverByIdentityInput: {error}"
                        ),
                        Some(request.trace.clone()),
                    )
                })?;
        let output = self
            .discovery
            .discover_by_identity(input)
            .await
            .map_err(|error| runtime_error_from_setup(error, &request))?;
        let output = serde_json::to_value(output).map_err(|error| {
            RuntimeInvocationError::provider_failed(
                format!("序列化 workspace.discover_by_identity 结果失败: {error}"),
                Some(request.trace.clone()),
            )
        })?;

        Ok(RuntimeInvocationOutput::new(output))
    }
}

fn runtime_error_from_setup(
    error: RuntimeGatewaySetupError,
    request: &RuntimeInvocationRequest,
) -> RuntimeInvocationError {
    match error {
        RuntimeGatewaySetupError::BadRequest(message) => {
            RuntimeInvocationError::invalid_request(message, Some(request.trace.clone()))
        }
        RuntimeGatewaySetupError::BackendOffline(message) => {
            RuntimeInvocationError::conflict(message, Some(request.trace.clone()))
        }
        RuntimeGatewaySetupError::TransportFailed(message)
        | RuntimeGatewaySetupError::ProviderFailed(message) => {
            RuntimeInvocationError::provider_failed(message, Some(request.trace.clone()))
        }
        RuntimeGatewaySetupError::Timeout => RuntimeInvocationError::timeout(
            format!("{} 执行超时", request.action_key),
            Some(request.trace.clone()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::workspace::{WorkspaceBinding, WorkspaceIdentityKind};
    use serde_json::json;

    use agentdash_domain::mcp_preset::{
        McpRuntimeBindingConfig, McpRuntimeBindingRule, McpRuntimeBindingSource,
        McpRuntimeBindingTarget, McpTransportConfig,
    };

    use super::*;
    use crate::runtime_gateway::{RuntimeActor, RuntimeContext, RuntimeGateway};
    use agentdash_application_ports::backend_transport::{GitRepoInfo, WorkspaceProbeInfo};
    use agentdash_application_ports::runtime_gateway_setup::{
        McpProbeTransportOutput, WorkspaceBrowseDirectoryEntry, WorkspaceBrowseDirectoryOutput,
        WorkspaceDetectGitOutput, WorkspaceDetectOutput,
    };

    struct FakeBackendTransport {
        online: bool,
        probe: WorkspaceProbeInfo,
    }

    struct FakeMcpProbeSetup {
        output: McpProbeTransportOutput,
    }

    #[async_trait]
    impl McpProbeSetupPort for FakeMcpProbeSetup {
        async fn probe_transport(
            &self,
            _input: McpProbeTransportInput,
        ) -> Result<McpProbeTransportOutput, RuntimeGatewaySetupError> {
            Ok(self.output.clone())
        }
    }

    fn fake_mcp_probe(output: McpProbeTransportOutput) -> Arc<dyn McpProbeSetupPort> {
        Arc::new(FakeMcpProbeSetup { output })
    }

    #[async_trait]
    impl WorkspaceDetectSetupPort for FakeBackendTransport {
        async fn detect_workspace(
            &self,
            input: WorkspaceDetectInput,
        ) -> Result<WorkspaceDetectOutput, RuntimeGatewaySetupError> {
            if input.root_ref.trim().is_empty() {
                return Err(RuntimeGatewaySetupError::BadRequest(
                    "root_ref 不能为空".to_string(),
                ));
            }
            if !self.online {
                return Err(RuntimeGatewaySetupError::BackendOffline(format!(
                    "目标 Backend 当前不在线: {}",
                    input.backend_id
                )));
            }

            let identity_kind = if self.probe.git.is_some() {
                WorkspaceIdentityKind::GitRepo
            } else if self.probe.p4.is_some() {
                WorkspaceIdentityKind::P4Workspace
            } else {
                WorkspaceIdentityKind::LocalDir
            };
            let confidence = if self.probe.git.is_some() || self.probe.p4.is_some() {
                "high"
            } else {
                "medium"
            };
            Ok(WorkspaceDetectOutput {
                identity_kind,
                identity_payload: json!({}),
                binding: WorkspaceBinding::new(
                    uuid::Uuid::nil(),
                    input.backend_id,
                    input.root_ref,
                    json!({}),
                ),
                confidence: confidence.to_string(),
                warnings: self.probe.warnings.clone(),
            })
        }
    }

    #[async_trait]
    impl WorkspaceDetectGitSetupPort for FakeBackendTransport {
        async fn detect_git(
            &self,
            input: WorkspaceDetectGitInput,
        ) -> Result<WorkspaceDetectGitOutput, RuntimeGatewaySetupError> {
            if input.root_ref.trim().is_empty() {
                return Err(RuntimeGatewaySetupError::BadRequest(
                    "root_ref 不能为空".to_string(),
                ));
            }
            if !self.online {
                return Err(RuntimeGatewaySetupError::BackendOffline(format!(
                    "目标 Backend 当前不在线: {}",
                    input.backend_id
                )));
            }
            let git = self.probe.git.clone().unwrap_or_default();
            Ok(WorkspaceDetectGitOutput {
                resolved_root_ref: input.root_ref,
                is_git_repo: git.is_git_repo,
                source_repo: git.source_repo,
                branch: git.branch,
                commit_hash: git.commit_hash,
            })
        }
    }

    #[async_trait]
    impl WorkspaceBrowseDirectorySetupPort for FakeBackendTransport {
        async fn browse_directory(
            &self,
            input: WorkspaceBrowseDirectoryInput,
        ) -> Result<WorkspaceBrowseDirectoryOutput, RuntimeGatewaySetupError> {
            if !self.online {
                return Err(RuntimeGatewaySetupError::BackendOffline(format!(
                    "目标 Backend 当前不在线: {}",
                    input.backend_id
                )));
            }
            Ok(WorkspaceBrowseDirectoryOutput {
                current_path: input.path.unwrap_or_else(|| "C:/".to_string()),
                entries: vec![WorkspaceBrowseDirectoryEntry {
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
            McpProbeTransportProvider::new(fake_mcp_probe(McpProbeTransportOutput::Ok {
                latency_ms: 0,
                tools: Vec::new(),
            })),
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
            McpProbeTransportProvider::new(fake_mcp_probe(McpProbeTransportOutput::Error {
                error: "relay unavailable".to_string(),
            })),
        ));
        let input = serde_json::to_value(McpProbeTransportInput {
            transport: McpTransportConfig::Stdio {
                command: "npx".to_string(),
                args: vec![],
                env: vec![],
                cwd: None,
            },
            route_policy: agentdash_domain::mcp_preset::McpRoutePolicy::Auto,
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
            McpProbeTransportProvider::new(fake_mcp_probe(McpProbeTransportOutput::Unsupported {
                reason: "workspace.detected_facts.p4.client_name".to_string(),
            })),
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
