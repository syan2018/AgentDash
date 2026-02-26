use sqlx::SqlitePool;

use agentdash_domain::backend::{BackendConfig, BackendType, BackendRepository, ViewConfig, UserPreferences};
use agentdash_domain::common::error::DomainError;

pub struct SqliteBackendRepository {
    pool: SqlitePool,
}

impl SqliteBackendRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
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
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl BackendRepository for SqliteBackendRepository {
    async fn add_backend(&self, config: &BackendConfig) -> Result<(), DomainError> {
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
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError> {
        let rows = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type FROM backends ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn get_backend(&self, id: &str) -> Result<BackendConfig, DomainError> {
        let row = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type FROM backends WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?
        .ok_or_else(|| DomainError::NotFound { entity: "backend", id: id.to_string() })?;

        Ok(row.into())
    }

    async fn remove_backend(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM backends WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError> {
        let rows = sqlx::query_as::<_, ViewRow>(
            "SELECT id, name, backend_ids, filters, sort_by FROM views ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn save_view(&self, view: &ViewConfig) -> Result<(), DomainError> {
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
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_preferences(&self) -> Result<UserPreferences, DomainError> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT value FROM user_preferences WHERE key = 'prefs'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        match row {
            Some((json,)) => Ok(serde_json::from_str(&json)?),
            None => Ok(UserPreferences::default()),
        }
    }

    async fn save_preferences(&self, prefs: &UserPreferences) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT OR REPLACE INTO user_preferences (key, value) VALUES ('prefs', ?)",
        )
        .bind(serde_json::to_string(prefs)?)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
