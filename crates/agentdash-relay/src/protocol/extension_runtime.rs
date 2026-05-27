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
