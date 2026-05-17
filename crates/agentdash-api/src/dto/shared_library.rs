use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::shared_library::SharedLibrarySourceStatus;
use agentdash_domain::shared_library::{
    InstalledAssetSource, LibraryAsset, LibraryAssetScope, LibraryAssetSource, LibraryAssetType,
};

#[derive(Debug, Serialize)]
pub struct LibraryAssetResponse {
    pub id: Uuid,
    pub asset_type: &'static str,
    pub scope: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub key: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    pub payload_digest: String,
    pub deprecated: bool,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<LibraryAsset> for LibraryAssetResponse {
    fn from(asset: LibraryAsset) -> Self {
        Self {
            id: asset.id,
            asset_type: asset.asset_type.as_str(),
            scope: asset.scope.as_str(),
            owner_id: asset.owner_id,
            key: asset.key,
            display_name: asset.display_name,
            description: asset.description,
            version: asset.version,
            source: asset.source.as_str(),
            source_ref: asset.source_ref,
            payload_digest: asset.payload_digest,
            deprecated: asset.deprecated,
            payload: asset.payload,
            created_at: asset.created_at,
            updated_at: asset.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ListLibraryAssetsQuery {
    #[serde(default)]
    pub asset_type: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub include_deprecated: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct SeedBuiltinLibraryAssetsRequest {
    #[serde(default)]
    pub asset_type: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InstallLibraryAssetRequest {
    pub library_asset_id: Uuid,
    #[serde(default)]
    pub target_key: Option<String>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "asset_kind", rename_all = "snake_case")]
pub enum InstallLibraryAssetResponse {
    ProjectAgent {
        agent_id: Uuid,
        project_agent_link_id: Uuid,
    },
    McpPreset {
        id: Uuid,
    },
    WorkflowTemplate {
        workflow_ids: Vec<Uuid>,
        lifecycle_id: Uuid,
    },
    SkillAsset {
        id: Uuid,
    },
}

#[derive(Debug, Serialize)]
pub struct ProjectAssetSourceStatusResponse {
    pub mcp_presets: Vec<ProjectAssetSourceStatusItemResponse>,
    pub skill_assets: Vec<ProjectAssetSourceStatusItemResponse>,
}

#[derive(Debug, Serialize)]
pub struct ProjectAssetSourceStatusItemResponse {
    pub asset_kind: &'static str,
    pub project_asset_id: Uuid,
    pub project_asset_key: String,
    pub installed_source: InstalledAssetSourceResponse,
    pub source_status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_source_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_source_digest: Option<String>,
}

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

pub fn source_status_tag(status: SharedLibrarySourceStatus) -> &'static str {
    status.as_str()
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
