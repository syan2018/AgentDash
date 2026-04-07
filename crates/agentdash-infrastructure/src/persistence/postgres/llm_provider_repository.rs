use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::llm_provider::{LlmProvider, LlmProviderRepository, WireProtocol};

pub struct PostgresLlmProviderRepository {
    pool: PgPool,
}

impl PostgresLlmProviderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS llm_providers (
                id             TEXT PRIMARY KEY,
                name           TEXT NOT NULL,
                slug           TEXT NOT NULL UNIQUE,
                protocol       TEXT NOT NULL,
                api_key        TEXT NOT NULL DEFAULT '',
                base_url       TEXT NOT NULL DEFAULT '',
                wire_api       TEXT NOT NULL DEFAULT '',
                default_model  TEXT NOT NULL DEFAULT '',
                models         TEXT NOT NULL DEFAULT '[]',
                blocked_models TEXT NOT NULL DEFAULT '[]',
                env_api_key    TEXT NOT NULL DEFAULT '',
                discovery_url  TEXT NOT NULL DEFAULT '',
                sort_order     INTEGER NOT NULL DEFAULT 0,
                enabled        BOOLEAN NOT NULL DEFAULT TRUE,
                created_at     TEXT NOT NULL,
                updated_at     TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

// ─── Row mapping ───

const COLUMNS: &str = "id, name, slug, protocol, api_key, base_url, wire_api, default_model, models, blocked_models, env_api_key, discovery_url, sort_order, enabled, created_at, updated_at";

#[derive(sqlx::FromRow)]
struct LlmProviderRow {
    id: String,
    name: String,
    slug: String,
    protocol: String,
    api_key: String,
    base_url: String,
    wire_api: String,
    default_model: String,
    models: String,
    blocked_models: String,
    env_api_key: String,
    discovery_url: String,
    sort_order: i32,
    enabled: bool,
    created_at: String,
    updated_at: String,
}

impl TryFrom<LlmProviderRow> for LlmProvider {
    type Error = DomainError;

    fn try_from(row: LlmProviderRow) -> Result<Self, Self::Error> {
        let protocol = WireProtocol::from_str(&row.protocol).ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "llm_providers.protocol: 未知协议 '{}'",
                row.protocol
            ))
        })?;
        Ok(LlmProvider {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| DomainError::InvalidConfig(format!("llm_providers.id: {e}")))?,
            name: row.name,
            slug: row.slug,
            protocol,
            api_key: row.api_key,
            base_url: row.base_url,
            wire_api: row.wire_api,
            default_model: row.default_model,
            models: parse_json_column(&row.models, "llm_providers.models")?,
            blocked_models: parse_json_column(&row.blocked_models, "llm_providers.blocked_models")?,
            env_api_key: row.env_api_key,
            discovery_url: row.discovery_url,
            sort_order: row.sort_order,
            enabled: row.enabled,
            created_at: super::parse_pg_timestamp_checked(
                &row.created_at,
                "llm_providers.created_at",
            )?,
            updated_at: super::parse_pg_timestamp_checked(
                &row.updated_at,
                "llm_providers.updated_at",
            )?,
        })
    }
}

#[async_trait::async_trait]
impl LlmProviderRepository for PostgresLlmProviderRepository {
    async fn create(&self, provider: &LlmProvider) -> Result<(), DomainError> {
        let models_json = serialize_json_column(&provider.models, "llm_providers.models")?;
        let blocked_json =
            serialize_json_column(&provider.blocked_models, "llm_providers.blocked_models")?;
        sqlx::query(&format!(
            "INSERT INTO llm_providers ({COLUMNS})
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)"
        ))
        .bind(provider.id.to_string())
        .bind(&provider.name)
        .bind(&provider.slug)
        .bind(provider.protocol.as_str())
        .bind(&provider.api_key)
        .bind(&provider.base_url)
        .bind(&provider.wire_api)
        .bind(&provider.default_model)
        .bind(models_json)
        .bind(blocked_json)
        .bind(&provider.env_api_key)
        .bind(&provider.discovery_url)
        .bind(provider.sort_order)
        .bind(provider.enabled)
        .bind(provider.created_at.to_rfc3339())
        .bind(provider.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<LlmProvider>, DomainError> {
        let sql = format!("SELECT {COLUMNS} FROM llm_providers WHERE id = $1");
        let row: Option<LlmProviderRow> = sqlx::query_as(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        row.map(LlmProvider::try_from).transpose()
    }

    async fn list_all(&self) -> Result<Vec<LlmProvider>, DomainError> {
        let sql = format!("SELECT {COLUMNS} FROM llm_providers ORDER BY sort_order, created_at");
        let rows: Vec<LlmProviderRow> = sqlx::query_as(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        rows.into_iter().map(LlmProvider::try_from).collect()
    }

    async fn list_enabled(&self) -> Result<Vec<LlmProvider>, DomainError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM llm_providers WHERE enabled = TRUE ORDER BY sort_order, created_at"
        );
        let rows: Vec<LlmProviderRow> = sqlx::query_as(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        rows.into_iter().map(LlmProvider::try_from).collect()
    }

    async fn update(&self, provider: &LlmProvider) -> Result<(), DomainError> {
        let models_json = serialize_json_column(&provider.models, "llm_providers.models")?;
        let blocked_json =
            serialize_json_column(&provider.blocked_models, "llm_providers.blocked_models")?;
        sqlx::query(
            "UPDATE llm_providers SET
                name = $1, slug = $2, protocol = $3, api_key = $4, base_url = $5,
                wire_api = $6, default_model = $7, models = $8, blocked_models = $9,
                env_api_key = $10, discovery_url = $11, sort_order = $12, enabled = $13,
                updated_at = $14
             WHERE id = $15",
        )
        .bind(&provider.name)
        .bind(&provider.slug)
        .bind(provider.protocol.as_str())
        .bind(&provider.api_key)
        .bind(&provider.base_url)
        .bind(&provider.wire_api)
        .bind(&provider.default_model)
        .bind(models_json)
        .bind(blocked_json)
        .bind(&provider.env_api_key)
        .bind(&provider.discovery_url)
        .bind(provider.sort_order)
        .bind(provider.enabled)
        .bind(provider.updated_at.to_rfc3339())
        .bind(provider.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM llm_providers WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn reorder(&self, ids: &[Uuid]) -> Result<(), DomainError> {
        for (i, id) in ids.iter().enumerate() {
            sqlx::query("UPDATE llm_providers SET sort_order = $1, updated_at = $2 WHERE id = $3")
                .bind(i as i32)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(id.to_string())
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }
        Ok(())
    }
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn serialize_json_column<T: serde::Serialize>(
    value: &T,
    field: &str,
) -> Result<String, DomainError> {
    serde_json::to_string(value)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}
