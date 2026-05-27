use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_domain::shared_library::{
    EXTENSION_PERMISSION_LOCAL_PROFILE_READ, ExtensionPermissionDecision,
    ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ProjectExtensionInstallation,
    ProjectExtensionInstallationRepository,
};
use agentdash_relay::{
    CommandExtensionActionInvokePayload, ExtensionPackageArtifactRelay,
    ResponseExtensionActionInvokePayload,
};
use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use super::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeContext,
    RuntimeInvocationError, RuntimeInvocationOutput, RuntimeInvocationRequest, RuntimeProvider,
    RuntimeTarget,
};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ExtensionRuntimeActionTransportError {
    #[error("backend offline: {backend_id}")]
    Offline { backend_id: String },
    #[error("backend command timeout: {backend_id}")]
    Timeout { backend_id: String },
    #[error("backend response dropped: {backend_id}")]
    ResponseDropped { backend_id: String },
    #[error("extension action relay failed: {0}")]
    Failed(String),
}

#[async_trait]
pub trait ExtensionRuntimeActionTransport: Send + Sync {
    async fn invoke_extension_action(
        &self,
        backend_id: &str,
        payload: CommandExtensionActionInvokePayload,
    ) -> Result<ResponseExtensionActionInvokePayload, ExtensionRuntimeActionTransportError>;
}

pub struct ExtensionRuntimeActionProvider {
    marker_key: RuntimeActionKey,
    installations: Arc<dyn ProjectExtensionInstallationRepository>,
    transport: Arc<dyn ExtensionRuntimeActionTransport>,
}

impl ExtensionRuntimeActionProvider {
    pub fn new(
        installations: Arc<dyn ProjectExtensionInstallationRepository>,
        transport: Arc<dyn ExtensionRuntimeActionTransport>,
    ) -> Self {
        Self {
            marker_key: RuntimeActionKey::parse("extension.runtime_action")
                .expect("builtin runtime action key should be valid"),
            installations,
            transport,
        }
    }
}

#[async_trait]
impl RuntimeProvider for ExtensionRuntimeActionProvider {
    fn action_key(&self) -> &RuntimeActionKey {
        &self.marker_key
    }

    fn action_kind(&self) -> RuntimeActionKind {
        RuntimeActionKind::SessionRuntime
    }

    fn describe_action(&self) -> RuntimeActionDescriptor {
        RuntimeActionDescriptor {
            action_key: self.marker_key.clone(),
            kind: RuntimeActionKind::SessionRuntime,
            description: Some("Project enabled extension runtime action proxy".to_string()),
            input_schema: None,
            output_schema: None,
            default_policy: Default::default(),
        }
    }

    fn supports(&self, action_key: &RuntimeActionKey, context: &RuntimeContext) -> bool {
        action_key.as_str().contains('.')
            && context.action_kind() == RuntimeActionKind::SessionRuntime
    }

    async fn invoke(
        &self,
        request: RuntimeInvocationRequest,
    ) -> Result<RuntimeInvocationOutput, RuntimeInvocationError> {
        let (session_id, project_id) = session_project(&request)?;
        let backend_id = backend_target(&request)?;
        let installations = self
            .installations
            .list_enabled_by_project(project_id)
            .await
            .map_err(|error| {
                RuntimeInvocationError::provider_failed(
                    format!("读取 Project extension installation 失败: {error}"),
                    Some(request.trace.clone()),
                )
            })?;

        let action_key = request.action_key.as_str();
        let (installation, action) = installations
            .iter()
            .find_map(|installation| {
                installation
                    .manifest
                    .runtime_actions
                    .iter()
                    .find(|action| action.action_key == action_key)
                    .map(|action| (installation, action))
            })
            .ok_or_else(|| {
                RuntimeInvocationError::capability_denied(
                    format!("extension runtime action 未启用或不可见: {action_key}"),
                    Some(request.trace.clone()),
                )
            })?;

        if action.kind != ExtensionRuntimeActionKind::SessionRuntime {
            return Err(RuntimeInvocationError::capability_denied(
                format!("extension action 不是 Session Runtime action: {action_key}"),
                Some(request.trace.clone()),
            ));
        }
        let permission_decisions = validate_action_permissions(installation, action, &request)?;

        let relay_payload = CommandExtensionActionInvokePayload {
            extension_key: installation.extension_key.clone(),
            extension_id: installation.manifest.extension_id.clone(),
            action_key: action.action_key.clone(),
            project_id: project_id.to_string(),
            session_id,
            input: request.input.clone(),
            package_artifact: installation.package_artifact.as_ref().map(|artifact| {
                ExtensionPackageArtifactRelay {
                    artifact_id: artifact.artifact_id.to_string(),
                    archive_digest: artifact.archive_digest.clone(),
                }
            }),
            trace_id: request.trace.trace_id.clone(),
            invocation_id: request.trace.invocation_id.clone(),
        };

        let response = self
            .transport
            .invoke_extension_action(&backend_id, relay_payload)
            .await
            .map_err(|error| transport_error_to_invocation(error, &request))?;

        let mut metadata = BTreeMap::new();
        metadata.insert("extension_key".to_string(), json!(response.extension_key));
        metadata.insert("extension_id".to_string(), json!(response.extension_id));
        metadata.insert("action_key".to_string(), json!(response.action_key));
        metadata.insert("backend_id".to_string(), json!(backend_id));
        metadata.insert("trace_id".to_string(), json!(request.trace.trace_id));
        metadata.insert(
            "invocation_id".to_string(),
            json!(request.trace.invocation_id),
        );
        for (key, value) in response.metadata {
            metadata.insert(key, value);
        }
        if !permission_decisions.is_empty() {
            metadata.insert(
                "permission_decisions".to_string(),
                serde_json::to_value(permission_decisions).map_err(|error| {
                    RuntimeInvocationError::provider_failed(
                        format!("序列化 extension permission decision 失败: {error}"),
                        Some(request.trace.clone()),
                    )
                })?,
            );
        }

        Ok(RuntimeInvocationOutput {
            output: response.output,
            metadata,
        })
    }
}

