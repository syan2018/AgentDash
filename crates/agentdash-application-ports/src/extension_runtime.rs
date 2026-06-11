use async_trait::async_trait;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionPackageArtifactPayload {
    pub artifact_id: String,
    pub archive_digest: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionRuntimeHostPayload {
    pub extension_key: String,
    pub extension_id: String,
    pub package_artifact: Option<ExtensionPackageArtifactPayload>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionChannelConsumerPayload {
    pub kind: String,
    pub extension_key: Option<String>,
    pub extension_id: Option<String>,
    pub dependency_alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionInvocationWorkspacePayload {
    pub mount_id: String,
    pub root_ref: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionActionInvokeRequest {
    pub extension_key: String,
    pub extension_id: String,
    pub action_key: String,
    pub project_id: String,
    pub session_id: String,
    pub input: Value,
    pub package_artifact: Option<ExtensionPackageArtifactPayload>,
    pub runtime_extensions: Vec<ExtensionRuntimeHostPayload>,
    pub workspace: Option<ExtensionInvocationWorkspacePayload>,
    pub trace_id: String,
    pub invocation_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionActionInvokeResponse {
    pub extension_key: String,
    pub extension_id: String,
    pub action_key: String,
    pub output: Value,
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionChannelInvokeRequest {
    pub provider_extension_key: String,
    pub provider_extension_id: String,
    pub channel_key: String,
    pub method: String,
    pub project_id: String,
    pub session_id: String,
    pub input: Value,
    pub package_artifact: ExtensionPackageArtifactPayload,
    pub consumer: ExtensionChannelConsumerPayload,
    pub workspace: Option<ExtensionInvocationWorkspacePayload>,
    pub trace_id: String,
    pub invocation_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionChannelInvokeResponse {
    pub provider_extension_key: String,
    pub provider_extension_id: String,
    pub channel_key: String,
    pub method: String,
    pub output: Value,
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ExtensionRuntimeActionTransportError {
    #[error("backend offline: {backend_id}")]
    Offline { backend_id: String },
    #[error("backend command timeout: {backend_id}")]
    Timeout { backend_id: String },
    #[error("backend response dropped: {backend_id}")]
    ResponseDropped { backend_id: String },
    #[error("extension action relay failed: {0}")]
    Failed(String),
}

#[async_trait]
pub trait ExtensionRuntimeActionTransport: Send + Sync {
    async fn invoke_extension_action(
        &self,
        backend_id: &str,
        request: ExtensionActionInvokeRequest,
    ) -> Result<ExtensionActionInvokeResponse, ExtensionRuntimeActionTransportError>;
}

#[async_trait]
pub trait ExtensionRuntimeChannelTransport: Send + Sync {
    async fn invoke_extension_channel(
        &self,
        backend_id: &str,
        request: ExtensionChannelInvokeRequest,
    ) -> Result<ExtensionChannelInvokeResponse, ExtensionRuntimeActionTransportError>;
}
