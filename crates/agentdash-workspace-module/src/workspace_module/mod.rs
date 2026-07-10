use agentdash_application_runtime_gateway::{
    OperationActorKind, OperationDescriptor, OperationReadiness,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleKind, WorkspaceModuleOperation,
    WorkspaceModuleOperationEffect, WorkspaceModuleOperationProvenance,
    WorkspaceModuleOperationReadiness, WorkspaceModuleOperationRef,
    WorkspaceModuleOperationReplayPolicy, WorkspaceModuleOperationVisibility,
    WorkspaceModulePresentation, WorkspaceModuleStatus, WorkspaceModuleSummary,
    WorkspaceModuleUiEntry,
};
use agentdash_domain::interaction::InteractionDefinitionRevision;
use agentdash_domain::operation::{OperationEffect, OperationReplayPolicy};
use thiserror::Error;

use crate::extension_runtime::ExtensionRuntimeProjection;

pub const MODULE_ID_EXTENSION_PREFIX: &str = "ext:";
pub const MODULE_ID_CANVAS_PREFIX: &str = "canvas:";

pub fn build_workspace_modules(
    extensions: &ExtensionRuntimeProjection,
    definitions: &[InteractionDefinitionRevision],
    operations: &[OperationDescriptor],
) -> Vec<WorkspaceModuleDescriptor> {
    let mut modules = build_extension_modules(extensions, operations);
    modules.extend(definitions.iter().map(build_canvas_definition_module));
    modules.sort_by(|left, right| left.summary.module_id.cmp(&right.summary.module_id));
    modules
}

fn build_canvas_definition_module(
    revision: &InteractionDefinitionRevision,
) -> WorkspaceModuleDescriptor {
    let definition_id = revision.definition_id.to_string();
    WorkspaceModuleDescriptor {
        summary: WorkspaceModuleSummary {
            module_id: format!("{MODULE_ID_CANVAS_PREFIX}{definition_id}"),
            kind: WorkspaceModuleKind::Canvas,
            title: revision.title.clone(),
            description: revision.description.clone(),
            source: definition_id.clone(),
            ui_summary: Some("1 view".to_string()),
            operation_summary: Vec::new(),
            permission_summary: Vec::new(),
            status: WorkspaceModuleStatus::ready(),
        },
        ui_entries: vec![WorkspaceModuleUiEntry {
            view_key: "preview".to_string(),
            renderer_kind: "canvas".to_string(),
            presentation_uri: Some(format!("canvas://{definition_id}")),
            uri_scheme: None,
            title: revision.title.clone(),
        }],
        operations: Vec::new(),
        runtime_backing: Some(format!("interaction_definition:{}", revision.revision_id)),
    }
}

fn build_extension_modules(
    projection: &ExtensionRuntimeProjection,
    operation_catalog: &[OperationDescriptor],
) -> Vec<WorkspaceModuleDescriptor> {
    projection
        .installations
        .iter()
        .map(|installation| {
            let extension_key = installation.extension_key.as_str();
            let ui_entries = projection
                .workspace_tabs
                .iter()
                .filter(|tab| tab.extension_key == extension_key && tab.loadability.available)
                .map(|tab| WorkspaceModuleUiEntry {
                    view_key: tab.type_id.clone(),
                    renderer_kind: match tab.renderer {
                        agentdash_domain::shared_library::ExtensionWorkspaceTabRendererDeclaration::Webview { .. } => "webview",
                        agentdash_domain::shared_library::ExtensionWorkspaceTabRendererDeclaration::CanvasPanel { .. } => "canvas_panel",
                    }
                    .to_string(),
                    presentation_uri: Some(format!("{}://panel", tab.uri_scheme)),
                    uri_scheme: Some(tab.uri_scheme.clone()),
                    title: tab.label.clone(),
                })
                .collect::<Vec<_>>();
            let operations = operation_catalog
                .iter()
                .filter(|operation| {
                    operation.operation_ref.provider.namespace == "extension"
                        && operation.operation_ref.provider.provider_key == extension_key
                })
                .map(extension_operation)
                .collect::<Vec<_>>();
            let operation_summary = operations
                .iter()
                .map(|operation| operation.operation_key.clone())
                .collect();
            let permission_summary = operations
                .iter()
                .flat_map(|operation| operation.permission_summary.iter().cloned())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            let status = if !ui_entries.is_empty()
                || operations.iter().any(|operation| operation.readiness.is_ready())
            {
                WorkspaceModuleStatus::ready()
            } else {
                let reason = operations
                    .iter()
                    .find_map(|operation| operation.readiness.message.clone())
                    .unwrap_or_else(|| {
                        "当前 UserWorkshop surface 没有可用 UI 或 Operation".to_string()
                    });
                WorkspaceModuleStatus::unavailable(reason)
            };
            WorkspaceModuleDescriptor {
                summary: WorkspaceModuleSummary {
                    module_id: format!("{MODULE_ID_EXTENSION_PREFIX}{extension_key}"),
                    kind: WorkspaceModuleKind::Extension,
                    title: installation.display_name.clone(),
                    description: installation.extension_id.clone(),
                    source: extension_key.to_string(),
                    ui_summary: (!ui_entries.is_empty())
                        .then(|| format!("{} views", ui_entries.len())),
                    operation_summary,
                    permission_summary,
                    status,
                },
                ui_entries,
                operations,
                runtime_backing: Some(format!("extension_runtime:{extension_key}")),
            }
        })
        .collect()
}

