use agentdash_application_operation_gateway::{OperationInvocationCommand, OperationTraceContext};
use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
};
use agentdash_domain::operation::{OperationOriginRef, OperationRef};
use serde::Deserialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::product::tool_service::{
    ApplicationWorkspaceModuleRuntimeToolService, compact_operation_result, completed, failed,
    rejected,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvokeArguments {
    module_id: String,
    operation_key: String,
    #[serde(default)]
    input: Value,
}

impl ApplicationWorkspaceModuleRuntimeToolService {
    pub(crate) async fn execute_invoke(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> ProductRuntimeToolOutcome {
        let arguments: InvokeArguments = match serde_json::from_value(request.arguments.clone()) {
            Ok(arguments) => arguments,
            Err(error) => {
                return rejected(
                    "workspace_module_invalid_arguments",
                    format!("invalid workspace_module_invoke arguments: {error}"),
                );
            }
        };
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        let Some(module) = surface
            .modules
            .iter()
            .find(|module| module.summary.module_id == arguments.module_id)
        else {
            return rejected(
                "workspace_module_not_found",
                format!("workspace module is not visible: {}", arguments.module_id),
            );
        };
        let matching = module
            .operations
            .iter()
            .filter(|operation| operation.operation_key == arguments.operation_key)
            .collect::<Vec<_>>();
        let [operation] = matching.as_slice() else {
            return rejected(
                if matching.is_empty() {
                    "workspace_module_operation_not_visible"
                } else {
                    "workspace_module_operation_ambiguous"
                },
                format!(
                    "module `{}` does not expose exactly one visible operation `{}`",
                    arguments.module_id, arguments.operation_key
                ),
            );
        };
        let operation_ref = match OperationRef::new(
            operation.operation_ref.namespace.clone(),
            operation.operation_ref.provider_key.clone(),
            operation.operation_ref.operation_key.clone(),
            operation.operation_ref.contract_version,
        ) {
            Ok(operation_ref) => operation_ref,
            Err(error) => {
                return rejected("workspace_module_invalid_operation_ref", error.to_string());
            }
        };
        match self
            .deps
            .operation_gateway
            .invoke(
                OperationInvocationCommand {
                    operation_ref,
                    input: arguments.input,
                    principal: surface.principal,
                    scope_ref: surface.scope,
                    origin: OperationOriginRef::AgentTool,
                    trace: OperationTraceContext::root(),
                    deadline: chrono::Utc::now() + chrono::Duration::seconds(30),
                    idempotency_key: Some(request.context.invocation_id),
                    attachment_ref: None,
                },
                CancellationToken::new(),
            )
            .await
        {
            Ok(result) => completed(compact_operation_result(result)),
            Err(error) => failed("workspace_module_operation_failed", error.to_string()),
        }
    }
}
