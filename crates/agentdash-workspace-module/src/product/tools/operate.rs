use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
};
use agentdash_contracts::workspace_module::WorkspaceModuleDescriptor;
use serde::Deserialize;
use serde_json::Value;

use crate::product::tool_service::{
    ApplicationWorkspaceModuleRuntimeToolService, completed, failed, rejected,
};
use crate::product::{WorkspaceModuleOperateRequest, WorkspaceModuleSurfaceEffect};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperateArguments {
    operation: String,
    #[serde(default)]
    input: Value,
}

impl ApplicationWorkspaceModuleRuntimeToolService {
    pub(crate) async fn execute_operate(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> ProductRuntimeToolOutcome {
        let arguments: OperateArguments = match serde_json::from_value(request.arguments.clone()) {
            Ok(arguments) => arguments,
            Err(error) => {
                return rejected(
                    "workspace_module_invalid_arguments",
                    format!("invalid workspace_module_operate arguments: {error}"),
                );
            }
        };
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        let outcome = match self
            .deps
            .providers
            .operate(WorkspaceModuleOperateRequest {
                context: &surface.provider_context,
                operation: arguments.operation.trim(),
                input: arguments.input,
            })
            .await
        {
            Ok(outcome) => outcome,
            Err(outcome) => return outcome,
        };
        let mut output = outcome.output;
        match outcome.surface_effect {
            WorkspaceModuleSurfaceEffect::Unchanged => {}
            WorkspaceModuleSurfaceEffect::RefreshRequired { module_id } => {
                let refreshed = match self.resolve_surface(&request).await {
                    Ok(surface) => surface,
                    Err(outcome) => return outcome,
                };
                let descriptor = match refreshed
                    .modules
                    .into_iter()
                    .find(|module| module.summary.module_id == module_id)
                {
                    Some(descriptor) => descriptor,
                    None => {
                        return failed(
                            "workspace_module_operate_projection_missing",
                            format!(
                                "Workspace Module mutation completed, but refreshed surface is missing `{module_id}`"
                            ),
                        );
                    }
                };
                if let Err(outcome) = attach_refreshed_descriptor(&mut output, descriptor) {
                    return outcome;
                }
            }
        }
        completed(output)
    }
}

fn attach_refreshed_descriptor(
    output: &mut Value,
    descriptor: WorkspaceModuleDescriptor,
) -> Result<(), ProductRuntimeToolOutcome> {
    let Some(details) = output.get_mut("details").and_then(Value::as_object_mut) else {
        return Err(failed(
            "workspace_module_operate_result_invalid",
            "Workspace Module provider operate output must contain object details",
        ));
    };
    details.insert("descriptor".to_owned(), serde_json::json!(descriptor));
    Ok(())
}

#[cfg(test)]
mod tests {
    use agentdash_contracts::workspace_module::{
        WorkspaceModuleKind, WorkspaceModuleStatus, WorkspaceModuleSummary,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn refreshed_descriptor_is_attached_only_by_the_operate_use_case() {
        let module_id = "canvas:fixture";
        let descriptor = WorkspaceModuleDescriptor {
            summary: WorkspaceModuleSummary {
                module_id: module_id.to_owned(),
                kind: WorkspaceModuleKind::new("canvas"),
                title: "Fixture".to_owned(),
                description: String::new(),
                source: "fixture".to_owned(),
                ui_summary: None,
                operation_summary: Vec::new(),
                permission_summary: Vec::new(),
                status: WorkspaceModuleStatus::ready(),
            },
            ui_entries: Vec::new(),
            operations: Vec::new(),
            runtime_backing: None,
            agent_state_projection: None,
        };
        let mut output = json!({
            "details": {
                "module_id": module_id
            }
        });

        attach_refreshed_descriptor(&mut output, descriptor).expect("attach descriptor");

        assert_eq!(
            output["details"]["descriptor"]["summary"]["module_id"],
            module_id
        );
    }
}
