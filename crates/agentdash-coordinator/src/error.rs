use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoordinatorError {
    #[error("数据库操作失败: {0}")]
    Database(#[from] sqlx::Error),

    #[error("后端未找到: {0}")]
    BackendNotFound(String),

    #[error("后端连接失败: {backend_id} - {reason}")]
    ConnectionFailed { backend_id: String, reason: String },

    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("配置无效: {0}")]
    InvalidConfig(String),
}
