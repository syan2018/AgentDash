use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use agentdash_application_ports::extension_runtime::{
    ExtensionActionInvokeRequest, ExtensionBackendServiceInvokeRequest,
    ExtensionBackendServiceTransport, ExtensionInvocationWorkspacePayload,
    ExtensionPackageArtifactPayload, ExtensionProtocolConsumerPayload,
    ExtensionProtocolInvokeRequest, ExtensionRuntimeActionTransport, ExtensionRuntimeHostPayload,
    ExtensionRuntimeProtocolTransport,
};
use agentdash_domain::operation::{
    OperationEffect, OperationProviderRef, OperationRef, OperationReplayPolicy,
};
use agentdash_domain::shared_library::{
    ExtensionGeneratedOperationDefinition, ExtensionGeneratedOperationDispatch,
    ExtensionGeneratedOperationVisibility, ProjectExtensionInstallation,
    ProjectExtensionInstallationRepository,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    DynamicOperationProvider, OperationActorKind, OperationAuthorizationScope, OperationDescriptor,
    OperationDispatch, OperationExecutionError, OperationExecutionPolicy,
    OperationInvocationEnvelope, OperationOriginRef, OperationPlacement, OperationPrincipal,
    OperationProvenance, OperationReadiness,
};

pub const EXTENSION_OPERATION_NAMESPACE: &str = "extension";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionOperationRuntimeContext {
    pub project_id: Uuid,
    pub backend_id: Option<String>,
    pub workspace: Option<ExtensionInvocationWorkspacePayload>,
}

#[async_trait]
pub trait ExtensionOperationContextPort: Send + Sync {
    async fn resolve_context(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<ExtensionOperationRuntimeContext, OperationExecutionError>;
}

pub struct ExtensionOperationProvider {
    installations: Arc<dyn ProjectExtensionInstallationRepository>,
    context: Arc<dyn ExtensionOperationContextPort>,
    actions: Arc<dyn ExtensionRuntimeActionTransport>,
    protocols: Arc<dyn ExtensionRuntimeProtocolTransport>,
    backend_services: Arc<dyn ExtensionBackendServiceTransport>,
}

impl ExtensionOperationProvider {
    pub fn new(
        installations: Arc<dyn ProjectExtensionInstallationRepository>,
        context: Arc<dyn ExtensionOperationContextPort>,
        actions: Arc<dyn ExtensionRuntimeActionTransport>,
        protocols: Arc<dyn ExtensionRuntimeProtocolTransport>,
        backend_services: Arc<dyn ExtensionBackendServiceTransport>,
    ) -> Self {
        Self {
            installations,
            context,
            actions,
            protocols,
            backend_services,
        }
    }

    async fn installations(
        &self,
        runtime: &ExtensionOperationRuntimeContext,
    ) -> Result<Vec<ProjectExtensionInstallation>, OperationExecutionError> {
        self.installations
            .list_enabled_by_project(runtime.project_id)
            .await
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))
    }

    async fn resolve_operation(
        &self,
        operation_ref: &OperationRef,
        runtime: &ExtensionOperationRuntimeContext,
    ) -> Result<
        (
            Vec<ProjectExtensionInstallation>,
            ProjectExtensionInstallation,
            ExtensionGeneratedOperationDefinition,
        ),
        OperationExecutionError,
    > {
        let installations = self.installations(runtime).await?;
        let resolved = installations
            .iter()
            .find(|installation| installation.extension_key == operation_ref.provider.provider_key)
            .and_then(|installation| {
                installation
                    .manifest
                    .operation_catalog
                    .iter()
                    .find(|operation| operation.operation_key == operation_ref.operation_key)
                    .cloned()
                    .map(|operation| (installation.clone(), operation))
            })
            .ok_or_else(|| OperationExecutionError::OperationUnavailable {
                operation_ref: operation_ref.clone(),
            })?;
        Ok((installations, resolved.0, resolved.1))
    }
}

#[async_trait]
impl DynamicOperationProvider for ExtensionOperationProvider {
    fn owns_provider(&self, provider: &OperationProviderRef) -> bool {
        provider.namespace == EXTENSION_OPERATION_NAMESPACE
    }

