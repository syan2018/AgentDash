use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionPackageArtifactRelay {
    pub artifact_id: String,
    pub archive_digest: String,
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
