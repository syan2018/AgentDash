use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 单条设置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}

/// 设置仓储接口
///
/// 提供键值对形式的持久化配置存储，支持按 category 前缀过滤。
#[async_trait::async_trait]
pub trait SettingsRepository: Send + Sync {
    /// 列出所有设置，可选按 `category_prefix` 前缀过滤
    async fn list(&self, category_prefix: Option<&str>)
    -> Result<Vec<Setting>, crate::DomainError>;

    /// 获取指定 key 的设置
    async fn get(&self, key: &str) -> Result<Option<Setting>, crate::DomainError>;

    /// 写入单条设置（不存在则创建，存在则更新）
    async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), crate::DomainError>;

    /// 批量写入设置
    async fn set_batch(
        &self,
        entries: &[(String, serde_json::Value)],
    ) -> Result<(), crate::DomainError>;

    /// 删除指定 key 的设置，返回是否有行被删除
    async fn delete(&self, key: &str) -> Result<bool, crate::DomainError>;
}
