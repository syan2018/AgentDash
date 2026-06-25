pub mod runtime_bridge;
pub mod runtime_tool_provider;
mod tools;
pub mod visibility;

use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch, WorkspaceModulePresentation,
    WorkspaceModuleStatus, WorkspaceModuleSummary, WorkspaceModuleUiEntry,
};
use agentdash_domain::canvas::{Canvas, CanvasAccessAction, CanvasAccessProjection, CanvasScope};
use agentdash_domain::shared_library::{
    ExtensionPermissionDeclaration, ExtensionRuntimeActionKind,
    ExtensionWorkspaceTabRendererDeclaration,
};
use thiserror::Error;

use crate::canvas::{
    CANVAS_BIND_DATA_OPERATION_KEY, CANVAS_BIND_DATA_ORIGIN,
    CANVAS_GET_INTERACTION_STATE_OPERATION_KEY, CANVAS_INSPECT_RENDER_STATE_OPERATION_KEY,
    CANVAS_MODULE_ID_PREFIX, CANVAS_PRESENTATION_SCHEME, CANVAS_PREVIEW_VIEW_KEY,
    CANVAS_RENDERER_KIND, CanvasWithAccess, canvas_module_id, canvas_presentation_uri,
    canvas_vfs_mount_id,
};
use crate::extension_runtime::ExtensionRuntimeProjection;

pub use runtime_bridge::{
    ResolvedInvocationBackend, SharedWorkspaceModuleAgentRunBridgeHandle,
    SharedWorkspaceModuleRuntimeGatewayHandle, WorkspaceModuleAgentRunBridge,
    delivery_runtime_session_id_from_context, project_authorization_context_from_identity,
    project_id_from_context, request_existing_canvas_visibility_for_runtime,
    resolve_invocation_backend, shared_runtime_vfs_from_context,
    submit_canvas_runtime_surface_update,
};
pub use runtime_tool_provider::WorkspaceModuleRuntimeToolProvider;
pub use tools::{
    WorkspaceModuleDescribeTool, WorkspaceModuleInvokeTool, WorkspaceModuleListTool,
    WorkspaceModuleOperateTool, WorkspaceModulePresentTool,
};
pub use visibility::{
    WorkspaceModuleVisibilityDiagnostic, WorkspaceModuleVisibilityProjection,
    resolve_workspace_module_visibility,
};

pub const MODULE_ID_EXTENSION_PREFIX: &str = "ext:";
pub const MODULE_ID_CANVAS_PREFIX: &str = CANVAS_MODULE_ID_PREFIX;
pub const MODULE_ID_BUILTIN_PREFIX: &str = "builtin:";

/// 轻量 input schema 校验（无外部 jsonschema 依赖）。
///
/// 覆盖 describe 出的 operation `input_schema` 的常见形态：顶层 `type` 与 `required`。
/// 校验范围刻意保守，只拦截类型大类不符与缺必填字段这类明确违例。
pub fn validate_input_against_schema(
    schema: &serde_json::Value,
    input: &serde_json::Value,
) -> Result<(), String> {
    let serde_json::Value::Object(schema_obj) = schema else {
        return Ok(());
    };

    if let Some(expected_type) = schema_obj.get("type").and_then(|t| t.as_str())
        && !json_value_matches_type(input, expected_type)
    {
        return Err(format!(
            "input 类型不匹配 schema：期望 `{expected_type}`，实际 `{}`",
            json_value_type_name(input)
        ));
    }

    if let Some(required) = schema_obj.get("required").and_then(|r| r.as_array()) {
        let obj = input.as_object();
        for field in required {
            let Some(name) = field.as_str() else { continue };
            let present = obj.is_some_and(|map| map.contains_key(name));
            if !present {
                return Err(format!("input 缺少 schema 要求的必填字段 `{name}`"));
            }
        }
    }

    Ok(())
}

fn json_value_matches_type(value: &serde_json::Value, expected: &str) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn json_value_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

pub fn build_workspace_modules(
    ext: &ExtensionRuntimeProjection,
    canvases: &[Canvas],
) -> Vec<WorkspaceModuleDescriptor> {
    let mut modules = Vec::new();
    modules.extend(build_extension_modules(ext));
    modules.extend(canvases.iter().map(|canvas| {
        let access = default_canvas_access_for_descriptor(canvas);
        build_canvas_module(canvas, &access)
    }));
    modules
}

