use std::sync::Arc;

use async_trait::async_trait;

use agentdash_application::ApplicationError;
use agentdash_application::workspace::{
    WorkspaceDetectionResult, WorkspacePlacementDetectInput, WorkspacePlacementRuntime,
};
use agentdash_application_extension_gateway::{
    ExtensionGateway, RuntimeActionKey, RuntimeActor, RuntimeContext, RuntimeInvocationError,
    RuntimeInvocationErrorKind, RuntimeInvocationRequest, WORKSPACE_DETECT_ACTION,
    WorkspaceDetectInput,
};

#[derive(Clone)]
pub struct ExtensionGatewayWorkspacePlacementRuntime {
    extension_gateway: Arc<ExtensionGateway>,
}

impl ExtensionGatewayWorkspacePlacementRuntime {
    pub fn new(extension_gateway: Arc<ExtensionGateway>) -> Self {
        Self { extension_gateway }
    }
}

#[async_trait]
impl WorkspacePlacementRuntime for ExtensionGatewayWorkspacePlacementRuntime {
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
        let request = RuntimeInvocationRequest::new(
            RuntimeActionKey::parse(WORKSPACE_DETECT_ACTION).map_err(|error| {
                ApplicationError::Internal(format!("内置 Runtime Action Key 非法: {error}"))
            })?,
            RuntimeActor::PlatformUser {
                user_id: input.user_id,
            },
            RuntimeContext::Setup {
                project_id: Some(input.project_id),
                workspace_id: input.workspace_id,
                backend_id: Some(input.backend_id),
                root_ref: Some(input.root_ref),
            },
            gateway_input,
        );
        let invocation = self
            .extension_gateway
            .invoke(request)
            .await
            .map_err(application_error_from_extension_gateway)?;
        serde_json::from_value::<WorkspaceDetectionResult>(invocation.output.output).map_err(
            |error| ApplicationError::Internal(format!("workspace.detect 返回值解析失败: {error}")),
        )
    }
}

fn application_error_from_extension_gateway(error: RuntimeInvocationError) -> ApplicationError {
    let message = error.to_string();
    match error.kind() {
        RuntimeInvocationErrorKind::InvalidRequest => ApplicationError::BadRequest(message),
        RuntimeInvocationErrorKind::CapabilityDenied => ApplicationError::Forbidden(message),
        RuntimeInvocationErrorKind::Conflict => ApplicationError::Conflict(message),
        RuntimeInvocationErrorKind::ProviderUnavailable | RuntimeInvocationErrorKind::Timeout => {
            ApplicationError::Unavailable(message)
        }
        RuntimeInvocationErrorKind::ProviderFailed => match error {
            RuntimeInvocationError::ProviderFailed { message, .. } => {
                ApplicationError::Internal(message)
            }
            _ => ApplicationError::Internal(message),
        },
    }
}
