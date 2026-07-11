use std::sync::Arc;

use async_trait::async_trait;

use agentdash_application::ApplicationError;
use agentdash_application::workspace::{
    WorkspaceDetectionResult, WorkspacePlacementDetectInput, WorkspacePlacementRuntime,
};
use agentdash_application_operation_gateway::{
    OperationExecutionError, OperationExecutionErrorKind, OperationGateway,
    OperationInvocationCommand, OperationOriginRef, OperationPrincipal, OperationScopeRef,
    OperationTraceContext, WORKSPACE_DETECT_ACTION, WorkspaceDetectInput, setup_operation_ref,
};
use agentdash_spi::AuthIdentity;
use chrono::{Duration, Utc};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct RuntimeGatewayWorkspacePlacementRuntime {
    operation_gateway: Arc<OperationGateway>,
    identity: AuthIdentity,
}

impl RuntimeGatewayWorkspacePlacementRuntime {
    pub fn new(operation_gateway: Arc<OperationGateway>, identity: AuthIdentity) -> Self {
        Self {
            operation_gateway,
            identity,
        }
    }
}

#[async_trait]
impl WorkspacePlacementRuntime for RuntimeGatewayWorkspacePlacementRuntime {
    async fn detect_workspace(
        &self,
        input: WorkspacePlacementDetectInput,
    ) -> Result<WorkspaceDetectionResult, ApplicationError> {
        let gateway_input = serde_json::to_value(WorkspaceDetectInput {
            backend_id: input.backend_id.clone(),
            root_ref: input.root_ref.clone(),
        })
        .map_err(|error| {
            ApplicationError::BadRequest(format!("workspace.detect 输入非法: {error}"))
        })?;
        let request = OperationInvocationCommand {
            operation_ref: setup_operation_ref(WORKSPACE_DETECT_ACTION)
                .map_err(application_error_from_operation_gateway)?,
            input: gateway_input,
            principal: OperationPrincipal::authenticated_user(self.identity.clone()),
            scope_ref: OperationScopeRef::EnvironmentSetup {
                project_id: Some(input.project_id),
                workspace_id: input.workspace_id,
                backend_id: Some(input.backend_id),
            },
            origin: OperationOriginRef::EnvironmentSetup,
            trace: OperationTraceContext::root(),
            deadline: Utc::now() + Duration::seconds(30),
            idempotency_key: None,
            attachment_ref: None,
        };
        let invocation = self
            .operation_gateway
            .invoke(request, CancellationToken::new())
            .await
            .map_err(application_error_from_operation_gateway)?;
        let agentdash_application_operation_gateway::OperationResultValue::Inline { value } =
            invocation.value
        else {
            return Err(ApplicationError::Internal(
                "workspace.detect 不应返回 result ref".to_string(),
            ));
        };
        serde_json::from_value::<WorkspaceDetectionResult>(value).map_err(|error| {
            ApplicationError::Internal(format!("workspace.detect 返回值解析失败: {error}"))
        })
    }
}

fn application_error_from_operation_gateway(error: OperationExecutionError) -> ApplicationError {
    let message = error.to_string();
    match error.kind() {
        OperationExecutionErrorKind::InvalidRequest => ApplicationError::BadRequest(message),
        OperationExecutionErrorKind::Denied => ApplicationError::Forbidden(message),
        OperationExecutionErrorKind::AuthorityChanged | OperationExecutionErrorKind::Cancelled => {
            ApplicationError::Conflict(message)
        }
        OperationExecutionErrorKind::Unavailable
        | OperationExecutionErrorKind::DeadlineExceeded => ApplicationError::Unavailable(message),
        OperationExecutionErrorKind::ProviderFailed
        | OperationExecutionErrorKind::InvalidOutput
        | OperationExecutionErrorKind::ResultStoreFailed => ApplicationError::Internal(message),
    }
}
