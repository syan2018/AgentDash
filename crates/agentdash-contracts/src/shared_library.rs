use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use serde_json::Value;
use ts_rs::TS;

use crate::mcp_preset::McpRoutePolicy;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryAssetType {
    AgentTemplate,
    McpServerTemplate,
    WorkflowTemplate,
    SkillTemplate,
    VfsMountTemplate,
    ExtensionTemplate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryAssetScope {
    Builtin,
    System,
    Org,
    User,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryAssetSource {
    Builtin,
    UserAuthored,
    RemoteImported,
    IntegrationEmbedded,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SharedLibrarySourceStatus {
    UpToDate,
    UpdateAvailable,
    SourceMissing,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct InstalledAssetSourceDto {
    pub library_asset_id: String,
    pub source_ref: String,
    pub source_version: String,
    pub source_digest: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LibraryExtensionPackageArtifactDto {
    pub id: String,
    pub package_name: String,
    pub package_version: String,
    pub asset_version: String,
    pub source_version: String,
    pub archive_digest: String,
    pub manifest_digest: String,
    pub byte_size: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LibraryAssetDto {
    pub id: String,
    pub asset_type: LibraryAssetType,
    pub scope: LibraryAssetScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub owner_id: Option<String>,
    pub key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub version: String,
    pub source: LibraryAssetSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_ref: Option<String>,
    pub payload_digest: String,
    pub deprecated: bool,
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub extension_package_artifact: Option<LibraryExtensionPackageArtifactDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, TS, Default)]
pub struct ListLibraryAssetsQuery {
    #[serde(default)]
    #[ts(optional)]
    pub asset_type: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub scope: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub include_deprecated: bool,
}

#[derive(Debug, Clone, Deserialize, TS, Default)]
pub struct SeedBuiltinLibraryAssetsRequest {
    #[serde(default)]
    #[ts(optional)]
    pub asset_type: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct InstallLibraryAssetRequest {
    pub library_asset_id: String,
    #[serde(default)]
    #[ts(optional)]
    pub target_key: Option<String>,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    #[ts(optional)]
    pub install_options: Option<InstallLibraryAssetOptions>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "asset_type", rename_all = "snake_case")]
pub enum InstallLibraryAssetOptions {
    McpServerTemplate { parameters: Value },
    AgentTemplate {
        #[serde(default)]
        dependency_mode: AgentTemplateDependencyMode,
        #[serde(default)]
        dependency_parameters: BTreeMap<String, Value>,
        #[serde(default)]
        overwrite_dependencies: bool,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTemplateDependencyMode {
    #[default]
    Required,
    All,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportTemplateDto {
    Http { url_template: String },
    Sse { url_template: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct McpServerTemplatePayloadDto {
    pub transport_template: McpTransportTemplateDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route_policy: Option<McpRoutePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parameter_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublishLibraryAssetKind {
    ProjectAgent,
    McpPreset,
    WorkflowBundle,
    SkillAsset,
    VfsMount,
    ExtensionInstallation,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct PublishLibraryAssetRequest {
    pub asset_kind: String,
    pub project_asset_id: String,
    #[serde(default = "default_user_scope")]
    pub scope: String,
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    pub version: String,
    #[serde(default)]
    pub overwrite: bool,
}

fn default_user_scope() -> String {
    "user".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "asset_kind", rename_all = "snake_case")]
pub enum InstallLibraryAssetResponse {
    ProjectAgent {
        project_agent_id: String,
    },
    McpPreset {
        id: String,
    },
    WorkflowTemplate {
        workflow_ids: Vec<String>,
        lifecycle_id: String,
    },
    SkillAsset {
        id: String,
    },
    VfsMount {
        id: String,
        mount_id: String,
    },
    ExtensionInstallation {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectAssetSourceStatusItemDto {
    pub asset_kind: String,
    pub project_asset_id: String,
    pub project_asset_key: String,
    pub installed_source: InstalledAssetSourceDto,
    pub source_status: SharedLibrarySourceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_source_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_source_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ProjectAssetSourceStatusDto {
    pub project_agents: Vec<ProjectAssetSourceStatusItemDto>,
    pub mcp_presets: Vec<ProjectAssetSourceStatusItemDto>,
    pub skill_assets: Vec<ProjectAssetSourceStatusItemDto>,
    pub vfs_mounts: Vec<ProjectAssetSourceStatusItemDto>,
    pub agent_procedures: Vec<ProjectAssetSourceStatusItemDto>,
    pub workflow_graphs: Vec<ProjectAssetSourceStatusItemDto>,
    pub extension_installations: Vec<ProjectAssetSourceStatusItemDto>,
}
