mod entity;
mod project_extension;
mod repository;
mod value_objects;

pub use entity::LibraryAsset;
pub use project_extension::{ProjectExtensionInstallation, ProjectExtensionInstallationRepository};
pub use repository::{LibraryAssetListFilter, LibraryAssetRepository};
pub use value_objects::{
    AgentMcpSlotTemplate, AgentTemplateConfig, AgentTemplatePayload, BuiltinSeed,
    EXTENSION_PERMISSION_LOCAL_PROFILE_READ, EXTENSION_PERMISSION_PROCESS_EXECUTE,
    ExtensionAssetRef, ExtensionBundleKind, ExtensionBundleRef, ExtensionCommandDefinition,
    ExtensionCommandHandler, ExtensionDependencyDeclaration, ExtensionFlagDefinition,
    ExtensionFlagType, ExtensionMessageRendererDefinition, ExtensionPermissionAccess,
    ExtensionPermissionDecision, ExtensionPermissionDecisionReason, ExtensionPermissionDeclaration,
    ExtensionProcessPermissionAccess, ExtensionProtocolChannelDefinition,
    ExtensionProtocolChannelMethodDefinition, ExtensionRendererDeclaration,
    ExtensionRuntimeActionDefinition, ExtensionRuntimeActionKind, ExtensionTemplatePayload,
    ExtensionWorkspaceTabDefinition, ExtensionWorkspaceTabRendererDeclaration,
    InlineMountFilePayload, InstalledAssetSource, IntegrationLibraryAssetSeed, LibraryAssetPayload,
    LibraryAssetScope, LibraryAssetSource, LibraryAssetType, McpServerTemplatePayload,
    McpTransportTemplate, ProjectAgentConfigOverride, SharedLibrarySourceStatus,
    SkillTemplateFilePayload, SkillTemplatePayload, VfsMountTemplatePayload,
    WorkflowTemplatePayload, normalize_workflow_lifecycle_value,
    normalize_workflow_template_payload_value, normalize_workflow_template_value,
};

pub fn seed_digest(payload: &serde_json::Value) -> Result<String, crate::DomainError> {
    use sha2::{Digest, Sha256};

    let bytes = serde_json::to_vec(payload).map_err(crate::DomainError::Serialization)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}
