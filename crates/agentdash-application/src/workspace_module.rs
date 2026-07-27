use agentdash_application_operation_gateway::{
    OperationActorKind, OperationDescriptor, OperationReadiness,
};
use agentdash_contracts::workspace_module::{
    WorkspaceModuleAgentStateProjection, WorkspaceModuleDescriptor, WorkspaceModuleKind,
    WorkspaceModuleOperation, WorkspaceModuleOperationEffect, WorkspaceModuleOperationProvenance,
    WorkspaceModuleOperationReadiness, WorkspaceModuleOperationRef,
    WorkspaceModuleOperationReplayPolicy, WorkspaceModuleOperationVisibility,
    WorkspaceModulePresentation, WorkspaceModuleStatus, WorkspaceModuleSummary,
    WorkspaceModuleUiEntry,
};
use agentdash_domain::interaction::{InteractionDefinitionRevision, InteractionInstance};
use agentdash_domain::operation::{OperationEffect, OperationReplayPolicy};
use thiserror::Error;

use crate::extension_runtime::ExtensionRuntimeProjection;

pub const MODULE_ID_EXTENSION_PREFIX: &str = "ext:";
pub const MODULE_ID_CANVAS_PREFIX: &str = "canvas:";
pub const MODULE_ID_INTERACTION_PREFIX: &str = "interaction:";
pub const MODULE_ID_BUILTIN_PREFIX: &str = "builtin:";

pub fn build_workspace_modules(
    extensions: &ExtensionRuntimeProjection,
    definitions: &[InteractionDefinitionRevision],
    operations: &[OperationDescriptor],
) -> Vec<WorkspaceModuleDescriptor> {
    let mut modules = build_extension_modules(extensions, operations);
    modules.extend(build_builtin_modules(operations));
    modules.extend(
        definitions
            .iter()
            .map(|revision| build_canvas_definition_module(revision, operations)),
    );
    modules.sort_by(|left, right| left.summary.module_id.cmp(&right.summary.module_id));
    modules
}

fn build_builtin_modules(
    operation_catalog: &[OperationDescriptor],
) -> Vec<WorkspaceModuleDescriptor> {
    let mut by_provider =
        std::collections::BTreeMap::<String, Vec<WorkspaceModuleOperation>>::new();
    for operation in operation_catalog
        .iter()
        .filter(|operation| operation.operation_ref.provider.namespace == "platform")
    {
        by_provider
            .entry(operation.operation_ref.provider.provider_key.clone())
            .or_default()
            .push(extension_operation(operation));
    }
    by_provider
        .into_iter()
        .map(|(provider_key, mut operations)| {
            operations.sort_by(|left, right| left.operation_key.cmp(&right.operation_key));
            let permission_summary = operations
                .iter()
                .flat_map(|operation| operation.permission_summary.iter().cloned())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            WorkspaceModuleDescriptor {
                summary: WorkspaceModuleSummary {
                    module_id: format!("{MODULE_ID_BUILTIN_PREFIX}{provider_key}"),
                    kind: WorkspaceModuleKind::Builtin,
                    title: builtin_module_title(&provider_key).to_string(),
                    description: format!(
                        "Native platform {provider_key} capabilities exposed as canonical Operations."
                    ),
                    source: provider_key.clone(),
                    ui_summary: None,
                    operation_summary: operations
                        .iter()
                        .map(|operation| operation.operation_key.clone())
                        .collect(),
                    permission_summary,
                    status: WorkspaceModuleStatus::ready(),
                },
                ui_entries: Vec::new(),
                operations,
                runtime_backing: Some(format!("platform_tool_broker:{provider_key}")),
                agent_state_projection: None,
            }
        })
        .collect()
}

fn builtin_module_title(provider_key: &str) -> &str {
    match provider_key {
        "vfs" => "Workspace Files",
        "process" => "Workspace Process",
        "task" => "Project Tasks",
        _ => "Platform Tools",
    }
}

fn build_canvas_definition_module(
    revision: &InteractionDefinitionRevision,
    operation_catalog: &[OperationDescriptor],
) -> WorkspaceModuleDescriptor {
    let definition_id = revision.definition_id.to_string();
    let operations = operation_catalog
        .iter()
        .filter(|operation| {
            operation.operation_ref.provider.namespace == "interaction"
                && operation
                    .operation_ref
                    .provider
                    .provider_key
                    .starts_with(&format!("{definition_id}."))
        })
        .map(extension_operation)
        .collect::<Vec<_>>();
    WorkspaceModuleDescriptor {
        summary: WorkspaceModuleSummary {
            module_id: format!("{MODULE_ID_CANVAS_PREFIX}{definition_id}"),
            kind: WorkspaceModuleKind::Canvas,
            title: revision.title.clone(),
            description: revision.description.clone(),
            source: definition_id.clone(),
            ui_summary: Some("1 view".to_string()),
            operation_summary: operations
                .iter()
                .map(|operation| operation.operation_key.clone())
                .collect(),
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
        operations,
        runtime_backing: Some(format!("interaction_definition:{}", revision.revision_id)),
        agent_state_projection: None,
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
                agent_state_projection: None,
            }
        })
        .collect()
}

