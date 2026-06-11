use sqlx::PgPool;

use agentdash_domain::DomainError;
use agentdash_domain::mcp_preset::{
    McpPreset, McpPresetRepository, McpPresetSource, McpRoutePolicy, McpRuntimeBindingConfig,
    McpTransportConfig,
};
use agentdash_domain::shared_library::InstalledAssetSource;

pub struct PostgresMcpPresetRepository {
    pool: PgPool,
}

impl PostgresMcpPresetRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["mcp_presets"]).await
    }
}

const COLS: &str = "id,project_id,key,display_name,description,transport,route_policy,runtime_binding,source,builtin_key,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";

#[async_trait::async_trait]
impl McpPresetRepository for PostgresMcpPresetRepository {
    async fn create(&self, preset: &McpPreset) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO mcp_presets ({COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)"
        ))
        .bind(preset.id.to_string())
        .bind(preset.project_id.to_string())
        .bind(&preset.key)
        .bind(&preset.display_name)
        .bind(preset.description.as_deref())
        .bind(serde_json::to_string(&preset.transport)?)
        .bind(serde_json::to_string(&preset.route_policy)?)
        .bind(runtime_binding_json(&preset.runtime_binding)?)
        .bind(preset.source.tag())
        .bind(preset.source.builtin_key())
        .bind(installed_library_asset_id(&preset.installed_source))
        .bind(installed_source_ref(&preset.installed_source))
        .bind(installed_source_version(&preset.installed_source))
        .bind(installed_source_digest(&preset.installed_source))
        .bind(installed_at(&preset.installed_source))
        .bind(preset.created_at)
        .bind(preset.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, id: uuid::Uuid) -> Result<Option<McpPreset>, DomainError> {
        sqlx::query_as::<_, McpPresetRow>(&format!("SELECT {COLS} FROM mcp_presets WHERE id = $1"))
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?
            .map(TryInto::try_into)
            .transpose()
    }

    async fn get_by_project_and_key(
        &self,
        project_id: uuid::Uuid,
        key: &str,
    ) -> Result<Option<McpPreset>, DomainError> {
        sqlx::query_as::<_, McpPresetRow>(&format!(
            "SELECT {COLS} FROM mcp_presets WHERE project_id = $1 AND key = $2"
        ))
        .bind(project_id.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_by_project(&self, project_id: uuid::Uuid) -> Result<Vec<McpPreset>, DomainError> {
        sqlx::query_as::<_, McpPresetRow>(&format!(
            "SELECT {COLS} FROM mcp_presets WHERE project_id = $1 ORDER BY created_at ASC"
        ))
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, preset: &McpPreset) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE mcp_presets SET key=$1, display_name=$2, description=$3, transport=$4, route_policy=$5, runtime_binding=$6, source=$7, builtin_key=$8, library_asset_id=$9, source_ref=$10, source_version=$11, source_digest=$12, installed_at=$13, updated_at=$14 WHERE id=$15",
        )
        .bind(&preset.key)
        .bind(&preset.display_name)
        .bind(preset.description.as_deref())
        .bind(serde_json::to_string(&preset.transport)?)
        .bind(serde_json::to_string(&preset.route_policy)?)
        .bind(runtime_binding_json(&preset.runtime_binding)?)
        .bind(preset.source.tag())
        .bind(preset.source.builtin_key())
        .bind(installed_library_asset_id(&preset.installed_source))
        .bind(installed_source_ref(&preset.installed_source))
        .bind(installed_source_version(&preset.installed_source))
        .bind(installed_source_digest(&preset.installed_source))
        .bind(installed_at(&preset.installed_source))
        .bind(preset.updated_at)
        .bind(preset.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "mcp_preset",
                id: preset.id.to_string(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM mcp_presets WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "mcp_preset",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn upsert_builtin(&self, preset: &McpPreset) -> Result<McpPreset, DomainError> {
        let McpPresetSource::Builtin { key } = &preset.source else {
            return Err(DomainError::InvalidConfig(
                "upsert_builtin 仅接受 source=builtin 的 Preset".to_string(),
            ));
        };

        // 根据 (project_id, builtin_key) 定位已有条目；存在则更新，不存在则插入。
        let existing = sqlx::query_as::<_, McpPresetRow>(&format!(
            "SELECT {COLS} FROM mcp_presets WHERE project_id = $1 AND builtin_key = $2"
        ))
        .bind(preset.project_id.to_string())
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        match existing {
            Some(row) => {
                let existing_preset: McpPreset = row.try_into()?;
                let mut merged = preset.clone();
                merged.id = existing_preset.id;
                merged.created_at = existing_preset.created_at;
                merged.updated_at = chrono::Utc::now();
                self.update(&merged).await?;
                Ok(merged)
            }
            None => {
                self.create(preset).await?;
                Ok(preset.clone())
            }
        }
    }
}

use super::db_err;

fn installed_library_asset_id(source: &Option<InstalledAssetSource>) -> Option<String> {
    source
        .as_ref()
        .map(|source| source.library_asset_id.to_string())
}

fn installed_source_ref(source: &Option<InstalledAssetSource>) -> Option<&str> {
    source.as_ref().map(|source| source.source_ref.as_str())
}

fn installed_source_version(source: &Option<InstalledAssetSource>) -> Option<&str> {
    source.as_ref().map(|source| source.source_version.as_str())
}

fn installed_source_digest(source: &Option<InstalledAssetSource>) -> Option<&str> {
    source.as_ref().map(|source| source.source_digest.as_str())
}

fn installed_at(source: &Option<InstalledAssetSource>) -> Option<chrono::DateTime<chrono::Utc>> {
    source.as_ref().map(|source| source.installed_at)
}

fn runtime_binding_json(
    runtime_binding: &Option<McpRuntimeBindingConfig>,
) -> Result<Option<String>, DomainError> {
    runtime_binding
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn parse_installed_source(
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Option<InstalledAssetSource>, DomainError> {
    let Some(library_asset_id) = library_asset_id else {
        return Ok(None);
    };
    Ok(Some(InstalledAssetSource {
        library_asset_id: library_asset_id.parse().map_err(|_| {
            DomainError::InvalidConfig(String::from("installed_source.library_asset_id 无效"))
        })?,
        source_ref: source_ref.ok_or_else(|| {
            DomainError::InvalidConfig(String::from("installed_source.source_ref 为空"))
        })?,
        source_version: source_version.ok_or_else(|| {
            DomainError::InvalidConfig(String::from("installed_source.source_version 为空"))
        })?,
        source_digest: source_digest.ok_or_else(|| {
            DomainError::InvalidConfig(String::from("installed_source.source_digest 为空"))
        })?,
        installed_at: installed_at.ok_or_else(|| {
            DomainError::InvalidConfig(String::from("installed_source.installed_at 为空"))
        })?,
    }))
}

#[derive(sqlx::FromRow)]
struct McpPresetRow {
    id: String,
    project_id: String,
    key: String,
    display_name: String,
    description: Option<String>,
    transport: String,
    route_policy: String,
    runtime_binding: Option<String>,
    source: String,
    builtin_key: Option<String>,
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<McpPresetRow> for McpPreset {
    type Error = DomainError;

    fn try_from(row: McpPresetRow) -> Result<Self, Self::Error> {
        let transport: McpTransportConfig =
            serde_json::from_str(&row.transport).map_err(|error| {
                DomainError::InvalidConfig(format!("mcp_presets.transport: {error}"))
            })?;
        let route_policy: McpRoutePolicy =
            serde_json::from_str(&row.route_policy).map_err(|error| {
                DomainError::InvalidConfig(format!("mcp_presets.route_policy: {error}"))
            })?;
        let runtime_binding: Option<McpRuntimeBindingConfig> = row
            .runtime_binding
            .as_deref()
            .map(|raw| {
                serde_json::from_str(raw).map_err(|error| {
                    DomainError::InvalidConfig(format!("mcp_presets.runtime_binding: {error}"))
                })
            })
            .transpose()?;

        let source = match row.source.as_str() {
            "builtin" => {
                let key = row.builtin_key.clone().ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "mcp_presets.source=builtin 但 builtin_key 为空".to_string(),
                    )
                })?;
                McpPresetSource::Builtin { key }
            }
            "user" => McpPresetSource::User,
            other => {
                return Err(DomainError::InvalidConfig(format!(
                    "mcp_presets.source 非法: {other}"
                )));
            }
        };

        Ok(McpPreset {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "mcp_preset",
                id: row.id.clone(),
            })?,
            project_id: row.project_id.parse().map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "mcp_presets.project_id 无效: {}",
                    row.project_id
                ))
            })?,
            key: row.key,
            display_name: row.display_name,
            description: row.description,
            transport,
            route_policy,
            runtime_binding,
            source,
            installed_source: parse_installed_source(
                row.library_asset_id,
                row.source_ref,
                row.source_version,
                row.source_digest,
                row.installed_at,
            )?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use agentdash_domain::mcp_preset::{McpPresetRepository, McpRoutePolicy, McpTransportConfig};

    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    async fn new_repo() -> Option<PostgresMcpPresetRepository> {
        let pool = test_pg_pool("mcp_preset_repository").await?;
        let repo = PostgresMcpPresetRepository::new(pool);
        repo.initialize()
            .await
            .expect("应能初始化 mcp_presets schema");
        Some(repo)
    }

    fn sample_http_transport() -> McpTransportConfig {
        McpTransportConfig::Http {
            url: "https://example.com/mcp".to_string(),
            headers: vec![],
        }
    }

    #[tokio::test]
    async fn create_and_get_preset_roundtrip() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project_id = Uuid::new_v4();
        let preset = McpPreset::new_user(
            project_id,
            "fetch-preset",
            "Fetch Preset",
            Some("demo".to_string()),
            sample_http_transport(),
            McpRoutePolicy::Direct,
        );

        repo.create(&preset).await.expect("create preset");

        let loaded = repo.get(preset.id).await.expect("get").expect("exists");
        assert_eq!(loaded.key, preset.key);
        assert_eq!(loaded.display_name, preset.display_name);
        assert_eq!(loaded.description.as_deref(), Some("demo"));
        assert_eq!(loaded.source, McpPresetSource::User);
        assert_eq!(loaded.transport, preset.transport);
        assert_eq!(loaded.route_policy, McpRoutePolicy::Direct);
    }

    #[tokio::test]
    async fn project_key_uniqueness_enforced() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project_id = Uuid::new_v4();
        let preset1 = McpPreset::new_user(
            project_id,
            "dup",
            "Duplicate",
            None,
            sample_http_transport(),
            McpRoutePolicy::Direct,
        );
        repo.create(&preset1).await.expect("create first");

        let preset2 = McpPreset::new_user(
            project_id,
            "dup",
            "Duplicate 2",
            None,
            sample_http_transport(),
            McpRoutePolicy::Direct,
        );
        let err = repo.create(&preset2).await.expect_err("dup should fail");
        match err {
            DomainError::InvalidConfig(msg) => {
                assert!(
                    msg.contains("mcp_presets") || msg.to_lowercase().contains("unique"),
                    "err = {msg}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn upsert_builtin_is_idempotent() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project_id = Uuid::new_v4();
        let preset = McpPreset::new_builtin(
            project_id,
            "fetch",
            "Fetch",
            "Fetch",
            Some("builtin fetch".to_string()),
            sample_http_transport(),
            McpRoutePolicy::Auto,
        );

        let first = repo.upsert_builtin(&preset).await.expect("first upsert");
        let second = repo.upsert_builtin(&preset).await.expect("second upsert");

        // 幂等：同一 (project_id, builtin_key) 下只保留一条，id 保持稳定。
        assert_eq!(first.id, second.id);

        let listed = repo
            .list_by_project(project_id)
            .await
            .expect("list builtin");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].source.builtin_key(), Some("fetch"));
    }

    #[tokio::test]
    async fn update_and_delete_user_preset() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let project_id = Uuid::new_v4();
        let mut preset = McpPreset::new_user(
            project_id,
            "edit-me",
            "Edit Me",
            None,
            sample_http_transport(),
            McpRoutePolicy::Direct,
        );
        repo.create(&preset).await.expect("create");

        preset.description = Some("updated".to_string());
        preset.touch();
        repo.update(&preset).await.expect("update");

        let loaded = repo.get(preset.id).await.expect("get").expect("exists");
        assert_eq!(loaded.description.as_deref(), Some("updated"));

        repo.delete(preset.id).await.expect("delete");
        assert!(
            repo.get(preset.id)
                .await
                .expect("get post-delete")
                .is_none()
        );
    }
}
