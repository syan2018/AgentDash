use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectExtensionPackageModeResponse {
    Packaged,
    DeclarationOnly,
    InvalidMissingArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectExtensionInstalledSourceResponse {
    pub library_asset_id: String,
    pub source_ref: String,
    pub source_version: String,
    pub source_digest: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectExtensionPackageArtifactRefResponse {
    pub artifact_id: String,
    pub package_name: String,
    pub package_version: String,
    pub asset_version: String,
    pub source_version: String,
    pub storage_ref: String,
    pub archive_digest: String,
    pub manifest_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectExtensionCapabilitySummaryResponse {
    pub commands: usize,
    pub flags: usize,
    pub message_renderers: usize,
    pub runtime_actions: usize,
    pub protocol_channels: usize,
    pub workspace_tabs: usize,
    pub permissions: usize,
    pub bundles: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectExtensionManagementItemResponse {
    pub installation_id: String,
    pub extension_key: String,
    pub extension_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub installed_source: Option<ProjectExtensionInstalledSourceResponse>,
    pub source_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_source_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_source_digest: Option<String>,
    pub package_mode: ProjectExtensionPackageModeResponse,
    pub package_artifact: Option<ProjectExtensionPackageArtifactRefResponse>,
    pub capability_summary: ProjectExtensionCapabilitySummaryResponse,
    pub manifest: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectExtensionManagementListResponse {
    pub extensions: Vec<ProjectExtensionManagementItemResponse>,
}