fn session_project(
    request: &RuntimeInvocationRequest,
) -> Result<(String, Uuid), RuntimeInvocationError> {
    match &request.context {
        RuntimeContext::Session {
            session_id,
            project_id: Some(project_id),
            ..
        } if !session_id.trim().is_empty() => Ok((session_id.clone(), *project_id)),
        RuntimeContext::Session {
            project_id: None, ..
        } => Err(RuntimeInvocationError::invalid_request(
            "extension runtime action 必须绑定 Project scoped Session context",
            Some(request.trace.clone()),
        )),
        _ => Err(RuntimeInvocationError::invalid_request(
            "extension runtime action 必须使用 Session context",
            Some(request.trace.clone()),
        )),
    }
}

fn backend_target(request: &RuntimeInvocationRequest) -> Result<String, RuntimeInvocationError> {
    match &request.target {
        Some(RuntimeTarget::Backend { backend_id }) if !backend_id.trim().is_empty() => {
            Ok(backend_id.clone())
        }
        _ => Err(RuntimeInvocationError::invalid_request(
            "extension runtime action 必须指定 Backend target",
            Some(request.trace.clone()),
        )),
    }
}

fn validate_action_permissions(
    installation: &ProjectExtensionInstallation,
    action: &ExtensionRuntimeActionDefinition,
    request: &RuntimeInvocationRequest,
) -> Result<Vec<ExtensionPermissionDecision>, RuntimeInvocationError> {
    let mut decisions = Vec::new();
    for permission in &action.permissions {
        let decision = installation
            .manifest
            .evaluate_action_permission(&action.action_key, permission);
        if !decision.allowed {
            return Err(RuntimeInvocationError::capability_denied(
                decision.denial_message(),
                Some(request.trace.clone()),
            ));
        }
        decisions.push(decision);
    }
    if installation.manifest.grants_local_profile_read()
        && !action
            .permissions
            .iter()
            .any(|permission| permission == EXTENSION_PERMISSION_LOCAL_PROFILE_READ)
    {
        let decision = installation.manifest.evaluate_action_permission(
            &action.action_key,
            EXTENSION_PERMISSION_LOCAL_PROFILE_READ,
        );
        return Err(RuntimeInvocationError::capability_denied(
            decision.denial_message(),
            Some(request.trace.clone()),
        ));
    }
    Ok(decisions)
}

