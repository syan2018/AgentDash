pub mod visibility;

use agentdash_application_extension_gateway::{
    RuntimeActionDescriptor, RuntimeActionKind, validate_json_schema_subset,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch, WorkspaceModuleOperationReadiness,
    WorkspaceModuleOperationReadinessKind, WorkspaceModuleOperationVisibility,
    WorkspaceModulePresentation, WorkspaceModuleStatus, WorkspaceModuleSummary,
    WorkspaceModuleUiEntry,
};
use agentdash_domain::canvas::{Canvas, CanvasAccessAction, CanvasAccessProjection, CanvasScope};
use agentdash_domain::shared_library::{
    ExtensionBundleKind, ExtensionPermissionDeclaration, ExtensionWorkspaceTabRendererDeclaration,
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
use crate::extension_runtime::{
    ExtensionBackendServiceProjection, ExtensionGeneratedOperationDispatch,
    ExtensionGeneratedOperationProjection, ExtensionGeneratedOperationVisibility,
};

pub use visibility::{
    WorkspaceModuleVisibilityDiagnostic, WorkspaceModuleVisibilityInput,
    WorkspaceModuleVisibilityProjection, project_agent_run_workspace_module_visibility,
    project_workspace_module_visibility, resolve_workspace_module_visibility,
    resolve_workspace_module_visibility_with_operation_context,
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
    build_workspace_modules_with_operation_context(
        ext,
        canvases,
        &WorkspaceModuleOperationContext::default(),
    )
}

pub fn build_workspace_modules_with_operation_context(
    ext: &ExtensionRuntimeProjection,
    canvases: &[Canvas],
    operation_context: &WorkspaceModuleOperationContext,
) -> Vec<WorkspaceModuleDescriptor> {
    let mut modules = Vec::new();
    modules.extend(build_extension_modules(ext, operation_context));
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
    build_workspace_modules_with_canvas_access_and_operation_context(
        ext,
        canvases,
        &WorkspaceModuleOperationContext::default(),
    )
}

pub fn build_workspace_modules_with_canvas_access_and_operation_context(
    ext: &ExtensionRuntimeProjection,
    canvases: &[CanvasWithAccess],
    operation_context: &WorkspaceModuleOperationContext,
) -> Vec<WorkspaceModuleDescriptor> {
    let mut modules = Vec::new();
    modules.extend(build_extension_modules(ext, operation_context));
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

#[derive(Debug, Clone)]
pub struct WorkspaceModuleRuntimeActionCatalog {
    descriptors: Vec<RuntimeActionDescriptor>,
    missing_descriptor_readiness: WorkspaceModuleOperationReadiness,
}

impl WorkspaceModuleRuntimeActionCatalog {
    pub fn from_descriptors(descriptors: Vec<RuntimeActionDescriptor>) -> Self {
        Self {
            descriptors,
            missing_descriptor_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::RuntimeActionUnavailable,
                "runtime action is not present in the ExtensionGateway actor/context catalog",
            ),
        }
    }

    pub fn missing_extension_gateway(reason: impl Into<String>) -> Self {
        Self {
            descriptors: Vec::new(),
            missing_descriptor_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::MissingExtensionGateway,
                reason,
            ),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            descriptors: Vec::new(),
            missing_descriptor_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::RuntimeActionUnavailable,
                reason,
            ),
        }
    }

    fn runtime_thread_action_descriptor(
        &self,
        action_key: &str,
    ) -> Option<&RuntimeActionDescriptor> {
        self.descriptors.iter().find(|descriptor| {
            descriptor.kind == RuntimeActionKind::RuntimeThread
                && descriptor.action_key.as_str() == action_key
        })
    }
}

