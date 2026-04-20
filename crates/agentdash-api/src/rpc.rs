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
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    UnprocessableEntity(String),
    ServiceUnavailable(String),
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
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            ApiError::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg),
            ApiError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
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
            agentdash_domain::DomainError::NotFound { .. } => ApiError::NotFound(err.to_string()),
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

impl From<agentdash_application::workflow::WorkflowApplicationError> for ApiError {
    fn from(err: agentdash_application::workflow::WorkflowApplicationError) -> Self {
        match err {
            agentdash_application::workflow::WorkflowApplicationError::BadRequest(message) => {
                ApiError::BadRequest(message)
            }
            agentdash_application::workflow::WorkflowApplicationError::NotFound(message) => {
                ApiError::NotFound(message)
            }
            agentdash_application::workflow::WorkflowApplicationError::Conflict(message) => {
                ApiError::Conflict(message)
            }
            agentdash_application::workflow::WorkflowApplicationError::Internal(message) => {
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
            E::Internal(message) => {
                // 兜底：捕获 Postgres `(project_id, name)` 唯一约束冲突透传——
                // 领域层 `DomainError::InvalidConfig` 会被 application 层映射为 Internal，
                // 若消息中包含 unique 关键字则按 Conflict 对外返回 409 而非 500。
                if looks_like_unique_violation(&message) {
                    ApiError::Conflict("mcp_preset 名称已存在，请换一个".to_string())
                } else {
                    ApiError::Internal(message)
                }
            }
        }
    }
}

/// 判断底层错误消息是否指向 `mcp_presets` 表的 unique 约束违反。
///
/// 严格约束作用域为 mcp_presets 表本身，避免误伤其他表的 unique 违反：
/// - 仅当消息里同时出现 "unique" 相关关键字 **且** 指向 `mcp_presets`
///   或其已知索引名（`idx_mcp_presets_project_name`、`idx_mcp_presets_project_builtin_key`）
///   时才判定为 unique 冲突。
/// - 这样即便未来其他模块复用该兜底路径或 migration 新增其他 unique 索引，
///   也不会把无关错误误伤为 409。
fn looks_like_unique_violation(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let unique_marker = lower.contains("duplicate key")
        || lower.contains("unique constraint")
        || lower.contains("unique");
    let scoped_to_mcp_presets = lower.contains("mcp_presets")
        || lower.contains("idx_mcp_presets_");
    unique_marker && scoped_to_mcp_presets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_violation_detection_covers_postgres_message() {
        assert!(looks_like_unique_violation(
            "error returned from database: duplicate key value violates unique constraint \"idx_mcp_presets_project_name\""
        ));
        assert!(looks_like_unique_violation(
            "unique constraint violation on idx_mcp_presets_project_builtin_key"
        ));
        assert!(looks_like_unique_violation(
            "mcp_presets unique(project_id,name) violation"
        ));
    }

    #[test]
    fn unique_violation_detection_ignores_unrelated_errors() {
        assert!(!looks_like_unique_violation("connection timeout"));
        assert!(!looks_like_unique_violation("invalid JSON payload"));
    }

    #[test]
    fn unique_violation_detection_ignores_other_tables_unique_violation() {
        // 其他表的 unique 冲突落到 mcp_preset handler 时不应被误判为 409
        assert!(!looks_like_unique_violation(
            "duplicate key value violates unique constraint \"idx_projects_name\""
        ));
        assert!(!looks_like_unique_violation(
            "duplicate key value violates unique constraint \"idx_canvases_project_title\""
        ));
    }
}
