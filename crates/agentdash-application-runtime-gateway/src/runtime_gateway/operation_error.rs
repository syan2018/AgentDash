use agentdash_domain::operation::OperationRef;

use super::OperationActorKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationExecutionErrorKind {
    InvalidRequest,
    AuthorityChanged,
    Denied,
    Unavailable,
    Cancelled,
    DeadlineExceeded,
    ProviderFailed,
    InvalidOutput,
    ResultStoreFailed,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum OperationExecutionError {
    #[error("Operation 请求非法: {message}")]
    InvalidRequest { message: String },
    #[error("Operation descriptor 非法 ({field}): {message}")]
    InvalidDescriptor {
        field: &'static str,
        message: String,
    },
    #[error("Operation authority revision 已变化: expected={expected}, current={current}")]
    AuthorityChanged { expected: String, current: String },
    #[error("Operation descriptor 在 admission 期间变化: {operation_ref:?}")]
    DescriptorChanged { operation_ref: OperationRef },
    #[error("Operation 不在当前 actor surface: {operation_ref:?}")]
    OperationUnavailable { operation_ref: OperationRef },
    #[error("Operation actor 被拒绝: {actor_kind:?}")]
    ActorDenied { actor_kind: OperationActorKind },
    #[error("Operation capability 被拒绝，缺少: {missing:?}")]
    CapabilitiesDenied { missing: Vec<String> },
    #[error("Operation replay policy 不允许持久 effect 重放")]
    ReplayDenied,
    #[error("Operation input schema 校验失败: {message}")]
    InputSchemaViolation { message: String },
    #[error("Operation 当前不可用 ({code}): {message}")]
    NotReady { code: String, message: String },
    #[error("Operation 已取消")]
    Cancelled,
    #[error("Operation deadline 已到期")]
    DeadlineExceeded,
    #[error("Operation provider 执行失败: {message}")]
    ProviderFailed { message: String },
    #[error("Operation output schema 校验失败: {message}")]
    OutputSchemaViolation { message: String },
    #[error("Operation output 编码失败: {message}")]
    OutputEncoding { message: String },
    #[error("Operation output 超限: actual={actual}, limit={limit}")]
    OutputTooLarge { actual: usize, limit: usize },
    #[error("Operation result store 失败: {message}")]
    ResultStoreFailed { message: String },
}

impl OperationExecutionError {
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest {
            message: message.into(),
        }
    }

    pub fn provider_failed(message: impl Into<String>) -> Self {
        Self::ProviderFailed {
            message: message.into(),
        }
    }

    pub fn result_store_failed(message: impl Into<String>) -> Self {
        Self::ResultStoreFailed {
            message: message.into(),
        }
    }

    pub fn kind(&self) -> OperationExecutionErrorKind {
        match self {
            Self::InvalidRequest { .. }
            | Self::InvalidDescriptor { .. }
            | Self::InputSchemaViolation { .. } => OperationExecutionErrorKind::InvalidRequest,
            Self::AuthorityChanged { .. } | Self::DescriptorChanged { .. } => {
                OperationExecutionErrorKind::AuthorityChanged
            }
            Self::ActorDenied { .. } | Self::CapabilitiesDenied { .. } | Self::ReplayDenied => {
                OperationExecutionErrorKind::Denied
            }
            Self::OperationUnavailable { .. } | Self::NotReady { .. } => {
                OperationExecutionErrorKind::Unavailable
            }
            Self::Cancelled => OperationExecutionErrorKind::Cancelled,
            Self::DeadlineExceeded => OperationExecutionErrorKind::DeadlineExceeded,
            Self::ProviderFailed { .. } => OperationExecutionErrorKind::ProviderFailed,
            Self::OutputSchemaViolation { .. }
            | Self::OutputEncoding { .. }
            | Self::OutputTooLarge { .. } => OperationExecutionErrorKind::InvalidOutput,
            Self::ResultStoreFailed { .. } => OperationExecutionErrorKind::ResultStoreFailed,
        }
    }

    pub fn code(&self) -> &'static str {
        match self.kind() {
            OperationExecutionErrorKind::InvalidRequest => "invalid_request",
            OperationExecutionErrorKind::AuthorityChanged => "authority_changed",
            OperationExecutionErrorKind::Denied => "denied",
            OperationExecutionErrorKind::Unavailable => "unavailable",
            OperationExecutionErrorKind::Cancelled => "cancelled",
            OperationExecutionErrorKind::DeadlineExceeded => "deadline_exceeded",
            OperationExecutionErrorKind::ProviderFailed => "provider_failed",
            OperationExecutionErrorKind::InvalidOutput => "invalid_output",
            OperationExecutionErrorKind::ResultStoreFailed => "result_store_failed",
        }
    }
}
