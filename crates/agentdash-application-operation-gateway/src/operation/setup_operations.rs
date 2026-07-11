use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_application_ports::runtime_gateway_setup::{
    MCP_PROBE_TRANSPORT_ACTION, McpProbeSetupPort, McpProbeTransportInput,
    RuntimeGatewaySetupError, WORKSPACE_BROWSE_DIRECTORY_ACTION, WORKSPACE_DETECT_ACTION,
    WORKSPACE_DETECT_GIT_ACTION, WORKSPACE_DISCOVER_BY_IDENTITY_ACTION,
    WorkspaceBrowseDirectoryInput, WorkspaceBrowseDirectorySetupPort, WorkspaceDetectGitInput,
    WorkspaceDetectGitSetupPort, WorkspaceDetectInput, WorkspaceDetectSetupPort,
    WorkspaceDiscoverByIdentityInput, WorkspaceDiscoverByIdentitySetupPort,
};
use agentdash_domain::operation::{OperationProviderRef, OperationRef};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{
    OperationActorKind, OperationAuthorityGrant, OperationAuthorityResolver,
    OperationAuthorizationScope, OperationDescriptor, OperationDispatch, OperationEffect,
    OperationExecutionError, OperationExecutionPolicy, OperationInvocationEnvelope,
    OperationOriginRef, OperationPlacement, OperationPrincipal, OperationPrincipalRef,
    OperationProvenance, OperationProvider, OperationReadiness, OperationReplayPolicy,
    OperationScopeRef,
};

pub const SETUP_OPERATION_NAMESPACE: &str = "agentdash";
pub const SETUP_OPERATION_PROVIDER_KEY: &str = "environment_setup";

pub fn setup_operation_ref(operation_key: &str) -> Result<OperationRef, OperationExecutionError> {
    OperationRef::new(
        SETUP_OPERATION_NAMESPACE,
        SETUP_OPERATION_PROVIDER_KEY,
        operation_key,
        1,
    )
    .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))
}

pub struct SetupOperationProvider {
    provider_ref: OperationProviderRef,
    mcp_probe: Arc<dyn McpProbeSetupPort>,
    workspace_detect: Arc<dyn WorkspaceDetectSetupPort>,
    workspace_detect_git: Arc<dyn WorkspaceDetectGitSetupPort>,
    workspace_browse: Arc<dyn WorkspaceBrowseDirectorySetupPort>,
    workspace_discover: Arc<dyn WorkspaceDiscoverByIdentitySetupPort>,
}

impl SetupOperationProvider {
    pub fn new(
        mcp_probe: Arc<dyn McpProbeSetupPort>,
        workspace_detect: Arc<dyn WorkspaceDetectSetupPort>,
        workspace_detect_git: Arc<dyn WorkspaceDetectGitSetupPort>,
        workspace_browse: Arc<dyn WorkspaceBrowseDirectorySetupPort>,
        workspace_discover: Arc<dyn WorkspaceDiscoverByIdentitySetupPort>,
    ) -> Self {
        Self {
            provider_ref: OperationProviderRef {
                namespace: SETUP_OPERATION_NAMESPACE.to_string(),
                provider_key: SETUP_OPERATION_PROVIDER_KEY.to_string(),
            },
            mcp_probe,
            workspace_detect,
            workspace_detect_git,
            workspace_browse,
            workspace_discover,
        }
    }

    fn descriptors(&self) -> Result<Vec<OperationDescriptor>, OperationExecutionError> {
        [
            (MCP_PROBE_TRANSPORT_ACTION, "探测 MCP transport"),
            (WORKSPACE_DETECT_ACTION, "探测 workspace identity"),
            (WORKSPACE_DETECT_GIT_ACTION, "探测 Git workspace"),
            (WORKSPACE_BROWSE_DIRECTORY_ACTION, "浏览本机目录"),
            (
                WORKSPACE_DISCOVER_BY_IDENTITY_ACTION,
                "按 identity 发现 workspace",
            ),
        ]
        .into_iter()
        .map(|(operation_key, title)| self.descriptor(operation_key, title))
        .collect()
    }

