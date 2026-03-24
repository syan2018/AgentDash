use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingScopeKind {
    System,
    User,
    Project,
}

impl SettingScopeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingScope {
    pub kind: SettingScopeKind,
    pub scope_id: Option<String>,
}

impl SettingScope {
    pub fn system() -> Self {
        Self {
            kind: SettingScopeKind::System,
            scope_id: None,
        }
    }

    pub fn user(user_id: impl Into<String>) -> Self {
        Self {
            kind: SettingScopeKind::User,
            scope_id: Some(user_id.into()),
        }
    }

    pub fn project(project_id: impl Into<String>) -> Self {
        Self {
            kind: SettingScopeKind::Project,
            scope_id: Some(project_id.into()),
        }
    }

    pub fn storage_scope_id(&self) -> &str {
        self.scope_id.as_deref().unwrap_or("")
    }
}

/// 单条设置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub scope_kind: SettingScopeKind,
    pub scope_id: Option<String>,
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
    async fn list(
        &self,
        scope: &SettingScope,
        category_prefix: Option<&str>,
    ) -> Result<Vec<Setting>, crate::DomainError>;

    /// 获取指定 key 的设置
    async fn get(
        &self,
        scope: &SettingScope,
        key: &str,
    ) -> Result<Option<Setting>, crate::DomainError>;

    /// 写入单条设置（不存在则创建，存在则更新）
    async fn set(
        &self,
        scope: &SettingScope,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), crate::DomainError>;

    /// 批量写入设置
    async fn set_batch(
        &self,
        scope: &SettingScope,
        entries: &[(String, serde_json::Value)],
    ) -> Result<(), crate::DomainError>;

    /// 删除指定 key 的设置，返回是否有行被删除
    async fn delete(&self, scope: &SettingScope, key: &str) -> Result<bool, crate::DomainError>;
}
