use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;
use serde_json::Value;

/// 统一 API 错误类型
///
/// 将各层的错误转换为 HTTP 响应，保持一致的错误格式。
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    BadRequestWithCode { message: String, error_code: String },
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    ConflictWithCode(Box<ApiErrorWithCode>),
    UnprocessableEntity(String),
    ServiceUnavailable(String),
    Internal(String),
}

#[derive(Debug)]
pub struct ApiErrorWithCode {
    pub message: String,
    pub error_code: String,
    pub replacement_command: Option<String>,
    pub detail: Option<Value>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replacement_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<Value>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message, error_code, replacement_command, detail) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg, None, None, None),
            ApiError::BadRequestWithCode {
                message,
                error_code,
            } => (
                StatusCode::BAD_REQUEST,
                message,
                Some(error_code),
                None,
                None,
            ),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg, None, None, None),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg, None, None, None),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg, None, None, None),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg, None, None, None),
            ApiError::ConflictWithCode(payload) => {
                let ApiErrorWithCode {
                    message,
                    error_code,
                    replacement_command,
                    detail,
                } = *payload;
                (
                    StatusCode::CONFLICT,
                    message,
                    Some(error_code),
                    replacement_command,
                    detail,
                )
            }
            ApiError::UnprocessableEntity(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, msg, None, None, None)
            }
            ApiError::ServiceUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, msg, None, None, None)
            }
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg, None, None, None),
        };

        let body = Json(ErrorResponse {
            error: message,
            code: status.as_u16(),
            error_code,
            replacement_command,
            detail,
        });

        (status, body).into_response()
    }
}

impl From<std::io::Error> for ApiError {
    /// session 持久化层统一返回 `io::Error`，对外一律视为 500 Internal。
    fn from(err: std::io::Error) -> Self {
        tracing::error!(error = %err, "session persistence IO error");
        ApiError::Internal(String::from("内部 IO 错误"))
    }
}

impl From<agentdash_spi::session_persistence::SessionStoreError> for ApiError {
    fn from(err: agentdash_spi::session_persistence::SessionStoreError) -> Self {
        use agentdash_spi::session_persistence::SessionStoreError as E;
        match err {
            E::NotFound(message) => ApiError::NotFound(message),
            E::InvalidInput(message) | E::InvalidData(message) => ApiError::BadRequest(message),
            E::Database(message) => {
                tracing::error!(error = %message, "session persistence database error");
                ApiError::Internal(String::from("内部数据库错误"))
            }
            E::Internal(message) => {
                tracing::error!(error = %message, "session persistence internal error");
                ApiError::Internal(String::from("内部会话持久化错误"))
            }
        }
    }
}

impl From<agentdash_domain::DomainError> for ApiError {
    fn from(err: agentdash_domain::DomainError) -> Self {
        match &err {
            agentdash_domain::DomainError::NotFound { .. } => ApiError::NotFound(err.to_string()),
            agentdash_domain::DomainError::Conflict { .. } => ApiError::Conflict(err.to_string()),
            agentdash_domain::DomainError::Forbidden { .. } => ApiError::Forbidden(err.to_string()),
            agentdash_domain::DomainError::InvalidTransition { .. } => {
                ApiError::BadRequest(err.to_string())
            }
            agentdash_domain::DomainError::InvalidConfig(_) => {
                ApiError::BadRequest(err.to_string())
            }
            agentdash_domain::DomainError::Serialization(_) => {
                ApiError::BadRequest(err.to_string())
            }
            agentdash_domain::DomainError::Database { .. } => {
                ApiError::Internal(String::from("内部数据库错误"))
            }
        }
    }
}

impl From<agentdash_application::ApplicationError> for ApiError {
    fn from(err: agentdash_application::ApplicationError) -> Self {
        use agentdash_application::ApplicationError as E;
        match err {
            E::BadRequest(message) | E::InvalidConfig(message) => ApiError::BadRequest(message),
            E::NotFound(message) => ApiError::NotFound(message),
            E::Forbidden(message) => ApiError::Forbidden(message),
            E::Conflict(message) => ApiError::Conflict(message),
            E::Unavailable(message) => ApiError::ServiceUnavailable(message),
            E::Internal(message) => ApiError::Internal(message),
        }
    }
}

impl From<agentdash_spi::ConnectorError> for ApiError {
    fn from(err: agentdash_spi::ConnectorError) -> Self {
        use agentdash_spi::ConnectorError as E;
        match err {
            E::InvalidConfig(message) => ApiError::BadRequest(message),
            E::ConnectionFailed(message) => ApiError::ServiceUnavailable(message),
            E::SpawnFailed(message) | E::Runtime(message) => ApiError::Internal(message),
            E::Io(error) => {
                tracing::error!(error = %error, "connector IO error");
                ApiError::Internal(String::from("内部连接器 IO 错误"))
            }
            E::Json(error) => ApiError::BadRequest(error.to_string()),
        }
    }
}

