use agentdash_domain::DomainError;

#[derive(Debug, thiserror::Error)]
pub enum WorkflowApplicationError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Internal(String),
}

impl From<DomainError> for WorkflowApplicationError {
    fn from(value: DomainError) -> Self {
        match value {
            DomainError::NotFound { entity, id } => {
                Self::NotFound(format!("实体未找到: {entity} (id={id})"))
            }
            DomainError::Conflict { .. } => Self::Conflict(value.to_string()),
            DomainError::Forbidden { .. } => Self::BadRequest(value.to_string()),
            DomainError::InvalidTransition { from, to } => {
                Self::Conflict(format!("状态迁移非法: {from} -> {to}"))
            }
            DomainError::Serialization(error) => Self::Internal(error.to_string()),
            DomainError::InvalidConfig(message) => Self::Internal(message),
            DomainError::Database { .. } => Self::Internal("内部数据库错误".to_string()),
        }
    }
}