    fn descriptor(
        &self,
        operation_key: &str,
        title: &str,
    ) -> Result<OperationDescriptor, OperationExecutionError> {
        let operation_ref = setup_operation_ref(operation_key)?;
        let (input_schema, output_schema) = setup_operation_schemas(operation_key)?;
        let execution_policy = OperationExecutionPolicy {
            max_inline_output_bytes: 1024 * 1024,
            ..OperationExecutionPolicy::default()
        };
        Ok(OperationDescriptor {
            operation_ref,
            title: title.to_string(),
            description: None,
            input_schema,
            output_schema,
            effect: OperationEffect::Read,
            replay_policy: OperationReplayPolicy::ReplaySafe,
            required_capabilities: BTreeSet::from([
                if operation_key == MCP_PROBE_TRANSPORT_ACTION {
                    "setup.mcp_probe".to_string()
                } else {
                    "setup.workspace".to_string()
                },
            ]),
            actor_visibility: BTreeSet::from([OperationActorKind::User]),
            execution_policy,
            readiness: OperationReadiness::Ready,
            provenance: OperationProvenance {
                source: "agentdash_setup".to_string(),
                artifact_digest: None,
            },
            dispatch: OperationDispatch {
                provider: self.provider_ref.clone(),
                route: operation_key.to_string(),
            },
        })
    }

    fn scoped_backend_id(
        scope: &OperationAuthorizationScope,
    ) -> Result<Option<&str>, OperationExecutionError> {
        match &scope.scope_ref {
            OperationScopeRef::EnvironmentSetup { backend_id, .. } => Ok(backend_id.as_deref()),
            _ => Err(OperationExecutionError::invalid_request(
                "Setup Operation 必须使用 EnvironmentSetup scope",
            )),
        }
    }
}

fn setup_operation_schemas(operation_key: &str) -> Result<(Value, Value), OperationExecutionError> {
    let schemas = match operation_key {
        MCP_PROBE_TRANSPORT_ACTION => (
            json!({
                "type": "object",
                "required": ["transport", "current_user"],
                "properties": {
                    "transport": true,
                    "route_policy": true,
                    "probe_target": true,
                    "current_user": { "type": "object" },
                    "runtime_binding": true
                },
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "required": ["status"],
                "properties": {
                    "status": { "type": "string", "enum": ["ok", "error", "unsupported"] },
                    "latency_ms": { "type": "integer" },
                    "tools": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["name", "description"],
                            "properties": {
                                "name": { "type": "string" },
                                "description": { "type": "string" }
                            },
                            "additionalProperties": false
                        }
                    },
                    "error": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        WORKSPACE_DETECT_ACTION => (
            backend_root_input_schema(),
            json!({
                "type": "object",
                "required": ["identity_kind", "identity_payload", "binding", "confidence", "warnings"],
                "properties": {
                    "identity_kind": { "type": "string" },
                    "identity_payload": true,
                    "binding": { "type": "object" },
                    "confidence": { "type": "string" },
                    "warnings": { "type": "array", "items": { "type": "string" } }
                },
                "additionalProperties": false
            }),
        ),
        WORKSPACE_DETECT_GIT_ACTION => (
            backend_root_input_schema(),
            json!({
                "type": "object",
                "required": ["resolved_root_ref", "is_git_repo", "source_repo", "branch", "commit_hash"],
                "properties": {
                    "resolved_root_ref": { "type": "string" },
                    "is_git_repo": { "type": "boolean" },
                    "source_repo": { "type": ["string", "null"] },
                    "branch": { "type": ["string", "null"] },
                    "commit_hash": { "type": ["string", "null"] }
                },
                "additionalProperties": false
            }),
        ),
        WORKSPACE_BROWSE_DIRECTORY_ACTION => (
            json!({
                "type": "object",
                "required": ["backend_id", "path"],
                "properties": {
                    "backend_id": { "type": "string" },
                    "path": { "type": ["string", "null"] }
                },
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "required": ["current_path", "entries"],
                "properties": {
                    "current_path": { "type": "string" },
                    "entries": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["name", "path", "is_dir"],
                            "properties": {
                                "name": { "type": "string" },
                                "path": { "type": "string" },
                                "is_dir": { "type": "boolean" }
                            },
                            "additionalProperties": false
                        }
                    }
                },
                "additionalProperties": false
            }),
        ),
        WORKSPACE_DISCOVER_BY_IDENTITY_ACTION => (
            json!({
                "type": "object",
                "required": ["backend_id", "workspaces"],
                "properties": {
                    "backend_id": { "type": "string" },
                    "workspaces": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["workspace_id", "identity_kind", "identity_payload"],
                            "properties": {
                                "workspace_id": { "type": "string" },
                                "identity_kind": { "type": "string" },
                                "identity_payload": true
                            },
                            "additionalProperties": false
                        }
                    }
                },
                "additionalProperties": false
            }),
            json!({
                "type": "object",
                "required": ["candidates", "skipped", "warnings"],
                "properties": {
                    "candidates": { "type": "array", "items": { "type": "object" } },
                    "skipped": { "type": "array", "items": { "type": "object" } },
                    "warnings": { "type": "array", "items": { "type": "string" } }
                },
                "additionalProperties": false
            }),
        ),
        _ => {
            return Err(OperationExecutionError::invalid_request(format!(
                "未知 Setup Operation: {operation_key}"
            )));
        }
    };
    Ok(schemas)
}

