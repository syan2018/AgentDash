//! Workspace Module 聚合层。
//!
//! 把 enabled extension（复用 `ExtensionRuntimeProjection`）+ visible canvas
//! 聚合为单一 workspace module read model。该层只做投影转换，不新建业务事实
//! 源——所有数据来自现成的 `extension_runtime` 投影与 `Canvas` 实体。
//!
//! 决策对齐：
//! - D3：`protocol_channels[].methods[]` 投影为其 provider extension module 的
//!   operation（`origin = "protocol_channel"`），不单独成 module。
//! - module_id 约定见 §4：`ext:{extension_key}` / `canvas:{mount_id}` /
//!   `builtin:{key}`。

pub(crate) mod skill_projection;
pub mod runtime_tool_provider;
mod tools;

use agentdash_contracts::workspace_module::{
    WorkspaceModuleCanvasHostAction, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationDispatch, WorkspaceModulePresentation,
    WorkspaceModuleStatus, WorkspaceModuleSummary, WorkspaceModuleUiEntry,
};
use agentdash_domain::canvas::Canvas;
use agentdash_domain::shared_library::{
    ExtensionRuntimeActionKind, ExtensionWorkspaceTabRendererDeclaration,
};

use crate::extension_runtime::ExtensionRuntimeProjection;
use crate::runtime_gateway::ExtensionInvocationWorkspaceContext;
use agentdash_domain::common::Vfs;
use thiserror::Error;

pub use tools::{
    WorkspaceModuleCreateTool, WorkspaceModuleDescribeTool, WorkspaceModuleInvokeTool,
    WorkspaceModuleListTool, WorkspaceModulePresentTool,
};
pub use runtime_tool_provider::WorkspaceModuleRuntimeToolProvider;

/// invoke 解析出的 backend target + workspace 上下文。
///
/// backend 解析优先级（design D2-3）：`session.backend_execution`（remote 显式）→
/// `vfs.default_mount().backend_id`（local）→ 都无则 `None`（调用方报"缺 backend"）。
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedInvocationBackend {
    pub backend_id: String,
    pub workspace: Option<ExtensionInvocationWorkspaceContext>,
}

/// 从 ExecutionContext 自推 backend + workspace（与 HTTP 侧
/// `select_extension_invocation_workspace` 等价的 application 层共享逻辑）。
///
/// - `explicit_backend_id`：来自 `session.backend_execution`（remote 已 claim 的 backend），
///   优先级最高；为 `None` 时回退到 vfs default mount 的 backend。
/// - workspace 取与 backend 匹配且 root_ref 非空的 mount（优先 default mount）。
pub fn resolve_invocation_backend(
    vfs: Option<&Vfs>,
    explicit_backend_id: Option<&str>,
) -> Option<ResolvedInvocationBackend> {
    let backend_id = match explicit_backend_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(id) => id.to_string(),
        None => {
            let mount = vfs?.default_mount()?;
            let id = mount.backend_id.trim();
            if id.is_empty() {
                return None;
            }
            id.to_string()
        }
    };
    let workspace = vfs.and_then(|vfs| select_invocation_workspace(vfs, &backend_id));
    Some(ResolvedInvocationBackend {
        backend_id,
        workspace,
    })
}

/// 选 backend 对应的 workspace mount（优先 default mount，再退第一个匹配 mount）。
/// 与 `routes/extension_runtime.rs::select_extension_invocation_workspace` 同款规则。
fn select_invocation_workspace(
    vfs: &Vfs,
    backend_id: &str,
) -> Option<ExtensionInvocationWorkspaceContext> {
    let backend_id = backend_id.trim();
    if backend_id.is_empty() {
        return None;
    }
    if let Some(default_mount_id) = vfs.default_mount_id.as_deref()
        && let Some(mount) = vfs.mounts.iter().find(|mount| {
            mount.id == default_mount_id
                && mount.backend_id == backend_id
                && !mount.root_ref.trim().is_empty()
        })
    {
        return Some(ExtensionInvocationWorkspaceContext::new(
            mount.id.clone(),
            mount.root_ref.trim().to_string(),
        ));
    }
    vfs.mounts
        .iter()
        .find(|mount| mount.backend_id == backend_id && !mount.root_ref.trim().is_empty())
        .map(|mount| {
            ExtensionInvocationWorkspaceContext::new(
                mount.id.clone(),
                mount.root_ref.trim().to_string(),
            )
        })
}

