use agentdash_domain::DomainError;

/// MCP Preset 应用层错误——对齐 WorkflowApplicationError 的四态划分，
/// 便于 API 层按 HTTP 语义直接映射。
#[derive(Debug, thiserror::Error)]
pub enum McpPresetApplicationError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Internal(String),
}

impl From<DomainError> for McpPresetApplicationError {
    fn from(value: DomainError) -> Self {
        match value {
            DomainError::NotFound { entity, id } => {
                Self::NotFound(format!("实体未找到: {entity} (id={id})"))
            }
            DomainError::InvalidTransition { from, to } => {
                Self::Conflict(format!("状态迁移非法: {from} -> {to}"))
            }
            DomainError::Serialization(error) => Self::Internal(error.to_string()),
            DomainError::InvalidConfig(message) => Self::Internal(message),
        }
    }
}