fn extension_operation(operation: &OperationDescriptor) -> WorkspaceModuleOperation {
    WorkspaceModuleOperation {
        operation_ref: WorkspaceModuleOperationRef {
            namespace: operation.operation_ref.provider.namespace.clone(),
            provider_key: operation.operation_ref.provider.provider_key.clone(),
            operation_key: operation.operation_ref.operation_key.clone(),
            contract_version: operation.operation_ref.contract_version,
        },
        operation_key: operation.operation_ref.operation_key.clone(),
        description: operation.description.clone().unwrap_or_default(),
        input_schema: Some(operation.input_schema.clone()),
        output_schema: Some(operation.output_schema.clone()),
        permission_summary: operation.required_capabilities.iter().cloned().collect(),
        visibility: if operation
            .actor_visibility
            .contains(&OperationActorKind::Agent)
        {
            WorkspaceModuleOperationVisibility::AgentAndPanel
        } else {
            WorkspaceModuleOperationVisibility::PanelOnly
        },
        effect: match operation.effect {
            OperationEffect::Read => WorkspaceModuleOperationEffect::Read,
            OperationEffect::LocalMutation => WorkspaceModuleOperationEffect::LocalMutation,
            OperationEffect::ExternalSideEffect => {
                WorkspaceModuleOperationEffect::ExternalSideEffect
            }
        },
        replay_policy: match operation.replay_policy {
            OperationReplayPolicy::NonReplayable => {
                WorkspaceModuleOperationReplayPolicy::NonReplayable
            }
            OperationReplayPolicy::Idempotent => WorkspaceModuleOperationReplayPolicy::Idempotent,
            OperationReplayPolicy::ReplaySafe => WorkspaceModuleOperationReplayPolicy::ReplaySafe,
        },
        provenance: WorkspaceModuleOperationProvenance {
            source: operation.provenance.source.clone(),
            artifact_digest: operation.provenance.artifact_digest.clone(),
        },
        readiness: match &operation.readiness {
            OperationReadiness::Ready => WorkspaceModuleOperationReadiness::ready(),
            OperationReadiness::Unavailable { code, message } => {
                WorkspaceModuleOperationReadiness::unavailable(code.clone(), message.clone())
            }
        },
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

pub fn build_workspace_module_presentation(
    module: &WorkspaceModuleDescriptor,
    view_key: &str,
    payload: Option<serde_json::Value>,
    diagnostics: Option<serde_json::Value>,
) -> Result<WorkspaceModulePresentation, WorkspaceModulePresentationError> {
    let entry = module
        .ui_entries
        .iter()
        .find(|entry| entry.view_key == view_key)
        .ok_or_else(|| WorkspaceModulePresentationError::ViewNotFound {
            module_id: module.summary.module_id.clone(),
            view_key: view_key.to_string(),
            available_views: module
                .ui_entries
                .iter()
                .map(|entry| entry.view_key.clone())
                .collect(),
        })?;
    let presentation_uri = entry
        .presentation_uri
        .clone()
        .or_else(|| {
            entry
                .uri_scheme
                .as_ref()
                .map(|scheme| format!("{scheme}://panel"))
        })
        .ok_or_else(
            || WorkspaceModulePresentationError::MissingPresentationUri {
                module_id: module.summary.module_id.clone(),
                view_key: view_key.to_string(),
                renderer_kind: entry.renderer_kind.clone(),
            },
        )?;
    Ok(WorkspaceModulePresentation {
        module_id: module.summary.module_id.clone(),
        view_key: entry.view_key.clone(),
        renderer_kind: entry.renderer_kind.clone(),
        presentation_uri,
        title: entry.title.clone(),
        payload,
        diagnostics,
    })
}
