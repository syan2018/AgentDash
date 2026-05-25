use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

pub use agentdash_contracts::shared_library::{
    InstallLibraryAssetRequest, InstallLibraryAssetResponse, InstalledAssetSourceDto,
    LibraryAssetDto, ListLibraryAssetsQuery, ProjectAssetSourceStatusDto,
    ProjectAssetSourceStatusItemDto, PublishLibraryAssetRequest, SeedBuiltinLibraryAssetsRequest,
    SharedLibrarySourceStatus as ContractSharedLibrarySourceStatus,
};
use agentdash_domain::shared_library::{
    InstalledAssetSource, LibraryAsset, LibraryAssetScope, LibraryAssetSource, LibraryAssetType,
    SharedLibrarySourceStatus,
};

pub use agentdash_contracts::shared_library::{
    LibraryAssetScope as ContractLibraryAssetScope,
    LibraryAssetSource as ContractLibraryAssetSource, LibraryAssetType as ContractLibraryAssetType,
};

#[derive(Debug, Serialize)]
pub struct InstalledAssetSourceResponse {
    pub library_asset_id: Uuid,
    pub source_ref: String,
    pub source_version: String,
    pub source_digest: String,
    pub installed_at: DateTime<Utc>,
}

impl From<InstalledAssetSource> for InstalledAssetSourceResponse {
    fn from(source: InstalledAssetSource) -> Self {
        Self {
            library_asset_id: source.library_asset_id,
            source_ref: source.source_ref,
            source_version: source.source_version,
            source_digest: source.source_digest,
            installed_at: source.installed_at,
        }
    }
}

pub fn library_asset_response(asset: LibraryAsset) -> LibraryAssetDto {
    LibraryAssetDto {
        id: asset.id.to_string(),
        asset_type: contract_asset_type(asset.asset_type),
        scope: contract_asset_scope(asset.scope),
        owner_id: asset.owner_id,
        key: asset.key,
        display_name: asset.display_name,
        description: asset.description,
        version: asset.version,
        source: contract_asset_source(asset.source),
        source_ref: asset.source_ref,
        payload_digest: asset.payload_digest,
        deprecated: asset.deprecated,
        payload: asset.payload,
        created_at: asset.created_at.to_rfc3339(),
        updated_at: asset.updated_at.to_rfc3339(),
    }
}

pub fn installed_asset_source_response(source: InstalledAssetSource) -> InstalledAssetSourceDto {
    InstalledAssetSourceDto {
        library_asset_id: source.library_asset_id.to_string(),
        source_ref: source.source_ref,
        source_version: source.source_version,
        source_digest: source.source_digest,
        installed_at: source.installed_at.to_rfc3339(),
    }
}

pub fn source_status_item_response(
    asset_kind: &'static str,
    project_asset_id: uuid::Uuid,
    project_asset_key: String,
    installed_source: InstalledAssetSource,
    source_status: SharedLibrarySourceStatus,
    current_source_version: Option<String>,
    current_source_digest: Option<String>,
) -> ProjectAssetSourceStatusItemDto {
    ProjectAssetSourceStatusItemDto {
        asset_kind: asset_kind.to_string(),
        project_asset_id: project_asset_id.to_string(),
        project_asset_key,
        installed_source: installed_asset_source_response(installed_source),
        source_status: contract_source_status(source_status),
        current_source_version,
        current_source_digest,
    }
}

pub fn project_source_status_response(
    project_agents: Vec<ProjectAssetSourceStatusItemDto>,
    mcp_presets: Vec<ProjectAssetSourceStatusItemDto>,
    skill_assets: Vec<ProjectAssetSourceStatusItemDto>,
    vfs_mounts: Vec<ProjectAssetSourceStatusItemDto>,
    workflow_definitions: Vec<ProjectAssetSourceStatusItemDto>,
    activity_lifecycle_definitions: Vec<ProjectAssetSourceStatusItemDto>,
    extension_installations: Vec<ProjectAssetSourceStatusItemDto>,
) -> ProjectAssetSourceStatusDto {
    ProjectAssetSourceStatusDto {
        project_agents,
        mcp_presets,
        skill_assets,
        vfs_mounts,
        workflow_definitions,
        activity_lifecycle_definitions,
        extension_installations,
    }
}

pub fn parse_asset_type(raw: &str) -> Result<LibraryAssetType, String> {
    LibraryAssetType::parse(raw).map_err(|error| error.to_string())
}

pub fn parse_asset_scope(raw: &str) -> Result<LibraryAssetScope, String> {
    LibraryAssetScope::parse(raw).map_err(|error| error.to_string())
}

#[allow(dead_code)]
pub fn parse_asset_source(raw: &str) -> Result<LibraryAssetSource, String> {
    LibraryAssetSource::parse(raw).map_err(|error| error.to_string())
}

pub fn contract_asset_type(
    asset_type: LibraryAssetType,
) -> agentdash_contracts::shared_library::LibraryAssetType {
    use agentdash_contracts::shared_library::LibraryAssetType as Contract;
    match asset_type {
        LibraryAssetType::AgentTemplate => Contract::AgentTemplate,
        LibraryAssetType::McpServerTemplate => Contract::McpServerTemplate,
        LibraryAssetType::WorkflowTemplate => Contract::WorkflowTemplate,
        LibraryAssetType::SkillTemplate => Contract::SkillTemplate,
        LibraryAssetType::VfsMountTemplate => Contract::VfsMountTemplate,
        LibraryAssetType::ExtensionTemplate => Contract::ExtensionTemplate,
    }
}

pub fn contract_asset_scope(
    scope: LibraryAssetScope,
) -> agentdash_contracts::shared_library::LibraryAssetScope {
    use agentdash_contracts::shared_library::LibraryAssetScope as Contract;
    match scope {
        LibraryAssetScope::Builtin => Contract::Builtin,
        LibraryAssetScope::System => Contract::System,
        LibraryAssetScope::Org => Contract::Org,
        LibraryAssetScope::User => Contract::User,
    }
}

pub fn contract_asset_source(
    source: LibraryAssetSource,
) -> agentdash_contracts::shared_library::LibraryAssetSource {
    use agentdash_contracts::shared_library::LibraryAssetSource as Contract;
    match source {
        LibraryAssetSource::Builtin => Contract::Builtin,
        LibraryAssetSource::UserAuthored => Contract::UserAuthored,
        LibraryAssetSource::RemoteImported => Contract::RemoteImported,
        LibraryAssetSource::PluginEmbedded => Contract::PluginEmbedded,
    }
}

pub fn contract_source_status(
    status: SharedLibrarySourceStatus,
) -> ContractSharedLibrarySourceStatus {
    match status {
        SharedLibrarySourceStatus::UpToDate => ContractSharedLibrarySourceStatus::UpToDate,
        SharedLibrarySourceStatus::UpdateAvailable => {
            ContractSharedLibrarySourceStatus::UpdateAvailable
        }
        SharedLibrarySourceStatus::SourceMissing => {
            ContractSharedLibrarySourceStatus::SourceMissing
        }
    }
}
