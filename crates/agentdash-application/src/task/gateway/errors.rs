//! Gateway 层错误映射 helpers。
//!
//! 职责：将底层 Domain / Connector / 通用错误映射到 `TaskExecutionError`。
//! 供 gateway 子模块统一复用，避免错误转换散落各处。

use agentdash_domain::DomainError;
use agentdash_spi::ConnectorError;

use crate::task::execution::TaskExecutionError;

pub fn map_domain_error(err: DomainError) -> TaskExecutionError {
    match &err {
        DomainError::NotFound { .. } => TaskExecutionError::NotFound(err.to_string()),
        DomainError::InvalidTransition { .. } => TaskExecutionError::BadRequest(err.to_string()),
        DomainError::InvalidConfig(_) => TaskExecutionError::BadRequest(err.to_string()),
        _ => TaskExecutionError::Internal(err.to_string()),
    }
}

pub fn map_internal_error<E: ToString>(err: E) -> TaskExecutionError {
    TaskExecutionError::Internal(err.to_string())
}

pub fn map_connector_error(err: ConnectorError) -> TaskExecutionError {
    match err {
        ConnectorError::InvalidConfig(message) => TaskExecutionError::BadRequest(message),
        ConnectorError::Runtime(message) => TaskExecutionError::Conflict(message),
        other => TaskExecutionError::Internal(other.to_string()),
    }
}
