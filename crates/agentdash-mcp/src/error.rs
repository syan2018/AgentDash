use agentdash_domain::DomainError;

/// MCP 层统一错误类型
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("领域层错误: {0}")]
    Domain(#[from] DomainError),

    #[error("实体未找到: {entity_type} id={id}")]
    NotFound {
        entity_type: &'static str,
        id: String,
    },

    #[error("操作不允许: {reason}")]
    Forbidden { reason: String },

    #[error("参数无效: {field} — {message}")]
    InvalidParam {
        field: &'static str,
        message: String,
    },

    #[error("作用域不匹配: 期望 {expected:?}, 实际 {actual:?}")]
    ScopeMismatch {
        expected: crate::scope::ToolScope,
        actual: crate::scope::ToolScope,
    },

    #[error("内部错误: {0}")]
    Internal(String),
}

impl McpError {
    pub fn not_found(entity_type: &'static str, id: impl ToString) -> Self {
        Self::NotFound {
            entity_type,
            id: id.to_string(),
        }
    }

    pub fn forbidden(reason: impl Into<String>) -> Self {
        Self::Forbidden {
            reason: reason.into(),
        }
    }

    pub fn invalid_param(field: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidParam {
            field,
            message: message.into(),
        }
    }
}

impl From<McpError> for rmcp::ErrorData {
    fn from(err: McpError) -> Self {
        match &err {
            McpError::NotFound { .. } | McpError::Forbidden { .. } | McpError::ScopeMismatch { .. } => {
                rmcp::ErrorData::invalid_request(err.to_string(), None)
            }
            McpError::InvalidParam { .. } => {
                rmcp::ErrorData::invalid_params(err.to_string(), None)
            }
            McpError::Domain(_) | McpError::Internal(_) => {
                rmcp::ErrorData::internal_error(err.to_string(), None)
            }
        }
    }
}
