use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_application::extension_runtime::extension_runtime_projection_from_installations;
use agentdash_application_operation_gateway::OperationDescriptor;
use agentdash_application_ports::product_runtime_tool::ProductRuntimeToolOutcome;
use agentdash_contracts::workspace_module::{
    WorkspaceModuleDescriptor, WorkspaceModuleKind, WorkspaceModuleOperation,
    WorkspaceModuleStatus, WorkspaceModuleSummary, WorkspaceModuleUiEntry,
};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use async_trait::async_trait;

use super::{
    WorkspaceModuleProvider, WorkspaceModuleProviderContext,
    workspace_module_operation_from_descriptor,
};

const MODULE_ID_EXTENSION_PREFIX: &str = "ext:";
const MODULE_ID_BUILTIN_PREFIX: &str = "builtin:";
const MODULE_ID_MCP_PREFIX: &str = "mcp:";

pub struct PlatformWorkspaceModuleProvider;

#[async_trait]
impl WorkspaceModuleProvider for PlatformWorkspaceModuleProvider {
    fn provider_key(&self) -> &str {
        "system.platform"
    }

    async fn modules(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<Vec<WorkspaceModuleDescriptor>, ProductRuntimeToolOutcome> {
        Ok(build_platform_and_mcp_modules(&context.operations))
    }

    fn owns_module(&self, module: &WorkspaceModuleDescriptor) -> bool {
        module
            .summary
            .module_id
            .starts_with(MODULE_ID_BUILTIN_PREFIX)
            || module.summary.module_id.starts_with(MODULE_ID_MCP_PREFIX)
    }
}

pub struct ExtensionWorkspaceModuleProvider {
    installations: Arc<dyn ProjectExtensionInstallationRepository>,
}

impl ExtensionWorkspaceModuleProvider {
    pub fn new(installations: Arc<dyn ProjectExtensionInstallationRepository>) -> Self {
        Self { installations }
    }

    async fn enabled_installations(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<
        Vec<agentdash_domain::shared_library::ProjectExtensionInstallation>,
        ProductRuntimeToolOutcome,
    > {
        self.installations
            .list_enabled_by_project(context.project_id)
            .await
            .map_err(|error| ProductRuntimeToolOutcome::Failed {
                code: "workspace_module_installation_query_failed".to_owned(),
                message: error.to_string(),
            })
    }
}

#[async_trait]
impl WorkspaceModuleProvider for ExtensionWorkspaceModuleProvider {
    fn provider_key(&self) -> &str {
        "system.extension"
    }

