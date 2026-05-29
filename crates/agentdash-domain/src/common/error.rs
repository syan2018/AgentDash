use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("实体未找到: {entity} (id={id})")]
    NotFound { entity: &'static str, id: String },

    #[error("资源冲突: {entity}.{constraint}: {message}")]
    Conflict {
        entity: &'static str,
        constraint: &'static str,
        message: String,
    },

    #[error("操作被拒绝: {action}: {reason}")]
    Forbidden {
        action: &'static str,
        reason: String,
    },

    #[error("状态迁移非法: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("配置无效: {0}")]
    InvalidConfig(String),

    #[error("数据库操作失败: {operation}")]
    Database {
        operation: &'static str,
        message: String,
    },
}
