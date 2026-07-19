use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_domain::DomainError;
use agentdash_platform_spi::PlatformRuntimeError;
use agentdash_platform_spi::session_persistence::SessionStoreError;

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

impl From<PlatformRuntimeError> for ApplicationError {
    fn from(error: PlatformRuntimeError) -> Self {
        match error {
            PlatformRuntimeError::InvalidConfig(message) => Self::BadRequest(message),
            PlatformRuntimeError::ConnectionFailed(message) => Self::Unavailable(message),
            PlatformRuntimeError::SpawnFailed(message) | PlatformRuntimeError::Runtime(message) => {
                Self::Internal(message)
            }
            PlatformRuntimeError::Io(error) => {
                let diagnostic_context =
                    DiagnosticErrorContext::new("agent_run.error_mapping", "connector_io");
                diag_error!(Error, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    error_source = "connector",
                    io_error_kind = ?error.kind(),
                    "AgentRun connector IO error"
                );
                Self::Internal("内部连接器 IO 错误".to_string())
            }
            PlatformRuntimeError::Json(error) => Self::BadRequest(error.to_string()),
        }
    }
}

impl From<std::io::Error> for ApplicationError {
    fn from(error: std::io::Error) -> Self {
        let diagnostic_context = DiagnosticErrorContext::new("agent_run.error_mapping", "io");
        diag_error!(Error, Subsystem::AgentRun,
            context = &diagnostic_context,
            error = &error,
            error_source = "application",
            io_error_kind = ?error.kind(),
            "AgentRun IO error"
        );
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
    Unavailable(String),
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

impl From<PlatformRuntimeError> for WorkflowApplicationError {
    fn from(value: PlatformRuntimeError) -> Self {
        match value {
            PlatformRuntimeError::InvalidConfig(message) => Self::BadRequest(message),
            PlatformRuntimeError::ConnectionFailed(message) => Self::Unavailable(message),
            PlatformRuntimeError::SpawnFailed(message) | PlatformRuntimeError::Runtime(message) => {
                Self::Internal(message)
            }
            PlatformRuntimeError::Io(error) => {
                let diagnostic_context =
                    DiagnosticErrorContext::new("agent_run.error_mapping", "workflow_connector_io");
                diag_error!(Error, Subsystem::AgentRun,
                    context = &diagnostic_context,
                    error = &error,
                    error_source = "workflow_connector",
                    io_error_kind = ?error.kind(),
                    "AgentRun workflow connector IO error"
                );
                Self::Internal("内部连接器 IO 错误".to_string())
            }
            PlatformRuntimeError::Json(error) => Self::BadRequest(error.to_string()),
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