pub fn build_workspace_modules_with_canvas_access(
    ext: &ExtensionRuntimeProjection,
    canvases: &[CanvasWithAccess],
) -> Vec<WorkspaceModuleDescriptor> {
    let mut modules = Vec::new();
    modules.extend(build_extension_modules(ext));
    modules.extend(
        canvases
            .iter()
            .filter(|value| value.access.can_view)
            .map(|value| build_canvas_module(&value.canvas, &value.access)),
    );
    modules
}

pub fn build_canvas_workspace_module(
    canvas: &Canvas,
    access: &CanvasAccessProjection,
) -> WorkspaceModuleDescriptor {
    build_canvas_module(canvas, access)
}

#[derive(Debug, Error)]
pub enum WorkspaceModulePresentationError {
    #[error("module `{module_id}` 无名为 `{view_key}` 的 UI view")]
    ViewNotFound {
        module_id: String,
        view_key: String,
        available_views: Vec<String>,
    },
    #[error("module `{module_id}` view `{view_key}` 没有 canonical presentation_uri")]
    MissingPresentationUri {
        module_id: String,
        view_key: String,
        renderer_kind: String,
    },
}

impl WorkspaceModulePresentationError {
    pub fn diagnostics(&self) -> serde_json::Value {
        match self {
            Self::ViewNotFound {
                module_id,
                view_key,
                available_views,
            } => serde_json::json!({
                "module_id": module_id,
                "view_key": view_key,
                "reason": "no_matching_ui_entry",
                "available_views": available_views,
            }),
            Self::MissingPresentationUri {
                module_id,
                view_key,
                renderer_kind,
            } => serde_json::json!({
                "module_id": module_id,
                "view_key": view_key,
                "renderer_kind": renderer_kind,
                "reason": "missing_presentation_uri",
            }),
        }
    }
}

pub fn build_workspace_module_presentation(
    module: &WorkspaceModuleDescriptor,
    view_key: &str,
    payload: Option<serde_json::Value>,
    diagnostics: Option<serde_json::Value>,
) -> Result<WorkspaceModulePresentation, WorkspaceModulePresentationError> {
    let Some(ui_entry) = module
        .ui_entries
        .iter()
        .find(|entry| entry.view_key == view_key)
    else {
        return Err(WorkspaceModulePresentationError::ViewNotFound {
            module_id: module.summary.module_id.clone(),
            view_key: view_key.to_string(),
            available_views: module
                .ui_entries
                .iter()
                .map(|entry| entry.view_key.clone())
                .collect(),
        });
    };

    let presentation_uri = ui_entry.presentation_uri.clone().or_else(|| {
        ui_entry
            .uri_scheme
            .as_ref()
            .map(|scheme| format!("{scheme}://panel"))
    });
    let Some(presentation_uri) = presentation_uri else {
        return Err(WorkspaceModulePresentationError::MissingPresentationUri {
            module_id: module.summary.module_id.clone(),
            view_key: view_key.to_string(),
            renderer_kind: ui_entry.renderer_kind.clone(),
        });
    };

    Ok(WorkspaceModulePresentation {
        module_id: module.summary.module_id.clone(),
        view_key: ui_entry.view_key.clone(),
        renderer_kind: ui_entry.renderer_kind.clone(),
        presentation_uri,
        title: ui_entry.title.clone(),
        payload,
        diagnostics,
    })
}

