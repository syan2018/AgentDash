use agentdash_application_operation_gateway::{
    OperationActorKind, OperationDescriptor, OperationReadiness,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleOperation, WorkspaceModuleOperationEffect,
    WorkspaceModuleOperationProvenance, WorkspaceModuleOperationReadiness,
    WorkspaceModuleOperationRef, WorkspaceModuleOperationReplayPolicy,
    WorkspaceModuleOperationVisibility, WorkspaceModulePresentation,
};
use agentdash_domain::operation::{OperationEffect, OperationReplayPolicy};
use thiserror::Error;

pub fn workspace_module_operation_from_descriptor(
    operation: &OperationDescriptor,
) -> WorkspaceModuleOperation {
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
            view_key: view_key.to_owned(),
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
                view_key: view_key.to_owned(),
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
