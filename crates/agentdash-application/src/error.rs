use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_domain::DomainError;
use agentdash_platform_spi::PlatformRuntimeError;

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
                let context =
                    DiagnosticErrorContext::new("application.error_conversion", "connector_io")
                        .with_field("error_source", "connector");
                diag_error!(
                    Error,
                    Subsystem::Infra,
                    context = &context,
                    error = &error,
                    error_source = "connector",
                    "connector IO error"
                );
                Self::Internal("内部连接器 IO 错误".to_string())
            }
            PlatformRuntimeError::Json(error) => Self::BadRequest(error.to_string()),
        }
    }
}

impl From<agentdash_workspace_module::error::ApplicationError> for ApplicationError {
    fn from(error: agentdash_workspace_module::error::ApplicationError) -> Self {
        match error {
            agentdash_workspace_module::error::ApplicationError::BadRequest(message) => {
                Self::BadRequest(message)
            }
            agentdash_workspace_module::error::ApplicationError::NotFound(message) => {
                Self::NotFound(message)
            }
            agentdash_workspace_module::error::ApplicationError::Forbidden(message) => {
                Self::Forbidden(message)
            }
            agentdash_workspace_module::error::ApplicationError::Conflict(message) => {
                Self::Conflict(message)
            }
            agentdash_workspace_module::error::ApplicationError::InvalidConfig(message) => {
                Self::InvalidConfig(message)
            }
            agentdash_workspace_module::error::ApplicationError::Internal(message) => {
                Self::Internal(message)
            }
        }
    }
}

impl From<agentdash_application_agentrun::ApplicationError> for ApplicationError {
    fn from(error: agentdash_application_agentrun::ApplicationError) -> Self {
        match error {
            agentdash_application_agentrun::ApplicationError::BadRequest(message) => {
                Self::BadRequest(message)
            }
            agentdash_application_agentrun::ApplicationError::NotFound(message) => {
                Self::NotFound(message)
            }
            agentdash_application_agentrun::ApplicationError::Forbidden(message) => {
                Self::Forbidden(message)
            }
            agentdash_application_agentrun::ApplicationError::Conflict(message) => {
                Self::Conflict(message)
            }
            agentdash_application_agentrun::ApplicationError::InvalidConfig(message) => {
                Self::InvalidConfig(message)
            }
            agentdash_application_agentrun::ApplicationError::Unavailable(message) => {
                Self::Unavailable(message)
            }
            agentdash_application_agentrun::ApplicationError::Internal(message) => {
                Self::Internal(message)
            }
        }
    }
}

impl From<agentdash_application_lifecycle::WorkflowApplicationError> for ApplicationError {
    fn from(error: agentdash_application_lifecycle::WorkflowApplicationError) -> Self {
        match error {
            agentdash_application_lifecycle::WorkflowApplicationError::BadRequest(message)
            | agentdash_application_lifecycle::WorkflowApplicationError::ModelRequired(message) => {
                Self::BadRequest(message)
            }
            agentdash_application_lifecycle::WorkflowApplicationError::NotFound(message) => {
                Self::NotFound(message)
            }
            agentdash_application_lifecycle::WorkflowApplicationError::Conflict(message) => {
                Self::Conflict(message)
            }
            agentdash_application_lifecycle::WorkflowApplicationError::Internal(message) => {
                Self::Internal(message)
            }
        }
    }
}

impl From<std::io::Error> for ApplicationError {
    fn from(error: std::io::Error) -> Self {
        let context = DiagnosticErrorContext::new("application.error_conversion", "io")
            .with_field("error_source", "std_io");
        diag_error!(
            Error,
            Subsystem::Infra,
            context = &context,
            error = &error,
            error_source = "std_io",
            "application IO error"
        );
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

impl From<crate::backend::BackendAuthorizationError> for ApplicationError {
    fn from(error: crate::backend::BackendAuthorizationError) -> Self {
        match error {
            crate::backend::BackendAuthorizationError::Domain(error) => Self::from(error),
            crate::backend::BackendAuthorizationError::Forbidden { .. } => {
                Self::Forbidden(error.to_string())
            }
        }
    }
}