    async fn operation_capabilities(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<BTreeSet<String>, ProductRuntimeToolOutcome> {
        Ok(self
            .enabled_installations(context)
            .await?
            .into_iter()
            .filter(|installation| {
                context
                    .visibility
                    .allows(&format!("ext:{}", installation.extension_key))
            })
            .map(|installation| format!("extension:{}", installation.extension_key))
            .collect())
    }

    async fn modules(
        &self,
        context: &WorkspaceModuleProviderContext,
    ) -> Result<Vec<WorkspaceModuleDescriptor>, ProductRuntimeToolOutcome> {
        let projection = extension_runtime_projection_from_installations(
            self.enabled_installations(context).await?,
        )
        .map_err(|error| ProductRuntimeToolOutcome::Failed {
            code: "workspace_module_extension_projection_failed".to_owned(),
            message: error.to_string(),
        })?;
        Ok(build_extension_modules(&projection, &context.operations)
            .into_iter()
            .filter(|module| {
                module.summary.module_id.starts_with("ext:")
                    && context.visibility.allows(&module.summary.module_id)
            })
            .collect())
    }

    fn owns_module(&self, module: &WorkspaceModuleDescriptor) -> bool {
        module.summary.module_id.starts_with("ext:")
    }
}

fn build_platform_and_mcp_modules(
    operation_catalog: &[OperationDescriptor],
) -> Vec<WorkspaceModuleDescriptor> {
    let mut modules = build_platform_modules(operation_catalog);
    modules.extend(build_mcp_modules(operation_catalog));
    modules.sort_by(|left, right| left.summary.module_id.cmp(&right.summary.module_id));
    modules
}

fn build_platform_modules(
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
            .push(workspace_module_operation_from_descriptor(operation));
    }
    by_provider
        .into_iter()
        .map(|(provider_key, mut operations)| {
            operations.sort_by(|left, right| left.operation_key.cmp(&right.operation_key));
            let permission_summary = operation_permissions(&operations);
            WorkspaceModuleDescriptor {
                summary: WorkspaceModuleSummary {
                    module_id: format!("{MODULE_ID_BUILTIN_PREFIX}{provider_key}"),
                    kind: WorkspaceModuleKind::new("builtin"),
                    title: platform_module_title(&provider_key).to_owned(),
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

fn build_mcp_modules(operation_catalog: &[OperationDescriptor]) -> Vec<WorkspaceModuleDescriptor> {
    let mut by_server = std::collections::BTreeMap::<String, Vec<WorkspaceModuleOperation>>::new();
    for operation in operation_catalog
        .iter()
        .filter(|operation| operation.operation_ref.provider.namespace == "mcp")
    {
        by_server
            .entry(operation.operation_ref.provider.provider_key.clone())
            .or_default()
            .push(workspace_module_operation_from_descriptor(operation));
    }
    by_server
        .into_iter()
        .map(|(server_key, mut operations)| {
            operations.sort_by(|left, right| left.operation_key.cmp(&right.operation_key));
            let permission_summary = operation_permissions(&operations);
            WorkspaceModuleDescriptor {
                summary: WorkspaceModuleSummary {
                    module_id: format!("{MODULE_ID_MCP_PREFIX}{server_key}"),
                    kind: WorkspaceModuleKind::new("mcp"),
                    title: format!("MCP · {server_key}"),
                    description: format!(
                        "MCP server {server_key} exposed through canonical OperationGateway descriptors."
                    ),
                    source: server_key.clone(),
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
                runtime_backing: Some(format!("operation_gateway:mcp:{server_key}")),
                agent_state_projection: None,
            }
        })
        .collect()
}

fn platform_module_title(provider_key: &str) -> &str {
    match provider_key {
        "vfs" => "Workspace Files",
        "process" => "Workspace Process",
        "task" => "Project Tasks",
        _ => "Platform Tools",
    }
}

fn build_extension_modules(
    projection: &agentdash_application::extension_runtime::ExtensionRuntimeProjection,
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
                    .to_owned(),
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
                .map(workspace_module_operation_from_descriptor)
                .collect::<Vec<_>>();
            let operation_summary = operations
                .iter()
                .map(|operation| operation.operation_key.clone())
                .collect();
            let permission_summary = operation_permissions(&operations);
            let status = if !ui_entries.is_empty()
                || operations
                    .iter()
                    .any(|operation| operation.readiness.is_ready())
            {
                WorkspaceModuleStatus::ready()
            } else {
                let reason = operations
                    .iter()
                    .find_map(|operation| operation.readiness.message.clone())
                    .unwrap_or_else(|| {
                        "当前 actor surface 没有可用 UI 或 Operation".to_owned()
                    });
                WorkspaceModuleStatus::unavailable(reason)
            };
            WorkspaceModuleDescriptor {
                summary: WorkspaceModuleSummary {
                    module_id: format!("{MODULE_ID_EXTENSION_PREFIX}{extension_key}"),
                    kind: WorkspaceModuleKind::new("extension"),
                    title: installation.display_name.clone(),
                    description: installation.extension_id.clone(),
                    source: extension_key.to_owned(),
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

fn operation_permissions(operations: &[WorkspaceModuleOperation]) -> Vec<String> {
    operations
        .iter()
        .flat_map(|operation| operation.permission_summary.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_operation_gateway::{
        OperationActorKind, OperationDispatch, OperationExecutionPolicy, OperationProvenance,
        OperationReadiness,
    };
    use agentdash_domain::operation::{OperationEffect, OperationRef, OperationReplayPolicy};

    #[test]
    fn platform_operations_project_as_builtin_modules() {
        let operation_ref =
            OperationRef::new("platform", "vfs", "fs_read", 1).expect("operation ref");
        let operation = OperationDescriptor {
            operation_ref: operation_ref.clone(),
            title: "fs_read".to_owned(),
            description: Some("Read a file".to_owned()),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!(true),
            effect: OperationEffect::Read,
            replay_policy: OperationReplayPolicy::ReplaySafe,
            required_capabilities: BTreeSet::from(["file_read".to_owned()]),
            actor_visibility: BTreeSet::from([OperationActorKind::Agent]),
            execution_policy: OperationExecutionPolicy::default(),
            readiness: OperationReadiness::Ready,
            provenance: OperationProvenance {
                source: "platform_tool_broker".to_owned(),
                artifact_digest: None,
            },
            dispatch: OperationDispatch {
                provider: operation_ref.provider,
                route: "fs_read".to_owned(),
            },
        };

        let modules = build_platform_and_mcp_modules(&[operation]);

        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].summary.module_id, "builtin:vfs");
        assert_eq!(modules[0].summary.kind, WorkspaceModuleKind::new("builtin"));
        assert_eq!(modules[0].operations[0].operation_key, "fs_read");
    }

    #[test]
    fn mcp_operations_project_as_server_modules_with_exact_refs() {
        let operation_ref = OperationRef::new("mcp", "docs", "search", 1).expect("operation ref");
        let operation = OperationDescriptor {
            operation_ref: operation_ref.clone(),
            title: "search".to_owned(),
            description: Some("Search docs".to_owned()),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!(true),
            effect: OperationEffect::ExternalSideEffect,
            replay_policy: OperationReplayPolicy::NonReplayable,
            required_capabilities: BTreeSet::from(["mcp:docs".to_owned()]),
            actor_visibility: BTreeSet::from([OperationActorKind::Agent]),
            execution_policy: OperationExecutionPolicy::default(),
            readiness: OperationReadiness::Ready,
            provenance: OperationProvenance {
                source: "agent_frame.mcp_surface".to_owned(),
                artifact_digest: None,
            },
            dispatch: OperationDispatch {
                provider: operation_ref.provider,
                route: "docs/search".to_owned(),
            },
        };

        let modules = build_platform_and_mcp_modules(&[operation]);

        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].summary.module_id, "mcp:docs");
        assert_eq!(modules[0].summary.kind, WorkspaceModuleKind::new("mcp"));
        assert_eq!(
            modules[0].operations[0].operation_ref,
            agentdash_contracts::workspace_module::WorkspaceModuleOperationRef {
                namespace: "mcp".to_owned(),
                provider_key: "docs".to_owned(),
                operation_key: "search".to_owned(),
                contract_version: 1,
            }
        );
    }
}
