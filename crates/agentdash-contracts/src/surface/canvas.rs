use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

use agentdash_agent_protocol::codex_app_server_protocol as codex;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasFileDto {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasImportMapDto {
    #[serde(default)]
    pub imports: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasSandboxConfigDto {
    #[serde(default)]
    pub libraries: Vec<String>,
    #[serde(default)]
    pub import_map: CanvasImportMapDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasDataBindingDto {
    pub alias: String,
    pub source_uri: String,
    #[serde(default)]
    pub content_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CanvasScopeDto {
    Personal,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CanvasListScopeDto {
    All,
    Mine,
    Shared,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasAccessDto {
    pub can_view: bool,
    pub can_edit_source: bool,
    pub can_publish: bool,
    pub can_manage_shared: bool,
    pub can_copy: bool,
    pub runtime_write_allowed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct ListCanvasesQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope: Option<CanvasListScopeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasResponse {
    pub canvas_id: String,
    pub project_id: String,
    pub owner_user_id: Option<String>,
    pub scope: CanvasScopeDto,
    pub access: CanvasAccessDto,
    pub canvas_mount_id: String,
    pub vfs_mount_id: String,
    pub title: String,
    pub description: String,
    pub entry_file: String,
    pub sandbox_config: CanvasSandboxConfigDto,
    pub files: Vec<CanvasFileDto>,
    pub bindings: Vec<CanvasDataBindingDto>,
    pub published_from_canvas_id: Option<String>,
    pub shared_canvas_id: Option<String>,
    pub cloned_from_canvas_id: Option<String>,
    pub published_at: Option<String>,
    pub published_by_user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CreateCanvasRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub canvas_mount_id: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entry_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sandbox_config: Option<CanvasSandboxConfigDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub files: Option<Vec<CanvasFileDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bindings: Option<Vec<CanvasDataBindingDto>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct UpdateCanvasRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entry_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sandbox_config: Option<CanvasSandboxConfigDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub files: Option<Vec<CanvasFileDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bindings: Option<Vec<CanvasDataBindingDto>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct DeleteCanvasResponse {
    pub deleted: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct PublishCanvasToProjectRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub canvas_mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CopyCanvasToPersonalRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub canvas_mount_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct UnpublishCanvasResponse {
    pub unpublished_canvas_id: String,
    pub source_canvas_id: Option<String>,
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
    pub canvas_mount_id: String,
    pub vfs_mount_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasAgentRunRuntimeBridgeSnapshotDto {
    pub enabled: bool,
    #[serde(default)]
    pub actions: Vec<RuntimeActionDescriptorDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasAgentRunRuntimeSnapshotDto {
    pub canvas_id: String,
    pub canvas_mount_id: String,
    pub vfs_mount_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resource_surface_ref: Option<String>,
    pub entry: String,
    pub files: Vec<CanvasRuntimeFileDto>,
    pub bindings: Vec<CanvasRuntimeBindingDto>,
    pub import_map: CanvasImportMapDto,
    pub libraries: Vec<String>,
    pub runtime_bridge: CanvasAgentRunRuntimeBridgeSnapshotDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeInvokeRequest {
    pub action_key: String,
    #[serde(default)]
    #[ts(type = "JsonValue")]
    pub input: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CanvasRuntimeObservationStatusDto {
    Building,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeViewportDto {
    pub width: i32,
    pub height: i32,
    pub device_pixel_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeDocumentStateDto {
    pub root_empty: bool,
    pub body_text_preview: String,
    pub element_count: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub focused_element: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeDiagnosticDto {
    pub level: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeObservationUpsertRequest {
    pub frame_id: String,
    pub generation: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub captured_at: Option<String>,
    pub status: CanvasRuntimeObservationStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
    pub viewport: CanvasRuntimeViewportDto,
    pub document: CanvasRuntimeDocumentStateDto,
    #[serde(default)]
    pub diagnostics: Vec<CanvasRuntimeDiagnosticDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub screenshot_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasRuntimeObservation {
    pub observation_id: String,
    pub run_id: String,
    pub agent_id: String,
    pub agent_run_canvas_ref: String,
    pub canvas_id: String,
    pub canvas_mount_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_trace_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_agent_frame_id: Option<String>,
    pub frame_id: String,
    pub generation: i32,
    pub captured_at: String,
    pub status: CanvasRuntimeObservationStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
    pub viewport: CanvasRuntimeViewportDto,
    pub document: CanvasRuntimeDocumentStateDto,
    #[serde(default)]
    pub diagnostics: Vec<CanvasRuntimeDiagnosticDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub screenshot_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasInteractionEventDto {
    pub kind: String,
    #[ts(type = "JsonValue")]
    pub payload: Value,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasInteractionSnapshotUpsertRequest {
    pub frame_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub updated_at: Option<String>,
    #[serde(default)]
    #[ts(type = "JsonValue")]
    pub state: Value,
    #[serde(default)]
    pub recent_events: Vec<CanvasInteractionEventDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasInteractionSnapshot {
    pub snapshot_id: String,
    pub run_id: String,
    pub agent_id: String,
    pub agent_run_canvas_ref: String,
    pub canvas_id: String,
    pub canvas_mount_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_trace_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_agent_frame_id: Option<String>,
    pub frame_id: String,
    pub updated_at: String,
    #[ts(type = "JsonValue")]
    pub state: Value,
    #[serde(default)]
    pub recent_events: Vec<CanvasInteractionEventDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CanvasAgentInputSubmitRequest {
    pub input: Vec<codex::UserInput>,
    pub client_command_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub interaction_snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub render_observation_id: Option<String>,
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
