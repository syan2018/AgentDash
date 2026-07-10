use agentdash_application::extension_management::{
    ProjectExtensionManagementItem, ProjectExtensionPackageMode,
};

pub use agentdash_contracts::extension_management::{
    ProjectExtensionCapabilitySummaryResponse, ProjectExtensionInstalledSourceResponse,
    ProjectExtensionManagementItemResponse, ProjectExtensionManagementListResponse,
    ProjectExtensionPackageArtifactRefResponse, ProjectExtensionPackageModeResponse,
};

pub fn project_extension_management_list_response(
    items: Vec<ProjectExtensionManagementItem>,
) -> ProjectExtensionManagementListResponse {
    ProjectExtensionManagementListResponse {
        extensions: items
            .into_iter()
            .map(project_extension_management_item_response)
            .collect(),
    }
}

fn project_extension_management_item_response(
    item: ProjectExtensionManagementItem,
) -> ProjectExtensionManagementItemResponse {
    let source = item.source_summary;
    ProjectExtensionManagementItemResponse {
        installation_id: item.installation.id.to_string(),
        extension_key: item.installation.extension_key,
        extension_id: item.installation.manifest.extension_id.clone(),
        display_name: item.installation.display_name,
        enabled: item.installation.enabled,
        installed_source: source
            .as_ref()
            .map(|source| ProjectExtensionInstalledSourceResponse {
                library_asset_id: source.installed_source.library_asset_id.to_string(),
                source_ref: source.installed_source.source_ref.clone(),
                source_version: source.installed_source.source_version.clone(),
                source_digest: source.installed_source.source_digest.clone(),
                installed_at: source.installed_source.installed_at.to_rfc3339(),
            }),
        source_status: source
            .as_ref()
            .map(|source| source.source_status.as_str().to_string()),
        current_source_version: source
            .as_ref()
            .and_then(|source| source.current_source_version.clone()),
        current_source_digest: source
            .as_ref()
            .and_then(|source| source.current_source_digest.clone()),
        package_mode: match item.package_mode {
            ProjectExtensionPackageMode::Packaged => ProjectExtensionPackageModeResponse::Packaged,
            ProjectExtensionPackageMode::DeclarationOnly => {
                ProjectExtensionPackageModeResponse::DeclarationOnly
            }
            ProjectExtensionPackageMode::InvalidMissingArtifact => {
                ProjectExtensionPackageModeResponse::InvalidMissingArtifact
            }
        },
        package_artifact: item.package_artifact.map(|artifact| {
            ProjectExtensionPackageArtifactRefResponse {
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
        capability_summary: ProjectExtensionCapabilitySummaryResponse {
            commands: item.capability_summary.commands,
            flags: item.capability_summary.flags,
            message_renderers: item.capability_summary.message_renderers,
            runtime_actions: item.capability_summary.runtime_actions,
            protocols: item.capability_summary.protocols,
            workspace_tabs: item.capability_summary.workspace_tabs,
            permissions: item.capability_summary.permissions,
            bundles: item.capability_summary.bundles,
        },
        manifest: serde_json::to_value(item.installation.manifest)
            .unwrap_or(serde_json::Value::Null),
        created_at: item.installation.created_at.to_rfc3339(),
        updated_at: item.installation.updated_at.to_rfc3339(),
    }
}