/// 轻量 input schema 校验（无外部 jsonschema 依赖）。
///
/// 覆盖 describe 出的 operation `input_schema` 的常见形态：顶层 `type` 与 `required`。
/// 校验范围刻意保守——只拦截"类型大类不符"与"缺必填字段"这类明确违例，避免引入完整
/// JSON Schema 运行时依赖。describe 暴露的 schema 与此校验成对（PRD 风险条）。
/// 返回 `Err(reason)` 表示 input 不满足 schema。
pub fn validate_input_against_schema(
    schema: &serde_json::Value,
    input: &serde_json::Value,
) -> Result<(), String> {
    let serde_json::Value::Object(schema_obj) = schema else {
        // 非 object schema（如 `true`/空）不约束。
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
        // 未知 type 关键字不约束。
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

/// module_id 前缀约定。
pub const MODULE_ID_EXTENSION_PREFIX: &str = "ext:";
pub const MODULE_ID_CANVAS_PREFIX: &str = "canvas:";
pub const MODULE_ID_BUILTIN_PREFIX: &str = "builtin:";

/// 聚合 enabled extension + visible canvas 为单一 module descriptor 列表。
///
/// builtin module 预留：本轮先空（参数占位由调用方决定，暂不接收 builtin 输入）。
pub fn build_workspace_modules(
    ext: &ExtensionRuntimeProjection,
    canvases: &[Canvas],
) -> Vec<WorkspaceModuleDescriptor> {
    let mut modules = Vec::new();
    modules.extend(build_extension_modules(ext));
    modules.extend(canvases.iter().map(build_canvas_module));
    modules
}

pub fn build_canvas_workspace_module(canvas: &Canvas) -> WorkspaceModuleDescriptor {
    build_canvas_module(canvas)
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

            // runtime_actions → operations（origin = runtime_action）
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

            // protocol_channels[].methods[] → operations（origin = protocol_channel，D3）
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

            // workspace_tabs → ui_entries
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

            // permissions → permission_summary
            let permission_summary: Vec<String> = ext
                .permissions
                .iter()
                .filter(|permission| permission.extension_key == extension_key)
                .map(|permission| describe_permission(&permission.permission))
                .collect();

            // bundles 缺失 → status unavailable
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

fn build_canvas_module(canvas: &Canvas) -> WorkspaceModuleDescriptor {
    let operations: Vec<WorkspaceModuleOperation> = vec![WorkspaceModuleOperation {
        operation_key: "canvas.bind_data".to_string(),
        origin: "host_canvas".to_string(),
        description: "Declare or update a data binding for this Canvas instance.".to_string(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "alias": {
                    "type": "string",
                    "description": "Runtime binding alias, exposed as bindings/<alias>.json"
                },
                "source_uri": {
                    "type": "string",
                    "description": "Source resource URI to bind into the Canvas runtime"
                },
                "content_type": {
                    "type": "string",
                    "description": "Optional content type, defaults to application/json"
                }
            },
            "required": ["alias", "source_uri"],
            "additionalProperties": false
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "canvas_id": {"type": "string"},
                "mount_id": {"type": "string"},
                "bindings": {"type": "array"}
            },
            "required": ["canvas_id", "mount_id", "bindings"]
        })),
        permission_summary: Vec::new(),
        dispatch: WorkspaceModuleOperationDispatch::HostCanvas {
            canvas_action: WorkspaceModuleCanvasHostAction::BindData,
        },
    }];

    // entry/files → ui_entry(canvas)
    let ui_entries = vec![WorkspaceModuleUiEntry {
        view_key: "preview".to_string(),
        renderer_kind: "canvas".to_string(),
        presentation_uri: Some(format!("canvas://{}", canvas.mount_id)),
        uri_scheme: Some("canvas".to_string()),
        title: canvas.title.clone(),
    }];

    let operation_summary = operations
        .iter()
        .map(|operation| operation.operation_key.clone())
        .collect::<Vec<_>>();

    let summary = WorkspaceModuleSummary {
        module_id: format!("{MODULE_ID_CANVAS_PREFIX}{}", canvas.mount_id),
        kind: WorkspaceModuleKind::Canvas,
        title: canvas.title.clone(),
        description: canvas.description.clone(),
        source: canvas.mount_id.clone(),
        ui_summary: ui_summary(ui_entries.len()),
        operation_summary,
        status: WorkspaceModuleStatus::ready(),
        permission_summary: Vec::new(),
    };

    WorkspaceModuleDescriptor {
        summary,
        ui_entries,
        operations,
        runtime_backing: Some(format!(
            "canvas:{}",
            crate::vfs::build_canvas_mount_id(canvas)
        )),
    }
}

