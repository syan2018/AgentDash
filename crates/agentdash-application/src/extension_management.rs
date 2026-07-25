use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::extension_package::ExtensionPackageArtifactRef;
use agentdash_domain::shared_library::{
    InstalledAssetSource, ProjectExtensionInstallation, SharedLibrarySourceStatus,
};

use crate::repository_set::RepositorySet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectExtensionPackageMode {
    Packaged,
    DeclarationOnly,
    InvalidMissingArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectExtensionCapabilitySummary {
    pub commands: usize,
    pub flags: usize,
    pub message_renderers: usize,
    pub runtime_actions: usize,
    pub protocols: usize,
    pub workspace_tabs: usize,
    pub permissions: usize,
    pub bundles: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectExtensionSourceSummary {
    pub installed_source: InstalledAssetSource,
    pub source_status: SharedLibrarySourceStatus,
    pub current_source_version: Option<String>,
    pub current_source_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectExtensionManagementItem {
    pub installation: ProjectExtensionInstallation,
    pub package_mode: ProjectExtensionPackageMode,
    pub package_artifact: Option<ExtensionPackageArtifactRef>,
    pub capability_summary: ProjectExtensionCapabilitySummary,
    pub source_summary: Option<ProjectExtensionSourceSummary>,
}

pub async fn list_project_extension_management_items(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<Vec<ProjectExtensionManagementItem>, DomainError> {
    let installations = repos
        .project_extension_installation_repo
        .list_by_project(project_id)
        .await?;
    let mut items = Vec::with_capacity(installations.len());
    for installation in installations {
        let source_summary = match &installation.installed_source {
            Some(source) => Some(source_summary(repos, source).await?),
            None => None,
        };
        let package_artifact = installation.package_artifact.clone();
        let package_mode = package_mode(&installation);
        let capability_summary = capability_summary(&installation);
        items.push(ProjectExtensionManagementItem {
            installation,
            package_mode,
            package_artifact,
            capability_summary,
            source_summary,
        });
    }
    Ok(items)
}

async fn source_summary(
    repos: &RepositorySet,
    installed_source: &InstalledAssetSource,
) -> Result<ProjectExtensionSourceSummary, DomainError> {
    let current = repos
        .shared_library_repo
        .get(installed_source.library_asset_id)
        .await?;
    let source_status = SharedLibrarySourceStatus::from_installed_source(
        installed_source,
        current.as_ref().map(|asset| asset.version.as_str()),
        current.as_ref().map(|asset| asset.payload_digest.as_str()),
        current.as_ref().is_none_or(|asset| asset.deprecated),
    );
    Ok(ProjectExtensionSourceSummary {
        installed_source: installed_source.clone(),
        source_status,
        current_source_version: current.as_ref().map(|asset| asset.version.clone()),
        current_source_digest: current.as_ref().map(|asset| asset.payload_digest.clone()),
    })
}

fn package_mode(installation: &ProjectExtensionInstallation) -> ProjectExtensionPackageMode {
    if installation.package_artifact.is_some() {
        ProjectExtensionPackageMode::Packaged
    } else if installation.manifest.requires_package_artifact() {
        ProjectExtensionPackageMode::InvalidMissingArtifact
    } else {
        ProjectExtensionPackageMode::DeclarationOnly
    }
}

fn capability_summary(
    installation: &ProjectExtensionInstallation,
) -> ProjectExtensionCapabilitySummary {
    let manifest = &installation.manifest;
    ProjectExtensionCapabilitySummary {
        commands: manifest.commands.len(),
        flags: manifest.flags.len(),
        message_renderers: manifest.message_renderers.len(),
        runtime_actions: manifest.runtime_actions.len(),
        protocols: manifest.protocols.len(),
        workspace_tabs: manifest.workspace_tabs.len(),
        permissions: manifest.permissions.len(),
        bundles: manifest.bundles.len(),
    }
}
