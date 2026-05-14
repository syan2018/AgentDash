use sqlx::PgPool;

use agentdash_domain::backend::{
    BackendConfig, BackendRepository, BackendType, LocalBackendClaim, UserPreferences, ViewConfig,
};
use agentdash_domain::common::error::DomainError;

pub struct PostgresBackendRepository {
    pool: PgPool,
}

impl PostgresBackendRepository {
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
                owner_user_id TEXT,
                profile_id TEXT,
                device_id TEXT,
                device JSONB NOT NULL DEFAULT '{}'::jsonb,
                last_claimed_at TIMESTAMPTZ,
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

            ALTER TABLE backends ADD COLUMN IF NOT EXISTS owner_user_id TEXT;
            ALTER TABLE backends ADD COLUMN IF NOT EXISTS profile_id TEXT;
            ALTER TABLE backends ADD COLUMN IF NOT EXISTS device_id TEXT;
            ALTER TABLE backends ADD COLUMN IF NOT EXISTS device JSONB NOT NULL DEFAULT '{}'::jsonb;
            ALTER TABLE backends ADD COLUMN IF NOT EXISTS last_claimed_at TIMESTAMPTZ;

            CREATE UNIQUE INDEX IF NOT EXISTS idx_backends_local_owner_profile_device
                ON backends (owner_user_id, profile_id, device_id)
                WHERE backend_type = 'local'
                  AND owner_user_id IS NOT NULL
                  AND profile_id IS NOT NULL
                  AND device_id IS NOT NULL;
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl BackendRepository for PostgresBackendRepository {
    async fn add_backend(&self, config: &BackendConfig) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO backends (
                id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                profile_id, device_id, device, last_claimed_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               endpoint = excluded.endpoint,
               auth_token = excluded.auth_token,
               enabled = excluded.enabled,
               backend_type = excluded.backend_type,
               owner_user_id = excluded.owner_user_id,
               profile_id = excluded.profile_id,
               device_id = excluded.device_id,
               device = excluded.device,
               last_claimed_at = excluded.last_claimed_at",
        )
        .bind(&config.id)
        .bind(&config.name)
        .bind(&config.endpoint)
        .bind(&config.auth_token)
        .bind(config.enabled)
        .bind(serde_json::to_string(&config.backend_type)?.trim_matches('"'))
        .bind(&config.owner_user_id)
        .bind(&config.profile_id)
        .bind(&config.device_id)
        .bind(&config.device)
        .bind(config.last_claimed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError> {
        let rows = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                    profile_id, device_id, COALESCE(device, '{}'::jsonb) AS device, last_claimed_at
             FROM backends ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn get_backend(&self, id: &str) -> Result<BackendConfig, DomainError> {
        let row = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                    profile_id, device_id, COALESCE(device, '{}'::jsonb) AS device, last_claimed_at
             FROM backends WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "backend",
            id: id.to_string(),
        })?;

        row.try_into()
    }

    async fn get_backend_by_auth_token(&self, token: &str) -> Result<BackendConfig, DomainError> {
        let rows = sqlx::query_as::<_, BackendRow>(
            "SELECT id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                    profile_id, device_id, COALESCE(device, '{}'::jsonb) AS device, last_claimed_at
             FROM backends WHERE auth_token = $1",
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
                .try_into()?),
            _ => Err(DomainError::InvalidConfig(
                "检测到重复 backend auth_token 配置".to_string(),
            )),
        }
    }

    async fn ensure_local_backend(
        &self,
        claim: &LocalBackendClaim,
    ) -> Result<BackendConfig, DomainError> {
        let row = sqlx::query_as::<_, BackendRow>(
            r#"
            INSERT INTO backends (
                id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                profile_id, device_id, device, last_claimed_at
            )
            VALUES ($1, $2, $3, $4, TRUE, 'local', $5, $6, $7, $8, now())
            ON CONFLICT (owner_user_id, profile_id, device_id)
                WHERE backend_type = 'local'
                  AND owner_user_id IS NOT NULL
                  AND profile_id IS NOT NULL
                  AND device_id IS NOT NULL
            DO UPDATE SET
                name = EXCLUDED.name,
                endpoint = EXCLUDED.endpoint,
                auth_token = CASE
                    WHEN $9 THEN EXCLUDED.auth_token
                    ELSE COALESCE(backends.auth_token, EXCLUDED.auth_token)
                END,
                enabled = TRUE,
                backend_type = 'local',
                device = EXCLUDED.device,
                last_claimed_at = now()
            RETURNING id, name, endpoint, auth_token, enabled, backend_type, owner_user_id,
                      profile_id, device_id, COALESCE(device, '{}'::jsonb) AS device, last_claimed_at
            "#,
        )
        .bind(&claim.backend_id)
        .bind(&claim.name)
        .bind(&claim.endpoint)
        .bind(&claim.auth_token)
        .bind(&claim.owner_user_id)
        .bind(&claim.profile_id)
        .bind(&claim.device_id)
        .bind(&claim.device)
        .bind(claim.rotate_token)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.try_into()
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

        rows.into_iter().map(TryInto::try_into).collect()
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
    owner_user_id: Option<String>,
    profile_id: Option<String>,
    device_id: Option<String>,
    device: serde_json::Value,
    last_claimed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl TryFrom<BackendRow> for BackendConfig {
    type Error = DomainError;

    fn try_from(row: BackendRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            name: row.name,
            endpoint: row.endpoint,
            auth_token: row.auth_token,
            enabled: row.enabled,
            backend_type: parse_backend_type(&row.backend_type)?,
            owner_user_id: row.owner_user_id,
            profile_id: row.profile_id,
            device_id: row.device_id,
            device: row.device,
            last_claimed_at: row.last_claimed_at,
        })
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

impl TryFrom<ViewRow> for ViewConfig {
    type Error = DomainError;

    fn try_from(row: ViewRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            name: row.name,
            backend_ids: parse_json_column(&row.backend_ids, "views.backend_ids")?,
            filters: parse_json_column(&row.filters, "views.filters")?,
            sort_by: row.sort_by,
        })
    }
}

