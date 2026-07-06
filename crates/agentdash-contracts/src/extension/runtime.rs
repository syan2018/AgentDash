use std::collections::BTreeMap;

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
pub enum ExtensionProcessPermissionAccessResponse {
    Execute,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionBundleKindResponse {
    ExtensionHost,
    BackendService,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionGeneratedOperationVisibilityResponse {
    PanelOnly,
    AgentAndPanel,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionGeneratedOperationDispatchResponse {
    RuntimeAction { action_key: String },
    ProtocolChannel { channel_key: String, method: String },
    BackendService { service_key: String, route: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionGeneratedOperationProvenanceResponse {
    pub capability_key: String,
    pub exposure_key: String,
    pub generated_from: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionGeneratedOperationProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub operation_key: String,
    pub description: String,
    pub visibility: ExtensionGeneratedOperationVisibilityResponse,
    pub input_schema: Value,
    pub output_schema: Value,
    pub permission_summary: Vec<String>,
    pub dispatch: ExtensionGeneratedOperationDispatchResponse,
    pub provenance: ExtensionGeneratedOperationProvenanceResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionFetchRouteTargetResponse {
    HttpProxy { capability_key: String },
    RuntimeAction { action_key: String },
    ProtocolChannel { channel_key: String, method: String },
    BackendService { service_key: String, route: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionFetchRouteProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub route_key: String,
    pub pattern: String,
    /// Fetch routes are panel bridge compatibility routes. Agent exposure is represented only by operation_catalog.
    pub panel_only: bool,
    pub target: ExtensionFetchRouteTargetResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionBackendServiceProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub service_key: String,
    pub runtime: String,
    pub entry: String,
    pub routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_path: Option<String>,
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
    CanvasPanel { entry: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionWorkspaceTabLoadabilityModeResponse {
    ExtensionHost,
    UiOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionWorkspaceTabLoadabilityResponse {
    pub available: bool,
    pub mode: ExtensionWorkspaceTabLoadabilityModeResponse,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionPermissionDeclarationResponse {
    LocalProfile {
        access: ExtensionPermissionAccessResponse,
    },
    Http {
        hosts: Vec<String>,
        access: ExtensionPermissionAccessResponse,
    },
    Workspace {
        access: ExtensionPermissionAccessResponse,
    },
    Env {
        names: Vec<String>,
        access: ExtensionPermissionAccessResponse,
    },
    Process {
        access: ExtensionProcessPermissionAccessResponse,
    },
    RuntimeAction {
        action_key: String,
    },
    ExtensionChannel {
        channel_key: String,
        methods: Vec<String>,
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
pub struct ExtensionProtocolChannelMethodProjectionResponse {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionProtocolChannelProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub channel_key: String,
    pub version: String,
    pub description: String,
    pub methods: Vec<ExtensionProtocolChannelMethodProjectionResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionDependencyDeclarationResponse {
    pub alias: String,
    pub extension_id: String,
    pub version: String,
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionDependencyProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub dependency: ExtensionDependencyDeclarationResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionWorkspaceTabProjectionResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub type_id: String,
    pub label: String,
    pub uri_scheme: String,
    pub renderer: ExtensionWorkspaceTabRendererResponse,
    pub loadability: ExtensionWorkspaceTabLoadabilityResponse,
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
    pub protocol_channels: Vec<ExtensionProtocolChannelProjectionResponse>,
    pub extension_dependencies: Vec<ExtensionDependencyProjectionResponse>,
    pub workspace_tabs: Vec<ExtensionWorkspaceTabProjectionResponse>,
    pub permissions: Vec<ExtensionPermissionProjectionResponse>,
    pub bundles: Vec<ExtensionBundleProjectionResponse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fetch_routes: Vec<ExtensionFetchRouteProjectionResponse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operation_catalog: Vec<ExtensionGeneratedOperationProjectionResponse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backend_services: Vec<ExtensionBackendServiceProjectionResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionRuntimeInvokeActionRequest {
    pub action_key: String,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionRuntimeInvokeChannelRequest {
    pub channel_key: String,
    pub method: String,
    #[serde(default)]
    pub input: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer_extension_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency_alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionRuntimeTraceResponse {
    pub trace_id: String,
    pub invocation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_trace_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionRuntimeInvocationOutputResponse {
    #[serde(default)]
    pub output: Value,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionRuntimeInvokeActionResponse {
    pub action_key: String,
    pub trace: ExtensionRuntimeTraceResponse,
    pub output: ExtensionRuntimeInvocationOutputResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionRuntimeInvokeChannelResponse {
    pub channel_key: String,
    pub method: String,
    pub trace: ExtensionRuntimeTraceResponse,
    pub output: ExtensionRuntimeInvocationOutputResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UninstallExtensionInstallationResponse {
    pub installation_id: String,
    pub extension_key: String,
}
