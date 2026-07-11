use std::sync::Arc;

use agentdash_application_ports::operation_script::{
    OperationScriptError, OperationScriptOperationCall, OperationScriptOperationExecutor,
    OperationScriptOperationResult,
};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::{
    OperationExecutionError, OperationExecutionErrorKind, OperationGateway,
    OperationInvocationCommand, OperationOriginRef, OperationPrincipal, OperationResultValue,
    OperationTraceContext,
};

/// Re-enters the canonical gateway for every script call so authority, capability,
/// schema, readiness and placement are evaluated at the actual call boundary.
pub struct GatewayOperationScriptExecutor {
    gateway: Arc<OperationGateway>,
}

impl GatewayOperationScriptExecutor {
    pub fn new(gateway: Arc<OperationGateway>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl OperationScriptOperationExecutor for GatewayOperationScriptExecutor {
    async fn execute(
        &self,
        call: OperationScriptOperationCall,
        cancel: CancellationToken,
    ) -> Result<OperationScriptOperationResult, OperationScriptError> {
        let principal = OperationPrincipal::server_resolved(call.context.principal.clone());
        let scope_ref = call.context.scope.clone();
        let origin = OperationOriginRef::OperationScriptNested {
            script_invocation_id: call.execution_id.to_string(),
        };
        let result = self
            .gateway
            .invoke(
                OperationInvocationCommand {
                    operation_ref: call.operation_ref,
                    input: call.input,
                    principal: principal.clone(),
                    scope_ref: scope_ref.clone(),
                    origin: origin.clone(),
                    trace: OperationTraceContext {
                        trace_id: call.parent_trace_id,
                        invocation_id: call.child_trace_id,
                        parent_invocation_id: Some(call.context.trace_id),
                        created_at: chrono::Utc::now(),
                    },
                    deadline: call.deadline,
                    idempotency_key: None,
                    attachment_ref: call.context.attachment_ref,
                },
                cancel.clone(),
            )
            .await
            .map_err(map_gateway_error)?;

        let value = match result.value {
            OperationResultValue::Inline { value } => value,
            OperationResultValue::Ref { result_ref } => self
                .gateway
                .resolve_result(&result_ref, &principal, &scope_ref, &origin, cancel)
                .await
                .map_err(map_gateway_error)?
                .ok_or_else(|| OperationScriptError::NestedOperation {
                    code: "result_not_authorized".into(),
                    outcome_unknown: false,
                })?,
        };
        Ok(OperationScriptOperationResult {
            value,
            outcome_unknown: false,
        })
    }
}

fn map_gateway_error(error: OperationExecutionError) -> OperationScriptError {
    let outcome_unknown = matches!(
        error.kind(),
        OperationExecutionErrorKind::Cancelled
            | OperationExecutionErrorKind::DeadlineExceeded
            | OperationExecutionErrorKind::ProviderFailed
            | OperationExecutionErrorKind::ResultStoreFailed
    );
    OperationScriptError::NestedOperation {
        code: error.code().to_owned(),
        outcome_unknown,
    }
}