fn backend_root_input_schema() -> Value {
    json!({
        "type": "object",
        "required": ["backend_id", "root_ref"],
        "properties": {
            "backend_id": { "type": "string" },
            "root_ref": { "type": "string" }
        },
        "additionalProperties": false
    })
}

#[async_trait]
impl OperationProvider for SetupOperationProvider {
    fn provider_ref(&self) -> &OperationProviderRef {
        &self.provider_ref
    }

    async fn discover(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        _cancel: CancellationToken,
    ) -> Result<Vec<OperationDescriptor>, OperationExecutionError> {
        if !matches!(
            principal.principal_ref(),
            OperationPrincipalRef::User { .. }
        ) || !matches!(scope.scope_ref, OperationScopeRef::EnvironmentSetup { .. })
            || !matches!(origin, OperationOriginRef::EnvironmentSetup)
        {
            return Ok(Vec::new());
        }
        let has_backend = matches!(
            &scope.scope_ref,
            OperationScopeRef::EnvironmentSetup {
                backend_id: Some(backend_id),
                ..
            } if !backend_id.trim().is_empty()
        );
        let mut descriptors = self.descriptors()?;
        if !has_backend {
            descriptors.retain(|descriptor| {
                descriptor.operation_ref.operation_key == MCP_PROBE_TRANSPORT_ACTION
            });
        }
        Ok(descriptors)
    }