fn runtime_action_origin(_kind: &ExtensionRuntimeActionKind) -> &'static str {
    "runtime_action"
}

fn tab_renderer_kind(renderer: &ExtensionWorkspaceTabRendererDeclaration) -> &'static str {
    match renderer {
        ExtensionWorkspaceTabRendererDeclaration::Webview { .. } => "webview",
        ExtensionWorkspaceTabRendererDeclaration::CanvasPanel { .. } => "canvas",
    }
}

fn ui_summary(count: usize) -> Option<String> {
    if count == 0 {
        None
    } else {
        Some(format!("{count} UI entry"))
    }
}

fn describe_permission(
    permission: &agentdash_domain::shared_library::ExtensionPermissionDeclaration,
) -> String {
    use agentdash_domain::shared_library::ExtensionPermissionDeclaration as P;
    match permission {
        P::LocalProfile { access } => format!("local.profile:{access:?}"),
        P::Http { hosts, access } => format!("http[{}]:{access:?}", hosts.join(",")),
        P::Workspace { access } => format!("workspace:{access:?}"),
        P::Env { names, access } => format!("env[{}]:{access:?}", names.join(",")),
        P::Process { access } => format!("process:{access:?}"),
        P::RuntimeAction { action_key } => format!("runtime_action:{action_key}"),
        P::ExtensionChannel {
            channel_key,
            methods,
        } => format!("channel:{channel_key}[{}]", methods.join(",")),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_contracts::workspace_module::WorkspaceModuleStatusKind;
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionPermissionAccess,
        ExtensionPermissionDeclaration, ExtensionProtocolChannelDefinition,
        ExtensionProtocolChannelMethodDefinition, ExtensionRuntimeActionDefinition,
        ExtensionRuntimeActionKind, ExtensionTemplatePayload, ExtensionWorkspaceTabDefinition,
        ExtensionWorkspaceTabRendererDeclaration, InstalledAssetSource,
        ProjectExtensionInstallation,
    };
    use uuid::Uuid;

    use super::*;
    use crate::canvas::build_canvas;
    use crate::extension_runtime::extension_runtime_projection_from_installations;

    fn source() -> InstalledAssetSource {
        InstalledAssetSource::new(
            Uuid::new_v4(),
            "integration:test:extension_template:demo",
            "0.1.0",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
    }

    fn manifest(extension_id: &str, with_bundle: bool) -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: extension_id.to_string(),
            package: ExtensionPackageMetadata {
                name: extension_id.to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![ExtensionRuntimeActionDefinition {
                action_key: format!("{extension_id}.profile"),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "read profile".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "object"}),
                permissions: vec!["local.profile.read".to_string()],
            }],
            protocol_channels: vec![ExtensionProtocolChannelDefinition {
                channel_key: format!("{extension_id}.api"),
                version: "1.0.0".to_string(),
                description: "demo API channel".to_string(),
                methods: vec![ExtensionProtocolChannelMethodDefinition {
                    name: "readProfile".to_string(),
                    description: "read profile through channel".to_string(),
                    input_schema: serde_json::json!({}),
                    output_schema: serde_json::json!({}),
                    permissions: vec!["local.profile.read".to_string()],
                }],
            }],
            extension_dependencies: vec![],
            workspace_tabs: vec![ExtensionWorkspaceTabDefinition {
                type_id: format!("{extension_id}.profile-panel"),
                label: "Profile".to_string(),
                uri_scheme: format!("{extension_id}-panel"),
                renderer: ExtensionWorkspaceTabRendererDeclaration::Webview {
                    entry: "dist/panel/index.html".to_string(),
                },
            }],
            permissions: vec![ExtensionPermissionDeclaration::LocalProfile {
                access: ExtensionPermissionAccess::Read,
            }],
            bundles: if with_bundle {
                vec![ExtensionBundleRef {
                    kind: ExtensionBundleKind::ExtensionHost,
                    entry: "dist/extension.js".to_string(),
                    digest:
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                            .to_string(),
                }]
            } else {
                vec![]
            },
        }
    }

    fn installation(key: &str, with_bundle: bool) -> ProjectExtensionInstallation {
        ProjectExtensionInstallation::new(
            Uuid::new_v4(),
            key,
            format!("{key} Extension"),
            manifest(key, with_bundle),
            source(),
        )
        .expect("valid installation")
    }

    #[test]
    fn aggregates_extension_and_canvas_modules() {
        let projection =
            extension_runtime_projection_from_installations(vec![installation("demo", true)])
                .expect("projection");
        let canvas = build_canvas(
            Uuid::new_v4(),
            Some("dashboard-a".to_string()),
            "Dashboard A".to_string(),
            "demo canvas".to_string(),
            Default::default(),
        )
        .expect("canvas");

        let modules = build_workspace_modules(&projection, std::slice::from_ref(&canvas));
        assert_eq!(modules.len(), 2);

        let extension = modules
            .iter()
            .find(|module| module.summary.kind == WorkspaceModuleKind::Extension)
            .expect("extension module");
        assert_eq!(extension.summary.module_id, "ext:demo");
        assert_eq!(extension.summary.source, "demo");

        // runtime_action + protocol_channel method 同列
        let origins: Vec<&str> = extension
            .operations
            .iter()
            .map(|operation| operation.origin.as_str())
            .collect();
        assert!(origins.contains(&"runtime_action"));
        assert!(origins.contains(&"protocol_channel"));
        let channel_op = extension
            .operations
            .iter()
            .find(|operation| operation.origin == "protocol_channel")
            .expect("channel-as-operation");
        assert_eq!(channel_op.operation_key, "demo.api.readProfile");
        // dispatch 结构化分量正确，channel_key 含点、method 保留原始驼峰
        assert_eq!(
            channel_op.dispatch,
            WorkspaceModuleOperationDispatch::ProtocolChannel {
                channel_key: "demo.api".to_string(),
                method_name: "readProfile".to_string(),
            }
        );
        let action_op = extension
            .operations
            .iter()
            .find(|operation| operation.origin == "runtime_action")
            .expect("action-as-operation");
        assert_eq!(
            action_op.dispatch,
            WorkspaceModuleOperationDispatch::RuntimeAction {
                action_key: "demo.profile".to_string(),
            }
        );

        // workspace tab → ui entry
        assert_eq!(extension.ui_entries.len(), 1);
        assert_eq!(extension.ui_entries[0].renderer_kind, "webview");

        let canvas_module = modules
            .iter()
            .find(|module| module.summary.kind == WorkspaceModuleKind::Canvas)
            .expect("canvas module");
        assert_eq!(canvas_module.summary.module_id, "canvas:dashboard-a");
        let canvas_op = canvas_module
            .operations
            .iter()
            .find(|operation| operation.operation_key == "canvas.bind_data")
            .expect("canvas bind operation");
        assert_eq!(canvas_op.origin, "host_canvas");
        assert_eq!(
            canvas_op.dispatch,
            WorkspaceModuleOperationDispatch::HostCanvas {
                canvas_action: WorkspaceModuleCanvasHostAction::BindData,
            }
        );
        assert_eq!(canvas_module.ui_entries.len(), 1);
        assert_eq!(canvas_module.ui_entries[0].renderer_kind, "canvas");
        assert_eq!(canvas_module.ui_entries[0].view_key, "preview");
        assert_eq!(
            canvas_module.ui_entries[0].presentation_uri.as_deref(),
            Some("canvas://dashboard-a")
        );
    }

    #[test]
    fn missing_bundle_marks_module_unavailable() {
        let projection =
            extension_runtime_projection_from_installations(vec![installation("nobundle", false)])
                .expect("projection");
        let modules = build_workspace_modules(&projection, &[]);
        assert_eq!(modules.len(), 1);
        assert_eq!(
            modules[0].summary.status.kind,
            WorkspaceModuleStatusKind::Unavailable
        );
        assert!(modules[0].summary.status.reason.is_some());
    }
}