impl Default for WorkspaceModuleRuntimeActionCatalog {
    fn default() -> Self {
        Self::missing_extension_gateway(
            "ExtensionGateway catalog is not attached to this workspace module projection",
        )
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceModuleOperationContext {
    pub runtime_actions: WorkspaceModuleRuntimeActionCatalog,
    pub channel_readiness: WorkspaceModuleOperationReadiness,
    pub backend_readiness: WorkspaceModuleOperationReadiness,
    pub backend_service_readiness: WorkspaceModuleOperationReadiness,
}

impl WorkspaceModuleOperationContext {
    pub fn ready(runtime_actions: Vec<RuntimeActionDescriptor>) -> Self {
        Self {
            runtime_actions: WorkspaceModuleRuntimeActionCatalog::from_descriptors(runtime_actions),
            channel_readiness: WorkspaceModuleOperationReadiness::ready(),
            backend_readiness: WorkspaceModuleOperationReadiness::ready(),
            backend_service_readiness: WorkspaceModuleOperationReadiness::ready(),
        }
    }
}

impl Default for WorkspaceModuleOperationContext {
    fn default() -> Self {
        Self {
            runtime_actions: WorkspaceModuleRuntimeActionCatalog::default(),
            channel_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::MissingChannelTransport,
                "extension channel transport is not attached to this workspace module projection",
            ),
            backend_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::MissingRuntimeBackendAnchor,
                "runtime backend anchor is not attached to this workspace module projection",
            ),
            backend_service_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable,
                "backendService bridge transport is not attached to this workspace module projection",
            ),
        }
    }
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

