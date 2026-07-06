use std::collections::{BTreeMap, btree_map::Entry};
use std::sync::Arc;

use agentdash_domain::DomainError;
use agentdash_domain::extension_package::ExtensionPackageArtifactRef;
use agentdash_domain::shared_library::{
    ExtensionBundleKind, ExtensionCommandHandler, ExtensionDependencyDeclaration,
    ExtensionFetchRouteDefinition, ExtensionFetchRouteScope, ExtensionFetchRouteTargetDefinition,
    ExtensionFlagType, ExtensionGeneratedOperationDefinition,
    ExtensionGeneratedOperationDispatch as DomainGeneratedOperationDispatch,
    ExtensionGeneratedOperationVisibility as DomainGeneratedOperationVisibility,
    ExtensionPermissionDeclaration, ExtensionProtocolChannelDefinition,
    ExtensionProtocolChannelMethodDefinition, ExtensionRendererDeclaration,
    ExtensionRuntimeActionKind, ExtensionWorkspaceTabRendererDeclaration, InstalledAssetSource,
    ProjectExtensionInstallation, ProjectExtensionInstallationRepository,
};
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtensionRuntimeProjection {
    pub installations: Vec<ExtensionInstallationProjection>,
    pub commands: Vec<ExtensionCommandProjection>,
    pub flags: Vec<ExtensionFlagProjection>,
    pub message_renderers: Vec<ExtensionMessageRendererProjection>,
    pub runtime_actions: Vec<ExtensionRuntimeActionProjection>,
    pub protocol_channels: Vec<ExtensionProtocolChannelProjection>,
    pub extension_dependencies: Vec<ExtensionDependencyProjection>,
    pub workspace_tabs: Vec<ExtensionWorkspaceTabProjection>,
    pub permissions: Vec<ExtensionPermissionProjection>,
    pub bundles: Vec<ExtensionBundleProjection>,
    pub fetch_routes: Vec<ExtensionFetchRouteProjection>,
    pub operation_catalog: Vec<ExtensionGeneratedOperationProjection>,
    pub backend_services: Vec<ExtensionBackendServiceProjection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionInstallationProjection {
    pub installation_id: Uuid,
    pub extension_key: String,
    pub extension_id: String,
    pub display_name: String,
    pub installed_source: Option<InstalledAssetSource>,
    pub package_artifact: Option<ExtensionPackageArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionCommandProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub name: String,
    pub description: String,
    pub handler: ExtensionCommandHandler,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionFlagProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub name: String,
    pub flag_type: ExtensionFlagType,
    pub default: serde_json::Value,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionMessageRendererProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub custom_type: String,
    pub renderer: ExtensionRendererDeclaration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionRuntimeActionProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub action_key: String,
    pub kind: ExtensionRuntimeActionKind,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionProtocolChannelProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub channel_key: String,
    pub version: String,
    pub description: String,
    pub methods: Vec<ExtensionProtocolChannelMethodProjection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionProtocolChannelMethodProjection {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionDependencyProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub dependency: ExtensionDependencyDeclaration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionWorkspaceTabProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub type_id: String,
    pub label: String,
    pub uri_scheme: String,
    pub renderer: ExtensionWorkspaceTabRendererDeclaration,
    pub loadability: ExtensionWorkspaceTabLoadabilityProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionWorkspaceTabLoadabilityMode {
    ExtensionHost,
    UiOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionWorkspaceTabLoadabilityProjection {
    pub available: bool,
    pub mode: ExtensionWorkspaceTabLoadabilityMode,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionPermissionProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub permission: ExtensionPermissionDeclaration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionBundleProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub kind: ExtensionBundleKind,
    pub entry: String,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionGeneratedOperationVisibility {
    PanelOnly,
    AgentAndPanel,
}

impl ExtensionGeneratedOperationVisibility {
    pub fn is_agent_visible(self) -> bool {
        self == Self::AgentAndPanel
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionGeneratedOperationDispatch {
    RuntimeAction { action_key: String },
    ProtocolChannel { channel_key: String, method: String },
    BackendService { service_key: String, route: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionGeneratedOperationProvenance {
    pub capability_key: String,
    pub exposure_key: String,
    pub generated_from: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionGeneratedOperationProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub operation_key: String,
    pub description: String,
    pub visibility: ExtensionGeneratedOperationVisibility,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub permission_summary: Vec<String>,
    pub dispatch: ExtensionGeneratedOperationDispatch,
    pub provenance: ExtensionGeneratedOperationProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionFetchRouteTargetProjection {
    HttpProxy { capability_key: String },
    RuntimeAction { action_key: String },
    ProtocolChannel { channel_key: String, method: String },
    BackendService { service_key: String, route: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionFetchRouteProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub route_key: String,
    pub pattern: String,
    pub panel_only: bool,
    pub target: ExtensionFetchRouteTargetProjection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionBackendServiceProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub service_key: String,
    pub runtime: String,
    pub entry: String,
    pub routes: Vec<String>,
    pub health_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UninstallExtensionInstallationInput {
    pub project_id: Uuid,
    pub installation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallExtensionInstallationOutput {
    pub installation_id: Uuid,
    pub extension_key: String,
}

pub async fn uninstall_extension_installation_with_repo(
    repo: &Arc<dyn ProjectExtensionInstallationRepository>,
    input: UninstallExtensionInstallationInput,
) -> Result<UninstallExtensionInstallationOutput, DomainError> {
    let installation = repo
        .get_by_project_and_id(input.project_id, input.installation_id)
        .await?
        .ok_or_else(|| DomainError::NotFound {
            entity: "project_extension_installation",
            id: input.installation_id.to_string(),
        })?;
    let extension_key = installation.extension_key.clone();
    let deleted = repo.delete(input.project_id, input.installation_id).await?;
    if !deleted {
        return Err(DomainError::NotFound {
            entity: "project_extension_installation",
            id: input.installation_id.to_string(),
        });
    }
    Ok(UninstallExtensionInstallationOutput {
        installation_id: input.installation_id,
        extension_key,
    })
}

pub fn extension_runtime_projection_from_installations(
    installations: Vec<ProjectExtensionInstallation>,
) -> Result<ExtensionRuntimeProjection, DomainError> {
    let mut projection = ExtensionRuntimeProjection::default();
    let mut action_keys = BTreeMap::new();
    let mut channel_keys = BTreeMap::new();
    let mut workspace_tab_type_ids = BTreeMap::new();
    let mut uri_schemes = BTreeMap::new();
    for installation in installations {
        let extension_key = installation.extension_key.clone();
        let extension_id = installation.manifest.extension_id.clone();
        let has_package_artifact = installation.package_artifact.is_some();
        let manifest = installation.manifest;
        let has_extension_host_bundle = manifest.bundles.iter().any(|bundle| {
            bundle.kind == ExtensionBundleKind::ExtensionHost && !bundle.entry.trim().is_empty()
        });
        for action in &manifest.runtime_actions {
            claim_unique_extension_runtime_key(
                &mut action_keys,
                "runtime action key",
                &action.action_key,
                &extension_key,
            )?;
        }
        for channel in &manifest.protocol_channels {
            claim_unique_extension_runtime_key(
                &mut channel_keys,
                "protocol channel key",
                &channel.channel_key,
                &extension_key,
            )?;
        }
        for tab in &manifest.workspace_tabs {
            claim_unique_extension_runtime_key(
                &mut workspace_tab_type_ids,
                "workspace tab type_id",
                &tab.type_id,
                &extension_key,
            )?;
            claim_unique_extension_runtime_key(
                &mut uri_schemes,
                "workspace tab uri_scheme",
                &tab.uri_scheme,
                &extension_key,
            )?;
        }
        projection
            .installations
            .push(ExtensionInstallationProjection {
                installation_id: installation.id,
                extension_key: extension_key.clone(),
                extension_id: extension_id.clone(),
                display_name: installation.display_name.clone(),
                installed_source: installation.installed_source,
                package_artifact: installation.package_artifact,
            });
        projection
            .commands
            .extend(
                manifest
                    .commands
                    .into_iter()
                    .map(|command| ExtensionCommandProjection {
                        extension_key: extension_key.clone(),
                        extension_id: extension_id.clone(),
                        name: command.name,
                        description: command.description,
                        handler: command.handler,
                    }),
            );
        projection.flags.extend(
            manifest
                .flags
                .into_iter()
                .map(|flag| ExtensionFlagProjection {
                    extension_key: extension_key.clone(),
                    extension_id: extension_id.clone(),
                    name: flag.name,
                    flag_type: flag.flag_type,
                    default: flag.default,
                    description: flag.description,
                }),
        );
        projection
            .message_renderers
            .extend(manifest.message_renderers.into_iter().map(|renderer| {
                ExtensionMessageRendererProjection {
                    extension_key: extension_key.clone(),
                    extension_id: extension_id.clone(),
                    custom_type: renderer.custom_type,
                    renderer: renderer.renderer,
                }
            }));
        projection
            .runtime_actions
            .extend(manifest.runtime_actions.into_iter().map(|action| {
                ExtensionRuntimeActionProjection {
                    extension_key: extension_key.clone(),
                    extension_id: extension_id.clone(),
                    action_key: action.action_key,
                    kind: action.kind,
                    description: action.description,
                    input_schema: action.input_schema,
                    output_schema: action.output_schema,
                    permissions: action.permissions,
                }
            }));
        projection.protocol_channels.extend(
            manifest
                .protocol_channels
                .into_iter()
                .map(|channel| protocol_channel_projection(&extension_key, &extension_id, channel)),
        );
        projection
            .extension_dependencies
            .extend(
                manifest
                    .extension_dependencies
                    .into_iter()
                    .map(|dependency| ExtensionDependencyProjection {
                        extension_key: extension_key.clone(),
                        extension_id: extension_id.clone(),
                        dependency,
                    }),
            );
        projection
            .workspace_tabs
            .extend(manifest.workspace_tabs.into_iter().map(|tab| {
                ExtensionWorkspaceTabProjection {
                    extension_key: extension_key.clone(),
                    extension_id: extension_id.clone(),
                    type_id: tab.type_id,
                    label: tab.label,
                    uri_scheme: tab.uri_scheme,
                    loadability: workspace_tab_loadability(
                        &tab.renderer,
                        has_package_artifact,
                        has_extension_host_bundle,
                    ),
                    renderer: tab.renderer,
                }
            }));
        projection
            .permissions
            .extend(manifest.permissions.into_iter().map(|permission| {
                ExtensionPermissionProjection {
                    extension_key: extension_key.clone(),
                    extension_id: extension_id.clone(),
                    permission,
                }
            }));
        projection
            .bundles
            .extend(
                manifest
                    .bundles
                    .into_iter()
                    .map(|bundle| ExtensionBundleProjection {
                        extension_key: extension_key.clone(),
                        extension_id: extension_id.clone(),
                        kind: bundle.kind,
                        entry: bundle.entry,
                        digest: bundle.digest,
                    }),
            );
        projection.fetch_routes.extend(
            manifest
                .fetch_routes
                .into_iter()
                .map(|route| fetch_route_projection(&extension_key, &extension_id, route)),
        );
        projection
            .operation_catalog
            .extend(manifest.operation_catalog.into_iter().map(|operation| {
                generated_operation_projection(&extension_key, &extension_id, operation)
            }));
        projection
            .backend_services
            .extend(manifest.backend_services.into_iter().map(|service| {
                ExtensionBackendServiceProjection {
                    extension_key: extension_key.clone(),
                    extension_id: extension_id.clone(),
                    service_key: service.service_key,
                    runtime: service.runtime,
                    entry: service.entry,
                    routes: service.routes,
                    health_path: service.health_path,
                }
            }));
    }
    Ok(projection)
}

fn fetch_route_projection(
    extension_key: &str,
    extension_id: &str,
    route: ExtensionFetchRouteDefinition,
) -> ExtensionFetchRouteProjection {
    let route_key = route
        .route_key
        .clone()
        .unwrap_or_else(|| generated_fetch_route_key(extension_key, &route.route));
    let panel_only = route
        .panel_only
        .unwrap_or_else(|| !matches!(route.scope, Some(ExtensionFetchRouteScope::AgentAndPanel)));
    let target = fetch_route_target_projection(&route);
    ExtensionFetchRouteProjection {
        extension_key: extension_key.to_string(),
        extension_id: extension_id.to_string(),
        route_key,
        pattern: route.route,
        panel_only,
        target,
    }
}

fn fetch_route_target_projection(
    route: &ExtensionFetchRouteDefinition,
) -> ExtensionFetchRouteTargetProjection {
    match &route.target {
        ExtensionFetchRouteTargetDefinition::HttpProxy {
            capability_key,
            base_url,
        } => ExtensionFetchRouteTargetProjection::HttpProxy {
            capability_key: capability_key
                .clone()
                .or_else(|| base_url.clone())
                .unwrap_or_else(|| "http_proxy".to_string()),
        },
        ExtensionFetchRouteTargetDefinition::RuntimeAction { action_key } => {
            ExtensionFetchRouteTargetProjection::RuntimeAction {
                action_key: action_key.clone(),
            }
        }
        ExtensionFetchRouteTargetDefinition::CustomChannel {
            channel_key,
            method,
        }
        | ExtensionFetchRouteTargetDefinition::ProtocolChannel {
            channel_key,
            method,
        } => ExtensionFetchRouteTargetProjection::ProtocolChannel {
            channel_key: channel_key.clone(),
            method: method.clone(),
        },
        ExtensionFetchRouteTargetDefinition::BackendService {
            service_key,
            route: target_route,
        } => ExtensionFetchRouteTargetProjection::BackendService {
            service_key: service_key.clone(),
            route: target_route.clone().unwrap_or_else(|| route.route.clone()),
        },
    }
}

fn generated_operation_projection(
    extension_key: &str,
    extension_id: &str,
    operation: ExtensionGeneratedOperationDefinition,
) -> ExtensionGeneratedOperationProjection {
    ExtensionGeneratedOperationProjection {
        extension_key: extension_key.to_string(),
        extension_id: extension_id.to_string(),
        operation_key: operation.operation_key,
        description: operation.description,
        visibility: match operation.visibility {
            DomainGeneratedOperationVisibility::PanelOnly => {
                ExtensionGeneratedOperationVisibility::PanelOnly
            }
            DomainGeneratedOperationVisibility::AgentAndPanel => {
                ExtensionGeneratedOperationVisibility::AgentAndPanel
            }
        },
        input_schema: operation.input_schema,
        output_schema: operation.output_schema,
        permission_summary: operation.permission_summary,
        dispatch: match operation.dispatch {
            DomainGeneratedOperationDispatch::RuntimeAction { action_key } => {
                ExtensionGeneratedOperationDispatch::RuntimeAction { action_key }
            }
            DomainGeneratedOperationDispatch::ProtocolChannel {
                channel_key,
                method,
            } => ExtensionGeneratedOperationDispatch::ProtocolChannel {
                channel_key,
                method,
            },
            DomainGeneratedOperationDispatch::BackendService { service_key, route } => {
                ExtensionGeneratedOperationDispatch::BackendService { service_key, route }
            }
        },
        provenance: ExtensionGeneratedOperationProvenance {
            capability_key: operation.provenance.capability_key,
            exposure_key: operation.provenance.exposure_key,
            generated_from: operation.provenance.generated_from,
        },
    }
}

fn generated_fetch_route_key(extension_key: &str, route: &str) -> String {
    let suffix: String = route
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if suffix.is_empty() {
        format!("{extension_key}.route")
    } else {
        format!("{extension_key}.{suffix}")
    }
}

fn workspace_tab_loadability(
    renderer: &ExtensionWorkspaceTabRendererDeclaration,
    has_package_artifact: bool,
    has_extension_host_bundle: bool,
) -> ExtensionWorkspaceTabLoadabilityProjection {
    match renderer {
        ExtensionWorkspaceTabRendererDeclaration::Webview { entry } => {
            if !has_package_artifact {
                return ExtensionWorkspaceTabLoadabilityProjection {
                    available: false,
                    mode: ExtensionWorkspaceTabLoadabilityMode::ExtensionHost,
                    reason: Some("extension package artifact 缺失，webview 无法加载".to_string()),
                };
            }
            if !has_extension_host_bundle {
                return ExtensionWorkspaceTabLoadabilityProjection {
                    available: false,
                    mode: ExtensionWorkspaceTabLoadabilityMode::ExtensionHost,
                    reason: Some(
                        "extension host bundle 缺失，webview runtime 无法加载".to_string(),
                    ),
                };
            }
            if entry.trim().is_empty() {
                return ExtensionWorkspaceTabLoadabilityProjection {
                    available: false,
                    mode: ExtensionWorkspaceTabLoadabilityMode::ExtensionHost,
                    reason: Some("webview renderer entry 为空".to_string()),
                };
            }
            ExtensionWorkspaceTabLoadabilityProjection {
                available: true,
                mode: ExtensionWorkspaceTabLoadabilityMode::ExtensionHost,
                reason: None,
            }
        }
        ExtensionWorkspaceTabRendererDeclaration::CanvasPanel { entry } => {
            if !has_package_artifact {
                return ExtensionWorkspaceTabLoadabilityProjection {
                    available: false,
                    mode: ExtensionWorkspaceTabLoadabilityMode::UiOnly,
                    reason: Some(
                        "extension package artifact 缺失，Canvas panel 无法加载".to_string(),
                    ),
                };
            }
            if entry.trim().is_empty() {
                return ExtensionWorkspaceTabLoadabilityProjection {
                    available: false,
                    mode: ExtensionWorkspaceTabLoadabilityMode::UiOnly,
                    reason: Some("Canvas panel renderer entry 为空".to_string()),
                };
            }
            ExtensionWorkspaceTabLoadabilityProjection {
                available: true,
                mode: ExtensionWorkspaceTabLoadabilityMode::UiOnly,
                reason: None,
            }
        }
    }
}

fn protocol_channel_projection(
    extension_key: &str,
    extension_id: &str,
    channel: ExtensionProtocolChannelDefinition,
) -> ExtensionProtocolChannelProjection {
    ExtensionProtocolChannelProjection {
        extension_key: extension_key.to_string(),
        extension_id: extension_id.to_string(),
        channel_key: channel.channel_key,
        version: channel.version,
        description: channel.description,
        methods: channel
            .methods
            .into_iter()
            .map(protocol_channel_method_projection)
            .collect(),
    }
}

fn protocol_channel_method_projection(
    method: ExtensionProtocolChannelMethodDefinition,
) -> ExtensionProtocolChannelMethodProjection {
    ExtensionProtocolChannelMethodProjection {
        name: method.name,
        description: method.description,
        input_schema: method.input_schema,
        output_schema: method.output_schema,
        permissions: method.permissions,
    }
}

fn claim_unique_extension_runtime_key(
    index: &mut BTreeMap<String, String>,
    field: &str,
    value: &str,
    extension_key: &str,
) -> Result<(), DomainError> {
    match index.entry(value.to_string()) {
        Entry::Vacant(slot) => {
            slot.insert(extension_key.to_string());
            Ok(())
        }
        Entry::Occupied(slot) => Err(DomainError::InvalidConfig(format!(
            "Project extension runtime {field} 冲突: `{value}` 同时由 `{}` 与 `{extension_key}` 声明",
            slot.get()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use agentdash_domain::extension_package::ExtensionPackageArtifactRef;
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBackendServiceDefinition, ExtensionFetchRouteDefinition, ExtensionFetchRouteScope,
        ExtensionFetchRouteTargetDefinition,
        ExtensionGeneratedOperationDefinition as DomainGeneratedOperationDefinition,
        ExtensionGeneratedOperationDispatch as DomainGeneratedOperationDispatch,
        ExtensionGeneratedOperationProvenance as DomainGeneratedOperationProvenance,
        ExtensionGeneratedOperationVisibility as DomainGeneratedOperationVisibility,
    };
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionCommandDefinition,
        ExtensionCommandHandler, ExtensionDependencyDeclaration, ExtensionFlagDefinition,
        ExtensionFlagType, ExtensionMessageRendererDefinition, ExtensionPermissionAccess,
        ExtensionPermissionDeclaration, ExtensionProtocolChannelDefinition,
        ExtensionProtocolChannelMethodDefinition, ExtensionRendererDeclaration,
        ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ExtensionTemplatePayload,
        ExtensionWorkspaceTabDefinition, ExtensionWorkspaceTabRendererDeclaration,
        InstalledAssetSource, ProjectExtensionInstallation,
    };

    use super::*;

    fn source() -> InstalledAssetSource {
        InstalledAssetSource::new(
            uuid::Uuid::new_v4(),
            "integration:test:extension_template:demo",
            "0.1.0",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
    }

    fn manifest(
        extension_id: &str,
        action_key: &str,
        tab_type_id: &str,
        uri_scheme: &str,
    ) -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: extension_id.to_string(),
            package: ExtensionPackageMetadata {
                name: extension_id.to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![ExtensionCommandDefinition {
                name: format!("{extension_id}:run"),
                description: "run demo".to_string(),
                handler: ExtensionCommandHandler::InjectMessage {
                    content: "run".to_string(),
                },
            }],
            flags: vec![ExtensionFlagDefinition {
                name: format!("{extension_id}.verbose"),
                flag_type: ExtensionFlagType::Bool,
                default: serde_json::Value::Bool(false),
                description: "verbose".to_string(),
            }],
            message_renderers: vec![ExtensionMessageRendererDefinition {
                custom_type: format!("{extension_id}.card"),
                renderer: ExtensionRendererDeclaration::JsonCard,
            }],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![ExtensionRuntimeActionDefinition {
                action_key: action_key.to_string(),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "read profile".to_string(),
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                permissions: vec!["local.profile.read".to_string()],
            }],
            protocol_channels: vec![ExtensionProtocolChannelDefinition {
                channel_key: format!("{extension_id}.api"),
                version: "1.0.0".to_string(),
                description: "demo API channel".to_string(),
                methods: vec![ExtensionProtocolChannelMethodDefinition {
                    name: "readProfile".to_string(),
                    description: "read profile through channel".to_string(),
                    input_schema: serde_json::json!({}),
                    output_schema: serde_json::json!({}),
                    permissions: vec!["local.profile.read".to_string()],
                }],
            }],
            extension_dependencies: vec![ExtensionDependencyDeclaration {
                alias: "self_api".to_string(),
                extension_id: extension_id.to_string(),
                version: "^1.0.0".to_string(),
                channels: vec![format!("{extension_id}.api")],
            }],
            workspace_tabs: vec![ExtensionWorkspaceTabDefinition {
                type_id: tab_type_id.to_string(),
                label: "Profile".to_string(),
                uri_scheme: uri_scheme.to_string(),
                renderer: ExtensionWorkspaceTabRendererDeclaration::Webview {
                    entry: "dist/panel/index.html".to_string(),
                },
            }],
            permissions: vec![ExtensionPermissionDeclaration::LocalProfile {
                access: ExtensionPermissionAccess::Read,
            }],
            fetch_routes: vec![],
            operation_catalog: vec![],
            backend_services: vec![],
            bundles: vec![ExtensionBundleRef {
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            }],
        }
    }

    fn installation(
        key: &str,
        action_key: &str,
        tab_type_id: &str,
        uri_scheme: &str,
    ) -> ProjectExtensionInstallation {
        ProjectExtensionInstallation::new(
            uuid::Uuid::new_v4(),
            key,
            format!("{key} Extension"),
            manifest(key, action_key, tab_type_id, uri_scheme),
            source(),
        )
        .expect("valid installation")
    }

    fn artifact_ref(extension_id: &str) -> ExtensionPackageArtifactRef {
        ExtensionPackageArtifactRef {
            artifact_id: uuid::Uuid::new_v4(),
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

    fn canvas_panel_manifest(extension_id: &str) -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: extension_id.to_string(),
            package: ExtensionPackageMetadata {
                name: format!("@agentdash/{extension_id}"),
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
            extension_dependencies: vec![],
            workspace_tabs: vec![ExtensionWorkspaceTabDefinition {
                type_id: format!("{extension_id}.panel"),
                label: "Canvas".to_string(),
                uri_scheme: extension_id.to_string(),
                renderer: ExtensionWorkspaceTabRendererDeclaration::CanvasPanel {
                    entry: "dist/canvas/runtime-snapshot.json".to_string(),
                },
            }],
            permissions: vec![],
            fetch_routes: vec![],
            operation_catalog: vec![],
            backend_services: vec![],
            bundles: vec![],
        }
    }

    #[test]
    fn flattens_enabled_extension_runtime_projection() {
        let projection = extension_runtime_projection_from_installations(vec![installation(
            "demo",
            "demo.profile",
            "demo.profile-panel",
            "demo",
        )])
        .expect("projection");

        assert_eq!(projection.installations.len(), 1);
        assert_eq!(projection.commands[0].name, "demo:run");
        assert_eq!(projection.flags[0].name, "demo.verbose");
        assert_eq!(projection.message_renderers[0].custom_type, "demo.card");
        assert_eq!(projection.runtime_actions[0].action_key, "demo.profile");
        assert_eq!(projection.protocol_channels[0].channel_key, "demo.api");
        assert_eq!(
            projection.protocol_channels[0].methods[0].name,
            "readProfile"
        );
        assert_eq!(
            projection.extension_dependencies[0].dependency.alias,
            "self_api"
        );
        assert_eq!(projection.workspace_tabs[0].type_id, "demo.profile-panel");
        assert!(!projection.workspace_tabs[0].loadability.available);
        assert_eq!(
            projection.workspace_tabs[0].loadability.mode,
            ExtensionWorkspaceTabLoadabilityMode::ExtensionHost
        );
        assert_eq!(projection.permissions.len(), 1);
        assert_eq!(projection.bundles[0].entry, "dist/extension.js");
    }

    #[test]
    fn projects_generated_operation_catalog_backend_services_and_fetch_routes() {
        let project_id = uuid::Uuid::new_v4();
        let mut manifest = manifest("demo", "demo.profile", "demo.panel", "demo");
        manifest.fetch_routes = vec![ExtensionFetchRouteDefinition {
            route_key: Some("demo.api-route".to_string()),
            route: "/api/**".to_string(),
            scope: Some(ExtensionFetchRouteScope::PanelOnly),
            panel_only: None,
            target: ExtensionFetchRouteTargetDefinition::BackendService {
                service_key: "demo.api".to_string(),
                route: Some("/api/**".to_string()),
            },
        }];
        manifest.backend_services = vec![ExtensionBackendServiceDefinition {
            service_key: "demo.api".to_string(),
            runtime: "node".to_string(),
            entry: "src/server/index.ts".to_string(),
            routes: vec!["/api/**".to_string()],
            health_path: Some("/health".to_string()),
        }];
        manifest.operation_catalog = vec![DomainGeneratedOperationDefinition {
            operation_key: "demo.search".to_string(),
            description: "Search through the generated backend service".to_string(),
            visibility: DomainGeneratedOperationVisibility::AgentAndPanel,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({"type": "object"}),
            permission_summary: vec!["backend_service:demo.api".to_string()],
            dispatch: DomainGeneratedOperationDispatch::BackendService {
                service_key: "demo.api".to_string(),
                route: "/api/search".to_string(),
            },
            provenance: DomainGeneratedOperationProvenance {
                capability_key: "api".to_string(),
                exposure_key: "search".to_string(),
                generated_from: "capability_exposure".to_string(),
            },
        }];
        let installation =
            ProjectExtensionInstallation::new(project_id, "demo", "Demo", manifest, source())
                .expect("valid generated manifest installation");

        let projection = extension_runtime_projection_from_installations(vec![installation])
            .expect("projection");

        assert_eq!(projection.fetch_routes.len(), 1);
        assert_eq!(projection.fetch_routes[0].pattern, "/api/**");
        assert!(projection.fetch_routes[0].panel_only);
        assert!(matches!(
            &projection.fetch_routes[0].target,
            ExtensionFetchRouteTargetProjection::BackendService { service_key, route }
                if service_key == "demo.api" && route == "/api/**"
        ));
        assert_eq!(projection.backend_services.len(), 1);
        assert_eq!(projection.backend_services[0].service_key, "demo.api");
        assert_eq!(
            projection.backend_services[0].health_path.as_deref(),
            Some("/health")
        );
        assert_eq!(projection.operation_catalog.len(), 1);
        assert!(matches!(
            &projection.operation_catalog[0].dispatch,
            ExtensionGeneratedOperationDispatch::BackendService { service_key, route }
                if service_key == "demo.api" && route == "/api/search"
        ));
    }

    #[test]
    fn canvas_panel_tab_is_ui_only_and_loadable_without_extension_host_bundle() {
        let project_id = uuid::Uuid::new_v4();
        let installation = ProjectExtensionInstallation::new_packaged(
            project_id,
            "canvas-demo",
            "Canvas Demo",
            canvas_panel_manifest("canvas-demo"),
            artifact_ref("canvas-demo"),
        )
        .expect("packaged canvas panel installation");

        let projection = extension_runtime_projection_from_installations(vec![installation])
            .expect("projection");

        assert!(projection.bundles.is_empty());
        let tab = &projection.workspace_tabs[0];
        assert!(tab.loadability.available);
        assert_eq!(
            tab.loadability.mode,
            ExtensionWorkspaceTabLoadabilityMode::UiOnly
        );
    }

    #[derive(Default)]
    struct FakeUninstallRepo {
        installations: Mutex<Vec<ProjectExtensionInstallation>>,
    }

    #[async_trait::async_trait]
    impl ProjectExtensionInstallationRepository for FakeUninstallRepo {
        async fn create(
            &self,
            installation: &ProjectExtensionInstallation,
        ) -> Result<(), DomainError> {
            self.installations
                .lock()
                .unwrap()
                .push(installation.clone());
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
            project_id: Uuid,
            extension_key: &str,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .find(|installation| {
                    installation.project_id == project_id
                        && installation.extension_key == extension_key
                })
                .cloned())
        }

        async fn get_by_project_and_id(
            &self,
            project_id: Uuid,
            installation_id: Uuid,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .find(|installation| {
                    installation.project_id == project_id && installation.id == installation_id
                })
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .filter(|installation| installation.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_enabled_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .filter(|installation| {
                    installation.project_id == project_id && installation.enabled
                })
                .cloned()
                .collect())
        }

        async fn delete(
            &self,
            project_id: Uuid,
            installation_id: Uuid,
        ) -> Result<bool, DomainError> {
            let mut guard = self.installations.lock().unwrap();
            let before = guard.len();
            guard.retain(|installation| {
                !(installation.project_id == project_id && installation.id == installation_id)
            });
            Ok(guard.len() < before)
        }
    }

    fn install_into_repo(
        repo: &FakeUninstallRepo,
        project_id: Uuid,
        extension_key: &str,
    ) -> ProjectExtensionInstallation {
        let manifest = manifest(
            extension_key,
            &format!("{extension_key}.action"),
            &format!("{extension_key}.panel"),
            extension_key,
        );
        let installation = ProjectExtensionInstallation::new(
            project_id,
            extension_key,
            format!("{extension_key} Extension"),
            manifest,
            source(),
        )
        .expect("valid installation");
        repo.installations
            .lock()
            .unwrap()
            .push(installation.clone());
        installation
    }

    #[tokio::test]
    async fn uninstall_extension_installation_returns_extension_key_and_removes_row() {
        let repo_inner = Arc::new(FakeUninstallRepo::default());
        let repo: Arc<dyn ProjectExtensionInstallationRepository> = repo_inner.clone();
        let project_id = Uuid::new_v4();
        let installation = install_into_repo(repo_inner.as_ref(), project_id, "demo");

        let output = uninstall_extension_installation_with_repo(
            &repo,
            UninstallExtensionInstallationInput {
                project_id,
                installation_id: installation.id,
            },
        )
        .await
        .expect("uninstall happy path");
        assert_eq!(output.installation_id, installation.id);
        assert_eq!(output.extension_key, "demo");

        let remaining = repo
            .list_by_project(project_id)
            .await
            .expect("list after uninstall");
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn uninstall_extension_installation_returns_not_found_for_missing_id() {
        let repo: Arc<dyn ProjectExtensionInstallationRepository> =
            Arc::new(FakeUninstallRepo::default());
        let project_id = Uuid::new_v4();
        let installation_id = Uuid::new_v4();

        let err = uninstall_extension_installation_with_repo(
            &repo,
            UninstallExtensionInstallationInput {
                project_id,
                installation_id,
            },
        )
        .await
        .expect_err("missing installation");
        match err {
            DomainError::NotFound { entity, id } => {
                assert_eq!(entity, "project_extension_installation");
                assert_eq!(id, installation_id.to_string());
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn uninstall_extension_installation_rejects_cross_project_access() {
        let repo_inner = Arc::new(FakeUninstallRepo::default());
        let repo: Arc<dyn ProjectExtensionInstallationRepository> = repo_inner.clone();
        let project_a = Uuid::new_v4();
        let project_b = Uuid::new_v4();
        let installation = install_into_repo(repo_inner.as_ref(), project_a, "demo");

        let err = uninstall_extension_installation_with_repo(
            &repo,
            UninstallExtensionInstallationInput {
                project_id: project_b,
                installation_id: installation.id,
            },
        )
        .await
        .expect_err("cross project should be NotFound");
        assert!(matches!(err, DomainError::NotFound { .. }));

        // Original installation must still exist for project A.
        let still_there = repo
            .get_by_project_and_id(project_a, installation.id)
            .await
            .expect("get after rejected uninstall");
        assert!(still_there.is_some());
    }

    #[test]
    fn rejects_project_extension_runtime_conflicts() {
        let duplicate_action = extension_runtime_projection_from_installations(vec![
            installation("alpha", "shared.action", "alpha.panel", "alpha"),
            installation("beta", "shared.action", "beta.panel", "beta"),
        ]);
        assert!(duplicate_action.is_err());

        let duplicate_tab = extension_runtime_projection_from_installations(vec![
            installation("alpha", "alpha.action", "shared.panel", "alpha"),
            installation("beta", "beta.action", "shared.panel", "beta"),
        ]);
        assert!(duplicate_tab.is_err());

        let duplicate_channel = extension_runtime_projection_from_installations(vec![
            installation("alpha", "alpha.action", "alpha.panel", "alpha"),
            installation("alpha", "alpha.other", "alpha.other-panel", "alpha-other"),
        ]);
        assert!(duplicate_channel.is_err());

        let duplicate_scheme = extension_runtime_projection_from_installations(vec![
            installation("alpha", "alpha.action", "alpha.panel", "shared"),
            installation("beta", "beta.action", "beta.panel", "shared"),
        ]);
        assert!(duplicate_scheme.is_err());
    }
}
