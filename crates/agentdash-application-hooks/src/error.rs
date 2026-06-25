#[derive(Debug, thiserror::Error)]
pub enum HookApplicationError {
    #[error("invalid hook script config: {0}")]
    InvalidConfig(String),
    #[error("hook script evaluation failed: {0}")]
    Internal(String),
}
