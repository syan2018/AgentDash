use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionPackageArtifactRelay {
    pub artifact_id: String,
    pub archive_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionRuntimeHostRelay {
    pub extension_key: String,
    pub extension_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_artifact: Option<ExtensionPackageArtifactRelay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionChannelConsumerRelay {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency_alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionInvocationWorkspaceRelay {
    pub mount_id: String,
    pub root_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandExtensionActionInvokePayload {
    pub extension_key: String,
    pub extension_id: String,
    pub action_key: String,
    pub project_id: String,
    pub session_id: String,
    #[serde(default)]
    pub input: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_artifact: Option<ExtensionPackageArtifactRelay>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_extensions: Vec<ExtensionRuntimeHostRelay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<ExtensionInvocationWorkspaceRelay>,
    pub trace_id: String,
    pub invocation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseExtensionActionInvokePayload {
    pub extension_key: String,
    pub extension_id: String,
    pub action_key: String,
    #[serde(default)]
    pub output: Value,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandExtensionChannelInvokePayload {
    pub provider_extension_key: String,
    pub provider_extension_id: String,
    pub channel_key: String,
    pub method: String,
    pub project_id: String,
    pub session_id: String,
    #[serde(default)]
    pub input: Value,
    pub package_artifact: ExtensionPackageArtifactRelay,
    pub consumer: ExtensionChannelConsumerRelay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<ExtensionInvocationWorkspaceRelay>,
    pub trace_id: String,
    pub invocation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseExtensionChannelInvokePayload {
    pub provider_extension_key: String,
    pub provider_extension_id: String,
    pub channel_key: String,
    pub method: String,
    #[serde(default)]
    pub output: Value,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionOperationDispatchRelay {
    RuntimeAction { action_key: String },
    ProtocolChannel { channel_key: String, method: String },
    BackendService { service_key: String, route: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionBackendServiceInvokeMetadataRelay {
    pub project_id: String,
    pub backend_id: String,
    pub extension_key: String,
    pub extension_id: String,
    pub service_key: String,
    pub route: String,
    pub trace_id: String,
    pub invocation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandExtensionBackendServiceInvokePayload {
    pub metadata: ExtensionBackendServiceInvokeMetadataRelay,
    pub session_id: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Vec<u8>>,
    pub package_artifact: ExtensionPackageArtifactRelay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<ExtensionInvocationWorkspaceRelay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionBackendServiceHttpResponseRelay {
    pub status: u16,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionBackendServiceReadinessRelay {
    Ready,
    MissingArtifact,
    MaterializeFailed,
    Starting,
    HealthFailed,
    ProcessExited,
    UnsupportedRuntime,
    ServiceUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionBackendServiceInvokeDiagnosticRelay {
    pub readiness: ExtensionBackendServiceReadinessRelay,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseExtensionBackendServiceInvokePayload {
    pub metadata: ExtensionBackendServiceInvokeMetadataRelay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<ExtensionBackendServiceHttpResponseRelay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<ExtensionBackendServiceInvokeDiagnosticRelay>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_service_dispatch_has_a_protocol_shape() {
        let dispatch = ExtensionOperationDispatchRelay::BackendService {
            service_key: "profile-service".to_string(),
            route: "/profiles/search".to_string(),
        };

        let encoded = serde_json::to_value(&dispatch).expect("serialize dispatch");

        assert_eq!(encoded["kind"], "backend_service");
        assert_eq!(encoded["service_key"], "profile-service");
        assert_eq!(encoded["route"], "/profiles/search");
    }
}
