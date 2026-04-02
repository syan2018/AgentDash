use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::llm_provider::{LlmProvider, LlmProviderRepository, WireProtocol};
use agentdash_domain::common::error::DomainError;

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

    /// 一次性迁移: 从旧 settings KV 表导入 LLM provider 配置
    pub async fn migrate_from_settings(
        &self,
        settings_repo: &dyn agentdash_domain::settings::SettingsRepository,
    ) -> Result<(), DomainError> {
        // 仅在 llm_providers 表为空时执行迁移
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM llm_providers")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        if count.0 > 0 {
            return Ok(());
        }

        let scope = agentdash_domain::settings::SettingScope::system();
        let settings = settings_repo.list(&scope, None).await?;

        let setting_map: std::collections::HashMap<String, String> = settings
            .into_iter()
            .filter(|s| s.key.starts_with("llm."))
            .filter_map(|s| {
                let val = s.value.as_str().map(|v| v.to_string())
                    .unwrap_or_else(|| s.value.to_string());
                if val.is_empty() {
                    None
                } else {
                    Some((s.key, val))
                }
            })
            .collect();

        if setting_map.is_empty() {
            return Ok(());
        }

        struct LegacySpec {
            provider_id: &'static str,
            name: &'static str,
            protocol: WireProtocol,
            api_key_key: &'static str,
            env_api_key: &'static str,
            base_url_key: Option<&'static str>,
            default_model_key: Option<&'static str>,
            default_model_fallback: &'static str,
            models_key: &'static str,
            blocked_models_key: &'static str,
            wire_api_key: Option<&'static str>,
            default_base_url: &'static str,
        }

        let legacy_specs = [
            LegacySpec {
                provider_id: "anthropic",
                name: "Anthropic Claude",
                protocol: WireProtocol::Anthropic,
                api_key_key: "llm.anthropic.api_key",
                env_api_key: "ANTHROPIC_API_KEY",
                base_url_key: None,
                default_model_key: None,
                default_model_fallback: "claude-sonnet-4-6-20250514",
                models_key: "llm.anthropic.models",
                blocked_models_key: "llm.anthropic.blocked_models",
                wire_api_key: None,
                default_base_url: "",
            },
            LegacySpec {
                provider_id: "gemini",
                name: "Google Gemini",
                protocol: WireProtocol::Gemini,
                api_key_key: "llm.gemini.api_key",
                env_api_key: "GEMINI_API_KEY",
                base_url_key: None,
                default_model_key: None,
                default_model_fallback: "gemini-2.5-flash",
                models_key: "llm.gemini.models",
                blocked_models_key: "llm.gemini.blocked_models",
                wire_api_key: None,
                default_base_url: "",
            },
            LegacySpec {
                provider_id: "deepseek",
                name: "DeepSeek",
                protocol: WireProtocol::OpenaiCompatible,
                api_key_key: "llm.deepseek.api_key",
                env_api_key: "DEEPSEEK_API_KEY",
                base_url_key: None,
                default_model_key: None,
                default_model_fallback: "deepseek-chat",
                models_key: "llm.deepseek.models",
                blocked_models_key: "llm.deepseek.blocked_models",
                wire_api_key: None,
                default_base_url: "https://api.deepseek.com/v1",
            },
            LegacySpec {
                provider_id: "groq",
                name: "Groq",
                protocol: WireProtocol::OpenaiCompatible,
                api_key_key: "llm.groq.api_key",
                env_api_key: "GROQ_API_KEY",
                base_url_key: None,
                default_model_key: None,
                default_model_fallback: "llama-3.3-70b-versatile",
                models_key: "llm.groq.models",
                blocked_models_key: "llm.groq.blocked_models",
                wire_api_key: None,
                default_base_url: "https://api.groq.com/openai/v1",
            },
            LegacySpec {
                provider_id: "xai",
                name: "xAI (Grok)",
                protocol: WireProtocol::OpenaiCompatible,
                api_key_key: "llm.xai.api_key",
                env_api_key: "XAI_API_KEY",
                base_url_key: None,
                default_model_key: None,
                default_model_fallback: "grok-3",
                models_key: "llm.xai.models",
                blocked_models_key: "llm.xai.blocked_models",
                wire_api_key: None,
                default_base_url: "https://api.x.ai/v1",
            },
            LegacySpec {
                provider_id: "openai",
                name: "OpenAI",
                protocol: WireProtocol::OpenaiCompatible,
                api_key_key: "llm.openai.api_key",
                env_api_key: "OPENAI_API_KEY",
                base_url_key: Some("llm.openai.base_url"),
                default_model_key: Some("llm.openai.default_model"),
                default_model_fallback: "gpt-5.4",
                models_key: "llm.openai.models",
                blocked_models_key: "llm.openai.blocked_models",
                wire_api_key: Some("llm.openai.wire_api"),
                default_base_url: "https://api.openai.com/v1",
            },
        ];

        let mut sort_order = 0i32;
        for spec in &legacy_specs {
            let has_key = setting_map.contains_key(spec.api_key_key);
            if !has_key {
                continue;
            }

            let api_key = setting_map.get(spec.api_key_key).cloned().unwrap_or_default();
            let base_url = spec.base_url_key
                .and_then(|k| setting_map.get(k))
                .cloned()
                .unwrap_or_else(|| spec.default_base_url.to_string());
            let default_model = spec.default_model_key
                .and_then(|k| setting_map.get(k))
                .cloned()
                .unwrap_or_else(|| spec.default_model_fallback.to_string());
            let models = setting_map.get(spec.models_key).cloned().unwrap_or_else(|| "[]".to_string());
            let blocked_models = setting_map.get(spec.blocked_models_key).cloned().unwrap_or_else(|| "[]".to_string());
            let wire_api = spec.wire_api_key
                .and_then(|k| setting_map.get(k))
                .cloned()
                .unwrap_or_default();

            let id = Uuid::new_v4();
            let now = chrono::Utc::now().to_rfc3339();

            sqlx::query(
                "INSERT INTO llm_providers (id, name, slug, protocol, api_key, base_url, wire_api, default_model, models, blocked_models, env_api_key, discovery_url, sort_order, enabled, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, TRUE, $14, $14)",
            )
            .bind(id.to_string())
            .bind(spec.name)
            .bind(spec.provider_id) // slug = legacy provider_id
            .bind(spec.protocol.as_str())
            .bind(&api_key)
            .bind(&base_url)
            .bind(&wire_api)
            .bind(&default_model)
            .bind(&models)
            .bind(&blocked_models)
            .bind(spec.env_api_key)
            .bind("")  // discovery_url: derive from base_url at runtime
            .bind(sort_order)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

            tracing::info!(
                "LlmProvider: 已从 settings 迁移 provider={} (id={})",
                spec.provider_id,
                id,
            );
            sort_order += 1;
        }

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