impl From<agentdash_application::lifecycle::WorkflowApplicationError> for ApiError {
    fn from(err: agentdash_application::lifecycle::WorkflowApplicationError) -> Self {
        match err {
            agentdash_application::lifecycle::WorkflowApplicationError::BadRequest(message) => {
                ApiError::BadRequest(message)
            }
            agentdash_application::lifecycle::WorkflowApplicationError::ModelRequired(message) => {
                ApiError::BadRequestWithCode {
                    message,
                    error_code: "model_required".to_string(),
                }
            }
            agentdash_application::lifecycle::WorkflowApplicationError::NotFound(message) => {
                ApiError::NotFound(message)
            }
            agentdash_application::lifecycle::WorkflowApplicationError::Conflict(message) => {
                ApiError::Conflict(message)
            }
            agentdash_application::lifecycle::WorkflowApplicationError::Internal(message) => {
                ApiError::Internal(message)
            }
        }
    }
}

impl From<agentdash_application::mcp_preset::McpPresetApplicationError> for ApiError {
    fn from(err: agentdash_application::mcp_preset::McpPresetApplicationError) -> Self {
        use agentdash_application::mcp_preset::McpPresetApplicationError as E;
        match err {
            E::BadRequest(message) => ApiError::BadRequest(message),
            E::NotFound(message) => ApiError::NotFound(message),
            E::Conflict(message) => ApiError::Conflict(message),
            E::Internal(message) => ApiError::Internal(message),
        }
    }
}

impl From<agentdash_application::skill_asset::SkillAssetApplicationError> for ApiError {
    fn from(err: agentdash_application::skill_asset::SkillAssetApplicationError) -> Self {
        use agentdash_application::skill_asset::SkillAssetApplicationError as E;
        match err {
            E::BadRequest(message) => ApiError::BadRequest(message),
            E::NotFound(message) => ApiError::NotFound(message),
            E::Conflict(message) => ApiError::Conflict(message),
            E::Internal(message) => ApiError::Internal(message),
        }
    }
}

impl From<agentdash_application::shared_library::PublishLibraryAssetError> for ApiError {
    fn from(err: agentdash_application::shared_library::PublishLibraryAssetError) -> Self {
        use agentdash_application::shared_library::PublishLibraryAssetError as E;
        match err {
            E::BadRequest(message) => ApiError::BadRequest(message),
            E::Conflict(message) => ApiError::Conflict(message),
            E::Domain(error) => ApiError::from(error),
        }
    }
}

impl From<agentdash_application::shared_library::ExternalMarketplaceLibraryError> for ApiError {
    fn from(err: agentdash_application::shared_library::ExternalMarketplaceLibraryError) -> Self {
        use agentdash_application::shared_library::ExternalMarketplaceLibraryError as E;
        match err {
            E::BadRequest(message) => ApiError::BadRequest(message),
            E::Conflict(message) => ApiError::Conflict(message),
            E::Domain(error) => ApiError::from(error),
        }
    }
}

impl From<agentdash_application_runtime_gateway::RuntimeInvocationError> for ApiError {
    fn from(err: agentdash_application_runtime_gateway::RuntimeInvocationError) -> Self {
        use agentdash_application_runtime_gateway::{
            RuntimeInvocationError as E, RuntimeInvocationErrorKind,
        };

        let message = err.to_string();
        match err.kind() {
            RuntimeInvocationErrorKind::InvalidRequest => ApiError::BadRequest(message),
            RuntimeInvocationErrorKind::CapabilityDenied => ApiError::Forbidden(message),
            RuntimeInvocationErrorKind::Conflict => ApiError::Conflict(message),
            RuntimeInvocationErrorKind::ProviderUnavailable => {
                ApiError::ServiceUnavailable(message)
            }
            RuntimeInvocationErrorKind::ProviderFailed => match err {
                E::ProviderFailed { message, .. } => ApiError::Internal(message),
                _ => ApiError::Internal(message),
            },
            RuntimeInvocationErrorKind::Timeout => ApiError::ServiceUnavailable(message),
        }
    }
}

impl From<agentdash_application::backend::BackendAuthorizationError> for ApiError {
    fn from(err: agentdash_application::backend::BackendAuthorizationError) -> Self {
        use agentdash_application::backend::BackendAuthorizationError as E;
        match err {
            E::Domain(error) => ApiError::from(error),
            E::Forbidden { .. } => ApiError::Forbidden(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::response::IntoResponse;
    use serde_json::Value;

    use super::*;

    #[tokio::test]
    async fn conflict_with_code_serializes_structured_command_error() {
        let response = ApiError::ConflictWithCode(Box::new(ApiErrorWithCode {
            message: "当前状态下新输入应作为下一轮消息发送。".to_string(),
            error_code: "command_unavailable".to_string(),
            replacement_command: Some("send_next".to_string()),
            detail: Some(serde_json::json!({ "state": "completed" })),
        }))
        .into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let value: Value = serde_json::from_slice(&body).expect("json body");

        assert_eq!(value["error_code"], "command_unavailable");
        assert_eq!(value["replacement_command"], "send_next");
        assert_eq!(value["detail"]["state"], "completed");
    }
}