fn build_extension_modules(ext: &ExtensionRuntimeProjection) -> Vec<WorkspaceModuleDescriptor> {
    ext.installations
        .iter()
        .map(|installation| {
            let extension_key = installation.extension_key.as_str();
            let mut operations: Vec<WorkspaceModuleOperation> = ext
                .runtime_actions
                .iter()
                .filter(|action| action.extension_key == extension_key)
                .map(|action| WorkspaceModuleOperation {
                    operation_key: action.action_key.clone(),
                    origin: runtime_action_origin(&action.kind).to_string(),
                    description: action.description.clone(),
                    input_schema: Some(action.input_schema.clone()),
                    output_schema: Some(action.output_schema.clone()),
                    permission_summary: action.permissions.clone(),
                    dispatch: WorkspaceModuleOperationDispatch::RuntimeAction {
                        action_key: action.action_key.clone(),
                    },
                })
                .collect();

            for channel in ext
                .protocol_channels
                .iter()
                .filter(|channel| channel.extension_key == extension_key)
            {
                for method in &channel.methods {
                    operations.push(WorkspaceModuleOperation {
                        operation_key: format!("{}.{}", channel.channel_key, method.name),
                        origin: "protocol_channel".to_string(),
                        description: method.description.clone(),
                        input_schema: Some(method.input_schema.clone()),
                        output_schema: Some(method.output_schema.clone()),
                        permission_summary: method.permissions.clone(),
                        dispatch: WorkspaceModuleOperationDispatch::ProtocolChannel {
                            channel_key: channel.channel_key.clone(),
                            method_name: method.name.clone(),
                        },
                    });
                }
            }

            let ui_entries: Vec<WorkspaceModuleUiEntry> = ext
                .workspace_tabs
                .iter()
                .filter(|tab| tab.extension_key == extension_key)
                .map(|tab| WorkspaceModuleUiEntry {
                    view_key: tab.type_id.clone(),
                    renderer_kind: tab_renderer_kind(&tab.renderer).to_string(),
                    presentation_uri: Some(format!("{}://panel", tab.uri_scheme)),
                    uri_scheme: Some(tab.uri_scheme.clone()),
                    title: tab.label.clone(),
                })
                .collect();

            let permission_summary: Vec<String> = ext
                .permissions
                .iter()
                .filter(|permission| permission.extension_key == extension_key)
                .map(|permission| describe_permission(&permission.permission))
                .collect();

            let has_bundle = ext
                .bundles
                .iter()
                .any(|bundle| bundle.extension_key == extension_key);
            let status = if has_bundle {
                WorkspaceModuleStatus::ready()
            } else {
                WorkspaceModuleStatus::unavailable("extension runtime bundle 缺失，模块无法加载")
            };

            let operation_summary = operations
                .iter()
                .map(|operation| operation.operation_key.clone())
                .collect::<Vec<_>>();

            let summary = WorkspaceModuleSummary {
                module_id: format!("{MODULE_ID_EXTENSION_PREFIX}{extension_key}"),
                kind: WorkspaceModuleKind::Extension,
                title: installation.display_name.clone(),
                description: installation.extension_id.clone(),
                source: extension_key.to_string(),
                ui_summary: ui_summary(ui_entries.len()),
                operation_summary,
                status,
                permission_summary: permission_summary.clone(),
            };

            WorkspaceModuleDescriptor {
                summary,
                ui_entries,
                operations,
                runtime_backing: Some(format!("extension_runtime:{extension_key}")),
            }
        })
        .collect()
}

fn build_canvas_module(
    canvas: &Canvas,
    access: &CanvasAccessProjection,
) -> WorkspaceModuleDescriptor {
    let mut operations: Vec<WorkspaceModuleOperation> = Vec::new();
    if access.can_view {
        operations.push(canvas_bind_data_operation());
    }
    operations.push(canvas_inspect_render_state_operation());
    operations.push(canvas_get_interaction_state_operation());

    let ui_entries = vec![WorkspaceModuleUiEntry {
        view_key: CANVAS_PREVIEW_VIEW_KEY.to_string(),
        renderer_kind: CANVAS_RENDERER_KIND.to_string(),
        presentation_uri: Some(canvas_presentation_uri(&canvas.mount_id)),
        uri_scheme: Some(CANVAS_PRESENTATION_SCHEME.to_string()),
        title: canvas.title.clone(),
    }];

    let operation_summary = operations
        .iter()
        .map(|operation| operation.operation_key.clone())
        .collect::<Vec<_>>();
    let permission_summary = canvas_permission_summary(access);

    let summary = WorkspaceModuleSummary {
        module_id: canvas_module_id(&canvas.mount_id),
        kind: WorkspaceModuleKind::Canvas,
        title: canvas.title.clone(),
        description: canvas.description.clone(),
        source: canvas.mount_id.clone(),
        ui_summary: ui_summary(ui_entries.len()),
        operation_summary,
        status: WorkspaceModuleStatus::ready(),
        permission_summary,
    };

    WorkspaceModuleDescriptor {
        summary,
        ui_entries,
        operations,
        runtime_backing: Some(format!(
            "canvas_vfs:{}",
            canvas_vfs_mount_id(&canvas.mount_id)
        )),
    }
}

