pub mod runtime_bridge;
pub mod runtime_tool_provider;
mod tools;
pub mod visibility;

use agentdash_application_runtime_gateway::validate_json_schema_subset;
use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch, WorkspaceModulePresentation,
    WorkspaceModuleStatus, WorkspaceModuleSummary, WorkspaceModuleUiEntry,
};
use agentdash_domain::canvas::{Canvas, CanvasAccessAction, CanvasAccessProjection, CanvasScope};
use agentdash_domain::shared_library::{
    ExtensionBundleKind, ExtensionPermissionDeclaration, ExtensionRuntimeActionKind,
    ExtensionWorkspaceTabRendererDeclaration,
};
use thiserror::Error;

use crate::canvas::{
    CANVAS_BIND_DATA_OPERATION_KEY, CANVAS_BIND_DATA_ORIGIN,
    CANVAS_GET_INTERACTION_STATE_OPERATION_KEY, CANVAS_INSPECT_OPERATION_KEY,
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

pub fn validate_input_against_schema(
    schema: &serde_json::Value,
    input: &serde_json::Value,
) -> Result<(), String> {
    validate_json_schema_subset(schema, input)
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
                .filter(|tab| tab.loadability.available)
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

            let has_extension_host_runtime = installation.package_artifact.is_some()
                && ext.bundles.iter().any(|bundle| {
                    bundle.extension_key == extension_key
                        && bundle.kind == ExtensionBundleKind::ExtensionHost
                        && !bundle.entry.trim().is_empty()
                });
            let has_available_ui_only_tab = ext.workspace_tabs.iter().any(|tab| {
                tab.extension_key == extension_key
                    && tab.loadability.available
                    && matches!(
                        tab.loadability.mode,
                        crate::extension_runtime::ExtensionWorkspaceTabLoadabilityMode::UiOnly
                    )
            });
            let status = if has_extension_host_runtime
                || (operations.is_empty() && has_available_ui_only_tab)
            {
                WorkspaceModuleStatus::ready()
            } else {
                WorkspaceModuleStatus::unavailable(
                    "extension host bundle 缺失，runtime operation 无法加载",
                )
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
    operations.push(canvas_inspect_operation());
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

fn canvas_inspect_operation() -> WorkspaceModuleOperation {
    WorkspaceModuleOperation {
        operation_key: CANVAS_INSPECT_OPERATION_KEY.to_string(),
        origin: CANVAS_BIND_DATA_ORIGIN.to_string(),
        description: "Inspect the latest user-visible runtime observation reported by this Canvas."
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
            canvas_action: WorkspaceModuleCanvasHostAction::Inspect,
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

#[cfg(test)]
mod tests {
    use agentdash_contracts::workspace_module::WorkspaceModuleStatusKind;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionRuntimeActionKind, ExtensionWorkspaceTabRendererDeclaration,
    };
    use uuid::Uuid;

    use crate::extension_runtime::{
        ExtensionBundleProjection, ExtensionInstallationProjection,
        ExtensionRuntimeActionProjection, ExtensionRuntimeProjection,
        ExtensionWorkspaceTabLoadabilityMode, ExtensionWorkspaceTabLoadabilityProjection,
        ExtensionWorkspaceTabProjection,
    };

    use super::{build_workspace_modules, validate_input_against_schema};

    #[test]
    fn workspace_module_accepts_ui_only_canvas_panel_without_extension_host_bundle() {
        let projection = ExtensionRuntimeProjection {
            installations: vec![ExtensionInstallationProjection {
                installation_id: Uuid::new_v4(),
                extension_key: "canvas-demo".to_string(),
                extension_id: "canvas-demo".to_string(),
                display_name: "Canvas Demo".to_string(),
                installed_source: None,
                package_artifact: None,
            }],
            workspace_tabs: vec![ExtensionWorkspaceTabProjection {
                extension_key: "canvas-demo".to_string(),
                extension_id: "canvas-demo".to_string(),
                type_id: "canvas-demo.panel".to_string(),
                label: "Canvas Demo".to_string(),
                uri_scheme: "canvas-demo".to_string(),
                renderer: ExtensionWorkspaceTabRendererDeclaration::CanvasPanel {
                    entry: "dist/canvas/runtime-snapshot.json".to_string(),
                },
                loadability: ExtensionWorkspaceTabLoadabilityProjection {
                    available: true,
                    mode: ExtensionWorkspaceTabLoadabilityMode::UiOnly,
                    reason: None,
                },
            }],
            ..Default::default()
        };

        let modules = build_workspace_modules(&projection, &[]);

        assert_eq!(modules.len(), 1);
        assert_eq!(
            modules[0].summary.status.kind,
            WorkspaceModuleStatusKind::Ready
        );
        assert_eq!(modules[0].ui_entries.len(), 1);
    }

    #[test]
    fn workspace_module_schema_validator_rejects_additional_properties() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "alias": { "type": "string" }
            },
            "required": ["alias"],
            "additionalProperties": false
        });

        assert!(
            validate_input_against_schema(
                &schema,
                &serde_json::json!({
                    "alias": "stats",
                    "extra": true
                })
            )
            .is_err()
        );
    }

    #[test]
    fn workspace_module_operation_runtime_requires_package_artifact() {
        let projection = ExtensionRuntimeProjection {
            installations: vec![ExtensionInstallationProjection {
                installation_id: Uuid::new_v4(),
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                display_name: "Ops Demo".to_string(),
                installed_source: None,
                package_artifact: None,
            }],
            runtime_actions: vec![ExtensionRuntimeActionProjection {
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                action_key: "ops-demo.run".to_string(),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "Run".to_string(),
                input_schema: serde_json::json!(true),
                output_schema: serde_json::json!(true),
                permissions: vec![],
            }],
            bundles: vec![ExtensionBundleProjection {
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: "sha256:bundle".to_string(),
            }],
            ..Default::default()
        };

        let modules = build_workspace_modules(&projection, &[]);

        assert_eq!(modules.len(), 1);
        assert_eq!(
            modules[0].summary.status.kind,
            WorkspaceModuleStatusKind::Unavailable
        );
    }
}
