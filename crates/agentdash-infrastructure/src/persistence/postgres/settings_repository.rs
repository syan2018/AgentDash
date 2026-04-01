use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::settings::{Setting, SettingScope, SettingScopeKind, SettingsRepository};

pub struct SqliteSettingsRepository {
    pool: PgPool,
}

impl SqliteSettingsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        create_scoped_settings_table(&self.pool, "settings").await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl SettingsRepository for SqliteSettingsRepository {
    async fn list(
        &self,
        scope: &SettingScope,
        category_prefix: Option<&str>,
    ) -> Result<Vec<Setting>, DomainError> {
        let rows = match category_prefix {
            Some(prefix) => {
                let pattern = format!("{prefix}%");
                sqlx::query_as::<_, SettingRow>(
                    "SELECT scope_kind, scope_id, key, value, updated_at
                     FROM settings
                     WHERE scope_kind = $1 AND scope_id = $2 AND key LIKE $3
                     ORDER BY key",
                )
                .bind(scope.kind.as_str())
                .bind(scope.storage_scope_id())
                .bind(pattern)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, SettingRow>(
                    "SELECT scope_kind, scope_id, key, value, updated_at
                     FROM settings
                     WHERE scope_kind = $1 AND scope_id = $2
                     ORDER BY key",
                )
                .bind(scope.kind.as_str())
                .bind(scope.storage_scope_id())
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(Setting::try_from).collect()
    }

    async fn get(&self, scope: &SettingScope, key: &str) -> Result<Option<Setting>, DomainError> {
        let row = sqlx::query_as::<_, SettingRow>(
            "SELECT scope_kind, scope_id, key, value, updated_at
             FROM settings
             WHERE scope_kind = $1 AND scope_id = $2 AND key = $3",
        )
        .bind(scope.kind.as_str())
        .bind(scope.storage_scope_id())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(Setting::try_from).transpose()
    }

    async fn set(
        &self,
        scope: &SettingScope,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), DomainError> {
        let value_str = serde_json::to_string(&value)?;
        sqlx::query(
            "INSERT INTO settings (scope_kind, scope_id, key, value, updated_at)
             VALUES ($1, $2, $3, $4, now())
             ON CONFLICT (scope_kind, scope_id, key)
             DO UPDATE SET value = EXCLUDED.value, updated_at = EXCLUDED.updated_at",
        )
        .bind(scope.kind.as_str())
        .bind(scope.storage_scope_id())
        .bind(key)
        .bind(value_str)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn set_batch(
        &self,
        scope: &SettingScope,
        entries: &[(String, serde_json::Value)],
    ) -> Result<(), DomainError> {
        for (key, value) in entries {
            self.set(scope, key, value.clone()).await?;
        }
        Ok(())
    }

    async fn delete(&self, scope: &SettingScope, key: &str) -> Result<bool, DomainError> {
        let result =
            sqlx::query("DELETE FROM settings WHERE scope_kind = $1 AND scope_id = $2 AND key = $3")
                .bind(scope.kind.as_str())
                .bind(scope.storage_scope_id())
                .bind(key)
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct SettingRow {
    scope_kind: String,
    scope_id: String,
    key: String,
    value: String,
    updated_at: String,
}

impl TryFrom<SettingRow> for Setting {
    type Error = DomainError;

    fn try_from(row: SettingRow) -> Result<Self, Self::Error> {
        let value: serde_json::Value = serde_json::from_str(&row.value)?;

        let updated_at = super::parse_pg_timestamp(&row.updated_at);
        let scope_kind = parse_scope_kind(&row.scope_kind);

        Ok(Setting {
            scope_kind,
            scope_id: normalize_scope_id(scope_kind, row.scope_id),
            key: row.key,
            value,
            updated_at,
        })
    }
}


fn parse_scope_kind(value: &str) -> SettingScopeKind {
    match value {
        "user" => SettingScopeKind::User,
        "project" => SettingScopeKind::Project,
        _ => SettingScopeKind::System,
    }
}

fn normalize_scope_id(kind: SettingScopeKind, scope_id: String) -> Option<String> {
    match kind {
        SettingScopeKind::System => None,
        SettingScopeKind::User | SettingScopeKind::Project => {
            let trimmed = scope_id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
    }
}

async fn create_scoped_settings_table(pool: &PgPool, table: &str) -> Result<(), DomainError> {
    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {table} (
            scope_kind TEXT NOT NULL,
            scope_id TEXT NOT NULL DEFAULT '',
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
            PRIMARY KEY (scope_kind, scope_id, key)
        )"
    ))
    .execute(pool)
    .await
    .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    sqlx::query(&format!(
        "CREATE INDEX IF NOT EXISTS idx_{table}_scope_key ON {table}(scope_kind, scope_id, key)"
    ))
    .execute(pool)
    .await
    .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use agentdash_domain::settings::SettingScope;

    async fn new_repo() -> SqliteSettingsRepository {
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("运行测试前需设置 TEST_DATABASE_URL");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("应能连接测试 PostgreSQL");
        let repo = SqliteSettingsRepository::new(pool);
        repo.initialize().await.expect("应能初始化 settings schema");
        repo
    }

    #[tokio::test]
    async fn persists_settings_by_scope() {
        let repo = new_repo().await;
        repo.set(
            &SettingScope::system(),
            "llm.openai.api_key",
            serde_json::json!("system-key"),
        )
        .await
        .expect("应能写入 system setting");
        repo.set(
            &SettingScope::user("alice"),
            "agent.pi.system_prompt",
            serde_json::json!("hello"),
        )
        .await
        .expect("应能写入 user setting");

        let system_entries = repo
            .list(&SettingScope::system(), Some("llm."))
            .await
            .expect("应能读取 system scope");
        let user_entries = repo
            .list(&SettingScope::user("alice"), Some("agent."))
            .await
            .expect("应能读取 user scope");
        let project_entries = repo
            .list(&SettingScope::project("project-1"), None)
            .await
            .expect("应能读取 project scope");

        assert_eq!(system_entries.len(), 1);
        assert_eq!(system_entries[0].scope_kind, SettingScopeKind::System);
        assert_eq!(system_entries[0].scope_id, None);
        assert_eq!(user_entries.len(), 1);
        assert_eq!(user_entries[0].scope_kind, SettingScopeKind::User);
        assert_eq!(user_entries[0].scope_id.as_deref(), Some("alice"));
        assert!(project_entries.is_empty());
    }
}
