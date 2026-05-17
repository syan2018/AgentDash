use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::shared_library::{
    LibraryAsset, LibraryAssetScope, LibraryAssetSource, LibraryAssetType,
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
