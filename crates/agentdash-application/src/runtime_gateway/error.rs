use super::types::{RuntimeActionKey, RuntimeTrace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeInvocationErrorKind {
    InvalidRequest,
    CapabilityDenied,
    Conflict,
    ProviderUnavailable,
    ProviderFailed,
    Timeout,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RuntimeInvocationError {
    #[error("runtime invocation 请求非法: {message}")]
    InvalidRequest {
        message: String,
        #[source]
        source: Option<RuntimeActionKeyErrorSource>,
        trace: Option<RuntimeTrace>,
    },
    #[error("runtime action 被拒绝: {message}")]
    CapabilityDenied {
        message: String,
        trace: Option<RuntimeTrace>,
    },
    #[error("runtime action 当前不可执行: {message}")]
    Conflict {
        message: String,
        trace: Option<RuntimeTrace>,
    },
    #[error("runtime provider 不可用: {action_key}")]
    ProviderUnavailable {
        action_key: RuntimeActionKey,
        trace: Option<RuntimeTrace>,
    },
    #[error("runtime provider 执行失败: {message}")]
    ProviderFailed {
        message: String,
        trace: Option<RuntimeTrace>,
    },
    #[error("runtime invocation 超时: {message}")]
    Timeout {
        message: String,
        trace: Option<RuntimeTrace>,
    },
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct RuntimeActionKeyErrorSource(pub String);

impl RuntimeInvocationError {
    pub fn invalid_request(message: impl Into<String>, trace: Option<RuntimeTrace>) -> Self {
        Self::InvalidRequest {
            message: message.into(),
            source: None,
            trace,
        }
    }

    pub fn invalid_action_key(
        message: impl Into<String>,
        source: impl Into<String>,
        trace: Option<RuntimeTrace>,
    ) -> Self {
        Self::InvalidRequest {
            message: message.into(),
            source: Some(RuntimeActionKeyErrorSource(source.into())),
            trace,
        }
    }

    pub fn capability_denied(message: impl Into<String>, trace: Option<RuntimeTrace>) -> Self {
        Self::CapabilityDenied {
            message: message.into(),
            trace,
        }
    }

    pub fn provider_failed(message: impl Into<String>, trace: Option<RuntimeTrace>) -> Self {
        Self::ProviderFailed {
            message: message.into(),
            trace,
        }
    }

    pub fn conflict(message: impl Into<String>, trace: Option<RuntimeTrace>) -> Self {
        Self::Conflict {
            message: message.into(),
            trace,
        }
    }

    pub fn timeout(message: impl Into<String>, trace: Option<RuntimeTrace>) -> Self {
        Self::Timeout {
            message: message.into(),
            trace,
        }
    }

    pub fn kind(&self) -> RuntimeInvocationErrorKind {
        match self {
            RuntimeInvocationError::InvalidRequest { .. } => {
                RuntimeInvocationErrorKind::InvalidRequest
            }
            RuntimeInvocationError::CapabilityDenied { .. } => {
                RuntimeInvocationErrorKind::CapabilityDenied
            }
            RuntimeInvocationError::Conflict { .. } => RuntimeInvocationErrorKind::Conflict,
            RuntimeInvocationError::ProviderUnavailable { .. } => {
                RuntimeInvocationErrorKind::ProviderUnavailable
            }
            RuntimeInvocationError::ProviderFailed { .. } => {
                RuntimeInvocationErrorKind::ProviderFailed
            }
            RuntimeInvocationError::Timeout { .. } => RuntimeInvocationErrorKind::Timeout,
        }
    }

    pub fn trace(&self) -> Option<&RuntimeTrace> {
        match self {
            RuntimeInvocationError::InvalidRequest { trace, .. }
            | RuntimeInvocationError::CapabilityDenied { trace, .. }
            | RuntimeInvocationError::Conflict { trace, .. }
            | RuntimeInvocationError::ProviderUnavailable { trace, .. }
            | RuntimeInvocationError::ProviderFailed { trace, .. }
            | RuntimeInvocationError::Timeout { trace, .. } => trace.as_ref(),
        }
    }

    pub fn with_trace_if_missing(self, fallback: RuntimeTrace) -> Self {
        match self {
            RuntimeInvocationError::InvalidRequest {
                message,
                source,
                trace,
            } => RuntimeInvocationError::InvalidRequest {
                message,
                source,
                trace: trace.or(Some(fallback)),
            },
            RuntimeInvocationError::CapabilityDenied { message, trace } => {
                RuntimeInvocationError::CapabilityDenied {
                    message,
                    trace: trace.or(Some(fallback)),
                }
            }
            RuntimeInvocationError::Conflict { message, trace } => {
                RuntimeInvocationError::Conflict {
                    message,
                    trace: trace.or(Some(fallback)),
                }
            }
            RuntimeInvocationError::ProviderUnavailable { action_key, trace } => {
                RuntimeInvocationError::ProviderUnavailable {
                    action_key,
                    trace: trace.or(Some(fallback)),
                }
            }
            RuntimeInvocationError::ProviderFailed { message, trace } => {
                RuntimeInvocationError::ProviderFailed {
                    message,
                    trace: trace.or(Some(fallback)),
                }
            }
            RuntimeInvocationError::Timeout { message, trace } => RuntimeInvocationError::Timeout {
                message,
                trace: trace.or(Some(fallback)),
            },
        }
    }
}