fn parse_backend_type(raw: &str) -> Result<BackendType, DomainError> {
    match raw {
        "local" => Ok(BackendType::Local),
        "remote" => Ok(BackendType::Remote),
        _ => Err(DomainError::InvalidConfig(format!(
            "backends.backend_type: 未知值 `{raw}`"
        ))),
    }
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    fn backend(id: &str, token: Option<&str>) -> BackendConfig {
        BackendConfig {
            id: id.to_string(),
            name: format!("backend-{id}"),
            endpoint: "ws://localhost".to_string(),
            auth_token: token.map(str::to_string),
            enabled: true,
            backend_type: BackendType::Local,
            owner_user_id: None,
            profile_id: None,
            device_id: None,
            device: serde_json::json!({}),
            last_claimed_at: None,
        }
    }

    async fn new_repo() -> Option<PostgresBackendRepository> {
        let pool = match test_pg_pool("backend_repository").await {
            Some(pool) => pool,
            None => return None,
        };
        let repo = PostgresBackendRepository::new(pool);
        repo.initialize().await.expect("应能初始化 schema");
        Some(repo)
    }

    #[tokio::test]
    async fn get_backend_by_auth_token_returns_matching_backend() {
        let Some(repo) = new_repo().await else {
            return;
        };
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
        let Some(repo) = new_repo().await else {
            return;
        };
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
        let Some(repo) = new_repo().await else {
            return;
        };
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

    #[tokio::test]
    async fn ensure_local_backend_reuses_existing_profile_device_token() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let claim = LocalBackendClaim {
            owner_user_id: "user-a".to_string(),
            profile_id: "desktop-local".to_string(),
            device_id: "device-a".to_string(),
            backend_id: "local-first".to_string(),
            name: "Desktop A".to_string(),
            endpoint: "ws://localhost/ws/backend".to_string(),
            auth_token: "token-a".to_string(),
            device: serde_json::json!({ "os": "windows" }),
            rotate_token: false,
        };
        let first = repo
            .ensure_local_backend(&claim)
            .await
            .expect("首次 ensure 应创建 backend");
        assert_eq!(first.id, "local-first");
        assert_eq!(first.auth_token.as_deref(), Some("token-a"));

        let mut second_claim = claim.clone();
        second_claim.backend_id = "local-second".to_string();
        second_claim.auth_token = "token-b".to_string();
        second_claim.name = "Desktop A renamed".to_string();
        let second = repo
            .ensure_local_backend(&second_claim)
            .await
            .expect("同一 profile/device 应复用 backend");

        assert_eq!(second.id, "local-first");
        assert_eq!(second.name, "Desktop A renamed");
        assert_eq!(second.auth_token.as_deref(), Some("token-a"));
        assert_eq!(second.owner_user_id.as_deref(), Some("user-a"));
        assert_eq!(second.profile_id.as_deref(), Some("desktop-local"));
        assert_eq!(second.device_id.as_deref(), Some("device-a"));
    }

    #[tokio::test]
    async fn ensure_local_backend_can_rotate_token() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let mut claim = LocalBackendClaim {
            owner_user_id: "user-a".to_string(),
            profile_id: "desktop-local".to_string(),
            device_id: "device-a".to_string(),
            backend_id: "local-first".to_string(),
            name: "Desktop A".to_string(),
            endpoint: "ws://localhost/ws/backend".to_string(),
            auth_token: "token-a".to_string(),
            device: serde_json::json!({}),
            rotate_token: false,
        };
        repo.ensure_local_backend(&claim)
            .await
            .expect("首次 ensure 应创建 backend");

        claim.auth_token = "token-b".to_string();
        claim.rotate_token = true;
        let rotated = repo
            .ensure_local_backend(&claim)
            .await
            .expect("显式 rotate 应替换 token");

        assert_eq!(rotated.auth_token.as_deref(), Some("token-b"));
    }
}
