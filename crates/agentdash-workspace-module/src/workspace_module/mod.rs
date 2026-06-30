pub mod runtime_bridge;
pub mod runtime_tool_provider;
mod tools;
pub mod visibility;

use std::collections::BTreeMap;

use agentdash_application_runtime_gateway::{
    EXTENSION_RUNTIME_DESCRIPTOR_EXTENSION_KEY_METADATA, RuntimeActionDescriptor,
    RuntimeActionKind, validate_json_schema_subset,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch, WorkspaceModuleOperationReadiness,
    WorkspaceModuleOperationReadinessKind, WorkspaceModulePresentation, WorkspaceModuleStatus,
    WorkspaceModuleSummary, WorkspaceModuleUiEntry,
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
                "runtime action is not present in the RuntimeGateway actor/context catalog",
            ),
        }
    }

    pub fn missing_runtime_gateway(reason: impl Into<String>) -> Self {
        Self {
            descriptors: Vec::new(),
            missing_descriptor_readiness: WorkspaceModuleOperationReadiness::unavailable(
                WorkspaceModuleOperationReadinessKind::MissingRuntimeGateway,
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
}

impl Default for WorkspaceModuleRuntimeActionCatalog {
    fn default() -> Self {
        Self::missing_runtime_gateway(
            "RuntimeGateway catalog is not attached to this workspace module projection",
        )
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceModuleOperationContext {
    pub runtime_actions: WorkspaceModuleRuntimeActionCatalog,
    pub channel_readiness: WorkspaceModuleOperationReadiness,
    pub backend_readiness: WorkspaceModuleOperationReadiness,
}

impl WorkspaceModuleOperationContext {
    pub fn ready(runtime_actions: Vec<RuntimeActionDescriptor>) -> Self {
        Self {
            runtime_actions: WorkspaceModuleRuntimeActionCatalog::from_descriptors(runtime_actions),
            channel_readiness: WorkspaceModuleOperationReadiness::ready(),
            backend_readiness: WorkspaceModuleOperationReadiness::ready(),
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
    let action_owner_by_key = unique_runtime_action_owners(ext);
    let duplicate_action_keys = duplicate_runtime_action_keys(ext);
    let descriptor_action_keys = operation_context
        .runtime_actions
        .descriptors
        .iter()
        .filter(|descriptor| descriptor.kind == RuntimeActionKind::SessionRuntime)
        .filter(|descriptor| !duplicate_action_keys.contains(descriptor.action_key.as_str()))
        .map(|descriptor| descriptor.action_key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut runtime_operations_by_extension: BTreeMap<String, Vec<WorkspaceModuleOperation>> =
        BTreeMap::new();
    for descriptor in &operation_context.runtime_actions.descriptors {
        if descriptor.kind != RuntimeActionKind::SessionRuntime {
            continue;
        }
        if duplicate_action_keys.contains(descriptor.action_key.as_str()) {
            continue;
        }
        let Some(extension_key) = descriptor_extension_key(descriptor).or_else(|| {
            action_owner_by_key
                .get(descriptor.action_key.as_str())
                .copied()
        }) else {
            continue;
        };
        if !ext
            .installations
            .iter()
            .any(|installation| installation.extension_key == extension_key)
        {
            continue;
        }
        runtime_operations_by_extension
            .entry(extension_key.to_string())
            .or_default()
            .push(runtime_action_operation_from_descriptor(
                descriptor,
                &operation_context.backend_readiness,
            ));
    }

    ext.installations
        .iter()
        .map(|installation| {
            let extension_key = installation.extension_key.as_str();
            let mut operations: Vec<WorkspaceModuleOperation> = runtime_operations_by_extension
                .get(extension_key)
                .cloned()
                .unwrap_or_default();

            operations.extend(
                ext.runtime_actions
                    .iter()
                    .filter(|action| action.extension_key == extension_key)
                    .filter(|action| !descriptor_action_keys.contains(action.action_key.as_str()))
                    .map(|action| {
                        let readiness = if duplicate_action_keys.contains(action.action_key.as_str())
                        {
                            WorkspaceModuleOperationReadiness::unavailable(
                                WorkspaceModuleOperationReadinessKind::RuntimeActionUnavailable,
                                format!(
                                    "runtime action `{}` is declared by multiple enabled extensions",
                                    action.action_key
                                ),
                            )
                        } else {
                            operation_context
                                .runtime_actions
                                .missing_descriptor_readiness
                                .clone()
                        };
                        unavailable_runtime_action_operation(
                            &action.action_key,
                            readiness,
                        )
                    }),
            );

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
                        readiness: first_unready_or_ready([
                            &operation_context.channel_readiness,
                            &operation_context.backend_readiness,
                        ]),
                    });
                }
            }

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

fn unique_runtime_action_owners(ext: &ExtensionRuntimeProjection) -> BTreeMap<&str, &str> {
    let duplicate_keys = duplicate_runtime_action_keys(ext);
    let mut owners = BTreeMap::new();
    for action in &ext.runtime_actions {
        if duplicate_keys.contains(action.action_key.as_str()) {
            continue;
        }
        owners
            .entry(action.action_key.as_str())
            .or_insert(action.extension_key.as_str());
    }
    owners
}

fn duplicate_runtime_action_keys(
    ext: &ExtensionRuntimeProjection,
) -> std::collections::BTreeSet<&str> {
    let mut seen_keys = std::collections::BTreeSet::new();
    let mut duplicate_keys = std::collections::BTreeSet::new();
    for action in &ext.runtime_actions {
        if !seen_keys.insert(action.action_key.as_str()) {
            duplicate_keys.insert(action.action_key.as_str());
        }
    }
    duplicate_keys
}

fn descriptor_extension_key(descriptor: &RuntimeActionDescriptor) -> Option<&str> {
    descriptor
        .metadata
        .get(EXTENSION_RUNTIME_DESCRIPTOR_EXTENSION_KEY_METADATA)?
        .as_str()
}

fn runtime_action_operation_from_descriptor(
    descriptor: &RuntimeActionDescriptor,
    backend_readiness: &WorkspaceModuleOperationReadiness,
) -> WorkspaceModuleOperation {
    let action_key = descriptor.action_key.as_str().to_string();
    WorkspaceModuleOperation {
        operation_key: action_key.clone(),
        origin: runtime_action_origin(descriptor.kind).to_string(),
        description: descriptor
            .description
            .clone()
            .unwrap_or_else(|| format!("Runtime action `{action_key}`")),
        input_schema: descriptor.input_schema.clone(),
        output_schema: descriptor.output_schema.clone(),
        permission_summary: descriptor.default_policy.required_capabilities.clone(),
        dispatch: WorkspaceModuleOperationDispatch::RuntimeAction { action_key },
        readiness: first_unready_or_ready([backend_readiness]),
    }
}

fn unavailable_runtime_action_operation(
    action_key: &str,
    readiness: WorkspaceModuleOperationReadiness,
) -> WorkspaceModuleOperation {
    WorkspaceModuleOperation {
        operation_key: action_key.to_string(),
        origin: "runtime_action".to_string(),
        description: format!(
            "Runtime action `{action_key}` is not available from the current RuntimeGateway catalog."
        ),
        input_schema: None,
        output_schema: None,
        permission_summary: Vec::new(),
        dispatch: WorkspaceModuleOperationDispatch::RuntimeAction {
            action_key: action_key.to_string(),
        },
        readiness,
    }
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

fn runtime_action_origin(kind: RuntimeActionKind) -> &'static str {
    match kind {
        RuntimeActionKind::SessionRuntime => "runtime_action",
        RuntimeActionKind::Setup => "setup_action",
    }
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
    use agentdash_application_runtime_gateway::{
        EXTENSION_RUNTIME_DESCRIPTOR_EXTENSION_KEY_METADATA, RuntimeActionDescriptor,
        RuntimeActionKey, RuntimeActionKind, RuntimePolicy,
    };
    use agentdash_contracts::workspace_module::{
        WorkspaceModuleOperationReadinessKind, WorkspaceModuleStatusKind,
    };
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

    use super::{
        WorkspaceModuleOperationContext, build_workspace_modules,
        build_workspace_modules_with_operation_context, validate_input_against_schema,
    };

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

    #[test]
    fn projection_runtime_actions_without_gateway_are_diagnostic_only() {
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

        let operation = modules[0]
            .operations
            .iter()
            .find(|operation| operation.operation_key == "ops-demo.run")
            .expect("diagnostic operation");
        assert_eq!(
            operation.readiness.kind,
            WorkspaceModuleOperationReadinessKind::MissingRuntimeGateway
        );
        assert!(operation.input_schema.is_none());
        assert!(operation.output_schema.is_none());
        assert!(operation.permission_summary.is_empty());
    }

    #[test]
    fn non_session_gateway_descriptor_does_not_mask_runtime_action_diagnostic() {
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
            .find(|operation| operation.operation_key == "ops-demo.run")
            .expect("diagnostic operation");
        assert_eq!(
            operation.readiness.kind,
            WorkspaceModuleOperationReadinessKind::RuntimeActionUnavailable
        );
        assert_eq!(
            operation.description,
            "Runtime action `ops-demo.run` is not available from the current RuntimeGateway catalog."
        );
        assert!(operation.input_schema.is_none());
    }

    #[test]
    fn duplicate_runtime_action_key_does_not_mark_last_owner_ready() {
        let action_key = "shared.profile";
        let projection = ExtensionRuntimeProjection {
            installations: vec![
                ExtensionInstallationProjection {
                    installation_id: Uuid::new_v4(),
                    extension_key: "first-demo".to_string(),
                    extension_id: "first-demo".to_string(),
                    display_name: "First Demo".to_string(),
                    installed_source: None,
                    package_artifact: None,
                },
                ExtensionInstallationProjection {
                    installation_id: Uuid::new_v4(),
                    extension_key: "last-demo".to_string(),
                    extension_id: "last-demo".to_string(),
                    display_name: "Last Demo".to_string(),
                    installed_source: None,
                    package_artifact: None,
                },
            ],
            runtime_actions: vec![
                ExtensionRuntimeActionProjection {
                    extension_key: "first-demo".to_string(),
                    extension_id: "first-demo".to_string(),
                    action_key: action_key.to_string(),
                    kind: ExtensionRuntimeActionKind::SessionRuntime,
                    description: "First manifest action".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "object"}),
                    permissions: vec![],
                },
                ExtensionRuntimeActionProjection {
                    extension_key: "last-demo".to_string(),
                    extension_id: "last-demo".to_string(),
                    action_key: action_key.to_string(),
                    kind: ExtensionRuntimeActionKind::SessionRuntime,
                    description: "Last manifest action".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "object"}),
                    permissions: vec![],
                },
            ],
            bundles: vec![
                ExtensionBundleProjection {
                    extension_key: "first-demo".to_string(),
                    extension_id: "first-demo".to_string(),
                    kind: ExtensionBundleKind::ExtensionHost,
                    entry: "dist/extension.js".to_string(),
                    digest: "sha256:first".to_string(),
                },
                ExtensionBundleProjection {
                    extension_key: "last-demo".to_string(),
                    extension_id: "last-demo".to_string(),
                    kind: ExtensionBundleKind::ExtensionHost,
                    entry: "dist/extension.js".to_string(),
                    digest: "sha256:last".to_string(),
                },
            ],
            ..Default::default()
        };
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert(
            EXTENSION_RUNTIME_DESCRIPTOR_EXTENSION_KEY_METADATA.to_string(),
            serde_json::json!("first-demo"),
        );
        let descriptor = RuntimeActionDescriptor {
            action_key: RuntimeActionKey::parse(action_key).expect("valid action key"),
            kind: RuntimeActionKind::SessionRuntime,
            description: Some("Gateway resolved descriptor".to_string()),
            input_schema: Some(serde_json::json!({"type": "object"})),
            output_schema: Some(serde_json::json!({"type": "object"})),
            default_policy: RuntimePolicy::default(),
            metadata,
        };
        let context = WorkspaceModuleOperationContext::ready(vec![descriptor]);

        let modules = build_workspace_modules_with_operation_context(&projection, &[], &context);

        let operations = modules
            .iter()
            .flat_map(|module| module.operations.iter())
            .filter(|operation| operation.operation_key == action_key)
            .collect::<Vec<_>>();
        assert_eq!(operations.len(), 2);
        assert!(operations.iter().all(|operation| {
            operation.readiness.kind
                == WorkspaceModuleOperationReadinessKind::RuntimeActionUnavailable
        }));
        assert!(operations.iter().all(|operation| {
            operation
                .readiness
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("declared by multiple enabled extensions"))
        }));
        assert!(
            operations
                .iter()
                .all(|operation| operation.description != "Gateway resolved descriptor")
        );
    }
}
