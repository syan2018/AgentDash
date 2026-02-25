use thiserror::Error;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("数据库操作失败: {0}")]
    Database(#[from] sqlx::Error),

    #[error("实体未找到: {entity} (id={id})")]
    NotFound { entity: &'static str, id: String },

    #[error("状态迁移非法: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),
}
