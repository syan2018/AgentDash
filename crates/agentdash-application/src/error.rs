use agentdash_domain::DomainError;
use agentdash_spi::ConnectorError;

#[derive(Debug, thiserror::Error)]
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
    Unavailable(String),
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

impl From<ConnectorError> for ApplicationError {
    fn from(error: ConnectorError) -> Self {
        match error {
            ConnectorError::InvalidConfig(message) => Self::BadRequest(message),
            ConnectorError::ConnectionFailed(message) => Self::Unavailable(message),
            ConnectorError::SpawnFailed(message) | ConnectorError::Runtime(message) => {
                Self::Internal(message)
            }
            ConnectorError::Io(error) => {
                tracing::error!(error = %error, "connector IO error");
                Self::Internal("内部连接器 IO 错误".to_string())
            }
            ConnectorError::Json(error) => Self::BadRequest(error.to_string()),
        }
    }
}

impl From<std::io::Error> for ApplicationError {
    fn from(error: std::io::Error) -> Self {
        tracing::error!(error = %error, "application IO error");
        Self::Internal("内部 IO 错误".to_string())
    }
}

impl From<crate::skill_asset::SkillAssetApplicationError> for ApplicationError {
    fn from(error: crate::skill_asset::SkillAssetApplicationError) -> Self {
        match error {
            crate::skill_asset::SkillAssetApplicationError::BadRequest(message) => {
                Self::BadRequest(message)
            }
            crate::skill_asset::SkillAssetApplicationError::NotFound(message) => {
                Self::NotFound(message)
            }
            crate::skill_asset::SkillAssetApplicationError::Conflict(message) => {
                Self::Conflict(message)
            }
            crate::skill_asset::SkillAssetApplicationError::Internal(message) => {
                Self::Internal(message)
            }
        }
    }
}

impl From<crate::task::execution::TaskExecutionError> for ApplicationError {
    fn from(error: crate::task::execution::TaskExecutionError) -> Self {
        match error {
            crate::task::execution::TaskExecutionError::BadRequest(message) => {
                Self::BadRequest(message)
            }
            crate::task::execution::TaskExecutionError::NotFound(message) => {
                Self::NotFound(message)
            }
            crate::task::execution::TaskExecutionError::Conflict(message) => {
                Self::Conflict(message)
            }
            crate::task::execution::TaskExecutionError::UnprocessableEntity(message) => {
                Self::BadRequest(message)
            }
            crate::task::execution::TaskExecutionError::Internal(message) => {
                Self::Internal(message)
            }
        }
    }
}
