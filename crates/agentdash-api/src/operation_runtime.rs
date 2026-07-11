use agentdash_application_operation_gateway::{
    OperationInvocationCommand, OperationOriginRef, OperationPrincipal, OperationResultValue,
    OperationScopeRef, OperationTraceContext, setup_operation_ref,
};
use agentdash_spi::AuthIdentity;
use chrono::{Duration, Utc};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::rpc::ApiError;

pub(crate) struct SetupOperationScope {
    pub project_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub backend_id: Option<String>,
}

pub(crate) async fn invoke_setup_operation(
    state: &AppState,
    identity: &AuthIdentity,
    operation_key: &str,
    input: Value,
    resolved_scope: SetupOperationScope,
) -> Result<Value, ApiError> {
    let principal = OperationPrincipal::authenticated_user(identity.clone());
    let scope_ref = OperationScopeRef::EnvironmentSetup {
        project_id: resolved_scope.project_id,
        workspace_id: resolved_scope.workspace_id,
        backend_id: resolved_scope.backend_id,
    };
    let origin = OperationOriginRef::EnvironmentSetup;
    let result = state
        .services
        .operation_gateway
        .invoke(
            OperationInvocationCommand {
                operation_ref: setup_operation_ref(operation_key)?,
                input,
                principal: principal.clone(),
                scope_ref: scope_ref.clone(),
                origin: origin.clone(),
                trace: OperationTraceContext::root(),
                deadline: Utc::now() + Duration::seconds(30),
                idempotency_key: None,
                attachment_ref: None,
            },
            CancellationToken::new(),
        )
        .await?;
    match result.value {
        OperationResultValue::Inline { value } => Ok(value),
        OperationResultValue::Ref { result_ref } => state
            .services
            .operation_gateway
            .resolve_result(
                &result_ref,
                &principal,
                &scope_ref,
                &origin,
                CancellationToken::new(),
            )
            .await?
            .ok_or_else(|| ApiError::ServiceUnavailable("Operation result 已过期".to_string())),
    }
}
