//! 中继协议错误类型和错误码定义

use serde::{Deserialize, Serialize};

/// 中继协议错误（嵌入在消息中）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayError {
    pub code: RelayErrorCode,
    pub message: String,
}

impl RelayError {
    pub fn new(code: RelayErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn auth_failed(message: impl Into<String>) -> Self {
        Self::new(RelayErrorCode::AuthFailed, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(RelayErrorCode::NotFound, message)
    }

    pub fn session_busy(message: impl Into<String>) -> Self {
        Self::new(RelayErrorCode::SessionBusy, message)
    }

    pub fn executor_not_found(message: impl Into<String>) -> Self {
        Self::new(RelayErrorCode::ExecutorNotFound, message)
    }

    pub fn io_error(message: impl Into<String>) -> Self {
        Self::new(RelayErrorCode::IoError, message)
    }

    pub fn invalid_message(message: impl Into<String>) -> Self {
        Self::new(RelayErrorCode::InvalidMessage, message)
    }

    pub fn runtime_error(message: impl Into<String>) -> Self {
        Self::new(RelayErrorCode::RuntimeError, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(RelayErrorCode::Timeout, message)
    }
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for RelayError {}

/// 机器可读的错误码
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelayErrorCode {
    #[serde(rename = "AUTH_FAILED")]
    AuthFailed,
    #[serde(rename = "FORBIDDEN")]
    Forbidden,
    #[serde(rename = "NOT_FOUND")]
    NotFound,
    #[serde(rename = "SESSION_BUSY")]
    SessionBusy,
    #[serde(rename = "EXECUTOR_NOT_FOUND")]
    ExecutorNotFound,
    #[serde(rename = "EXECUTOR_UNAVAILABLE")]
    ExecutorUnavailable,
    #[serde(rename = "SPAWN_FAILED")]
    SpawnFailed,
    #[serde(rename = "RUNTIME_ERROR")]
    RuntimeError,
    #[serde(rename = "IO_ERROR")]
    IoError,
    #[serde(rename = "INVALID_MESSAGE")]
    InvalidMessage,
    #[serde(rename = "TIMEOUT")]
    Timeout,
}

impl RelayErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthFailed => "AUTH_FAILED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::SessionBusy => "SESSION_BUSY",
            Self::ExecutorNotFound => "EXECUTOR_NOT_FOUND",
            Self::ExecutorUnavailable => "EXECUTOR_UNAVAILABLE",
            Self::SpawnFailed => "SPAWN_FAILED",
            Self::RuntimeError => "RUNTIME_ERROR",
            Self::IoError => "IO_ERROR",
            Self::InvalidMessage => "INVALID_MESSAGE",
            Self::Timeout => "TIMEOUT",
        }
    }
}
