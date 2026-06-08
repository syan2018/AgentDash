//! Workspace Module 聚合层。
//!
//! 把 enabled extension（复用 `ExtensionRuntimeProjection`）+ visible canvas
//! 聚合为单一 `WorkspaceModuleDescriptor` 契约。该层只做投影转换，不新建业务事实
//! 源——所有数据来自现成的 `extension_runtime` 投影与 `Canvas` 实体。
//!
//! 决策对齐：
//! - D3：`protocol_channels[].methods[]` 投影为其 provider extension module 的
//!   operation（`origin = "protocol_channel"`），不单独成 module。
//! - module_id 约定见 §4：`ext:{extension_key}` / `canvas:{mount_id}` /
//!   `builtin:{key}`。

mod tools;

use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleKind, WorkspaceModuleOperation, WorkspaceModuleStatus,
    WorkspaceModuleSummary, WorkspaceModuleUiEntry,
};
use agentdash_domain::canvas::Canvas;
use agentdash_domain::shared_library::{
    ExtensionRuntimeActionKind, ExtensionWorkspaceTabRendererDeclaration,
};

use crate::extension_runtime::ExtensionRuntimeProjection;

pub use tools::{WorkspaceModuleDescribeTool, WorkspaceModuleListTool};

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
    // canvas bindings → operations（origin = canvas，schema 先给最小）
    let operations: Vec<WorkspaceModuleOperation> = canvas
        .bindings
        .iter()
        .map(|binding| WorkspaceModuleOperation {
            operation_key: format!("binding.{}", binding.alias),
            origin: "canvas".to_string(),
            description: format!(
                "canvas data binding `{}` <- {} ({})",
                binding.alias, binding.source_uri, binding.content_type
            ),
            input_schema: None,
            output_schema: None,
            permission_summary: Vec::new(),
        })
        .collect();

    // entry/files → ui_entry(canvas)
    let ui_entries = vec![WorkspaceModuleUiEntry {
        view_key: canvas.entry_file.clone(),
        renderer_kind: "canvas".to_string(),
        uri_scheme: Some(crate::vfs::build_canvas_mount_id(canvas)),
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
        runtime_backing: Some(format!("canvas:{}", crate::vfs::build_canvas_mount_id(canvas))),
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

        // workspace tab → ui entry
        assert_eq!(extension.ui_entries.len(), 1);
        assert_eq!(extension.ui_entries[0].renderer_kind, "webview");

        let canvas_module = modules
            .iter()
            .find(|module| module.summary.kind == WorkspaceModuleKind::Canvas)
            .expect("canvas module");
        assert_eq!(canvas_module.summary.module_id, "canvas:dashboard-a");
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
            agentdash_contracts::workspace_module::WorkspaceModuleStatusKind::Unavailable
        );
        assert!(modules[0].summary.status.reason.is_some());
    }
}
