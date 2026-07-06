use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, TS)]
pub struct DesktopUpdateCheckQuery {
    #[serde(default)]
    #[ts(optional)]
    pub platform: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub arch: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub current_version: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopUpdateStatus {
    UpdateAvailable,
    UpToDate,
    LatestAvailable,
    UnsupportedTarget,
    Unconfigured,
    FetchFailed,
    InvalidManifest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopUpdateRecommendedVersionSource {
    Manifest,
    Environment,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DesktopUpdateArtifact {
    pub url: String,
    pub sha256: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DesktopManualInstallerArtifact {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DesktopUpdateRelease {
    pub product: String,
    pub channel: String,
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub published_at: String,
    pub release_notes: String,
    pub updater: DesktopUpdateArtifact,
    pub manual_installer: DesktopManualInstallerArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DesktopUpdatePolicy {
    pub min_desktop_version: Option<String>,
    pub recommended_desktop_version: Option<String>,
    pub min_desktop_version_configured: bool,
    pub recommended_desktop_version_source: DesktopUpdateRecommendedVersionSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DesktopUpdateDiagnostics {
    pub manifest_url_configured: bool,
    pub code: Option<String>,
    pub message: Option<String>,
    pub fetched_at: Option<String>,
    pub cache_hit: bool,
    #[ts(type = "number")]
    pub cache_ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct DesktopUpdateCheckResponse {
    pub status: DesktopUpdateStatus,
    pub channel: String,
    pub platform: String,
    pub arch: String,
    pub current_version: Option<String>,
    pub latest: Option<DesktopUpdateRelease>,
    pub update_available: Option<bool>,
    pub policy: DesktopUpdatePolicy,
    pub diagnostics: DesktopUpdateDiagnostics,
}
