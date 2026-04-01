use sqlx::PgPool;

use agentdash_domain::backend::{
    BackendConfig, BackendRepository, BackendType, UserPreferences, ViewConfig,
};
use agentdash_domain::common::error::DomainError;

pub struct SqliteBackendRepository {
    pool: PgPool,
}

impl SqliteBackendRepository {
    pub fn new(pool: PgPool) -> Self {
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
                created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
            );

            CREATE TABLE IF NOT EXISTS views (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                backend_ids TEXT NOT NULL DEFAULT '[]',
                filters TEXT NOT NULL DEFAULT '{}',
                sort_by TEXT,
                created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
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
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               endpoint = excluded.endpoint,
               auth_token = excluded.auth_token,
               enabled = excluded.enabled,
               backend_type = excluded.backend_type",
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
            "SELECT id, name, endpoint, auth_token, enabled, backend_type FROM backends WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?
        .ok_or_else(|| DomainError::NotFound { entity: "backend", id: id.to_string() })?;

        Ok(row.into())
    }

    async fn get_backend_by_auth_token(&self, token: &str) -> Result<BackendConfig, DomainError> {
        let rows = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type FROM backends WHERE auth_token = $1",
        )
        .bind(token)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        match rows.len() {
            0 => Err(DomainError::NotFound {
                entity: "backend_auth_token",
                id: token.to_string(),
            }),
            1 => Ok(rows
                .into_iter()
                .next()
                .expect("rows.len() == 1 时必须存在")
                .into()),
            _ => Err(DomainError::InvalidConfig(
                "检测到重复 backend auth_token 配置".to_string(),
            )),
        }
    }

    async fn remove_backend(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM backends WHERE id = $1")
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
            "INSERT INTO views (id, name, backend_ids, filters, sort_by)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                backend_ids = EXCLUDED.backend_ids,
                filters = EXCLUDED.filters,
                sort_by = EXCLUDED.sort_by",
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
            "INSERT INTO user_preferences (key, value) VALUES ('prefs', $1)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    fn backend(id: &str, token: Option<&str>) -> BackendConfig {
        BackendConfig {
            id: id.to_string(),
            name: format!("backend-{id}"),
            endpoint: "ws://localhost".to_string(),
            auth_token: token.map(str::to_string),
            enabled: true,
            backend_type: BackendType::Local,
        }
    }

    async fn new_repo() -> SqliteBackendRepository {
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("运行测试前需设置 TEST_DATABASE_URL");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("应能连接测试 PostgreSQL");
        let repo = SqliteBackendRepository::new(pool);
        repo.initialize().await.expect("应能初始化 schema");
        repo
    }

    #[tokio::test]
    async fn get_backend_by_auth_token_returns_matching_backend() {
        let repo = new_repo().await;
        repo.add_backend(&backend("local-a", Some("secret-a")))
            .await
            .expect("应能插入 backend");

        let found = repo
            .get_backend_by_auth_token("secret-a")
            .await
            .expect("应能按 token 查到 backend");

        assert_eq!(found.id, "local-a");
    }

    #[tokio::test]
    async fn get_backend_by_auth_token_rejects_duplicate_token_binding() {
        let repo = new_repo().await;
        repo.add_backend(&backend("local-a", Some("shared-token")))
            .await
            .expect("应能插入首个 backend");
        repo.add_backend(&backend("local-b", Some("shared-token")))
            .await
            .expect("当前 schema 允许重复 token，用于验证运行时收口");

        let err = repo
            .get_backend_by_auth_token("shared-token")
            .await
            .expect_err("重复 token 绑定应在查询时失败");

        assert!(matches!(err, DomainError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn add_backend_overwrites_existing_backend_with_same_id() {
        let repo = new_repo().await;
        repo.add_backend(&backend("local-a", Some("secret-a")))
            .await
            .expect("应能插入 backend");

        let mut updated = backend("local-a", Some("secret-b"));
        updated.name = "renamed".to_string();
        repo.add_backend(&updated)
            .await
            .expect("相同 id 应覆盖保存");

        let found = repo
            .get_backend("local-a")
            .await
            .expect("应能取回覆盖后的 backend");

        assert_eq!(found.name, "renamed");
        assert_eq!(found.auth_token.as_deref(), Some("secret-b"));
    }
}
