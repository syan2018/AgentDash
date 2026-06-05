use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::shared_library::{LibraryAssetDto, LibraryAssetType};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceSourceProviderKindDto {
    Integration,
    Builtin,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceSourceTrustLevelDto {
    Curated,
    Organization,
    PublicIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct MarketplaceSourceDto {
    pub source_key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub provider_kind: MarketplaceSourceProviderKindDto,
    pub supported_asset_types: Vec<LibraryAssetType>,
    pub trust_level: MarketplaceSourceTrustLevelDto,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
pub struct ListExternalMarketplaceAssetsQuery {
    #[serde(default)]
    #[ts(optional)]
    pub source_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub asset_type: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub query: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub cursor: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceInstallRequirementKindDto {
    EnvVar,
    Secret,
    Permission,
    AssetDependency,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExternalMarketplaceInstallRequirementDto {
    pub kind: MarketplaceInstallRequirementKindDto,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExternalMarketplaceAssetListingDto {
    pub source_key: String,
    pub external_id: String,
    pub asset_type: LibraryAssetType,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub version: String,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub updated_at: Option<String>,
    pub install_requirements: Vec<ExternalMarketplaceInstallRequirementDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExternalMarketplaceAssetPageDto {
    pub items: Vec<ExternalMarketplaceAssetListingDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExternalMarketplaceAssetDetailDto {
    pub listing: ExternalMarketplaceAssetListingDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub homepage_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repository_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct ImportExternalMarketplaceAssetRequest {
    pub source_key: String,
    pub external_id: String,
    pub asset_type: String,
    pub import_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ImportExternalMarketplaceAssetResponse {
    pub asset: LibraryAssetDto,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct RefreshExternalMarketplaceAssetRequest {
    pub source_key: String,
    pub external_id: String,
    pub asset_type: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalMarketplaceRefreshStatus {
    UpToDate,
    UpdateAvailable,
    SourceMissing,
    NotImported,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RefreshExternalMarketplaceAssetResponse {
    pub source_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remote_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remote_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub local_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub local_digest: Option<String>,
    pub status: ExternalMarketplaceRefreshStatus,
}
