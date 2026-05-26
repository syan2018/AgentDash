use std::collections::{BTreeMap, btree_map::Entry};

use agentdash_domain::DomainError;
use agentdash_domain::extension_package::ExtensionPackageArtifactRef;
use agentdash_domain::shared_library::{
    ExtensionBundleKind, ExtensionCommandHandler, ExtensionFlagType,
    ExtensionPermissionDeclaration, ExtensionRendererDeclaration, ExtensionRuntimeActionKind,
    ExtensionWorkspaceTabRendererDeclaration, InstalledAssetSource, ProjectExtensionInstallation,
};
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtensionRuntimeProjection {
    pub installations: Vec<ExtensionInstallationProjection>,
    pub commands: Vec<ExtensionCommandProjection>,
    pub flags: Vec<ExtensionFlagProjection>,
    pub message_renderers: Vec<ExtensionMessageRendererProjection>,
    pub runtime_actions: Vec<ExtensionRuntimeActionProjection>,
    pub workspace_tabs: Vec<ExtensionWorkspaceTabProjection>,
    pub permissions: Vec<ExtensionPermissionProjection>,
    pub bundles: Vec<ExtensionBundleProjection>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionWorkspaceTabProjection {
    pub extension_key: String,
    pub extension_id: String,
    pub type_id: String,
    pub label: String,
    pub uri_scheme: String,
    pub renderer: ExtensionWorkspaceTabRendererDeclaration,
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

pub fn extension_runtime_projection_from_installations(
    installations: Vec<ProjectExtensionInstallation>,
) -> Result<ExtensionRuntimeProjection, DomainError> {
    let mut projection = ExtensionRuntimeProjection::default();
    let mut action_keys = BTreeMap::new();
    let mut workspace_tab_type_ids = BTreeMap::new();
    let mut uri_schemes = BTreeMap::new();
    for installation in installations {
        let extension_key = installation.extension_key.clone();
        let extension_id = installation.manifest.extension_id.clone();
        let manifest = installation.manifest;
        for action in &manifest.runtime_actions {
            claim_unique_extension_runtime_key(
                &mut action_keys,
                "runtime action key",
                &action.action_key,
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
        projection
            .workspace_tabs
            .extend(manifest.workspace_tabs.into_iter().map(|tab| {
                ExtensionWorkspaceTabProjection {
                    extension_key: extension_key.clone(),
                    extension_id: extension_id.clone(),
                    type_id: tab.type_id,
                    label: tab.label,
                    uri_scheme: tab.uri_scheme,
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
    }
    Ok(projection)
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
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionCommandDefinition,
        ExtensionCommandHandler, ExtensionFlagDefinition, ExtensionFlagType,
        ExtensionMessageRendererDefinition, ExtensionPermissionAccess,
        ExtensionPermissionDeclaration, ExtensionRendererDeclaration,
        ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ExtensionTemplatePayload,
        ExtensionWorkspaceTabDefinition, ExtensionWorkspaceTabRendererDeclaration,
        InstalledAssetSource, ProjectExtensionInstallation,
    };

    use super::*;

    fn source() -> InstalledAssetSource {
        InstalledAssetSource::new(
            uuid::Uuid::new_v4(),
            "plugin:test:extension_template:demo",
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
        assert_eq!(projection.workspace_tabs[0].type_id, "demo.profile-panel");
        assert_eq!(projection.permissions.len(), 1);
        assert_eq!(projection.bundles[0].entry, "dist/extension.js");
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

        let duplicate_scheme = extension_runtime_projection_from_installations(vec![
            installation("alpha", "alpha.action", "alpha.panel", "shared"),
            installation("beta", "beta.action", "beta.panel", "shared"),
        ]);
        assert!(duplicate_scheme.is_err());
    }
}