    async fn resolve_placement(
        &self,
        descriptor: &OperationDescriptor,
        _principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        _origin: &OperationOriginRef,
        _cancel: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError> {
        if descriptor.operation_ref.operation_key == MCP_PROBE_TRANSPORT_ACTION {
            return Ok(OperationPlacement::Cloud);
        }
        let backend_id = Self::scoped_backend_id(scope)?
            .filter(|backend_id| !backend_id.trim().is_empty())
            .ok_or_else(|| {
                OperationExecutionError::invalid_request(
                    "Workspace Setup Operation 缺少 server-resolved backend placement",
                )
            })?;
        Ok(OperationPlacement::LocalBackend {
            backend_id: backend_id.to_string(),
        })
    }

    async fn invoke(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        _cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError> {
        let operation_key = descriptor.operation_ref.operation_key.as_str();
        let result: Result<Value, RuntimeGatewaySetupError> = async {
            match operation_key {
                MCP_PROBE_TRANSPORT_ACTION => {
                    let input = decode_input::<McpProbeTransportInput>(&envelope.input)?;
                    encode_output(self.mcp_probe.probe_transport(input).await?)
                }
                WORKSPACE_DETECT_ACTION => {
                    let mut input = decode_input::<WorkspaceDetectInput>(&envelope.input)?;
                    input.backend_id = placement_backend_id(&envelope)?.to_string();
                    encode_output(self.workspace_detect.detect_workspace(input).await?)
                }
                WORKSPACE_DETECT_GIT_ACTION => {
                    let mut input = decode_input::<WorkspaceDetectGitInput>(&envelope.input)?;
                    input.backend_id = placement_backend_id(&envelope)?.to_string();
                    encode_output(self.workspace_detect_git.detect_git(input).await?)
                }
                WORKSPACE_BROWSE_DIRECTORY_ACTION => {
                    let mut input = decode_input::<WorkspaceBrowseDirectoryInput>(&envelope.input)?;
                    input.backend_id = placement_backend_id(&envelope)?.to_string();
                    encode_output(self.workspace_browse.browse_directory(input).await?)
                }
                WORKSPACE_DISCOVER_BY_IDENTITY_ACTION => {
                    let mut input =
                        decode_input::<WorkspaceDiscoverByIdentityInput>(&envelope.input)?;
                    input.backend_id = placement_backend_id(&envelope)?.to_string();
                    encode_output(self.workspace_discover.discover_by_identity(input).await?)
                }
                _ => Err(RuntimeGatewaySetupError::BadRequest(format!(
                    "未知 Setup Operation: {operation_key}"
                ))),
            }
        }
        .await;
        result.map_err(map_setup_error)
    }
}

#[async_trait]
pub trait SetupOperationAccessPort: Send + Sync {
    async fn resolve_access(
        &self,
        identity: &agentdash_spi::AuthIdentity,
        scope: &OperationAuthorizationScope,
        cancel: CancellationToken,
    ) -> Result<OperationAuthorityGrant, OperationExecutionError>;
}

pub struct SetupOperationAuthorityResolver {
    access: Arc<dyn SetupOperationAccessPort>,
}

impl SetupOperationAuthorityResolver {
    pub fn new(access: Arc<dyn SetupOperationAccessPort>) -> Self {
        Self { access }
    }
}

#[async_trait]
impl OperationAuthorityResolver for SetupOperationAuthorityResolver {
    async fn resolve(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<OperationAuthorityGrant, OperationExecutionError> {
        let Some(identity) = principal.user_identity() else {
            return Err(OperationExecutionError::ActorDenied {
                actor_kind: principal.actor_kind(),
            });
        };
        if identity.user_id.trim().is_empty()
            || !matches!(scope.scope_ref, OperationScopeRef::EnvironmentSetup { .. })
            || !matches!(origin, OperationOriginRef::EnvironmentSetup)
        {
            return Err(OperationExecutionError::invalid_request(
                "Setup Operation authority 必须由 authenticated User host 解析",
            ));
        }
        self.access.resolve_access(identity, scope, cancel).await
    }
}

fn decode_input<T: DeserializeOwned>(input: &Value) -> Result<T, RuntimeGatewaySetupError> {
    serde_json::from_value(input.clone()).map_err(|error| {
        RuntimeGatewaySetupError::BadRequest(format!("Setup Operation input 非法: {error}"))
    })
}

fn encode_output<T: Serialize>(output: T) -> Result<Value, RuntimeGatewaySetupError> {
    serde_json::to_value(output)
        .map_err(|error| RuntimeGatewaySetupError::ProviderFailed(error.to_string()))
}

fn placement_backend_id(
    envelope: &OperationInvocationEnvelope,
) -> Result<&str, RuntimeGatewaySetupError> {
    match &envelope.placement {
        OperationPlacement::LocalBackend { backend_id } => Ok(backend_id),
        OperationPlacement::Cloud => Err(RuntimeGatewaySetupError::BadRequest(
            "Workspace Setup Operation 缺少 local backend placement".to_string(),
        )),
    }
}

fn map_setup_error(error: RuntimeGatewaySetupError) -> OperationExecutionError {
    match error {
        RuntimeGatewaySetupError::BadRequest(message) => {
            OperationExecutionError::invalid_request(message)
        }
        RuntimeGatewaySetupError::BackendOffline(message) => OperationExecutionError::NotReady {
            code: "backend_offline".to_string(),
            message,
        },
        RuntimeGatewaySetupError::Timeout => OperationExecutionError::DeadlineExceeded,
        RuntimeGatewaySetupError::TransportFailed(message)
        | RuntimeGatewaySetupError::ProviderFailed(message) => {
            OperationExecutionError::provider_failed(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use agentdash_application_ports::runtime_gateway_setup::{
        McpProbeTransportOutput, WorkspaceBrowseDirectoryOutput,
    };
    use agentdash_spi::{AuthIdentity, AuthMode};
    use chrono::{Duration, Utc};

    use super::*;
    use crate::{
        EphemeralOperationResultStore, OperationGateway, OperationInvocationCommand,
        OperationResultValue, OperationTraceContext, TracingOperationAuditSink,
        validate_json_schema_subset,
    };

    struct FakeSetupPorts {
        browse_calls: AtomicUsize,
    }

    #[async_trait]
    impl McpProbeSetupPort for FakeSetupPorts {
        async fn probe_transport(
            &self,
            _input: McpProbeTransportInput,
        ) -> Result<McpProbeTransportOutput, RuntimeGatewaySetupError> {
            Ok(McpProbeTransportOutput::Unsupported {
                reason: "test".to_string(),
            })
        }
    }

    #[async_trait]
    impl WorkspaceDetectSetupPort for FakeSetupPorts {
        async fn detect_workspace(
            &self,
            _input: WorkspaceDetectInput,
        ) -> Result<
            agentdash_application_ports::runtime_gateway_setup::WorkspaceDetectOutput,
            RuntimeGatewaySetupError,
        > {
            Err(RuntimeGatewaySetupError::ProviderFailed(
                "not used".to_string(),
            ))
        }
    }

    #[async_trait]
    impl WorkspaceDetectGitSetupPort for FakeSetupPorts {
        async fn detect_git(
            &self,
            _input: WorkspaceDetectGitInput,
        ) -> Result<
            agentdash_application_ports::runtime_gateway_setup::WorkspaceDetectGitOutput,
            RuntimeGatewaySetupError,
        > {
            Err(RuntimeGatewaySetupError::ProviderFailed(
                "not used".to_string(),
            ))
        }
    }

    #[async_trait]
    impl WorkspaceBrowseDirectorySetupPort for FakeSetupPorts {
        async fn browse_directory(
            &self,
            _input: WorkspaceBrowseDirectoryInput,
        ) -> Result<WorkspaceBrowseDirectoryOutput, RuntimeGatewaySetupError> {
            self.browse_calls.fetch_add(1, Ordering::SeqCst);
            Ok(WorkspaceBrowseDirectoryOutput {
                current_path: "/tmp".to_string(),
                entries: Vec::new(),
            })
        }
    }

    #[async_trait]
    impl WorkspaceDiscoverByIdentitySetupPort for FakeSetupPorts {
        async fn discover_by_identity(
            &self,
            _input: WorkspaceDiscoverByIdentityInput,
        ) -> Result<
            agentdash_application_ports::runtime_gateway_setup::WorkspaceDiscoverByIdentityOutput,
            RuntimeGatewaySetupError,
        > {
            Err(RuntimeGatewaySetupError::ProviderFailed(
                "not used".to_string(),
            ))
        }
    }

    struct SequencedAccess {
        calls: AtomicUsize,
        revoke_on_call: Option<usize>,
    }

    #[async_trait]
    impl SetupOperationAccessPort for SequencedAccess {
        async fn resolve_access(
            &self,
            _identity: &AuthIdentity,
            scope: &OperationAuthorizationScope,
            _cancel: CancellationToken,
        ) -> Result<OperationAuthorityGrant, OperationExecutionError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            let has_backend = matches!(
                scope.scope_ref,
                OperationScopeRef::EnvironmentSetup {
                    backend_id: Some(_),
                    ..
                }
            );
            let mut capabilities = BTreeSet::from(["setup.mcp_probe".to_string()]);
            if has_backend && self.revoke_on_call.is_none_or(|revoke| call < revoke) {
                capabilities.insert("setup.workspace".to_string());
            }
            Ok(OperationAuthorityGrant {
                authority_revision: "test-authority".to_string(),
                capabilities,
            })
        }
    }

    fn test_identity() -> AuthIdentity {
        AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: "user-1".to_string(),
            subject: "user-1".to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: Vec::new(),
            is_admin: false,
            provider: None,
            extra: Value::Null,
        }
    }

    fn provider(ports: Arc<FakeSetupPorts>) -> Arc<SetupOperationProvider> {
        SetupOperationProvider::new(
            ports.clone(),
            ports.clone(),
            ports.clone(),
            ports.clone(),
            ports,
        )
        .into()
    }

    fn command(backend_id: Option<&str>, input: Value) -> OperationInvocationCommand {
        OperationInvocationCommand {
            operation_ref: setup_operation_ref(WORKSPACE_BROWSE_DIRECTORY_ACTION)
                .expect("valid operation ref"),
            input,
            principal: OperationPrincipal::authenticated_user(test_identity()),
            scope_ref: OperationScopeRef::EnvironmentSetup {
                project_id: None,
                workspace_id: None,
                backend_id: backend_id.map(str::to_string),
            },
            origin: OperationOriginRef::EnvironmentSetup,
            trace: OperationTraceContext::root(),
            deadline: Utc::now() + Duration::seconds(5),
            idempotency_key: None,
            attachment_ref: None,
        }
    }

    fn gateway(ports: Arc<FakeSetupPorts>, access: Arc<SequencedAccess>) -> OperationGateway {
        OperationGateway::try_new(
            Arc::new(SetupOperationAuthorityResolver::new(access)),
            [provider(ports) as Arc<dyn OperationProvider>],
            [],
            Arc::new(EphemeralOperationResultStore::default()),
            Arc::new(TracingOperationAuditSink),
        )
        .expect("valid setup operation gateway")
    }

    #[tokio::test]
    async fn malformed_input_is_rejected_before_setup_port() {
        let ports = Arc::new(FakeSetupPorts {
            browse_calls: AtomicUsize::new(0),
        });
        let gateway = gateway(
            ports.clone(),
            Arc::new(SequencedAccess {
                calls: AtomicUsize::new(0),
                revoke_on_call: None,
            }),
        );
        let error = gateway
            .invoke(
                command(Some("local"), json!({ "backend_id": "forged" })),
                CancellationToken::new(),
            )
            .await
            .expect_err("missing path must fail schema validation");
        assert_eq!(
            error.kind(),
            crate::OperationExecutionErrorKind::InvalidRequest
        );
        assert_eq!(ports.browse_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn missing_backend_scope_does_not_expose_workspace_operations() {
        let ports = Arc::new(FakeSetupPorts {
            browse_calls: AtomicUsize::new(0),
        });
        let gateway = gateway(
            ports.clone(),
            Arc::new(SequencedAccess {
                calls: AtomicUsize::new(0),
                revoke_on_call: None,
            }),
        );
        let error = gateway
            .invoke(
                command(None, json!({ "backend_id": "forged", "path": null })),
                CancellationToken::new(),
            )
            .await
            .expect_err("workspace operation needs server-resolved backend scope");
        assert_eq!(
            error.kind(),
            crate::OperationExecutionErrorKind::Unavailable
        );
        assert_eq!(ports.browse_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn capability_revocation_is_rechecked_before_dispatch() {
        let ports = Arc::new(FakeSetupPorts {
            browse_calls: AtomicUsize::new(0),
        });
        let gateway = gateway(
            ports.clone(),
            Arc::new(SequencedAccess {
                calls: AtomicUsize::new(0),
                revoke_on_call: Some(3),
            }),
        );
        let error = gateway
            .invoke(
                command(
                    Some("local"),
                    json!({ "backend_id": "forged", "path": null }),
                ),
                CancellationToken::new(),
            )
            .await
            .expect_err("revoked setup.workspace must fail before dispatch");
        assert_eq!(error.kind(), crate::OperationExecutionErrorKind::Denied);
        assert_eq!(ports.browse_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn structured_output_schema_rejects_invalid_shape() {
        let (_, output_schema) = setup_operation_schemas(WORKSPACE_BROWSE_DIRECTORY_ACTION)
            .expect("known setup operation");
        assert!(
            validate_json_schema_subset(
                &output_schema,
                &json!({ "current_path": "/tmp", "entries": [{ "name": "a" }] })
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn server_placement_overrides_backend_id_in_typed_input() {
        let ports = Arc::new(FakeSetupPorts {
            browse_calls: AtomicUsize::new(0),
        });
        let gateway = gateway(
            ports.clone(),
            Arc::new(SequencedAccess {
                calls: AtomicUsize::new(0),
                revoke_on_call: None,
            }),
        );
        let result = gateway
            .invoke(
                command(
                    Some("resolved-local"),
                    json!({ "backend_id": "forged", "path": null }),
                ),
                CancellationToken::new(),
            )
            .await
            .expect("valid browse should execute");
        assert!(matches!(result.value, OperationResultValue::Inline { .. }));
        assert_eq!(ports.browse_calls.load(Ordering::SeqCst), 1);
    }
}
