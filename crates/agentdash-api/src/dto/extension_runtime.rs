use agentdash_application::extension_runtime::ExtensionRuntimeProjection;
use agentdash_application::extension_runtime::ExtensionWorkspaceTabLoadabilityMode;
use agentdash_domain::shared_library::{
    ExtensionBundleKind, ExtensionCommandHandler, ExtensionDependencyDeclaration,
    ExtensionFlagType, ExtensionPermissionAccess, ExtensionPermissionDeclaration,
    ExtensionProcessPermissionAccess, ExtensionRendererDeclaration, ExtensionRuntimeActionKind,
    ExtensionWorkspaceTabRendererDeclaration,
};

pub use agentdash_contracts::extension_runtime::{
    ExtensionBundleKindResponse, ExtensionBundleProjectionResponse,
    ExtensionCommandHandlerResponse, ExtensionCommandProjectionResponse,
    ExtensionDependencyDeclarationResponse, ExtensionDependencyProjectionResponse,
    ExtensionFlagProjectionResponse, ExtensionFlagTypeResponse,
    ExtensionInstallationProjectionResponse, ExtensionInstalledAssetSourceResponse,
    ExtensionMessageRendererDeclarationResponse, ExtensionMessageRendererProjectionResponse,
    ExtensionPackageArtifactRefResponse, ExtensionPermissionAccessResponse,
    ExtensionPermissionDeclarationResponse, ExtensionPermissionProjectionResponse,
    ExtensionProcessPermissionAccessResponse, ExtensionProtocolChannelMethodProjectionResponse,
    ExtensionProtocolChannelProjectionResponse, ExtensionRuntimeActionKindResponse,
    ExtensionRuntimeActionProjectionResponse, ExtensionRuntimeProjectionResponse,
    ExtensionWorkspaceTabLoadabilityModeResponse, ExtensionWorkspaceTabLoadabilityResponse,
    ExtensionWorkspaceTabProjectionResponse, ExtensionWorkspaceTabRendererResponse,
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
        protocol_channels: projection
            .protocol_channels
            .into_iter()
            .map(|channel| ExtensionProtocolChannelProjectionResponse {
                extension_key: channel.extension_key,
                extension_id: channel.extension_id,
                channel_key: channel.channel_key,
                version: channel.version,
                description: channel.description,
                methods: channel
                    .methods
                    .into_iter()
                    .map(|method| ExtensionProtocolChannelMethodProjectionResponse {
                        name: method.name,
                        description: method.description,
                        input_schema: method.input_schema,
                        output_schema: method.output_schema,
                        permissions: method.permissions,
                    })
                    .collect(),
            })
            .collect(),
        extension_dependencies: projection
            .extension_dependencies
            .into_iter()
            .map(|dependency| ExtensionDependencyProjectionResponse {
                extension_key: dependency.extension_key,
                extension_id: dependency.extension_id,
                dependency: extension_dependency_response(dependency.dependency),
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
                    ExtensionWorkspaceTabRendererDeclaration::CanvasPanel { entry } => {
                        ExtensionWorkspaceTabRendererResponse::CanvasPanel { entry }
                    }
                },
                loadability: ExtensionWorkspaceTabLoadabilityResponse {
                    available: tab.loadability.available,
                    mode: match tab.loadability.mode {
                        ExtensionWorkspaceTabLoadabilityMode::ExtensionHost => {
                            ExtensionWorkspaceTabLoadabilityModeResponse::ExtensionHost
                        }
                        ExtensionWorkspaceTabLoadabilityMode::UiOnly => {
                            ExtensionWorkspaceTabLoadabilityModeResponse::UiOnly
                        }
                    },
                    reason: tab.loadability.reason,
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
        ExtensionPermissionDeclaration::Http { hosts, access } => {
            ExtensionPermissionDeclarationResponse::Http {
                hosts,
                access: extension_permission_access_response(access),
            }
        }
        ExtensionPermissionDeclaration::Workspace { access } => {
            ExtensionPermissionDeclarationResponse::Workspace {
                access: extension_permission_access_response(access),
            }
        }
        ExtensionPermissionDeclaration::Env { names, access } => {
            ExtensionPermissionDeclarationResponse::Env {
                names,
                access: extension_permission_access_response(access),
            }
        }
        ExtensionPermissionDeclaration::Process { access } => {
            ExtensionPermissionDeclarationResponse::Process {
                access: extension_process_permission_access_response(access),
            }
        }
        ExtensionPermissionDeclaration::RuntimeAction { action_key } => {
            ExtensionPermissionDeclarationResponse::RuntimeAction { action_key }
        }
        ExtensionPermissionDeclaration::ExtensionChannel {
            channel_key,
            methods,
        } => ExtensionPermissionDeclarationResponse::ExtensionChannel {
            channel_key,
            methods,
        },
    }
}

fn extension_dependency_response(
    dependency: ExtensionDependencyDeclaration,
) -> ExtensionDependencyDeclarationResponse {
    ExtensionDependencyDeclarationResponse {
        alias: dependency.alias,
        extension_id: dependency.extension_id,
        version: dependency.version,
        channels: dependency.channels,
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

fn extension_process_permission_access_response(
    access: ExtensionProcessPermissionAccess,
) -> ExtensionProcessPermissionAccessResponse {
    match access {
        ExtensionProcessPermissionAccess::Execute => {
            ExtensionProcessPermissionAccessResponse::Execute
        }
    }
}
