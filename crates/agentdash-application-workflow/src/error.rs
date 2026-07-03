use agentdash_application_ports::lifecycle_materialization::LifecycleMaterializationError;
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_domain::DomainError;
use agentdash_spi::ConnectorError;
use agentdash_spi::session_persistence::SessionStoreError;

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
                let context =
                    DiagnosticErrorContext::new("workflow.error_conversion", "connector_io")
                        .with_field("error_source", "connector");
                diag_error!(
                    Error,
                    Subsystem::Workflow,
                    context = &context,
                    error = &error,
                    error_source = "connector",
                    "workflow connector IO error"
                );
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

impl From<LifecycleMaterializationError> for WorkflowApplicationError {
    fn from(value: LifecycleMaterializationError) -> Self {
        match value {
            LifecycleMaterializationError::Rejected { message } => Self::BadRequest(message),
            LifecycleMaterializationError::MissingDependency { message }
            | LifecycleMaterializationError::Internal { message } => Self::Internal(message),
            LifecycleMaterializationError::Repository { message, .. } => Self::Internal(message),
        }
    }
}
