use agentdash_application::extension_runtime::ExtensionRuntimeProjection;
use agentdash_domain::shared_library::{
    ExtensionBundleKind, ExtensionCommandHandler, ExtensionFlagType, ExtensionPermissionAccess,
    ExtensionPermissionDeclaration, ExtensionRendererDeclaration, ExtensionRuntimeActionKind,
    ExtensionWorkspaceTabRendererDeclaration,
};

pub use agentdash_contracts::extension_runtime::{
    ExtensionBundleKindResponse, ExtensionBundleProjectionResponse,
    ExtensionCommandHandlerResponse, ExtensionCommandProjectionResponse,
    ExtensionFlagProjectionResponse, ExtensionFlagTypeResponse,
    ExtensionInstallationProjectionResponse, ExtensionInstalledAssetSourceResponse,
    ExtensionMessageRendererDeclarationResponse, ExtensionMessageRendererProjectionResponse,
    ExtensionPackageArtifactRefResponse, ExtensionPermissionAccessResponse,
    ExtensionPermissionDeclarationResponse, ExtensionPermissionProjectionResponse,
    ExtensionRuntimeActionKindResponse, ExtensionRuntimeActionProjectionResponse,
    ExtensionRuntimeProjectionResponse, ExtensionWorkspaceTabProjectionResponse,
    ExtensionWorkspaceTabRendererResponse,
};

pub fn extension_runtime_projection_response(
    projection: ExtensionRuntimeProjection,
) -> ExtensionRuntimeProjectionResponse {
    ExtensionRuntimeProjectionResponse {
        installations: projection
            .installations
            .into_iter()
            .map(|installation| ExtensionInstallationProjectionResponse {
                installation_id: installation.installation_id.to_string(),
                extension_key: installation.extension_key,
                extension_id: installation.extension_id,
                display_name: installation.display_name,
                installed_source: installation.installed_source.map(|source| {
                    ExtensionInstalledAssetSourceResponse {
                        library_asset_id: source.library_asset_id.to_string(),
                        source_ref: source.source_ref,
                        source_version: source.source_version,
                        source_digest: source.source_digest,
                        installed_at: source.installed_at.to_rfc3339(),
                    }
                }),
                package_artifact: installation.package_artifact.map(|artifact| {
                    ExtensionPackageArtifactRefResponse {
                        artifact_id: artifact.artifact_id.to_string(),
                        package_name: artifact.package_name,
                        package_version: artifact.package_version,
                        asset_version: artifact.asset_version,
                        source_version: artifact.source_version,
                        storage_ref: artifact.storage_ref,
                        archive_digest: artifact.archive_digest,
                        manifest_digest: artifact.manifest_digest,
                    }
                }),
            })
            .collect(),
        commands: projection
            .commands
            .into_iter()
            .map(|command| ExtensionCommandProjectionResponse {
                extension_key: command.extension_key,
                extension_id: command.extension_id,
                name: command.name,
                description: command.description,
                handler: match command.handler {
                    ExtensionCommandHandler::InjectMessage { content } => {
                        ExtensionCommandHandlerResponse::InjectMessage { content }
                    }
                },
            })
            .collect(),
        flags: projection
            .flags
            .into_iter()
            .map(|flag| ExtensionFlagProjectionResponse {
                extension_key: flag.extension_key,
                extension_id: flag.extension_id,
                name: flag.name,
                flag_type: match flag.flag_type {
                    ExtensionFlagType::Bool => ExtensionFlagTypeResponse::Bool,
                    ExtensionFlagType::String => ExtensionFlagTypeResponse::String,
                },
                default: flag.default,
                description: flag.description,
            })
            .collect(),
        message_renderers: projection
            .message_renderers
            .into_iter()
            .map(|renderer| ExtensionMessageRendererProjectionResponse {
                extension_key: renderer.extension_key,
                extension_id: renderer.extension_id,
                custom_type: renderer.custom_type,
                renderer: match renderer.renderer {
                    ExtensionRendererDeclaration::JsonCard => {
                        ExtensionMessageRendererDeclarationResponse::JsonCard
                    }
                    ExtensionRendererDeclaration::Markdown => {
                        ExtensionMessageRendererDeclarationResponse::Markdown
                    }
                },
            })
            .collect(),
        runtime_actions: projection
            .runtime_actions
            .into_iter()
            .map(|action| ExtensionRuntimeActionProjectionResponse {
                extension_key: action.extension_key,
                extension_id: action.extension_id,
                action_key: action.action_key,
                kind: match action.kind {
                    ExtensionRuntimeActionKind::SessionRuntime => {
                        ExtensionRuntimeActionKindResponse::SessionRuntime
                    }
                    ExtensionRuntimeActionKind::Setup => ExtensionRuntimeActionKindResponse::Setup,
                },
                description: action.description,
                input_schema: action.input_schema,
                output_schema: action.output_schema,
                permissions: action.permissions,
            })
            .collect(),
        workspace_tabs: projection
            .workspace_tabs
            .into_iter()
            .map(|tab| ExtensionWorkspaceTabProjectionResponse {
                extension_key: tab.extension_key,
                extension_id: tab.extension_id,
                type_id: tab.type_id,
                label: tab.label,
                uri_scheme: tab.uri_scheme,
                renderer: match tab.renderer {
                    ExtensionWorkspaceTabRendererDeclaration::Webview { entry } => {
                        ExtensionWorkspaceTabRendererResponse::Webview { entry }
                    }
                },
            })
            .collect(),
        permissions: projection
            .permissions
            .into_iter()
            .map(|permission| ExtensionPermissionProjectionResponse {
                extension_key: permission.extension_key,
                extension_id: permission.extension_id,
                permission: extension_permission_response(permission.permission),
            })
            .collect(),
        bundles: projection
            .bundles
            .into_iter()
            .map(|bundle| ExtensionBundleProjectionResponse {
                extension_key: bundle.extension_key,
                extension_id: bundle.extension_id,
                kind: match bundle.kind {
                    ExtensionBundleKind::ExtensionHost => {
                        ExtensionBundleKindResponse::ExtensionHost
                    }
                },
                entry: bundle.entry,
                digest: bundle.digest,
            })
            .collect(),
    }
}

fn extension_permission_response(
    permission: ExtensionPermissionDeclaration,
) -> ExtensionPermissionDeclarationResponse {
    match permission {
        ExtensionPermissionDeclaration::LocalProfile { access } => {
            ExtensionPermissionDeclarationResponse::LocalProfile {
                access: extension_permission_access_response(access),
            }
        }
        ExtensionPermissionDeclaration::Workspace { access } => {
            ExtensionPermissionDeclarationResponse::Workspace {
                access: extension_permission_access_response(access),
            }
        }
        ExtensionPermissionDeclaration::RuntimeAction { action_key } => {
            ExtensionPermissionDeclarationResponse::RuntimeAction { action_key }
        }
    }
}

fn extension_permission_access_response(
    access: ExtensionPermissionAccess,
) -> ExtensionPermissionAccessResponse {
    match access {
        ExtensionPermissionAccess::Read => ExtensionPermissionAccessResponse::Read,
        ExtensionPermissionAccess::Write => ExtensionPermissionAccessResponse::Write,
        ExtensionPermissionAccess::ReadWrite => ExtensionPermissionAccessResponse::ReadWrite,
    }
}
