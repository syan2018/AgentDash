use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
};
use serde::Deserialize;
use serde_json::Value;

use crate::product::WorkspaceModuleOperateRequest;
use crate::product::tool_service::{
    ApplicationWorkspaceModuleRuntimeToolService, completed, rejected,
};

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
        match self
            .deps
            .providers
            .operate(WorkspaceModuleOperateRequest {
                context: &surface.provider_context,
                operation: arguments.operation.trim(),
                input: arguments.input,
            })
            .await
        {
            Ok(output) => completed(output),
            Err(outcome) => outcome,
        }
    }
}
