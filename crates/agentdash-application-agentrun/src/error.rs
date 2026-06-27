use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::DomainError;
use agentdash_spi::ConnectorError;
use agentdash_spi::session_persistence::SessionStoreError;

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
                diag!(Error, Subsystem::AgentRun,
        error = %error, "agentrun connector IO error");
                Self::Internal("内部连接器 IO 错误".to_string())
            }
            ConnectorError::Json(error) => Self::BadRequest(error.to_string()),
        }
    }
}

impl From<std::io::Error> for ApplicationError {
    fn from(error: std::io::Error) -> Self {
        diag!(Error, Subsystem::AgentRun,
        error = %error, "agentrun IO error");
        Self::Internal("内部 IO 错误".to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowApplicationError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    ModelRequired(String),
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

impl From<ConnectorError> for WorkflowApplicationError {
    fn from(value: ConnectorError) -> Self {
        match value {
            ConnectorError::InvalidConfig(message) => Self::BadRequest(message),
            ConnectorError::ConnectionFailed(message) => Self::Internal(message),
            ConnectorError::SpawnFailed(message) | ConnectorError::Runtime(message) => {
                Self::Internal(message)
            }
            ConnectorError::Io(error) => {
                diag!(Error, Subsystem::AgentRun,
        error = %error, "agentrun workflow connector IO error");
                Self::Internal("内部连接器 IO 错误".to_string())
            }
            ConnectorError::Json(error) => Self::BadRequest(error.to_string()),
        }
    }
}

impl From<SessionStoreError> for WorkflowApplicationError {
    fn from(value: SessionStoreError) -> Self {
        match value {
            SessionStoreError::NotFound(message) => Self::NotFound(message),
            SessionStoreError::InvalidInput(message) => Self::BadRequest(message),
            SessionStoreError::InvalidData(message) => Self::Internal(message),
            SessionStoreError::Database(_) => Self::Internal("内部会话持久化错误".to_string()),
            SessionStoreError::Internal(message) => Self::Internal(message),
        }
    }
}