    async fn discover(
        &self,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<Vec<OperationDescriptor>, OperationExecutionError> {
        let runtime = self
            .context
            .resolve_context(principal, scope, origin, cancel)
            .await?;
        let mut descriptors = Vec::new();
        for installation in self.installations(&runtime).await? {
            for operation in &installation.manifest.operation_catalog {
                if operation.visibility == ExtensionGeneratedOperationVisibility::PanelOnly
                    && principal.actor_kind() != OperationActorKind::User
                {
                    continue;
                }
                descriptors.push(descriptor_from_operation(
                    &installation,
                    operation,
                    &runtime,
                )?);
            }
        }
        Ok(descriptors)
    }

    async fn resolve_placement(
        &self,
        descriptor: &OperationDescriptor,
        principal: &OperationPrincipal,
        scope: &OperationAuthorizationScope,
        origin: &OperationOriginRef,
        cancel: CancellationToken,
    ) -> Result<OperationPlacement, OperationExecutionError> {
        let runtime = self
            .context
            .resolve_context(principal, scope, origin, cancel)
            .await?;
        self.resolve_operation(&descriptor.operation_ref, &runtime)
            .await?;
        runtime
            .backend_id
            .map(|backend_id| OperationPlacement::LocalBackend { backend_id })
            .ok_or_else(|| OperationExecutionError::NotReady {
                code: "extension_backend_unavailable".to_string(),
                message: "Extension Operation 缺少 authorized backend placement".to_string(),
            })
    }

    async fn invoke(
        &self,
        descriptor: &OperationDescriptor,
        envelope: OperationInvocationEnvelope,
        cancel: CancellationToken,
    ) -> Result<Value, OperationExecutionError> {
        let runtime = self
            .context
            .resolve_context(
                &envelope.principal,
                &envelope.scope,
                &envelope.origin,
                cancel.clone(),
            )
            .await?;
        let backend_id =
            runtime
                .backend_id
                .clone()
                .ok_or_else(|| OperationExecutionError::NotReady {
                    code: "extension_backend_unavailable".to_string(),
                    message: "Extension Operation 缺少 authorized backend placement".to_string(),
                })?;
        let (installations, installation, operation) = self
            .resolve_operation(&descriptor.operation_ref, &runtime)
            .await?;
        if cancel.is_cancelled() {
            return Err(OperationExecutionError::Cancelled);
        }
        dispatch_operation(
            self,
            installations,
            installation,
            operation,
            runtime,
            backend_id,
            envelope,
        )
        .await
    }
}

fn descriptor_from_operation(
    installation: &ProjectExtensionInstallation,
    operation: &ExtensionGeneratedOperationDefinition,
    runtime: &ExtensionOperationRuntimeContext,
) -> Result<OperationDescriptor, OperationExecutionError> {
    let operation_ref = OperationRef::new(
        EXTENSION_OPERATION_NAMESPACE,
        installation.extension_key.clone(),
        operation.operation_key.clone(),
        1,
    )
    .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
    let artifact_digest = installation
        .package_artifact
        .as_ref()
        .map(|artifact| artifact.archive_digest.clone());
    let readiness = if installation.package_artifact.is_none() {
        OperationReadiness::Unavailable {
            code: "extension_artifact_missing".to_string(),
            message: format!(
                "Extension `{}` 缺少 package artifact",
                installation.extension_key
            ),
        }
    } else if runtime.backend_id.is_none() {
        OperationReadiness::Unavailable {
            code: "extension_backend_unavailable".to_string(),
            message: "当前 surface 没有 authorized backend placement".to_string(),
        }
    } else {
        OperationReadiness::Ready
    };
    let actor_visibility = match operation.visibility {
        ExtensionGeneratedOperationVisibility::PanelOnly => {
            BTreeSet::from([OperationActorKind::User])
        }
        ExtensionGeneratedOperationVisibility::AgentAndPanel => {
            BTreeSet::from([OperationActorKind::User, OperationActorKind::Agent])
        }
    };
    Ok(OperationDescriptor {
        title: operation.operation_key.clone(),
        description: Some(operation.description.clone()),
        input_schema: operation.input_schema.clone(),
        output_schema: operation.output_schema.clone(),
        effect: OperationEffect::ExternalSideEffect,
        replay_policy: OperationReplayPolicy::NonReplayable,
        required_capabilities: BTreeSet::from([format!(
            "extension:{}",
            installation.extension_key
        )]),
        actor_visibility,
        execution_policy: OperationExecutionPolicy::default(),
        readiness,
        provenance: OperationProvenance {
            source: operation.provenance.generated_from.clone(),
            artifact_digest,
        },
        dispatch: OperationDispatch {
            provider: operation_ref.provider.clone(),
            route: dispatch_route(&operation.dispatch),
        },
        operation_ref,
    })
}

fn dispatch_route(dispatch: &ExtensionGeneratedOperationDispatch) -> String {
    match dispatch {
        ExtensionGeneratedOperationDispatch::RuntimeAction { action_key } => {
            format!("action:{action_key}")
        }
        ExtensionGeneratedOperationDispatch::ProtocolMethod {
            protocol_key,
            method,
        } => format!("protocol:{protocol_key}/{method}"),
        ExtensionGeneratedOperationDispatch::BackendService { service_key, route } => {
            format!("service:{service_key}{route}")
        }
    }
}

async fn dispatch_operation(
    provider: &ExtensionOperationProvider,
    installations: Vec<ProjectExtensionInstallation>,
    installation: ProjectExtensionInstallation,
    operation: ExtensionGeneratedOperationDefinition,
    runtime: ExtensionOperationRuntimeContext,
    backend_id: String,
    envelope: OperationInvocationEnvelope,
) -> Result<Value, OperationExecutionError> {
    let artifact = installation.package_artifact.as_ref().ok_or_else(|| {
        OperationExecutionError::NotReady {
            code: "extension_artifact_missing".to_string(),
            message: format!(
                "Extension `{}` 缺少 package artifact",
                installation.extension_key
            ),
        }
    })?;
    let package_artifact = ExtensionPackageArtifactPayload {
        artifact_id: artifact.artifact_id.to_string(),
        archive_digest: artifact.archive_digest.clone(),
    };
    let execution_id = envelope.trace.invocation_id.clone();
    match operation.dispatch {
        ExtensionGeneratedOperationDispatch::RuntimeAction { action_key } => provider
            .actions
            .invoke_extension_action(
                &backend_id,
                ExtensionActionInvokeRequest {
                    extension_key: installation.extension_key.clone(),
                    extension_id: installation.manifest.extension_id.clone(),
                    action_key,
                    project_id: runtime.project_id.to_string(),
                    execution_id,
                    input: envelope.input,
                    package_artifact: Some(package_artifact),
                    runtime_extensions: runtime_hosts(&installations),
                    workspace: runtime.workspace,
                    trace_id: envelope.trace.trace_id,
                    invocation_id: envelope.trace.invocation_id,
                },
            )
            .await
            .map(|response| response.output)
            .map_err(|error| OperationExecutionError::provider_failed(error.to_string())),
        ExtensionGeneratedOperationDispatch::ProtocolMethod {
            protocol_key,
            method,
        } => {
            let protocol = installation
                .manifest
                .protocols
                .iter()
                .find(|protocol| protocol.protocol_key == protocol_key)
                .ok_or_else(|| OperationExecutionError::invalid_request("protocol 不存在"))?;
            provider
                .protocols
                .invoke_extension_protocol(
                    &backend_id,
                    ExtensionProtocolInvokeRequest {
                        provider_extension_key: installation.extension_key.clone(),
                        provider_extension_id: installation.manifest.extension_id.clone(),
                        protocol_key,
                        protocol_version: protocol.version.clone(),
                        method,
                        project_id: runtime.project_id.to_string(),
                        execution_id,
                        input: envelope.input,
                        package_artifact,
                        consumer: ExtensionProtocolConsumerPayload {
                            kind: "operation".to_string(),
                            extension_key: None,
                            extension_id: None,
                            dependency_alias: None,
                        },
                        workspace: runtime.workspace,
                        trace_id: envelope.trace.trace_id,
                        invocation_id: envelope.trace.invocation_id,
                    },
                )
                .await
                .map(|response| response.output)
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))
        }
        ExtensionGeneratedOperationDispatch::BackendService { service_key, route } => {
            let body = serde_json::to_vec(&envelope.input)
                .map_err(|error| OperationExecutionError::invalid_request(error.to_string()))?;
            let response = provider
                .backend_services
                .invoke_extension_backend_service(
                    &backend_id,
                    ExtensionBackendServiceInvokeRequest {
                        extension_key: installation.extension_key,
                        extension_id: installation.manifest.extension_id,
                        service_key,
                        route,
                        project_id: runtime.project_id.to_string(),
                        execution_id,
                        method: "POST".to_string(),
                        headers: BTreeMap::from([(
                            "content-type".to_string(),
                            "application/json".to_string(),
                        )]),
                        body: Some(body),
                        package_artifact,
                        workspace: runtime.workspace,
                        trace_id: envelope.trace.trace_id,
                        invocation_id: envelope.trace.invocation_id,
                    },
                )
                .await
                .map_err(|error| OperationExecutionError::provider_failed(error.to_string()))?;
            Ok(json!({
                "response": response.response.map(|value| json!({
                    "status": value.status,
                    "headers": value.headers,
                    "body": value.body,
                })),
                "diagnostic": response.diagnostic.map(|value| json!({
                    "readiness": format!("{:?}", value.readiness),
                    "code": value.code,
                    "message": value.message,
                    "retryable": value.retryable,
                    "details": value.details,
                })),
            }))
        }
    }
}

