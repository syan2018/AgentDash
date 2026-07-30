use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
};
use serde::Deserialize;
use serde_json::json;

use crate::product::tool_service::{
    ApplicationWorkspaceModuleRuntimeToolService, compact_diagnostics, compact_module_descriptor,
    completed, rejected,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeArguments {
    module_id: String,
}

impl ApplicationWorkspaceModuleRuntimeToolService {
    pub(crate) async fn execute_describe(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> ProductRuntimeToolOutcome {
        let arguments: DescribeArguments = match serde_json::from_value(request.arguments.clone()) {
            Ok(arguments) => arguments,
            Err(error) => {
                return rejected(
                    "workspace_module_invalid_arguments",
                    format!("invalid workspace_module_describe arguments: {error}"),
                );
            }
        };
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        let Some(module) = surface
            .modules
            .into_iter()
            .find(|module| module.summary.module_id == arguments.module_id)
        else {
            return rejected(
                "workspace_module_not_found",
                format!("workspace module is not visible: {}", arguments.module_id),
            );
        };
        completed(json!({
            "module": compact_module_descriptor(module),
            "surface_readiness": if surface.diagnostics.is_empty() { "ready" } else { "degraded" },
            "surface_diagnostics": compact_diagnostics(surface.diagnostics),
        }))
    }
}
