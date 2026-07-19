use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolKind, ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
    ProductRuntimeToolService,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::{WaitActivityRequest, WaitActivityService, WaitToolContext};

#[async_trait]
impl ProductRuntimeToolService for WaitActivityService {
    fn kind(&self) -> ProductRuntimeToolKind {
        ProductRuntimeToolKind::Wait
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(WaitActivityRequest))
            .unwrap_or_else(|_| serde_json::json!({ "type": "object" }))
    }

    async fn execute(&self, request: ProductRuntimeToolRequest) -> ProductRuntimeToolOutcome {
        let arguments: WaitActivityRequest = match serde_json::from_value(request.arguments) {
            Ok(value) => value,
            Err(error) => {
                return ProductRuntimeToolOutcome::Rejected {
                    code: "invalid_wait_arguments".to_owned(),
                    message: error.to_string(),
                };
            }
        };
        let context = WaitToolContext {
            runtime_thread_id: Some(request.context.runtime_thread_id),
            turn_id: request.context.turn_id,
            owner: None,
        };
        let scope = match self.resolve_scope(&context).await {
            Ok(scope) => scope,
            Err(error) => {
                return ProductRuntimeToolOutcome::Failed {
                    code: "wait_scope_resolution_failed".to_owned(),
                    message: error.to_string(),
                };
            }
        };
        if scope.run_id != Some(request.context.target.run_id)
            || scope.agent_id != Some(request.context.target.agent_id)
        {
            return ProductRuntimeToolOutcome::Rejected {
                code: "stale_wait_product_target".to_owned(),
                message: "RuntimeThread wait scope differs from the authorized Product target"
                    .to_owned(),
            };
        }
        match self
            .wait(context, arguments, CancellationToken::new())
            .await
        {
            Ok(result) => ProductRuntimeToolOutcome::Completed {
                output: serde_json::json!({
                    "type": "wait",
                    "status": result.status,
                    "timed_out": result.timed_out,
                    "cursor": result.cursor,
                    "items": result.items,
                }),
            },
            Err(error) => ProductRuntimeToolOutcome::Failed {
                code: "wait_execution_failed".to_owned(),
                message: error.to_string(),
            },
        }
    }
}
