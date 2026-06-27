use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionPackageArtifactResponse {
    pub id: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub extension_id: String,
    pub package_name: String,
    pub package_version: String,
    pub asset_version: String,
    pub source_version: String,
    pub storage_ref: String,
    pub archive_digest: String,
    pub manifest_digest: String,
    pub manifest: Value,
    pub byte_size: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InstallExtensionPackageArtifactRequest {
    pub extension_key: Option<String>,
    pub display_name: Option<String>,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionPackageInstallationResponse {
    pub installation_id: String,
    pub extension_key: String,
    pub extension_id: String,
    pub package_artifact_id: String,
    pub archive_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ImportExtensionPackageResponse {
    pub artifact: ExtensionPackageArtifactResponse,
    pub installation: ExtensionPackageInstallationResponse,
}
