use agentdash_domain::DomainError;

pub use agentdash_workspace_module::extension_runtime::{
    ExtensionBackendServiceProjection, ExtensionBundleProjection, ExtensionCommandProjection,
    ExtensionDependencyProjection, ExtensionFetchRouteProjection,
    ExtensionFetchRouteTargetProjection, ExtensionFlagProjection,
    ExtensionGeneratedOperationDispatch, ExtensionGeneratedOperationProjection,
    ExtensionGeneratedOperationProvenance, ExtensionGeneratedOperationVisibility,
    ExtensionInstallationProjection, ExtensionMessageRendererProjection,
    ExtensionPermissionProjection, ExtensionProtocolMethodProjection, ExtensionProtocolProjection,
    ExtensionRuntimeActionProjection, ExtensionRuntimeProjection,
    ExtensionWorkspaceTabLoadabilityMode, ExtensionWorkspaceTabLoadabilityProjection,
    ExtensionWorkspaceTabProjection, UninstallExtensionInstallationInput,
    UninstallExtensionInstallationOutput, extension_runtime_projection_from_installations,
    uninstall_extension_installation_with_repo,
};

use crate::repository_set::RepositorySet;

pub async fn uninstall_extension_installation(
    repos: &RepositorySet,
    input: UninstallExtensionInstallationInput,
) -> Result<UninstallExtensionInstallationOutput, DomainError> {
    uninstall_extension_installation_with_repo(&repos.project_extension_installation_repo, input)
        .await
}
