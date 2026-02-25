use sqlx::SqlitePool;

use crate::config::{BackendConfig, BackendType, UserPreferences, ViewConfig};
use crate::error::CoordinatorError;

/// CoordinatorManager — 中控层管理器
///
/// 职责：
/// 1. 管理后端连接列表（增删改查）
/// 2. 管理视图配置
/// 3. 存储用户偏好
pub struct CoordinatorManager {
    pool: SqlitePool,
}

impl CoordinatorManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 初始化中控层数据库表
    pub async fn initialize(&self) -> Result<(), CoordinatorError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS backends (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                endpoint TEXT NOT NULL,
                auth_token TEXT,
                enabled INTEGER NOT NULL DEFAULT 1,
                backend_type TEXT NOT NULL DEFAULT 'local',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS views (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                backend_ids TEXT NOT NULL DEFAULT '[]',
                filters TEXT NOT NULL DEFAULT '{}',
                sort_by TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS user_preferences (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // --- 后端管理 ---

    pub async fn add_backend(&self, config: &BackendConfig) -> Result<(), CoordinatorError> {
        sqlx::query(
            "INSERT INTO backends (id, name, endpoint, auth_token, enabled, backend_type)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&config.id)
        .bind(&config.name)
        .bind(&config.endpoint)
        .bind(&config.auth_token)
        .bind(config.enabled)
        .bind(serde_json::to_string(&config.backend_type)?.trim_matches('"'))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_backends(&self) -> Result<Vec<BackendConfig>, CoordinatorError> {
        let rows = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type FROM backends ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn get_backend(&self, id: &str) -> Result<BackendConfig, CoordinatorError> {
        let row = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type FROM backends WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| CoordinatorError::BackendNotFound(id.to_string()))?;

        Ok(row.into())
    }

    pub async fn remove_backend(&self, id: &str) -> Result<(), CoordinatorError> {
        sqlx::query("DELETE FROM backends WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // --- 视图管理 ---

    pub async fn list_views(&self) -> Result<Vec<ViewConfig>, CoordinatorError> {
        let rows = sqlx::query_as::<_, ViewRow>(
            "SELECT id, name, backend_ids, filters, sort_by FROM views ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    pub async fn save_view(&self, view: &ViewConfig) -> Result<(), CoordinatorError> {
        sqlx::query(
            "INSERT OR REPLACE INTO views (id, name, backend_ids, filters, sort_by)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&view.id)
        .bind(&view.name)
        .bind(serde_json::to_string(&view.backend_ids)?)
        .bind(view.filters.to_string())
        .bind(&view.sort_by)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // --- 用户偏好 ---

    pub async fn get_preferences(&self) -> Result<UserPreferences, CoordinatorError> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT value FROM user_preferences WHERE key = 'prefs'",
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some((json,)) => Ok(serde_json::from_str(&json)?),
            None => Ok(UserPreferences::default()),
        }
    }

    pub async fn save_preferences(
        &self,
        prefs: &UserPreferences,
    ) -> Result<(), CoordinatorError> {
        sqlx::query(
            "INSERT OR REPLACE INTO user_preferences (key, value) VALUES ('prefs', ?)",
        )
        .bind(serde_json::to_string(prefs)?)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// --- SQLx 行映射 ---

#[derive(sqlx::FromRow)]
struct BackendRow {
    id: String,
    name: String,
    endpoint: String,
    auth_token: Option<String>,
    enabled: bool,
    backend_type: String,
}

impl From<BackendRow> for BackendConfig {
    fn from(row: BackendRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            endpoint: row.endpoint,
            auth_token: row.auth_token,
            enabled: row.enabled,
            backend_type: match row.backend_type.as_str() {
                "remote" => BackendType::Remote,
                _ => BackendType::Local,
            },
        }
    }
}

#[derive(sqlx::FromRow)]
struct ViewRow {
    id: String,
    name: String,
    backend_ids: String,
    filters: String,
    sort_by: Option<String>,
}

impl From<ViewRow> for ViewConfig {
    fn from(row: ViewRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            backend_ids: serde_json::from_str(&row.backend_ids).unwrap_or_default(),
            filters: serde_json::from_str(&row.filters).unwrap_or_default(),
            sort_by: row.sort_by,
        }
    }
}