fn canvas_bind_data_operation() -> WorkspaceModuleOperation {
    WorkspaceModuleOperation {
        operation_key: CANVAS_BIND_DATA_OPERATION_KEY.to_string(),
        origin: CANVAS_BIND_DATA_ORIGIN.to_string(),
        description: "Declare or update a data binding for this Canvas in the current AgentRun runtime surface.".to_string(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "alias": {"type": "string"},
                "source_uri": {"type": "string"},
                "content_type": {"type": "string"}
            },
            "required": ["alias", "source_uri"],
            "additionalProperties": false
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "canvas_id": {"type": "string"},
                "canvas_mount_id": {"type": "string"},
                "vfs_mount_id": {"type": "string"},
                "bindings": {"type": "array"}
            },
            "required": ["canvas_id", "canvas_mount_id", "vfs_mount_id", "bindings"]
        })),
        permission_summary: vec!["canvas.runtime_binding:write".to_string()],
        dispatch: WorkspaceModuleOperationDispatch::HostCanvas {
            canvas_action: WorkspaceModuleCanvasHostAction::BindData,
        },
    }
}

fn canvas_inspect_render_state_operation() -> WorkspaceModuleOperation {
    WorkspaceModuleOperation {
        operation_key: CANVAS_INSPECT_RENDER_STATE_OPERATION_KEY.to_string(),
        origin: CANVAS_BIND_DATA_ORIGIN.to_string(),
        description: "Inspect the latest render observation reported by this Canvas runtime."
            .to_string(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {"observation": { "type": ["object", "null"] }},
            "required": ["observation"]
        })),
        permission_summary: vec!["canvas.runtime:inspect".to_string()],
        dispatch: WorkspaceModuleOperationDispatch::HostCanvas {
            canvas_action: WorkspaceModuleCanvasHostAction::InspectRenderState,
        },
    }
}

fn canvas_get_interaction_state_operation() -> WorkspaceModuleOperation {
    WorkspaceModuleOperation {
        operation_key: CANVAS_GET_INTERACTION_STATE_OPERATION_KEY.to_string(),
        origin: CANVAS_BIND_DATA_ORIGIN.to_string(),
        description: "Read the latest user interaction snapshot explicitly exposed by this Canvas."
            .to_string(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {"snapshot": { "type": ["object", "null"] }},
            "required": ["snapshot"]
        })),
        permission_summary: vec!["canvas.interaction:read".to_string()],
        dispatch: WorkspaceModuleOperationDispatch::HostCanvas {
            canvas_action: WorkspaceModuleCanvasHostAction::GetInteractionState,
        },
    }
}

fn canvas_permission_summary(access: &CanvasAccessProjection) -> Vec<String> {
    if access.allows(CanvasAccessAction::EditSource) {
        vec!["canvas.source:edit".to_string()]
    } else {
        vec!["canvas.source:read_only".to_string()]
    }
}

fn default_canvas_access_for_descriptor(canvas: &Canvas) -> CanvasAccessProjection {
    let editable = canvas.scope == CanvasScope::Personal;
    CanvasAccessProjection {
        can_view: true,
        can_edit_source: editable,
        can_publish: editable,
        can_manage_shared: false,
        can_copy: true,
        runtime_write_allowed: editable,
    }
}

fn runtime_action_origin(_kind: &ExtensionRuntimeActionKind) -> &'static str {
    "runtime_action"
}

fn tab_renderer_kind(renderer: &ExtensionWorkspaceTabRendererDeclaration) -> &'static str {
    match renderer {
        ExtensionWorkspaceTabRendererDeclaration::Webview { .. } => "webview",
        ExtensionWorkspaceTabRendererDeclaration::CanvasPanel { .. } => CANVAS_RENDERER_KIND,
    }
}

fn ui_summary(count: usize) -> Option<String> {
    if count == 0 {
        None
    } else {
        Some(format!("{count} UI entry"))
    }
}

fn describe_permission(permission: &ExtensionPermissionDeclaration) -> String {
    match permission {
        ExtensionPermissionDeclaration::LocalProfile { access } => {
            format!("local.profile:{access:?}")
        }
        ExtensionPermissionDeclaration::Http { hosts, access } => {
            format!("http[{}]:{access:?}", hosts.join(","))
        }
        ExtensionPermissionDeclaration::Workspace { access } => format!("workspace:{access:?}"),
        ExtensionPermissionDeclaration::Env { names, access } => {
            format!("env[{}]:{access:?}", names.join(","))
        }
        ExtensionPermissionDeclaration::Process { access } => format!("process:{access:?}"),
        ExtensionPermissionDeclaration::RuntimeAction { action_key } => {
            format!("runtime_action:{action_key}")
        }
        ExtensionPermissionDeclaration::ExtensionChannel {
            channel_key,
            methods,
        } => format!("channel:{channel_key}[{}]", methods.join(",")),
    }
}
