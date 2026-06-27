use agentdash_domain::DomainError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    InvalidConfig(String),
    #[error("{0}")]
    Internal(String),
}

impl From<DomainError> for ApplicationError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::NotFound { .. } => Self::NotFound(error.to_string()),
            DomainError::Forbidden { .. } => Self::Forbidden(error.to_string()),
            DomainError::Conflict { .. } | DomainError::InvalidTransition { .. } => {
                Self::Conflict(error.to_string())
            }
            DomainError::InvalidConfig(message) => Self::InvalidConfig(message),
            DomainError::Serialization(error) => Self::BadRequest(error.to_string()),
            DomainError::Database { .. } => Self::Internal("内部数据库错误".to_string()),
        }
    }
}
