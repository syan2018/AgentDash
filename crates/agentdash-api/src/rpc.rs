use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

/// 统一 API 错误类型
///
/// 将各层的错误转换为 HTTP 响应，保持一致的错误格式。
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: u16,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorResponse {
            error: message,
            code: status.as_u16(),
        });

        (status, body).into_response()
    }
}

impl From<agentdash_domain::DomainError> for ApiError {
    fn from(err: agentdash_domain::DomainError) -> Self {
        match &err {
            agentdash_domain::DomainError::NotFound { .. } => {
                ApiError::NotFound(err.to_string())
            }
            agentdash_domain::DomainError::InvalidTransition { .. } => {
                ApiError::BadRequest(err.to_string())
            }
            agentdash_domain::DomainError::InvalidConfig(_) => {
                ApiError::BadRequest(err.to_string())
            }
            _ => ApiError::Internal(err.to_string()),
        }
    }
}
