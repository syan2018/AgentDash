use async_trait::async_trait;
use std::collections::BTreeMap;

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
pub struct ExtensionProtocolConsumerPayload {
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
pub struct ExtensionProtocolInvokeRequest {
    pub provider_extension_key: String,
    pub provider_extension_id: String,
    pub protocol_key: String,
    pub protocol_version: String,
    pub method: String,
    pub project_id: String,
    pub session_id: String,
    pub input: Value,
    pub package_artifact: ExtensionPackageArtifactPayload,
    pub consumer: ExtensionProtocolConsumerPayload,
    pub workspace: Option<ExtensionInvocationWorkspacePayload>,
    pub trace_id: String,
    pub invocation_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionProtocolInvokeResponse {
    pub provider_extension_key: String,
    pub provider_extension_id: String,
    pub protocol_key: String,
    pub protocol_version: String,
    pub method: String,
    pub output: Value,
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionBackendServiceInvokeMetadataPayload {
    pub project_id: String,
    pub backend_id: String,
    pub extension_key: String,
    pub extension_id: String,
    pub service_key: String,
    pub route: String,
    pub trace_id: String,
    pub invocation_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionBackendServiceInvokeRequest {
    pub extension_key: String,
    pub extension_id: String,
    pub service_key: String,
    pub route: String,
    pub project_id: String,
    pub session_id: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub package_artifact: ExtensionPackageArtifactPayload,
    pub workspace: Option<ExtensionInvocationWorkspacePayload>,
    pub trace_id: String,
    pub invocation_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionBackendServiceHttpResponsePayload {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionBackendServiceReadinessPayload {
    Ready,
    MissingArtifact,
    MaterializeFailed,
    Starting,
    HealthFailed,
    ProcessExited,
    UnsupportedRuntime,
    ServiceUnavailable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionBackendServiceInvokeDiagnosticPayload {
    pub readiness: ExtensionBackendServiceReadinessPayload,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionBackendServiceInvokeResponse {
    pub metadata: ExtensionBackendServiceInvokeMetadataPayload,
    pub response: Option<ExtensionBackendServiceHttpResponsePayload>,
    pub diagnostic: Option<ExtensionBackendServiceInvokeDiagnosticPayload>,
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
pub trait ExtensionRuntimeProtocolTransport: Send + Sync {
    async fn invoke_extension_protocol(
        &self,
        backend_id: &str,
        request: ExtensionProtocolInvokeRequest,
    ) -> Result<ExtensionProtocolInvokeResponse, ExtensionRuntimeActionTransportError>;
}

#[async_trait]
pub trait ExtensionBackendServiceTransport: Send + Sync {
    async fn invoke_extension_backend_service(
        &self,
        backend_id: &str,
        request: ExtensionBackendServiceInvokeRequest,
    ) -> Result<ExtensionBackendServiceInvokeResponse, ExtensionRuntimeActionTransportError>;
}
