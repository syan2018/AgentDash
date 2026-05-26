use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionRuntimeActionKindResponse {
    SessionRuntime,
    Setup,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionFlagTypeResponse {
    Bool,
    String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionPermissionAccessResponse {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionBundleKindResponse {
    ExtensionHost,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionCommandHandlerResponse {
    InjectMessage { content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionMessageRendererDeclarationResponse {
    JsonCard,
    Markdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionWorkspaceTabRendererResponse {
    Webview { entry: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionPermissionDeclarationResponse {
    LocalProfile {
        access: ExtensionPermissionAccessResponse,
    },
    Workspace {
        access: ExtensionPermissionAccessResponse,
    },
    RuntimeAction {
        action_key: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionInstalledAssetSourceResponse {
    pub library_asset_id: String,
    pub source_ref: String,
    pub source_version: String,
    pub source_digest: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionPackageArtifactRefResponse {
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
pub struct ExtensionInstallationProjectionResponse {
    pub installation_id: String,
    pub extension_key: String,
    pub extension_id: String,
    pub display_name: String,
    pub installed_source: Option<ExtensionInstalledAssetSourceResponse>,
    pub package_artifact: Option<ExtensionPackageArtifactRefResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionCommandProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub name: String,
    pub description: String,
    pub handler: ExtensionCommandHandlerResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionFlagProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub name: String,
    pub flag_type: ExtensionFlagTypeResponse,
    pub default: Value,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionMessageRendererProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub custom_type: String,
    pub renderer: ExtensionMessageRendererDeclarationResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionRuntimeActionProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub action_key: String,
    pub kind: ExtensionRuntimeActionKindResponse,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionWorkspaceTabProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub type_id: String,
    pub label: String,
    pub uri_scheme: String,
    pub renderer: ExtensionWorkspaceTabRendererResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionPermissionProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub permission: ExtensionPermissionDeclarationResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionBundleProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub kind: ExtensionBundleKindResponse,
    pub entry: String,
    pub digest: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct ExtensionRuntimeProjectionResponse {
    pub installations: Vec<ExtensionInstallationProjectionResponse>,
    pub commands: Vec<ExtensionCommandProjectionResponse>,
    pub flags: Vec<ExtensionFlagProjectionResponse>,
    pub message_renderers: Vec<ExtensionMessageRendererProjectionResponse>,
    pub runtime_actions: Vec<ExtensionRuntimeActionProjectionResponse>,
    pub workspace_tabs: Vec<ExtensionWorkspaceTabProjectionResponse>,
    pub permissions: Vec<ExtensionPermissionProjectionResponse>,
    pub bundles: Vec<ExtensionBundleProjectionResponse>,
}
