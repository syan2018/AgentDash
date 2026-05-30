use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_application_ports::extension_runtime::{
    ExtensionRuntimeActionTransport, ExtensionRuntimeActionTransportError,
    ExtensionRuntimeChannelTransport,
};
use agentdash_domain::shared_library::{
    ExtensionDependencyDeclaration, ExtensionPermissionDecision,
    ExtensionProtocolChannelDefinition, ExtensionProtocolChannelMethodDefinition,
    ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ProjectExtensionInstallation,
    ProjectExtensionInstallationRepository,
};
use agentdash_relay::{
    CommandExtensionActionInvokePayload, CommandExtensionChannelInvokePayload,
    ExtensionChannelConsumerRelay, ExtensionInvocationWorkspaceRelay,
    ExtensionPackageArtifactRelay, ExtensionRuntimeHostRelay,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{
    RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeContext,
    RuntimeInvocationError, RuntimeInvocationOutput, RuntimeInvocationRequest, RuntimeProvider,
    RuntimeTarget,
};

pub const EXTENSION_INVOCATION_WORKSPACE_METADATA_KEY: &str = "extension_invocation_workspace";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionInvocationWorkspaceContext {
    pub mount_id: String,
    pub root_ref: String,
}

impl ExtensionInvocationWorkspaceContext {
    pub fn new(mount_id: impl Into<String>, root_ref: impl Into<String>) -> Self {
        Self {
            mount_id: mount_id.into(),
            root_ref: root_ref.into(),
        }
    }

    fn into_relay(self) -> ExtensionInvocationWorkspaceRelay {
        ExtensionInvocationWorkspaceRelay {
            mount_id: self.mount_id,
            root_ref: self.root_ref,
        }
    }
}

pub fn attach_extension_invocation_workspace(
    request: &mut RuntimeInvocationRequest,
    workspace: Option<ExtensionInvocationWorkspaceContext>,
) {
    match workspace {
        Some(workspace) => {
            request.metadata.insert(
                EXTENSION_INVOCATION_WORKSPACE_METADATA_KEY.to_string(),
                json!({
                    "mount_id": workspace.mount_id,
                    "root_ref": workspace.root_ref,
                }),
            );
        }
        None => {
            request
                .metadata
                .remove(EXTENSION_INVOCATION_WORKSPACE_METADATA_KEY);
        }
    }
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
        let artifact = installation.package_artifact.as_ref().ok_or_else(|| {
            RuntimeInvocationError::conflict(
                format!(
                    "extension runtime action `{action_key}` 所属安装 `{}` 缺少 package artifact",
                    installation.extension_key
                ),
                Some(request.trace.clone()),
            )
        })?;
        let permission_decisions = validate_action_permissions(installation, action, &request)?;
        let workspace = extension_invocation_workspace_from_metadata(&request)?;

        let relay_payload = CommandExtensionActionInvokePayload {
            extension_key: installation.extension_key.clone(),
            extension_id: installation.manifest.extension_id.clone(),
            action_key: action.action_key.clone(),
            project_id: project_id.to_string(),
            session_id,
            input: request.input.clone(),
            package_artifact: Some(ExtensionPackageArtifactRelay {
                artifact_id: artifact.artifact_id.to_string(),
                archive_digest: artifact.archive_digest.clone(),
            }),
            runtime_extensions: runtime_host_relays(&installations),
            workspace: workspace.map(ExtensionInvocationWorkspaceContext::into_relay),
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
    Ok(decisions)
}

fn extension_invocation_workspace_from_metadata(
    request: &RuntimeInvocationRequest,
) -> Result<Option<ExtensionInvocationWorkspaceContext>, RuntimeInvocationError> {
    let Some(value) = request
        .metadata
        .get(EXTENSION_INVOCATION_WORKSPACE_METADATA_KEY)
    else {
        return Ok(None);
    };
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|error| {
            RuntimeInvocationError::invalid_request(
                format!("extension invocation workspace metadata 非法: {error}"),
                Some(request.trace.clone()),
            )
        })
}

fn runtime_host_relays(
    installations: &[ProjectExtensionInstallation],
) -> Vec<ExtensionRuntimeHostRelay> {
    installations
        .iter()
        .filter_map(|installation| {
            installation
                .package_artifact
                .as_ref()
                .map(|artifact| ExtensionRuntimeHostRelay {
                    extension_key: installation.extension_key.clone(),
                    extension_id: installation.manifest.extension_id.clone(),
                    package_artifact: Some(ExtensionPackageArtifactRelay {
                        artifact_id: artifact.artifact_id.to_string(),
                        archive_digest: artifact.archive_digest.clone(),
                    }),
                })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionRuntimeChannelInvokeRequest {
    pub project_id: Uuid,
    pub session_id: String,
    pub backend_id: String,
    pub workspace: Option<ExtensionInvocationWorkspaceContext>,
    pub consumer: ExtensionRuntimeChannelConsumer,
    pub channel_key: String,
    pub dependency_alias: Option<String>,
    pub method: String,
    pub input: serde_json::Value,
    pub trace: super::RuntimeTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionRuntimeChannelConsumer {
    ExtensionPanel { extension_key: String },
    UserCanvas { canvas_id: Option<Uuid> },
    SessionUser,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionRuntimeChannelInvokeResult {
    pub channel_key: String,
    pub method: String,
    pub trace: super::RuntimeTrace,
    pub output: RuntimeInvocationOutput,
}

pub struct ExtensionRuntimeChannelInvoker {
    installations: Arc<dyn ProjectExtensionInstallationRepository>,
    transport: Arc<dyn ExtensionRuntimeChannelTransport>,
}

impl ExtensionRuntimeChannelInvoker {
    pub fn new(
        installations: Arc<dyn ProjectExtensionInstallationRepository>,
        transport: Arc<dyn ExtensionRuntimeChannelTransport>,
    ) -> Self {
        Self {
            installations,
            transport,
        }
    }

    pub async fn invoke(
        &self,
        request: ExtensionRuntimeChannelInvokeRequest,
    ) -> Result<ExtensionRuntimeChannelInvokeResult, RuntimeInvocationError> {
        let installations = self
            .installations
            .list_enabled_by_project(request.project_id)
            .await
            .map_err(|error| {
                RuntimeInvocationError::provider_failed(
                    format!("读取 Project extension installation 失败: {error}"),
                    Some(request.trace.clone()),
                )
            })?;
        let resolved = resolve_channel_invocation(&installations, &request)?;
        let artifact = resolved.provider.package_artifact.as_ref().ok_or_else(|| {
            RuntimeInvocationError::conflict(
                format!(
                    "extension channel provider `{}` 缺少 package artifact",
                    resolved.provider.extension_key
                ),
                Some(request.trace.clone()),
            )
        })?;
        let relay_payload = CommandExtensionChannelInvokePayload {
            provider_extension_key: resolved.provider.extension_key.clone(),
            provider_extension_id: resolved.provider.manifest.extension_id.clone(),
            channel_key: resolved.channel.channel_key.clone(),
            method: resolved.method.name.clone(),
            project_id: request.project_id.to_string(),
            session_id: request.session_id.clone(),
            input: request.input.clone(),
            package_artifact: ExtensionPackageArtifactRelay {
                artifact_id: artifact.artifact_id.to_string(),
                archive_digest: artifact.archive_digest.clone(),
            },
            consumer: channel_consumer_relay(
                &request.consumer,
                resolved.consumer_installation,
                resolved.dependency_alias,
            ),
            workspace: request
                .workspace
                .clone()
                .map(ExtensionInvocationWorkspaceContext::into_relay),
            trace_id: request.trace.trace_id.clone(),
            invocation_id: request.trace.invocation_id.clone(),
        };
        let response = self
            .transport
            .invoke_extension_channel(&request.backend_id, relay_payload)
            .await
            .map_err(|error| transport_error_to_channel_invocation(error, &request))?;
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "provider_extension_key".to_string(),
            json!(response.provider_extension_key),
        );
        metadata.insert(
            "provider_extension_id".to_string(),
            json!(response.provider_extension_id),
        );
        metadata.insert("channel_key".to_string(), json!(response.channel_key));
        metadata.insert("method".to_string(), json!(response.method));
        metadata.insert("backend_id".to_string(), json!(request.backend_id));
        metadata.insert("trace_id".to_string(), json!(request.trace.trace_id));
        metadata.insert(
            "invocation_id".to_string(),
            json!(request.trace.invocation_id),
        );
        if let Some(alias) = resolved.dependency_alias {
            metadata.insert("dependency_alias".to_string(), json!(alias));
        }
        for (key, value) in response.metadata {
            metadata.insert(key, value);
        }
        Ok(ExtensionRuntimeChannelInvokeResult {
            channel_key: resolved.channel.channel_key.clone(),
            method: resolved.method.name.clone(),
            trace: request.trace,
            output: RuntimeInvocationOutput {
                output: response.output,
                metadata,
            },
        })
    }
}

struct ResolvedChannelInvocation<'a> {
    provider: &'a ProjectExtensionInstallation,
    channel: &'a ExtensionProtocolChannelDefinition,
    method: &'a ExtensionProtocolChannelMethodDefinition,
    consumer_installation: Option<&'a ProjectExtensionInstallation>,
    dependency_alias: Option<&'a str>,
}

fn resolve_channel_invocation<'a>(
    installations: &'a [ProjectExtensionInstallation],
    request: &'a ExtensionRuntimeChannelInvokeRequest,
) -> Result<ResolvedChannelInvocation<'a>, RuntimeInvocationError> {
    let consumer_installation = match &request.consumer {
        ExtensionRuntimeChannelConsumer::ExtensionPanel { extension_key } => installations
            .iter()
            .find(|installation| installation.extension_key == *extension_key),
        ExtensionRuntimeChannelConsumer::UserCanvas { .. }
        | ExtensionRuntimeChannelConsumer::SessionUser => None,
    };
    if matches!(
        request.consumer,
        ExtensionRuntimeChannelConsumer::ExtensionPanel { .. }
    ) && consumer_installation.is_none()
    {
        return Err(RuntimeInvocationError::capability_denied(
            "extension channel consumer 未安装或未启用",
            Some(request.trace.clone()),
        ));
    }

    let (channel_key, dependency_alias) =
        resolve_requested_channel_key(consumer_installation, request)?;
    let (provider, channel) = installations
        .iter()
        .find_map(|installation| {
            installation
                .manifest
                .protocol_channels
                .iter()
                .find(|channel| channel.channel_key == channel_key)
                .map(|channel| (installation, channel))
        })
        .ok_or_else(|| {
            RuntimeInvocationError::capability_denied(
                format!("extension channel 未启用或不可见: {channel_key}"),
                Some(request.trace.clone()),
            )
        })?;
    let method = channel
        .methods
        .iter()
        .find(|method| method.name == request.method)
        .ok_or_else(|| {
            RuntimeInvocationError::capability_denied(
                format!(
                    "extension channel method 未声明: {}.{}",
                    channel.channel_key, request.method
                ),
                Some(request.trace.clone()),
            )
        })?;

    ensure_consumer_dependency(
        consumer_installation,
        provider,
        channel,
        dependency_alias,
        request,
    )?;
    Ok(ResolvedChannelInvocation {
        provider,
        channel,
        method,
        consumer_installation,
        dependency_alias,
    })
}

fn resolve_requested_channel_key<'a>(
    consumer: Option<&'a ProjectExtensionInstallation>,
    request: &'a ExtensionRuntimeChannelInvokeRequest,
) -> Result<(String, Option<&'a str>), RuntimeInvocationError> {
    if let Some(alias) = request.dependency_alias.as_deref() {
        let consumer = consumer.ok_or_else(|| {
            RuntimeInvocationError::capability_denied(
                "dependency alias 只能由 extension consumer 使用",
                Some(request.trace.clone()),
            )
        })?;
        let dependency = consumer
            .manifest
            .extension_dependencies
            .iter()
            .find(|dependency| dependency.alias == alias)
            .ok_or_else(|| {
                RuntimeInvocationError::capability_denied(
                    format!("extension dependency alias 未声明: {alias}"),
                    Some(request.trace.clone()),
                )
            })?;
        let channel_key =
            select_dependency_channel(dependency, &request.channel_key).ok_or_else(|| {
                RuntimeInvocationError::capability_denied(
                    format!(
                        "extension dependency `{alias}` 未声明 channel: {}",
                        request.channel_key
                    ),
                    Some(request.trace.clone()),
                )
            })?;
        return Ok((channel_key, Some(alias)));
    }

    let raw = request.channel_key.trim();
    if raw.is_empty() {
        return Err(RuntimeInvocationError::invalid_request(
            "extension channel key 不能为空",
            Some(request.trace.clone()),
        ));
    }
    if raw.contains('.') {
        return Ok((raw.to_string(), None));
    }
    let consumer = consumer.ok_or_else(|| {
        RuntimeInvocationError::invalid_request(
            format!("短 channel key `{raw}` 需要 extension consumer scope"),
            Some(request.trace.clone()),
        )
    })?;
    Ok((format!("{}.{}", consumer.extension_key, raw), None))
}

fn select_dependency_channel(
    dependency: &ExtensionDependencyDeclaration,
    requested: &str,
) -> Option<String> {
    let requested = requested.trim();
    if requested.is_empty() {
        return dependency.channels.first().cloned();
    }
    if requested.contains('.') {
        dependency
            .channels
            .iter()
            .find(|channel| channel.as_str() == requested)
            .cloned()
    } else {
        dependency
            .channels
            .iter()
            .find(|channel| channel.rsplit('.').next() == Some(requested))
            .cloned()
    }
}

fn ensure_consumer_dependency(
    consumer: Option<&ProjectExtensionInstallation>,
    provider: &ProjectExtensionInstallation,
    channel: &ExtensionProtocolChannelDefinition,
    dependency_alias: Option<&str>,
    request: &ExtensionRuntimeChannelInvokeRequest,
) -> Result<(), RuntimeInvocationError> {
    let Some(consumer) = consumer else {
        return Ok(());
    };
    if consumer.extension_key == provider.extension_key {
        return Ok(());
    }
    let dependency = dependency_alias
        .and_then(|alias| {
            consumer
                .manifest
                .extension_dependencies
                .iter()
                .find(|dependency| dependency.alias == alias)
        })
        .or_else(|| {
            consumer
                .manifest
                .extension_dependencies
                .iter()
                .find(|dependency| {
                    dependency.extension_id == provider.manifest.extension_id
                        && dependency
                            .channels
                            .iter()
                            .any(|key| key == &channel.channel_key)
                })
        })
        .ok_or_else(|| {
            RuntimeInvocationError::capability_denied(
                format!(
                    "extension `{}` 未声明依赖 channel `{}`",
                    consumer.extension_key, channel.channel_key
                ),
                Some(request.trace.clone()),
            )
        })?;
    if dependency.extension_id != provider.manifest.extension_id {
        return Err(RuntimeInvocationError::capability_denied(
            format!(
                "extension dependency `{}` 指向 `{}`，但 channel provider 是 `{}`",
                dependency.alias, dependency.extension_id, provider.manifest.extension_id
            ),
            Some(request.trace.clone()),
        ));
    }
    if !dependency
        .channels
        .iter()
        .any(|key| key == &channel.channel_key)
    {
        return Err(RuntimeInvocationError::capability_denied(
            format!(
                "extension dependency `{}` 未声明 channel `{}`",
                dependency.alias, channel.channel_key
            ),
            Some(request.trace.clone()),
        ));
    }
    if !version_satisfies(&dependency.version, &provider.manifest.package.version) {
        return Err(RuntimeInvocationError::capability_denied(
            format!(
                "extension dependency `{}` 版本要求 `{}` 与 provider 版本 `{}` 不匹配",
                dependency.alias, dependency.version, provider.manifest.package.version
            ),
            Some(request.trace.clone()),
        ));
    }
    Ok(())
}

fn version_satisfies(requirement: &str, actual: &str) -> bool {
    let requirement = requirement.trim();
    if requirement == "*" || requirement.is_empty() {
        return true;
    }
    if let Some(base) = requirement.strip_prefix('^') {
        let Some(base_version) = parse_semver_prefix(base) else {
            return false;
        };
        let Some(actual_version) = parse_semver_prefix(actual) else {
            return false;
        };
        return actual_version.0 == base_version.0 && actual_version >= base_version;
    }
    requirement == actual
}

fn parse_semver_prefix(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch_raw = parts.next().unwrap_or("0");
    let patch_digits = patch_raw
        .chars()
        .take_while(|value| value.is_ascii_digit())
        .collect::<String>();
    let patch = if patch_digits.is_empty() {
        0
    } else {
        patch_digits.parse().ok()?
    };
    Some((major, minor, patch))
}

fn channel_consumer_relay(
    consumer: &ExtensionRuntimeChannelConsumer,
    consumer_installation: Option<&ProjectExtensionInstallation>,
    dependency_alias: Option<&str>,
) -> ExtensionChannelConsumerRelay {
    match consumer {
        ExtensionRuntimeChannelConsumer::ExtensionPanel { extension_key } => {
            ExtensionChannelConsumerRelay {
                kind: "extension_panel".to_string(),
                extension_key: Some(extension_key.clone()),
                extension_id: consumer_installation
                    .map(|installation| installation.manifest.extension_id.clone()),
                dependency_alias: dependency_alias.map(str::to_string),
            }
        }
        ExtensionRuntimeChannelConsumer::UserCanvas { canvas_id } => {
            ExtensionChannelConsumerRelay {
                kind: "canvas".to_string(),
                extension_key: None,
                extension_id: canvas_id.map(|id| id.to_string()),
                dependency_alias: dependency_alias.map(str::to_string),
            }
        }
        ExtensionRuntimeChannelConsumer::SessionUser => ExtensionChannelConsumerRelay {
            kind: "session_user".to_string(),
            extension_key: None,
            extension_id: None,
            dependency_alias: dependency_alias.map(str::to_string),
        },
    }
}

fn transport_error_to_channel_invocation(
    error: ExtensionRuntimeActionTransportError,
    request: &ExtensionRuntimeChannelInvokeRequest,
) -> RuntimeInvocationError {
    match error {
        ExtensionRuntimeActionTransportError::Offline { backend_id } => {
            RuntimeInvocationError::conflict(
                format!("extension channel backend offline: {backend_id}"),
                Some(request.trace.clone()),
            )
        }
        ExtensionRuntimeActionTransportError::Timeout { backend_id } => {
            RuntimeInvocationError::timeout(
                format!("extension channel backend timeout: {backend_id}"),
                Some(request.trace.clone()),
            )
        }
        ExtensionRuntimeActionTransportError::ResponseDropped { backend_id } => {
            RuntimeInvocationError::provider_failed(
                format!("extension channel backend response dropped: {backend_id}"),
                Some(request.trace.clone()),
            )
        }
        ExtensionRuntimeActionTransportError::Failed(message) => {
            RuntimeInvocationError::provider_failed(message, Some(request.trace.clone()))
        }
    }
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
    use agentdash_domain::extension_package::{
        ExtensionPackageArtifactRef, ExtensionPackageMetadata,
    };
    use agentdash_domain::shared_library::{
        ExtensionDependencyDeclaration, ExtensionPermissionAccess, ExtensionPermissionDeclaration,
        ExtensionProtocolChannelDefinition, ExtensionProtocolChannelMethodDefinition,
        ExtensionRuntimeActionDefinition, ExtensionTemplatePayload, InstalledAssetSource,
        ProjectExtensionInstallation,
    };
    use agentdash_relay::{
        ResponseExtensionActionInvokePayload, ResponseExtensionChannelInvokePayload,
    };
    use serde_json::json;

    use super::*;
    use crate::runtime_gateway::{
        RuntimeActor, RuntimeGateway, RuntimeInvocationErrorKind, RuntimeTarget, RuntimeTrace,
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

    struct FakeChannelTransport {
        result: Result<ResponseExtensionChannelInvokePayload, ExtensionRuntimeActionTransportError>,
        last_payload: StdMutex<Option<CommandExtensionChannelInvokePayload>>,
    }

    #[async_trait]
    impl ExtensionRuntimeChannelTransport for FakeChannelTransport {
        async fn invoke_extension_channel(
            &self,
            backend_id: &str,
            payload: CommandExtensionChannelInvokePayload,
        ) -> Result<ResponseExtensionChannelInvokePayload, ExtensionRuntimeActionTransportError>
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
        assert_eq!(
            payload
                .workspace
                .as_ref()
                .map(|workspace| (workspace.mount_id.as_str(), workspace.root_ref.as_str())),
            Some(("main", "D:/Workspaces/demo"))
        );
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
    async fn runtime_action_requires_package_artifact() {
        let project_id = Uuid::new_v4();
        let transport = Arc::new(FakeTransport {
            result: Ok(response_payload(json!({ "ok": true }))),
            last_payload: StdMutex::new(None),
        });
        let gateway = RuntimeGateway::new().with_dynamic_provider(Arc::new(
            ExtensionRuntimeActionProvider::new(
                Arc::new(FakeInstallationRepo {
                    installations: vec![missing_package_installation(project_id)],
                }),
                transport.clone(),
            ),
        ));

        let err = gateway
            .invoke(request(project_id, "local-hello.profile"))
            .await
            .expect_err("missing artifact");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::Conflict);
        assert!(transport.last_payload.lock().expect("lock").is_none());
    }

    #[tokio::test]
    async fn top_level_permission_summary_does_not_gate_declared_action_permission() {
        let project_id = Uuid::new_v4();
        let transport = Arc::new(FakeTransport {
            result: Ok(response_payload(json!({ "username": "local-user" }))),
            last_payload: StdMutex::new(None),
        });
        let gateway = RuntimeGateway::new().with_dynamic_provider(Arc::new(
            ExtensionRuntimeActionProvider::new(
                Arc::new(FakeInstallationRepo {
                    installations: vec![installation(project_id, false, true)],
                }),
                transport,
            ),
        ));

        let result = gateway
            .invoke(request(project_id, "local-hello.profile"))
            .await
            .expect("invoke");

        assert_eq!(result.output.output["username"], "local-user");
    }

    #[tokio::test]
    async fn gateway_does_not_pre_gate_missing_action_permission() {
        let project_id = Uuid::new_v4();
        let transport = Arc::new(FakeTransport {
            result: Ok(response_payload(json!({ "ok": true }))),
            last_payload: StdMutex::new(None),
        });
        let gateway = RuntimeGateway::new().with_dynamic_provider(Arc::new(
            ExtensionRuntimeActionProvider::new(
                Arc::new(FakeInstallationRepo {
                    installations: vec![installation(project_id, true, false)],
                }),
                transport.clone(),
            ),
        ));

        let result = gateway
            .invoke(request(project_id, "local-hello.profile"))
            .await
            .expect("gateway should route to local host");

        assert_eq!(result.output.output["ok"], true);
        assert!(transport.last_payload.lock().expect("lock").is_some());
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

    #[tokio::test]
    async fn channel_invoker_routes_dependency_alias() {
        let project_id = Uuid::new_v4();
        let transport = Arc::new(FakeChannelTransport {
            result: Ok(channel_response_payload(json!({ "ok": true }))),
            last_payload: StdMutex::new(None),
        });
        let invoker = ExtensionRuntimeChannelInvoker::new(
            Arc::new(FakeInstallationRepo {
                installations: vec![
                    provider_channel_installation(project_id),
                    consumer_channel_installation(project_id, "^1.0.0", true),
                ],
            }),
            transport.clone(),
        );

        let result = invoker
            .invoke(channel_request(project_id, "api", Some("provider")))
            .await
            .expect("invoke channel");

        assert_eq!(result.output.output["ok"], true);
        assert_eq!(result.output.metadata["provider_extension_key"], "provider");
        assert_eq!(result.output.metadata["dependency_alias"], "provider");
        let payload = transport
            .last_payload
            .lock()
            .expect("lock")
            .clone()
            .expect("payload");
        assert_eq!(payload.provider_extension_key, "provider");
        assert_eq!(payload.channel_key, "provider.api");
        assert_eq!(
            payload.consumer.dependency_alias.as_deref(),
            Some("provider")
        );
        assert_eq!(
            payload
                .workspace
                .as_ref()
                .map(|workspace| (workspace.mount_id.as_str(), workspace.root_ref.as_str())),
            Some(("main", "D:/Workspaces/demo"))
        );
    }

    #[tokio::test]
    async fn channel_invoker_rejects_missing_provider() {
        let project_id = Uuid::new_v4();
        let invoker = ExtensionRuntimeChannelInvoker::new(
            Arc::new(FakeInstallationRepo {
                installations: vec![consumer_channel_installation(project_id, "^1.0.0", true)],
            }),
            Arc::new(FakeChannelTransport {
                result: Ok(channel_response_payload(json!({}))),
                last_payload: StdMutex::new(None),
            }),
        );

        let err = invoker
            .invoke(channel_request(project_id, "api", Some("provider")))
            .await
            .expect_err("missing provider");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }

    #[tokio::test]
    async fn channel_invoker_rejects_missing_dependency() {
        let project_id = Uuid::new_v4();
        let invoker = ExtensionRuntimeChannelInvoker::new(
            Arc::new(FakeInstallationRepo {
                installations: vec![
                    provider_channel_installation(project_id),
                    consumer_channel_installation(project_id, "^1.0.0", false),
                ],
            }),
            Arc::new(FakeChannelTransport {
                result: Ok(channel_response_payload(json!({}))),
                last_payload: StdMutex::new(None),
            }),
        );

        let err = invoker
            .invoke(channel_request(project_id, "provider.api", None))
            .await
            .expect_err("missing dependency");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
    }

    #[tokio::test]
    async fn channel_invoker_rejects_dependency_version_mismatch() {
        let project_id = Uuid::new_v4();
        let invoker = ExtensionRuntimeChannelInvoker::new(
            Arc::new(FakeInstallationRepo {
                installations: vec![
                    provider_channel_installation(project_id),
                    consumer_channel_installation(project_id, "^2.0.0", true),
                ],
            }),
            Arc::new(FakeChannelTransport {
                result: Ok(channel_response_payload(json!({}))),
                last_payload: StdMutex::new(None),
            }),
        );

        let err = invoker
            .invoke(channel_request(project_id, "api", Some("provider")))
            .await
            .expect_err("version mismatch");

        assert_eq!(err.kind(), RuntimeInvocationErrorKind::CapabilityDenied);
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
        attach_extension_invocation_workspace(
            &mut request,
            Some(ExtensionInvocationWorkspaceContext::new(
                "main",
                "D:/Workspaces/demo",
            )),
        );
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

    fn channel_response_payload(
        output: serde_json::Value,
    ) -> ResponseExtensionChannelInvokePayload {
        ResponseExtensionChannelInvokePayload {
            provider_extension_key: "provider".to_string(),
            provider_extension_id: "provider".to_string(),
            channel_key: "provider.api".to_string(),
            method: "echo".to_string(),
            output,
            metadata: Default::default(),
        }
    }

    fn channel_request(
        project_id: Uuid,
        channel_key: &str,
        dependency_alias: Option<&str>,
    ) -> ExtensionRuntimeChannelInvokeRequest {
        ExtensionRuntimeChannelInvokeRequest {
            project_id,
            session_id: "session-1".to_string(),
            backend_id: "backend-1".to_string(),
            workspace: Some(ExtensionInvocationWorkspaceContext::new(
                "main",
                "D:/Workspaces/demo",
            )),
            consumer: ExtensionRuntimeChannelConsumer::ExtensionPanel {
                extension_key: "consumer".to_string(),
            },
            channel_key: channel_key.to_string(),
            dependency_alias: dependency_alias.map(str::to_string),
            method: "echo".to_string(),
            input: json!({ "source": "test" }),
            trace: RuntimeTrace::new(),
        }
    }

    fn provider_channel_installation(project_id: Uuid) -> ProjectExtensionInstallation {
        ProjectExtensionInstallation::new_packaged(
            project_id,
            "provider",
            "Provider",
            provider_channel_manifest(),
            artifact_ref("provider"),
        )
        .expect("provider installation")
    }

    fn consumer_channel_installation(
        project_id: Uuid,
        version: &str,
        include_dependency: bool,
    ) -> ProjectExtensionInstallation {
        ProjectExtensionInstallation::new_packaged(
            project_id,
            "consumer",
            "Consumer",
            consumer_channel_manifest(version, include_dependency),
            artifact_ref("consumer"),
        )
        .expect("consumer installation")
    }

    fn artifact_ref(extension_id: &str) -> ExtensionPackageArtifactRef {
        ExtensionPackageArtifactRef {
            artifact_id: Uuid::new_v4(),
            package_name: format!("@agentdash/{extension_id}"),
            package_version: "1.0.0".to_string(),
            asset_version: "1.0.0".to_string(),
            source_version: "1.0.0".to_string(),
            storage_ref: format!("extensions/{extension_id}.agentdash-extension.tgz"),
            archive_digest:
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            manifest_digest:
                "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                    .to_string(),
        }
    }

    fn installation(
        project_id: Uuid,
        include_top_level_permission: bool,
        include_action_permission: bool,
    ) -> ProjectExtensionInstallation {
        let manifest = manifest(include_top_level_permission, include_action_permission);
        manifest.validate().expect("manifest");
        ProjectExtensionInstallation::new_from_library_package(
            project_id,
            "local-hello",
            "Local Hello",
            manifest,
            installed_source(),
            artifact_ref("local-hello"),
        )
        .expect("installation")
    }

    fn missing_package_installation(project_id: Uuid) -> ProjectExtensionInstallation {
        let manifest = manifest(true, true);
        ProjectExtensionInstallation::new(
            project_id,
            "local-hello",
            "Local Hello",
            manifest,
            installed_source(),
        )
        .expect("installation")
    }

    fn installed_source() -> InstalledAssetSource {
        InstalledAssetSource::new(
            Uuid::new_v4(),
            "plugin:test:extension_template:local-hello",
            "0.1.0",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
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
            protocol_channels: vec![],
            extension_dependencies: vec![],
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

    fn provider_channel_manifest() -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: "provider".to_string(),
            package: ExtensionPackageMetadata {
                name: "@agentdash/provider".to_string(),
                version: "1.0.0".to_string(),
            },
            asset_version: "1.0.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![],
            protocol_channels: vec![ExtensionProtocolChannelDefinition {
                channel_key: "provider.api".to_string(),
                version: "1.0.0".to_string(),
                description: "Provider API".to_string(),
                methods: vec![ExtensionProtocolChannelMethodDefinition {
                    name: "echo".to_string(),
                    description: "Echo input".to_string(),
                    input_schema: json!({}),
                    output_schema: json!({}),
                    permissions: vec![],
                }],
            }],
            extension_dependencies: vec![],
            workspace_tabs: vec![],
            permissions: vec![],
            bundles: vec![],
        }
    }

    fn consumer_channel_manifest(
        version: &str,
        include_dependency: bool,
    ) -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: "consumer".to_string(),
            package: ExtensionPackageMetadata {
                name: "@agentdash/consumer".to_string(),
                version: "1.0.0".to_string(),
            },
            asset_version: "1.0.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![],
            protocol_channels: vec![],
            extension_dependencies: if include_dependency {
                vec![ExtensionDependencyDeclaration {
                    alias: "provider".to_string(),
                    extension_id: "provider".to_string(),
                    version: version.to_string(),
                    channels: vec!["provider.api".to_string()],
                }]
            } else {
                vec![]
            },
            workspace_tabs: vec![],
            permissions: vec![],
            bundles: vec![],
        }
    }
}