pub fn build_interaction_runtime_module(
    instance: &InteractionInstance,
    revision: &InteractionDefinitionRevision,
    operation_catalog: &[OperationDescriptor],
) -> Result<WorkspaceModuleDescriptor, agentdash_domain::interaction::InteractionError> {
    let definition_id = revision.definition_id.to_string();
    let provider_key = format!("{definition_id}.{}", revision.revision_id);
    let operations = operation_catalog
        .iter()
        .filter(|operation| {
            operation.operation_ref.provider.namespace == "interaction"
                && operation.operation_ref.provider.provider_key == provider_key
        })
        .map(extension_operation)
        .collect::<Vec<_>>();
    let projection = revision.agent_projection.project(&instance.state)?;
    Ok(WorkspaceModuleDescriptor {
        summary: WorkspaceModuleSummary {
            module_id: format!("{MODULE_ID_INTERACTION_PREFIX}{}", instance.id),
            kind: WorkspaceModuleKind::Interaction,
            title: revision.title.clone(),
            description: revision.description.clone(),
            source: instance.id.to_string(),
            ui_summary: Some("1 runtime view".to_string()),
            operation_summary: operations
                .iter()
                .map(|operation| operation.operation_key.clone())
                .collect(),
            permission_summary: Vec::new(),
            status: WorkspaceModuleStatus::ready(),
        },
        ui_entries: vec![WorkspaceModuleUiEntry {
            view_key: "runtime".to_string(),
            renderer_kind: "canvas".to_string(),
            presentation_uri: Some(format!("interaction://{}", instance.id)),
            uri_scheme: None,
            title: revision.title.clone(),
        }],
        operations,
        runtime_backing: Some(format!("interaction_instance:{}", instance.id)),
        agent_state_projection: Some(WorkspaceModuleAgentStateProjection {
            instance_id: instance.id.to_string(),
            definition_id: instance.definition_id.to_string(),
            definition_revision_id: instance.definition_revision_id.to_string(),
            state_revision: instance.state_revision,
            values: projection,
        }),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::interaction::{
        InteractionAgentProjection, InteractionOwner, InteractionRetention, SourceBundle,
        SourceFile, SourceSandboxConfig,
    };
    use uuid::Uuid;

    #[test]
    fn native_platform_operations_project_as_builtin_modules() {
        let operation_ref =
            agentdash_domain::operation::OperationRef::new("platform", "vfs", "fs_read", 1)
                .expect("operation ref");
        let operation = OperationDescriptor {
            operation_ref: operation_ref.clone(),
            title: "fs_read".to_string(),
            description: Some("Read a file".to_string()),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!(true),
            effect: OperationEffect::Read,
            replay_policy: OperationReplayPolicy::ReplaySafe,
            required_capabilities: std::collections::BTreeSet::from(["file_read".to_string()]),
            actor_visibility: std::collections::BTreeSet::from([OperationActorKind::Agent]),
            execution_policy:
                agentdash_application_operation_gateway::OperationExecutionPolicy::default(),
            readiness: OperationReadiness::Ready,
            provenance: agentdash_application_operation_gateway::OperationProvenance {
                source: "platform_tool_broker".to_string(),
                artifact_digest: None,
            },
            dispatch: agentdash_application_operation_gateway::OperationDispatch {
                provider: operation_ref.provider,
                route: "fs_read".to_string(),
            },
        };

        let modules = build_builtin_modules(&[operation]);

        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].summary.module_id, "builtin:vfs");
        assert_eq!(modules[0].summary.kind, WorkspaceModuleKind::Builtin);
        assert_eq!(modules[0].operations[0].operation_key, "fs_read");
    }

    #[test]
    fn interaction_runtime_module_projects_only_allowlisted_state() {
        let project_id = Uuid::new_v4();
        let mut revision = InteractionDefinitionRevision::new_canvas_v1(
            Uuid::new_v4(),
            1,
            project_id,
            InteractionOwner::Project(project_id),
            "Runtime",
            "",
            SourceBundle::new(
                "index.html",
                vec![SourceFile::new("index.html", "<main />", None).expect("file")],
                SourceSandboxConfig::default(),
            )
            .expect("bundle"),
            serde_json::json!({"public": {"value": 1}, "secret": "hidden"}),
            serde_json::json!({"type": "object"}),
            "user-1",
        )
        .expect("revision");
        revision.agent_projection = InteractionAgentProjection {
            version: 1,
            allowed_state_paths: vec!["/public/value".into()],
        };
        let instance = InteractionInstance::new_v1(
            InteractionOwner::Project(project_id),
            revision.definition_id,
            revision.revision_id,
            revision.initial_state.clone(),
            InteractionRetention { retain_until: None },
        )
        .expect("instance");

        let module =
            build_interaction_runtime_module(&instance, &revision, &[]).expect("runtime module");
        let projection = module.agent_state_projection.expect("projection");
        assert_eq!(
            module.summary.module_id,
            format!("interaction:{}", instance.id)
        );
        assert_eq!(projection.values["/public/value"], serde_json::json!(1));
        assert_eq!(projection.values.len(), 1);
    }
}