fn transport_error_to_invocation(
    error: ExtensionRuntimeActionTransportError,
    request: &RuntimeInvocationRequest,
) -> RuntimeInvocationError {
    match error {
        ExtensionRuntimeActionTransportError::Offline { backend_id } => {
            RuntimeInvocationError::conflict(
                format!("extension action backend offline: {backend_id}"),
                Some(request.trace.clone()),
            )
        }
        ExtensionRuntimeActionTransportError::Timeout { backend_id } => {
            RuntimeInvocationError::timeout(
                format!("extension action backend timeout: {backend_id}"),
                Some(request.trace.clone()),
            )
        }
        ExtensionRuntimeActionTransportError::ResponseDropped { backend_id } => {
            RuntimeInvocationError::provider_failed(
                format!("extension action backend response dropped: {backend_id}"),
                Some(request.trace.clone()),
            )
        }
        ExtensionRuntimeActionTransportError::Failed(message) => {
            RuntimeInvocationError::provider_failed(message, Some(request.trace.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionPermissionAccess, ExtensionPermissionDeclaration,
        ExtensionRuntimeActionDefinition, ExtensionTemplatePayload, InstalledAssetSource,
        ProjectExtensionInstallation,
    };
    use serde_json::json;

    use super::*;
    use crate::runtime_gateway::{
        RuntimeActor, RuntimeGateway, RuntimeInvocationErrorKind, RuntimeTarget,
    };

    #[derive(Default)]
    struct FakeInstallationRepo {
        installations: Vec<ProjectExtensionInstallation>,
    }

    #[async_trait]
    impl ProjectExtensionInstallationRepository for FakeInstallationRepo {
        async fn create(
            &self,
            _installation: &ProjectExtensionInstallation,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn update(
            &self,
            _installation: &ProjectExtensionInstallation,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_project_and_key(
            &self,
            _project_id: Uuid,
            _extension_key: &str,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(None)
        }

        async fn get_by_project_and_id(
            &self,
            _project_id: Uuid,
            _installation_id: Uuid,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(None)
        }

        async fn list_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self.installations.clone())
        }

        async fn list_enabled_by_project(
            &self,
            _project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self.installations.clone())
        }

        async fn delete(
            &self,
            _project_id: Uuid,
            _installation_id: Uuid,
        ) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    struct FakeTransport {
        result: Result<ResponseExtensionActionInvokePayload, ExtensionRuntimeActionTransportError>,
        last_payload: StdMutex<Option<CommandExtensionActionInvokePayload>>,
    }

    #[async_trait]
    impl ExtensionRuntimeActionTransport for FakeTransport {
        async fn invoke_extension_action(
            &self,
            backend_id: &str,
            payload: CommandExtensionActionInvokePayload,
        ) -> Result<ResponseExtensionActionInvokePayload, ExtensionRuntimeActionTransportError>
        {
            assert_eq!(backend_id, "backend-1");
            *self.last_payload.lock().expect("lock") = Some(payload);
            self.result.clone()
        }
    }

    #[tokio::test]
    async fn gateway_invokes_enabled_extension_action() {
        let project_id = Uuid::new_v4();
        let transport = Arc::new(FakeTransport {
            result: Ok(response_payload(json!({ "username": "local-user" }))),
            last_payload: StdMutex::new(None),
        });
        let gateway = RuntimeGateway::new().with_dynamic_provider(Arc::new(
            ExtensionRuntimeActionProvider::new(
                Arc::new(FakeInstallationRepo {
                    installations: vec![installation(project_id, true, true)],
                }),
                transport.clone(),
            ),
        ));

        let result = gateway
            .invoke(request(project_id, "local-hello.profile"))
            .await
            .expect("invoke");

        assert_eq!(result.output.output["username"], "local-user");
        assert_eq!(result.output.metadata["extension_id"], "local-hello");
        assert_eq!(result.output.metadata["backend_id"], "backend-1");
        assert_eq!(
            result.output.metadata["permission_decisions"][0]["requested_permission"],
            "local.profile.read"
        );
        let payload = transport
            .last_payload
            .lock()
            .expect("lock")
            .clone()
            .expect("payload");
        assert_eq!(payload.trace_id, result.trace.trace_id);
        assert_eq!(payload.action_key, "local-hello.profile");
    }

    #[tokio::test]
    async fn missing_extension_action_is_capability_denied() {
        let project_id = Uuid::new_v4();
        let gateway = RuntimeGateway::new().with_dynamic_provider(Arc::new(
            ExtensionRuntimeActionProvider::new(
                Arc::new(FakeInstallationRepo::default()),
                Arc::new(FakeTransport {
                    result: Ok(response_payload(json!({}))),
                    last_payload: StdMutex::new(None),
                }),
            ),
        ));

        let err = gateway
            .invoke(request(project_id, "local-hello.profile"))
            .await
            .expect_err("missing action");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }

    #[tokio::test]
    async fn undeclared_action_permission_is_rejected() {
        let project_id = Uuid::new_v4();
        let gateway = RuntimeGateway::new().with_dynamic_provider(Arc::new(
            ExtensionRuntimeActionProvider::new(
                Arc::new(FakeInstallationRepo {
                    installations: vec![installation(project_id, false, true)],
                }),
                Arc::new(FakeTransport {
                    result: Ok(response_payload(json!({}))),
                    last_payload: StdMutex::new(None),
                }),
            ),
        ));

        let err = gateway
            .invoke(request(project_id, "local-hello.profile"))
            .await
            .expect_err("permission denied");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }

    #[tokio::test]
    async fn missing_action_permission_is_rejected() {
        let project_id = Uuid::new_v4();
        let gateway = RuntimeGateway::new().with_dynamic_provider(Arc::new(
            ExtensionRuntimeActionProvider::new(
                Arc::new(FakeInstallationRepo {
                    installations: vec![installation(project_id, true, false)],
                }),
                Arc::new(FakeTransport {
                    result: Ok(response_payload(json!({}))),
                    last_payload: StdMutex::new(None),
                }),
            ),
        ));

        let err = gateway
            .invoke(request(project_id, "local-hello.profile"))
            .await
            .expect_err("permission denied");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }

    #[tokio::test]
    async fn unknown_action_permission_is_rejected() {
        let project_id = Uuid::new_v4();
        let mut installation = installation(project_id, true, true);
        installation.manifest.runtime_actions[0].permissions = vec!["local.profile.admin".into()];
        let gateway = RuntimeGateway::new().with_dynamic_provider(Arc::new(
            ExtensionRuntimeActionProvider::new(
                Arc::new(FakeInstallationRepo {
                    installations: vec![installation],
                }),
                Arc::new(FakeTransport {
                    result: Ok(response_payload(json!({}))),
                    last_payload: StdMutex::new(None),
                }),
            ),
        ));

        let err = gateway
            .invoke(request(project_id, "local-hello.profile"))
            .await
            .expect_err("permission denied");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }

    #[tokio::test]
    async fn offline_backend_maps_to_conflict() {
        let project_id = Uuid::new_v4();
        let gateway = RuntimeGateway::new().with_dynamic_provider(Arc::new(
            ExtensionRuntimeActionProvider::new(
                Arc::new(FakeInstallationRepo {
                    installations: vec![installation(project_id, true, true)],
                }),
                Arc::new(FakeTransport {
                    result: Err(ExtensionRuntimeActionTransportError::Offline {
                        backend_id: "backend-1".to_string(),
                    }),
                    last_payload: StdMutex::new(None),
                }),
            ),
        ));

        let err = gateway
            .invoke(request(project_id, "local-hello.profile"))
            .await
            .expect_err("offline");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::Conflict);
    }

    #[test]
    fn provider_supports_session_extension_action_shape() {
        let provider = ExtensionRuntimeActionProvider::new(
            Arc::new(FakeInstallationRepo::default()),
            Arc::new(FakeTransport {
                result: Ok(response_payload(json!({}))),
                last_payload: StdMutex::new(None),
            }),
        );
        assert!(provider.supports(
            &RuntimeActionKey::parse("local-hello.profile").expect("key"),
            &RuntimeContext::Session {
                session_id: "session-1".to_string(),
                project_id: Some(Uuid::new_v4()),
                workspace_id: None,
            },
        ));
    }

    fn request(project_id: Uuid, action_key: &str) -> RuntimeInvocationRequest {
        let mut request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(action_key).expect("key"),
            RuntimeActor::SessionUser {
                session_id: "session-1".to_string(),
                user_id: None,
            },
            RuntimeContext::Session {
                session_id: "session-1".to_string(),
                project_id: Some(project_id),
                workspace_id: None,
            },
            json!({}),
        );
        request.target = Some(RuntimeTarget::Backend {
            backend_id: "backend-1".to_string(),
        });
        request
    }

    fn response_payload(output: serde_json::Value) -> ResponseExtensionActionInvokePayload {
        ResponseExtensionActionInvokePayload {
            extension_key: "local-hello".to_string(),
            extension_id: "local-hello".to_string(),
            action_key: "local-hello.profile".to_string(),
            output,
            metadata: Default::default(),
        }
    }

    fn installation(
        project_id: Uuid,
        include_top_level_permission: bool,
        include_action_permission: bool,
    ) -> ProjectExtensionInstallation {
        let manifest = manifest(include_top_level_permission, include_action_permission);
        manifest.validate().expect("manifest");
        ProjectExtensionInstallation::new(
            project_id,
            "local-hello",
            "Local Hello",
            manifest,
            InstalledAssetSource::new(
                Uuid::new_v4(),
                "plugin:test:extension_template:local-hello",
                "0.1.0",
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        )
        .expect("installation")
    }

    fn manifest(
        include_top_level_permission: bool,
        include_action_permission: bool,
    ) -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: "local-hello".to_string(),
            package: ExtensionPackageMetadata {
                name: "@agentdash/local-hello".to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![ExtensionRuntimeActionDefinition {
                action_key: "local-hello.profile".to_string(),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "Read profile".to_string(),
                input_schema: json!({}),
                output_schema: json!({}),
                permissions: if include_action_permission {
                    vec!["local.profile.read".to_string()]
                } else {
                    vec![]
                },
            }],
            workspace_tabs: vec![],
            permissions: if include_top_level_permission {
                vec![ExtensionPermissionDeclaration::LocalProfile {
                    access: ExtensionPermissionAccess::Read,
                }]
            } else {
                vec![]
            },
            bundles: vec![],
        }
    }
}
