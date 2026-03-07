use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::SqlitePool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::settings::{Setting, SettingsRepository};

pub struct SqliteSettingsRepository {
    pool: SqlitePool,
}

impl SqliteSettingsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl SettingsRepository for SqliteSettingsRepository {
    async fn list(&self, category_prefix: Option<&str>) -> Result<Vec<Setting>, DomainError> {
        let rows = match category_prefix {
            Some(prefix) => {
                let pattern = format!("{prefix}%");
                sqlx::query_as::<_, SettingRow>(
                    "SELECT key, value, updated_at FROM settings WHERE key LIKE ? ORDER BY key",
                )
                .bind(pattern)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, SettingRow>(
                    "SELECT key, value, updated_at FROM settings ORDER BY key",
                )
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(Setting::try_from).collect()
    }

    async fn get(&self, key: &str) -> Result<Option<Setting>, DomainError> {
        let row = sqlx::query_as::<_, SettingRow>(
            "SELECT key, value, updated_at FROM settings WHERE key = ?",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(Setting::try_from).transpose()
    }

    async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), DomainError> {
        let value_str = serde_json::to_string(&value)?;
        sqlx::query(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?, ?, datetime('now'))",
        )
        .bind(key)
        .bind(value_str)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn set_batch(&self, entries: &[(String, serde_json::Value)]) -> Result<(), DomainError> {
        for (key, value) in entries {
            self.set(key, value.clone()).await?;
        }
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }
}

// --- SQLx 行映射 ---

#[derive(sqlx::FromRow)]
struct SettingRow {
    key: String,
    value: String,
    updated_at: String,
}

impl TryFrom<SettingRow> for Setting {
    type Error = DomainError;

    fn try_from(row: SettingRow) -> Result<Self, Self::Error> {
        let value: serde_json::Value = serde_json::from_str(&row.value)?;

        let naive = NaiveDateTime::parse_from_str(&row.updated_at, "%Y-%m-%d %H:%M:%S")
            .map_err(|e| DomainError::InvalidConfig(format!("日期解析失败: {e}")))?;
        let updated_at: DateTime<Utc> = DateTime::from_naive_utc_and_offset(naive, Utc);

        Ok(Setting {
            key: row.key,
            value,
            updated_at,
        })
    }
}