fn build_extension_modules(
    ext: &ExtensionRuntimeProjection,
    operation_context: &WorkspaceModuleOperationContext,
) -> Vec<WorkspaceModuleDescriptor> {
    ext.installations
        .iter()
        .map(|installation| {
            let extension_key = installation.extension_key.as_str();
            let mut operations: Vec<WorkspaceModuleOperation> = ext
                .operation_catalog
                .iter()
                .filter(|operation| operation.extension_key == extension_key)
                .map(|operation| {
                    operation_from_generated_projection(
                        operation,
                        operation_context,
                        &ext.backend_services,
                    )
                })
                .collect();

            operations.sort_by(|left, right| left.operation_key.cmp(&right.operation_key));

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
            let has_backend_service_runtime = installation.package_artifact.is_some()
                && ext.backend_services.iter().any(|service| {
                    service.extension_key == extension_key
                        && !service.service_key.trim().is_empty()
                        && !service.entry.trim().is_empty()
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
                || has_backend_service_runtime
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

fn operation_from_generated_projection(
    operation: &ExtensionGeneratedOperationProjection,
    operation_context: &WorkspaceModuleOperationContext,
    backend_services: &[ExtensionBackendServiceProjection],
) -> WorkspaceModuleOperation {
    let (origin, dispatch, readiness) = match &operation.dispatch {
        ExtensionGeneratedOperationDispatch::RuntimeAction { action_key } => {
            let readiness = if operation_context
                .runtime_actions
                .runtime_thread_action_descriptor(action_key)
                .is_some()
            {
                first_unready_or_ready([&operation_context.backend_readiness])
            } else {
                operation_context
                    .runtime_actions
                    .missing_descriptor_readiness
                    .clone()
            };
            (
                "runtime_action",
                WorkspaceModuleOperationDispatch::RuntimeAction {
                    action_key: action_key.clone(),
                },
                readiness,
            )
        }
        ExtensionGeneratedOperationDispatch::ProtocolChannel {
            channel_key,
            method,
        } => (
            "protocol_channel",
            WorkspaceModuleOperationDispatch::ProtocolChannel {
                channel_key: channel_key.clone(),
                method_name: method.clone(),
            },
            first_unready_or_ready([
                &operation_context.channel_readiness,
                &operation_context.backend_readiness,
            ]),
        ),
        ExtensionGeneratedOperationDispatch::BackendService { service_key, route } => {
            let readiness = backend_service_operation_readiness(
                operation,
                service_key,
                route,
                backend_services,
                operation_context,
            );
            (
                "backend_service",
                WorkspaceModuleOperationDispatch::BackendService {
                    service_key: service_key.clone(),
                    route: route.clone(),
                },
                readiness,
            )
        }
    };
    WorkspaceModuleOperation {
        operation_key: operation.operation_key.clone(),
        origin: origin.to_string(),
        description: operation.description.clone(),
        input_schema: Some(operation.input_schema.clone()),
        output_schema: Some(operation.output_schema.clone()),
        permission_summary: operation.permission_summary.clone(),
        visibility: operation_visibility(operation.visibility),
        provenance: generated_operation_provenance(operation),
        dispatch,
        readiness,
    }
}

fn backend_service_operation_readiness(
    operation: &ExtensionGeneratedOperationProjection,
    service_key: &str,
    route: &str,
    backend_services: &[ExtensionBackendServiceProjection],
    operation_context: &WorkspaceModuleOperationContext,
) -> WorkspaceModuleOperationReadiness {
    let Some(service) = backend_services.iter().find(|service| {
        service.extension_key == operation.extension_key && service.service_key == service_key
    }) else {
        return WorkspaceModuleOperationReadiness::unavailable(
            WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable,
            format!(
                "backendService `{service_key}` is not declared by extension `{}`",
                operation.extension_key
            ),
        );
    };
    if !service
        .routes
        .iter()
        .any(|pattern| route_matches(pattern, route))
    {
        return WorkspaceModuleOperationReadiness::unavailable(
            WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable,
            format!(
                "backendService `{service_key}` route `{route}` is not declared by extension `{}`",
                operation.extension_key
            ),
        );
    }
    first_unready_or_ready([
        &operation_context.backend_readiness,
        &operation_context.backend_service_readiness,
    ])
}

fn route_matches(pattern: &str, route: &str) -> bool {
    let pattern = route_pattern_path(pattern);
    let route = strip_query(route.trim());
    if pattern == route {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return route == prefix || route.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return route.starts_with(prefix);
    }
    false
}

fn route_pattern_path(pattern: &str) -> &str {
    let pattern = strip_query(pattern.trim());
    let Some(rest) = pattern
        .strip_prefix("http://")
        .or_else(|| pattern.strip_prefix("https://"))
    else {
        return pattern;
    };
    rest.find('/').map_or("/", |index| &rest[index..])
}

fn strip_query(value: &str) -> &str {
    value.split_once('?').map_or(value, |(path, _)| path)
}

fn operation_visibility(
    visibility: ExtensionGeneratedOperationVisibility,
) -> WorkspaceModuleOperationVisibility {
    match visibility {
        ExtensionGeneratedOperationVisibility::PanelOnly => {
            WorkspaceModuleOperationVisibility::PanelOnly
        }
        ExtensionGeneratedOperationVisibility::AgentAndPanel => {
            WorkspaceModuleOperationVisibility::AgentAndPanel
        }
    }
}

fn generated_operation_provenance(
    operation: &ExtensionGeneratedOperationProjection,
) -> serde_json::Value {
    serde_json::json!({
        "extension_key": operation.extension_key,
        "extension_id": operation.extension_id,
        "capability_key": operation.provenance.capability_key,
        "exposure_key": operation.provenance.exposure_key,
        "generated_from": operation.provenance.generated_from,
    })
}

fn first_unready_or_ready<'a, I>(readinesses: I) -> WorkspaceModuleOperationReadiness
where
    I: IntoIterator<Item = &'a WorkspaceModuleOperationReadiness>,
{
    readinesses
        .into_iter()
        .find(|readiness| !readiness.is_ready())
        .cloned()
        .unwrap_or_else(WorkspaceModuleOperationReadiness::ready)
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
        visibility: WorkspaceModuleOperationVisibility::AgentAndPanel,
        provenance: host_canvas_operation_provenance(CANVAS_BIND_DATA_OPERATION_KEY),
        dispatch: WorkspaceModuleOperationDispatch::HostCanvas {
            canvas_action: WorkspaceModuleCanvasHostAction::BindData,
        },
        readiness: WorkspaceModuleOperationReadiness::ready(),
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
        visibility: WorkspaceModuleOperationVisibility::AgentAndPanel,
        provenance: host_canvas_operation_provenance(CANVAS_INSPECT_OPERATION_KEY),
        dispatch: WorkspaceModuleOperationDispatch::HostCanvas {
            canvas_action: WorkspaceModuleCanvasHostAction::Inspect,
        },
        readiness: WorkspaceModuleOperationReadiness::ready(),
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
        visibility: WorkspaceModuleOperationVisibility::AgentAndPanel,
        provenance: host_canvas_operation_provenance(CANVAS_GET_INTERACTION_STATE_OPERATION_KEY),
        dispatch: WorkspaceModuleOperationDispatch::HostCanvas {
            canvas_action: WorkspaceModuleCanvasHostAction::GetInteractionState,
        },
        readiness: WorkspaceModuleOperationReadiness::ready(),
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

fn host_canvas_operation_provenance(operation_key: &str) -> serde_json::Value {
    serde_json::json!({
        "capability_key": "canvas",
        "exposure_key": operation_key,
        "generated_from": "host_canvas",
    })
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
        ExtensionPermissionDeclaration::BackendService {
            service_key,
            routes,
        } => format!("backend_service:{service_key}[{}]", routes.join(",")),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_application_extension_gateway::{
        RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimePolicy,
    };
    use agentdash_contracts::workspace_module::{
        WorkspaceModuleOperationReadiness, WorkspaceModuleOperationReadinessKind,
        WorkspaceModuleOperationVisibility, WorkspaceModuleStatusKind,
    };
    use agentdash_domain::extension_package::ExtensionPackageArtifactRef;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionRuntimeActionKind, ExtensionWorkspaceTabRendererDeclaration,
    };
    use uuid::Uuid;

    use crate::extension_runtime::{
        ExtensionBackendServiceProjection, ExtensionBundleProjection,
        ExtensionGeneratedOperationDispatch, ExtensionGeneratedOperationProjection,
        ExtensionGeneratedOperationProvenance, ExtensionGeneratedOperationVisibility,
        ExtensionInstallationProjection, ExtensionRuntimeActionProjection,
        ExtensionRuntimeProjection, ExtensionWorkspaceTabLoadabilityMode,
        ExtensionWorkspaceTabLoadabilityProjection, ExtensionWorkspaceTabProjection,
    };

    use super::{
        WorkspaceModuleOperationContext, WorkspaceModuleOperationDispatch, build_workspace_modules,
        build_workspace_modules_with_operation_context, validate_input_against_schema,
    };

    fn operation_catalog_runtime_action(
        extension_key: &str,
        operation_key: &str,
        action_key: &str,
        visibility: ExtensionGeneratedOperationVisibility,
    ) -> ExtensionGeneratedOperationProjection {
        ExtensionGeneratedOperationProjection {
            extension_key: extension_key.to_string(),
            extension_id: extension_key.to_string(),
            operation_key: operation_key.to_string(),
            description: "Read profile".to_string(),
            visibility,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "additionalProperties": false
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "profile": { "type": "object" }
                },
                "additionalProperties": true
            }),
            permission_summary: vec!["local.profile.read".to_string()],
            dispatch: ExtensionGeneratedOperationDispatch::RuntimeAction {
                action_key: action_key.to_string(),
            },
            provenance: ExtensionGeneratedOperationProvenance {
                capability_key: "profile".to_string(),
                exposure_key: "read".to_string(),
                generated_from: "capability_exposure".to_string(),
            },
        }
    }

    fn operation_catalog_backend_service(
        extension_key: &str,
        operation_key: &str,
        service_key: &str,
        route: &str,
        visibility: ExtensionGeneratedOperationVisibility,
    ) -> ExtensionGeneratedOperationProjection {
        ExtensionGeneratedOperationProjection {
            extension_key: extension_key.to_string(),
            extension_id: extension_key.to_string(),
            operation_key: operation_key.to_string(),
            description: "Search profile backend".to_string(),
            visibility,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({"type": "object"}),
            permission_summary: vec![format!("backend_service:{service_key}")],
            dispatch: ExtensionGeneratedOperationDispatch::BackendService {
                service_key: service_key.to_string(),
                route: route.to_string(),
            },
            provenance: ExtensionGeneratedOperationProvenance {
                capability_key: "profile".to_string(),
                exposure_key: "search".to_string(),
                generated_from: "capability_exposure".to_string(),
            },
        }
    }

    fn artifact_ref(extension_key: &str) -> ExtensionPackageArtifactRef {
        ExtensionPackageArtifactRef {
            artifact_id: Uuid::new_v4(),
            package_name: format!("@agentdash/{extension_key}"),
            package_version: "0.1.0".to_string(),
            asset_version: "0.1.0".to_string(),
            source_version: "0.1.0".to_string(),
            storage_ref: format!("extensions/{extension_key}.agentdash-extension.tgz"),
            archive_digest: "sha256:archive".to_string(),
            manifest_digest: "sha256:manifest".to_string(),
        }
    }

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
                kind: ExtensionRuntimeActionKind::RuntimeThread,
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

    #[test]
    fn runtime_actions_without_operation_catalog_are_not_agent_operations() {
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
                kind: ExtensionRuntimeActionKind::RuntimeThread,
                description: "Manifest description must not become executable metadata".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "object"}),
                permissions: vec!["manifest.permission".to_string()],
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
        assert!(modules[0].operations.is_empty());
        assert!(modules[0].summary.operation_summary.is_empty());
    }

    #[test]
    fn operation_catalog_runtime_action_requires_runtime_thread_gateway_descriptor() {
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
                kind: ExtensionRuntimeActionKind::RuntimeThread,
                description: "Manifest action".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "object"}),
                permissions: vec![],
            }],
            bundles: vec![ExtensionBundleProjection {
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: "sha256:bundle".to_string(),
            }],
            operation_catalog: vec![operation_catalog_runtime_action(
                "ops-demo",
                "profile.read",
                "ops-demo.run",
                ExtensionGeneratedOperationVisibility::AgentAndPanel,
            )],
            ..Default::default()
        };
        let setup_descriptor = RuntimeActionDescriptor {
            action_key: RuntimeActionKey::parse("ops-demo.run").expect("valid action key"),
            kind: RuntimeActionKind::Setup,
            description: Some("setup descriptor".to_string()),
            input_schema: Some(serde_json::json!({"type": "object"})),
            output_schema: Some(serde_json::json!({"type": "object"})),
            default_policy: RuntimePolicy::default(),
            metadata: Default::default(),
        };
        let context = WorkspaceModuleOperationContext::ready(vec![setup_descriptor]);

        let modules = build_workspace_modules_with_operation_context(&projection, &[], &context);

        let operation = modules[0]
            .operations
            .iter()
            .find(|operation| operation.operation_key == "profile.read")
            .expect("diagnostic operation");
        assert_eq!(
            operation.readiness.kind,
            WorkspaceModuleOperationReadinessKind::RuntimeActionUnavailable
        );
        assert_eq!(operation.description, "Read profile");
        assert_eq!(
            operation.visibility,
            WorkspaceModuleOperationVisibility::AgentAndPanel
        );
        assert_eq!(
            operation
                .input_schema
                .as_ref()
                .and_then(|schema| schema.get("type")),
            Some(&serde_json::json!("object"))
        );
    }

    #[test]
    fn operation_catalog_runtime_action_uses_generated_projection_metadata() {
        let action_key = "ops-demo.run";
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
                action_key: action_key.to_string(),
                kind: ExtensionRuntimeActionKind::RuntimeThread,
                description: "Manifest action metadata is not an operation fact".to_string(),
                input_schema: serde_json::json!(true),
                output_schema: serde_json::json!(true),
                permissions: vec!["manifest.permission".to_string()],
            }],
            bundles: vec![ExtensionBundleProjection {
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: "sha256:bundle".to_string(),
            }],
            operation_catalog: vec![operation_catalog_runtime_action(
                "ops-demo",
                "profile.read",
                action_key,
                ExtensionGeneratedOperationVisibility::AgentAndPanel,
            )],
            ..Default::default()
        };
        let descriptor = RuntimeActionDescriptor {
            action_key: RuntimeActionKey::parse(action_key).expect("valid action key"),
            kind: RuntimeActionKind::RuntimeThread,
            description: Some("Gateway resolved descriptor".to_string()),
            input_schema: Some(serde_json::json!({"type": "object"})),
            output_schema: Some(serde_json::json!({"type": "object"})),
            default_policy: RuntimePolicy::default(),
            metadata: Default::default(),
        };
        let context = WorkspaceModuleOperationContext::ready(vec![descriptor]);

        let modules = build_workspace_modules_with_operation_context(&projection, &[], &context);

        let operation = modules[0]
            .operations
            .iter()
            .find(|operation| operation.operation_key == "profile.read")
            .expect("generated operation");
        assert_eq!(
            operation.readiness.kind,
            WorkspaceModuleOperationReadinessKind::Ready
        );
        assert_eq!(operation.origin, "runtime_action");
        assert_eq!(operation.description, "Read profile");
        assert_eq!(
            operation.permission_summary,
            vec!["local.profile.read".to_string()]
        );
        assert_eq!(
            operation.provenance.get("generated_from"),
            Some(&serde_json::json!("capability_exposure"))
        );
    }

    #[test]
    fn operation_catalog_backend_service_is_unavailable_without_bridge_transport() {
        let projection = ExtensionRuntimeProjection {
            installations: vec![ExtensionInstallationProjection {
                installation_id: Uuid::new_v4(),
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                display_name: "Ops Demo".to_string(),
                installed_source: None,
                package_artifact: Some(artifact_ref("ops-demo")),
            }],
            backend_services: vec![ExtensionBackendServiceProjection {
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                service_key: "profile-service".to_string(),
                runtime: "node".to_string(),
                entry: "dist/backend/server.mjs".to_string(),
                routes: vec!["/profiles/**".to_string()],
                health_path: Some("/health".to_string()),
            }],
            operation_catalog: vec![operation_catalog_backend_service(
                "ops-demo",
                "profile.search",
                "profile-service",
                "/profiles/search",
                ExtensionGeneratedOperationVisibility::AgentAndPanel,
            )],
            ..Default::default()
        };

        let operation_context = WorkspaceModuleOperationContext {
            backend_readiness: WorkspaceModuleOperationReadiness::ready(),
            backend_service_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable,
                "backendService bridge transport is not attached to this runtime",
            ),
            ..WorkspaceModuleOperationContext::default()
        };
        let modules =
            build_workspace_modules_with_operation_context(&projection, &[], &operation_context);

        let operation = modules[0]
            .operations
            .iter()
            .find(|operation| operation.operation_key == "profile.search")
            .expect("backend service operation");
        assert_eq!(
            operation.readiness.kind,
            WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable
        );
        assert!(matches!(
            &operation.dispatch,
            WorkspaceModuleOperationDispatch::BackendService { .. }
        ));
    }

    #[test]
    fn operation_catalog_backend_service_is_ready_with_backend_and_bridge() {
        let projection = ExtensionRuntimeProjection {
            installations: vec![ExtensionInstallationProjection {
                installation_id: Uuid::new_v4(),
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                display_name: "Ops Demo".to_string(),
                installed_source: None,
                package_artifact: Some(artifact_ref("ops-demo")),
            }],
            backend_services: vec![ExtensionBackendServiceProjection {
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                service_key: "profile-service".to_string(),
                runtime: "node".to_string(),
                entry: "dist/backend/server.mjs".to_string(),
                routes: vec!["/profiles/**".to_string()],
                health_path: Some("/health".to_string()),
            }],
            operation_catalog: vec![operation_catalog_backend_service(
                "ops-demo",
                "profile.search",
                "profile-service",
                "/profiles/search",
                ExtensionGeneratedOperationVisibility::AgentAndPanel,
            )],
            ..Default::default()
        };
        let context = WorkspaceModuleOperationContext::ready(Vec::new());

        let modules = build_workspace_modules_with_operation_context(&projection, &[], &context);

        assert_eq!(
            modules[0].summary.status.kind,
            WorkspaceModuleStatusKind::Ready
        );
        let operation = modules[0]
            .operations
            .iter()
            .find(|operation| operation.operation_key == "profile.search")
            .expect("backend service operation");
        assert_eq!(
            operation.readiness.kind,
            WorkspaceModuleOperationReadinessKind::Ready
        );
    }

    #[test]
    fn operation_catalog_backend_service_matches_absolute_route_pattern_by_path() {
        let projection = ExtensionRuntimeProjection {
            installations: vec![ExtensionInstallationProjection {
                installation_id: Uuid::new_v4(),
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                display_name: "Ops Demo".to_string(),
                installed_source: None,
                package_artifact: Some(artifact_ref("ops-demo")),
            }],
            backend_services: vec![ExtensionBackendServiceProjection {
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                service_key: "profile-service".to_string(),
                runtime: "node".to_string(),
                entry: "dist/backend/server.mjs".to_string(),
                routes: vec!["http://localhost:4510/profiles/**".to_string()],
                health_path: Some("/health".to_string()),
            }],
            operation_catalog: vec![operation_catalog_backend_service(
                "ops-demo",
                "profile.search",
                "profile-service",
                "/profiles/search?query=abc",
                ExtensionGeneratedOperationVisibility::AgentAndPanel,
            )],
            ..Default::default()
        };
        let context = WorkspaceModuleOperationContext::ready(Vec::new());

        let modules = build_workspace_modules_with_operation_context(&projection, &[], &context);

        let operation = modules[0]
            .operations
            .iter()
            .find(|operation| operation.operation_key == "profile.search")
            .expect("backend service operation");
        assert_eq!(
            operation.readiness.kind,
            WorkspaceModuleOperationReadinessKind::Ready
        );
    }

    #[test]
    fn operation_catalog_backend_service_rejects_route_mismatch() {
        let projection = ExtensionRuntimeProjection {
            installations: vec![ExtensionInstallationProjection {
                installation_id: Uuid::new_v4(),
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                display_name: "Ops Demo".to_string(),
                installed_source: None,
                package_artifact: Some(artifact_ref("ops-demo")),
            }],
            backend_services: vec![ExtensionBackendServiceProjection {
                extension_key: "ops-demo".to_string(),
                extension_id: "ops-demo".to_string(),
                service_key: "profile-service".to_string(),
                runtime: "node".to_string(),
                entry: "dist/backend/server.mjs".to_string(),
                routes: vec!["/profiles/**".to_string()],
                health_path: None,
            }],
            operation_catalog: vec![operation_catalog_backend_service(
                "ops-demo",
                "profile.search",
                "profile-service",
                "/admin/search",
                ExtensionGeneratedOperationVisibility::AgentAndPanel,
            )],
            ..Default::default()
        };

        let modules = build_workspace_modules_with_operation_context(
            &projection,
            &[],
            &WorkspaceModuleOperationContext::ready(Vec::new()),
        );

        let operation = modules[0]
            .operations
            .iter()
            .find(|operation| operation.operation_key == "profile.search")
            .expect("backend service operation");
        assert_eq!(
            operation.readiness.kind,
            WorkspaceModuleOperationReadinessKind::BackendServiceUnavailable
        );
    }
}
