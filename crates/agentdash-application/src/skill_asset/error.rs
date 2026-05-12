#[derive(Debug, thiserror::Error)]
pub enum SkillAssetApplicationError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Internal(String),
}

impl From<agentdash_domain::DomainError> for SkillAssetApplicationError {
    fn from(error: agentdash_domain::DomainError) -> Self {
        match error {
            agentdash_domain::DomainError::NotFound { .. } => Self::NotFound(error.to_string()),
            agentdash_domain::DomainError::InvalidConfig(message) => Self::Internal(message),
            agentdash_domain::DomainError::Serialization(error) => {
                Self::BadRequest(error.to_string())
            }
            other => Self::Internal(other.to_string()),
        }
    }
}
