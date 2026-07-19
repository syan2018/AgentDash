//! Marketplace Source SPI.
//!
//! Host integrations implement this port to expose external Skill / MCP
//! marketplace catalogs without binding the contract layer to HTTP clients,
//! databases, or concrete import services.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use agentdash_domain::shared_library::LibraryAssetType;

/// How the marketplace source is provided to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceSourceProviderKind {
    Integration,
    Builtin,
}

/// Trust posture surfaced by the provider descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceSourceTrustLevel {
    Curated,
    Organization,
    PublicIndex,
}

/// Static metadata for a marketplace source provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceSourceDescriptor {
    pub source_key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub provider_kind: MarketplaceSourceProviderKind,
    #[serde(default)]
    pub supported_asset_types: Vec<LibraryAssetType>,
    pub trust_level: MarketplaceSourceTrustLevel,
    pub enabled: bool,
}

/// Query parameters for listing external marketplace assets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceAssetQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_type: Option<LibraryAssetType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// One page of external marketplace assets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MarketplaceAssetPage {
    pub items: Vec<MarketplaceAssetListing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Install-time requirement advertised by a marketplace listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceInstallRequirement {
    pub kind: MarketplaceInstallRequirementKind,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceInstallRequirementKind {
    EnvVar,
    Secret,
    Permission,
    AssetDependency,
}

/// Lightweight listing entry returned by a source provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceAssetListing {
    pub source_key: String,
    pub external_id: String,
    pub asset_type: LibraryAssetType,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub install_requirements: Vec<MarketplaceInstallRequirement>,
}

/// Detail view for one external marketplace asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketplaceAssetDetail {
    pub listing: MarketplaceAssetListing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
}

/// Common fetched asset fields shared by the first supported asset families.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketplaceFetchedAssetPayload {
    pub source_key: String,
    pub external_id: String,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub payload: Value,
}

/// Fetched external payload ready for later import into Shared Library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "asset_type", content = "asset", rename_all = "snake_case")]
pub enum MarketplaceFetchedAsset {
    SkillTemplate(MarketplaceFetchedAssetPayload),
    McpServerTemplate(MarketplaceFetchedAssetPayload),
}

impl MarketplaceFetchedAsset {
    pub fn asset_type(&self) -> LibraryAssetType {
        match self {
            Self::SkillTemplate(_) => LibraryAssetType::SkillTemplate,
            Self::McpServerTemplate(_) => LibraryAssetType::McpServerTemplate,
        }
    }

    pub fn payload(&self) -> &MarketplaceFetchedAssetPayload {
        match self {
            Self::SkillTemplate(payload) | Self::McpServerTemplate(payload) => payload,
        }
    }
}

/// Error surfaced by a marketplace source provider.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum MarketplaceSourceError {
    #[error("marketplace source request is invalid: {0}")]
    BadRequest(String),
    #[error("marketplace asset `{external_id}` was not found in source `{source_key}`")]
    NotFound {
        source_key: String,
        external_id: String,
    },
    #[error("marketplace source is unavailable: {0}")]
    Unavailable(String),
    #[error("marketplace source failed internally: {0}")]
    Internal(String),
}

/// Provides external marketplace assets for later Shared Library import.
#[async_trait]
pub trait MarketplaceSourceProvider: Send + Sync {
    fn descriptor(&self) -> MarketplaceSourceDescriptor;

    async fn list_assets(
        &self,
        query: MarketplaceAssetQuery,
    ) -> Result<MarketplaceAssetPage, MarketplaceSourceError>;

    async fn get_asset_detail(
        &self,
        external_id: &str,
    ) -> Result<MarketplaceAssetDetail, MarketplaceSourceError>;

    async fn fetch_asset_payload(
        &self,
        external_id: &str,
    ) -> Result<MarketplaceFetchedAsset, MarketplaceSourceError>;
}