fn runtime_hosts(
    installations: &[ProjectExtensionInstallation],
) -> Vec<ExtensionRuntimeHostPayload> {
    installations
        .iter()
        .map(|installation| ExtensionRuntimeHostPayload {
            extension_key: installation.extension_key.clone(),
            extension_id: installation.manifest.extension_id.clone(),
            package_artifact: installation.package_artifact.as_ref().map(|artifact| {
                ExtensionPackageArtifactPayload {
                    artifact_id: artifact.artifact_id.to_string(),
                    archive_digest: artifact.archive_digest.clone(),
                }
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_application_ports::extension_runtime::{
        ExtensionActionInvokeResponse, ExtensionBackendServiceInvokeResponse,
        ExtensionProtocolInvokeResponse, ExtensionRuntimeActionTransportError,
    };
    use agentdash_domain::DomainError;
    use agentdash_domain::extension_package::{
        ExtensionPackageArtifactRef, ExtensionPackageMetadata,
    };
    use agentdash_domain::operation::OperationScopeRef;
    use agentdash_domain::shared_library::{
        ExtensionGeneratedOperationProvenance, ExtensionRuntimeActionDefinition,
        ExtensionRuntimeActionKind, ExtensionTemplatePayload,
    };
    use agentdash_spi::{AuthIdentity, AuthMode};

    use super::*;

    struct FixtureRepo(Vec<ProjectExtensionInstallation>);

    #[async_trait]
    impl ProjectExtensionInstallationRepository for FixtureRepo {
        async fn create(&self, _: &ProjectExtensionInstallation) -> Result<(), DomainError> {
            unreachable!()
        }
        async fn update(&self, _: &ProjectExtensionInstallation) -> Result<(), DomainError> {
            unreachable!()
        }
        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            extension_key: &str,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .0
                .iter()
                .find(|value| {
                    value.project_id == project_id && value.extension_key == extension_key
                })
                .cloned())
        }
        async fn get_by_project_and_id(
            &self,
            project_id: Uuid,
            installation_id: Uuid,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .0
                .iter()
                .find(|value| value.project_id == project_id && value.id == installation_id)
                .cloned())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .0
                .iter()
                .filter(|value| value.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn list_enabled_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .0
                .iter()
                .filter(|value| value.project_id == project_id && value.enabled)
                .cloned()
                .collect())
        }
        async fn delete(&self, _: Uuid, _: Uuid) -> Result<bool, DomainError> {
            unreachable!()
        }
    }

    struct CapturingContext {
        runtime: ExtensionOperationRuntimeContext,
        origins: Mutex<Vec<OperationOriginRef>>,
    }

    #[async_trait]
    impl ExtensionOperationContextPort for CapturingContext {
        async fn resolve_context(
            &self,
            _: &OperationPrincipal,
            _: &OperationAuthorizationScope,
            origin: &OperationOriginRef,
            _: CancellationToken,
        ) -> Result<ExtensionOperationRuntimeContext, OperationExecutionError> {
            self.origins.lock().expect("origins").push(origin.clone());
            Ok(self.runtime.clone())
        }
    }

    struct FixtureTransport;

    #[async_trait]
    impl ExtensionRuntimeActionTransport for FixtureTransport {
        async fn invoke_extension_action(
            &self,
            _: &str,
            _: ExtensionActionInvokeRequest,
        ) -> Result<ExtensionActionInvokeResponse, ExtensionRuntimeActionTransportError> {
            unreachable!()
        }
    }

    #[async_trait]
    impl ExtensionRuntimeProtocolTransport for FixtureTransport {
        async fn invoke_extension_protocol(
            &self,
            _: &str,
            _: ExtensionProtocolInvokeRequest,
        ) -> Result<ExtensionProtocolInvokeResponse, ExtensionRuntimeActionTransportError> {
            unreachable!()
        }
    }

    #[async_trait]
    impl ExtensionBackendServiceTransport for FixtureTransport {
        async fn invoke_extension_backend_service(
            &self,
            _: &str,
            _: ExtensionBackendServiceInvokeRequest,
        ) -> Result<ExtensionBackendServiceInvokeResponse, ExtensionRuntimeActionTransportError>
        {
            unreachable!()
        }
    }

    fn installation(
        project_id: Uuid,
        key: &str,
        with_operation: bool,
    ) -> ProjectExtensionInstallation {
        let action_key = format!("{key}.run");
        let operation_catalog = with_operation.then(|| ExtensionGeneratedOperationDefinition {
            operation_key: action_key.clone(),
            description: "Run".to_string(),
            visibility: ExtensionGeneratedOperationVisibility::AgentAndPanel,
            input_schema: json!({ "type": "object" }),
            output_schema: json!(true),
            permission_summary: Vec::new(),
            dispatch: ExtensionGeneratedOperationDispatch::RuntimeAction {
                action_key: action_key.clone(),
            },
            provenance: ExtensionGeneratedOperationProvenance {
                capability_key: "run".to_string(),
                exposure_key: "run".to_string(),
                generated_from: "test".to_string(),
            },
        });
        let manifest = ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: key.to_string(),
            package: ExtensionPackageMetadata {
                name: key.to_string(),
                version: "1.0.0".to_string(),
            },
            asset_version: "1.0.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: operation_catalog
                .as_ref()
                .map(|_| {
                    vec![ExtensionRuntimeActionDefinition {
                        action_key,
                        kind: ExtensionRuntimeActionKind::Runtime,
                        description: "Run".to_string(),
                        input_schema: json!({ "type": "object" }),
                        output_schema: json!(true),
                        permissions: vec![],
                    }]
                })
                .unwrap_or_default(),
            protocols: vec![],
            extension_dependencies: vec![],
            workspace_tabs: vec![],
            ui_components: vec![],
            permissions: vec![],
            fetch_routes: vec![],
            operation_catalog: operation_catalog.into_iter().collect(),
            backend_services: vec![],
            bundles: vec![],
        };
        ProjectExtensionInstallation::new_packaged(
            project_id,
            key,
            key,
            manifest,
            ExtensionPackageArtifactRef {
                artifact_id: Uuid::new_v4(),
                package_name: key.to_string(),
                package_version: "1.0.0".to_string(),
                asset_version: "1.0.0".to_string(),
                source_version: "1.0.0".to_string(),
                storage_ref: format!("{key}.tgz"),
                archive_digest:
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
                manifest_digest:
                    "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                        .to_string(),
            },
        )
        .expect("installation")
    }

    fn principal() -> OperationPrincipal {
        OperationPrincipal::authenticated_user(AuthIdentity {
            auth_mode: AuthMode::Personal,
            user_id: "user-1".to_string(),
            subject: "user-1".to_string(),
            display_name: None,
            email: None,
            avatar_url: None,
            groups: vec![],
            is_admin: false,
            provider: None,
            extra: Value::Null,
        })
    }

    #[tokio::test]
    async fn placement_preserves_panel_origin_for_context_resolution() {
        let project_id = Uuid::new_v4();
        let installation = installation(project_id, "demo", true);
        let installation_id = installation.id;
        let context = Arc::new(CapturingContext {
            runtime: ExtensionOperationRuntimeContext {
                project_id,
                backend_id: Some("backend-1".to_string()),
                workspace: None,
            },
            origins: Mutex::new(Vec::new()),
        });
        let transport = Arc::new(FixtureTransport);
        let provider = ExtensionOperationProvider::new(
            Arc::new(FixtureRepo(vec![installation])),
            context.clone(),
            transport.clone(),
            transport.clone(),
            transport,
        );
        let scope = OperationAuthorizationScope {
            scope_ref: OperationScopeRef::Project { project_id },
            authority_revision: "rev-1".to_string(),
        };
        let origin = OperationOriginRef::ExtensionPanel { installation_id };
        let descriptor = provider
            .discover(&principal(), &scope, &origin, CancellationToken::new())
            .await
            .expect("discover")
            .remove(0);
        provider
            .resolve_placement(
                &descriptor,
                &principal(),
                &scope,
                &origin,
                CancellationToken::new(),
            )
            .await
            .expect("placement");
        assert_eq!(
            context.origins.lock().expect("origins").as_slice(),
            &[origin.clone(), origin]
        );
    }

    #[test]
    fn runtime_hosts_include_every_enabled_dependency_artifact() {
        let project_id = Uuid::new_v4();
        let hosts = runtime_hosts(&[
            installation(project_id, "provider", true),
            installation(project_id, "dependency", false),
        ]);
        assert_eq!(hosts.len(), 2);
        assert!(hosts.iter().all(|host| host.package_artifact.is_some()));
        assert_eq!(hosts[1].extension_key, "dependency");
    }
}
