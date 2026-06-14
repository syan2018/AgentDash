use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasImportMapDto {
    #[serde(default)]
    pub imports: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeFileDto {
    pub path: String,
    pub content: String,
    pub file_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeBindingDto {
    pub alias: String,
    pub source_uri: String,
    pub data_path: String,
    pub content_type: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeBridgeSnapshotDto {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub surface: Option<RuntimeSurfaceDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeSnapshotDto {
    pub canvas_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resource_surface_ref: Option<String>,
    pub entry: String,
    pub files: Vec<CanvasRuntimeFileDto>,
    pub bindings: Vec<CanvasRuntimeBindingDto>,
    pub import_map: CanvasImportMapDto,
    pub libraries: Vec<String>,
    pub runtime_bridge: CanvasRuntimeBridgeSnapshotDto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActionKindDto {
    SessionRuntime,
    Setup,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimePolicyDto {
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timeout_ms: Option<i64>,
    #[serde(default)]
    pub allow_background: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeActionDescriptorDto {
    pub action_key: String,
    pub kind: RuntimeActionKindDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub output_schema: Option<Value>,
    pub default_policy: RuntimePolicyDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeContextDto {
    Session {
        session_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        project_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        workspace_id: Option<String>,
    },
    Setup {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        project_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        workspace_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        backend_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        root_ref: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSurfaceDto {
    pub context: RuntimeContextDto,
    pub actions: Vec<RuntimeActionDescriptorDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTraceDto {
    pub trace_id: String,
    pub invocation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent_trace_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeInvocationOutputDto {
    #[ts(type = "JsonValue")]
    pub output: Value,
    #[serde(default)]
    #[ts(type = "{ [key in string]?: JsonValue }")]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeInvocationResultDto {
    pub action_key: String,
    pub trace: RuntimeTraceDto,
    pub output: RuntimeInvocationOutputDto,
}
