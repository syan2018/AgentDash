use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::llm_provider::{
    LlmCredentialMode, LlmProvider, LlmProviderCredentialRepository, LlmProviderRepository,
    LlmProviderUserCredential, WireProtocol,
};

pub struct PostgresLlmProviderRepository {
    pool: PgPool,
}

impl PostgresLlmProviderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["llm_providers"]).await
    }
}

pub struct PostgresLlmProviderCredentialRepository {
    pool: PgPool,
}

impl PostgresLlmProviderCredentialRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["llm_provider_user_credentials"],
        )
        .await
    }
}

// ─── Row mapping ───

const COLUMNS: &str = "id, name, slug, protocol, credential_mode, global_api_key_ciphertext, base_url, wire_api, default_model, models, blocked_models, env_api_key, discovery_url, sort_order, enabled, created_at, updated_at";

#[derive(sqlx::FromRow)]
struct LlmProviderRow {
    id: String,
    name: String,
    slug: String,
    protocol: String,
    credential_mode: String,
    global_api_key_ciphertext: String,
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
        let protocol = row.protocol.parse::<WireProtocol>().map_err(|_| {
            DomainError::InvalidConfig(format!(
                "llm_providers.protocol: 未知协议 '{}'",
                row.protocol
            ))
        })?;
        let credential_mode = row
            .credential_mode
            .parse::<LlmCredentialMode>()
            .map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "llm_providers.credential_mode: 未知策略 '{}'",
                    row.credential_mode
                ))
            })?;
        Ok(LlmProvider {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| DomainError::InvalidConfig(format!("llm_providers.id: {e}")))?,
            name: row.name,
            slug: row.slug,
            protocol,
            credential_mode,
            global_api_key_ciphertext: row.global_api_key_ciphertext,
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
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)"
        ))
        .bind(provider.id.to_string())
        .bind(&provider.name)
        .bind(&provider.slug)
        .bind(provider.protocol.as_str())
        .bind(provider.credential_mode.as_str())
        .bind(&provider.global_api_key_ciphertext)
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
                name = $1, slug = $2, protocol = $3, credential_mode = $4,
                global_api_key_ciphertext = $5, base_url = $6, wire_api = $7,
                default_model = $8, models = $9, blocked_models = $10, env_api_key = $11,
                discovery_url = $12, sort_order = $13, enabled = $14, updated_at = $15
             WHERE id = $16",
        )
        .bind(&provider.name)
        .bind(&provider.slug)
        .bind(provider.protocol.as_str())
        .bind(provider.credential_mode.as_str())
        .bind(&provider.global_api_key_ciphertext)
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

const CREDENTIAL_COLUMNS: &str =
    "id, provider_id, user_id, api_key_ciphertext, created_at, updated_at";

#[derive(sqlx::FromRow)]
struct LlmProviderUserCredentialRow {
    id: String,
    provider_id: String,
    user_id: String,
    api_key_ciphertext: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<LlmProviderUserCredentialRow> for LlmProviderUserCredential {
    type Error = DomainError;

    fn try_from(row: LlmProviderUserCredentialRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id).map_err(|error| {
                DomainError::InvalidConfig(format!("llm_provider_user_credentials.id: {error}"))
            })?,
            provider_id: Uuid::parse_str(&row.provider_id).map_err(|error| {
                DomainError::InvalidConfig(format!(
                    "llm_provider_user_credentials.provider_id: {error}"
                ))
            })?,
            user_id: row.user_id,
            api_key_ciphertext: row.api_key_ciphertext,
            created_at: super::parse_pg_timestamp_checked(
                &row.created_at,
                "llm_provider_user_credentials.created_at",
            )?,
            updated_at: super::parse_pg_timestamp_checked(
                &row.updated_at,
                "llm_provider_user_credentials.updated_at",
            )?,
        })
    }
}

#[async_trait::async_trait]
impl LlmProviderCredentialRepository for PostgresLlmProviderCredentialRepository {
    async fn get_for_user_provider(
        &self,
        user_id: &str,
        provider_id: Uuid,
    ) -> Result<Option<LlmProviderUserCredential>, DomainError> {
        let sql = format!(
            "SELECT {CREDENTIAL_COLUMNS}
             FROM llm_provider_user_credentials
             WHERE user_id = $1 AND provider_id = $2"
        );
        let row: Option<LlmProviderUserCredentialRow> = sqlx::query_as(&sql)
            .bind(user_id)
            .bind(provider_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
        row.map(LlmProviderUserCredential::try_from).transpose()
    }

    async fn list_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<LlmProviderUserCredential>, DomainError> {
        let sql = format!(
            "SELECT {CREDENTIAL_COLUMNS}
             FROM llm_provider_user_credentials
             WHERE user_id = $1
             ORDER BY updated_at DESC"
        );
        let rows: Vec<LlmProviderUserCredentialRow> = sqlx::query_as(&sql)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
        rows.into_iter()
            .map(LlmProviderUserCredential::try_from)
            .collect()
    }

    async fn upsert_for_user_provider(
        &self,
        credential: &LlmProviderUserCredential,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO llm_provider_user_credentials
                (id, provider_id, user_id, api_key_ciphertext, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT(provider_id, user_id) DO UPDATE SET
                api_key_ciphertext = EXCLUDED.api_key_ciphertext,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(credential.id.to_string())
        .bind(credential.provider_id.to_string())
        .bind(&credential.user_id)
        .bind(&credential.api_key_ciphertext)
        .bind(credential.created_at.to_rfc3339())
        .bind(credential.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
        Ok(())
    }

    async fn delete_for_user_provider(
        &self,
        user_id: &str,
        provider_id: Uuid,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "DELETE FROM llm_provider_user_credentials
             WHERE user_id = $1 AND provider_id = $2",
        )
        .bind(user_id)
        .bind(provider_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
        Ok(result.rows_affected() > 0)
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
