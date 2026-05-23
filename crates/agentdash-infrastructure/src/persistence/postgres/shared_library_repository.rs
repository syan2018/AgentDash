use sqlx::PgPool;
use sqlx::types::Json;

use agentdash_domain::DomainError;
use agentdash_domain::shared_library::{
    LibraryAsset, LibraryAssetListFilter, LibraryAssetRepository, LibraryAssetScope,
    LibraryAssetSource, LibraryAssetType, normalize_workflow_template_payload_value,
};

pub struct PostgresSharedLibraryRepository {
    pool: PgPool,
}

impl PostgresSharedLibraryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS library_assets (
                id TEXT PRIMARY KEY,
                asset_type TEXT NOT NULL,
                scope TEXT NOT NULL,
                owner_id TEXT,
                key TEXT NOT NULL,
                display_name TEXT NOT NULL,
                description TEXT,
                version TEXT NOT NULL,
                source TEXT NOT NULL,
                source_ref TEXT,
                payload_digest TEXT NOT NULL,
                deprecated BOOLEAN NOT NULL DEFAULT FALSE,
                payload JSONB NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                CONSTRAINT library_assets_type_check CHECK (
                    asset_type IN ('agent_template', 'mcp_server_template', 'workflow_template', 'skill_template', 'filespace_template', 'extension_template')
                ),
                CONSTRAINT library_assets_scope_check CHECK (
                    scope IN ('builtin', 'system', 'org', 'user')
                ),
                CONSTRAINT library_assets_source_check CHECK (
                    source IN ('builtin', 'user_authored', 'remote_imported', 'plugin_embedded')
                )
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_library_assets_identity ON library_assets(asset_type, scope, COALESCE(owner_id, ''), key)",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_library_assets_asset_type ON library_assets(asset_type)",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_library_assets_scope_owner ON library_assets(scope, owner_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_library_assets_source_ref ON library_assets(source_ref)",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        normalize_workflow_template_assets(&self.pool).await?;

        Ok(())
    }
}

const COLS: &str = "id,asset_type,scope,owner_id,key,display_name,description,version,source,source_ref,payload_digest,deprecated,payload,created_at,updated_at";

#[async_trait::async_trait]
impl LibraryAssetRepository for PostgresSharedLibraryRepository {
    async fn create(&self, asset: &LibraryAsset) -> Result<(), DomainError> {
        asset.typed_payload()?;
        sqlx::query(&format!(
            "INSERT INTO library_assets ({COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)"
        ))
        .bind(asset.id.to_string())
        .bind(asset.asset_type.as_str())
        .bind(asset.scope.as_str())
        .bind(asset.owner_id.as_deref())
        .bind(&asset.key)
        .bind(&asset.display_name)
        .bind(asset.description.as_deref())
        .bind(&asset.version)
        .bind(asset.source.as_str())
        .bind(asset.source_ref.as_deref())
        .bind(&asset.payload_digest)
        .bind(asset.deprecated)
        .bind(Json(asset.payload.clone()))
        .bind(asset.created_at.to_rfc3339())
        .bind(asset.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, id: uuid::Uuid) -> Result<Option<LibraryAsset>, DomainError> {
        sqlx::query_as::<_, LibraryAssetRow>(&format!(
            "SELECT {COLS} FROM library_assets WHERE id = $1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn find_by_identity(
        &self,
        asset_type: LibraryAssetType,
        scope: LibraryAssetScope,
        owner_id: Option<&str>,
        key: &str,
    ) -> Result<Option<LibraryAsset>, DomainError> {
        sqlx::query_as::<_, LibraryAssetRow>(&format!(
            "SELECT {COLS} FROM library_assets WHERE asset_type = $1 AND scope = $2 AND COALESCE(owner_id, '') = COALESCE($3, '') AND key = $4"
        ))
        .bind(asset_type.as_str())
        .bind(scope.as_str())
        .bind(owner_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list(&self, filter: LibraryAssetListFilter) -> Result<Vec<LibraryAsset>, DomainError> {
        let rows = sqlx::query_as::<_, LibraryAssetRow>(&format!(
            "SELECT {COLS} FROM library_assets
             WHERE ($1::text IS NULL OR asset_type = $1)
               AND ($2::text IS NULL OR scope = $2)
               AND ($3::text IS NULL OR owner_id = $3)
               AND ($4::boolean OR deprecated = FALSE)
             ORDER BY asset_type ASC, scope ASC, display_name ASC, key ASC"
        ))
        .bind(filter.asset_type.map(LibraryAssetType::as_str))
        .bind(filter.scope.map(LibraryAssetScope::as_str))
        .bind(filter.owner_id.as_deref())
        .bind(filter.include_deprecated)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, asset: &LibraryAsset) -> Result<(), DomainError> {
        asset.typed_payload()?;
        let result = sqlx::query(
            "UPDATE library_assets SET asset_type=$1, scope=$2, owner_id=$3, key=$4, display_name=$5, description=$6, version=$7, source=$8, source_ref=$9, payload_digest=$10, deprecated=$11, payload=$12, updated_at=$13 WHERE id=$14",
        )
        .bind(asset.asset_type.as_str())
        .bind(asset.scope.as_str())
        .bind(asset.owner_id.as_deref())
        .bind(&asset.key)
        .bind(&asset.display_name)
        .bind(asset.description.as_deref())
        .bind(&asset.version)
        .bind(asset.source.as_str())
        .bind(asset.source_ref.as_deref())
        .bind(&asset.payload_digest)
        .bind(asset.deprecated)
        .bind(Json(asset.payload.clone()))
        .bind(asset.updated_at.to_rfc3339())
        .bind(asset.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "library_asset",
                id: asset.id.to_string(),
            });
        }
        Ok(())
    }

    async fn upsert(&self, asset: &LibraryAsset) -> Result<LibraryAsset, DomainError> {
        asset.typed_payload()?;

        let existing = self
            .find_by_identity(
                asset.asset_type,
                asset.scope,
                asset.owner_id.as_deref(),
                &asset.key,
            )
            .await?;

        match existing {
            Some(existing) => {
                let mut merged = asset.clone();
                merged.id = existing.id;
                merged.created_at = existing.created_at;
                merged.updated_at = chrono::Utc::now();
                self.update(&merged).await?;
                Ok(merged)
            }
            None => {
                self.create(asset).await?;
                Ok(asset.clone())
            }
        }
    }
}

fn db_err(error: sqlx::Error) -> DomainError {
    DomainError::InvalidConfig(format!("library_assets: {error}"))
}

async fn normalize_workflow_template_assets(pool: &PgPool) -> Result<(), DomainError> {
    let rows = sqlx::query_as::<_, WorkflowTemplatePayloadRow>(
        "SELECT id,payload FROM library_assets WHERE asset_type = 'workflow_template' AND payload #> '{template,lifecycle,steps}' IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    for row in rows {
        let payload = normalize_workflow_template_payload_value(row.payload.0)?;
        let payload_digest = agentdash_domain::shared_library::seed_digest(&payload)?;
        sqlx::query(
            "UPDATE library_assets SET payload=$1,payload_digest=$2,updated_at=$3 WHERE id=$4",
        )
        .bind(Json(payload))
        .bind(payload_digest)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(row.id)
        .execute(pool)
        .await
        .map_err(db_err)?;
    }

    Ok(())
}

#[derive(sqlx::FromRow)]
struct WorkflowTemplatePayloadRow {
    id: String,
    payload: Json<serde_json::Value>,
}

#[derive(sqlx::FromRow)]
struct LibraryAssetRow {
    id: String,
    asset_type: String,
    scope: String,
    owner_id: Option<String>,
    key: String,
    display_name: String,
    description: Option<String>,
    version: String,
    source: String,
    source_ref: Option<String>,
    payload_digest: String,
    deprecated: bool,
    payload: Json<serde_json::Value>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<LibraryAssetRow> for LibraryAsset {
    type Error = DomainError;

    fn try_from(row: LibraryAssetRow) -> Result<Self, Self::Error> {
        let asset_type = LibraryAssetType::parse(&row.asset_type)?;
        let scope = LibraryAssetScope::parse(&row.scope)?;
        let source = LibraryAssetSource::parse(&row.source)?;
        let payload = row.payload.0;
        agentdash_domain::shared_library::LibraryAssetPayload::validate(asset_type, &payload)?;

        Ok(LibraryAsset {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "library_asset",
                id: row.id.clone(),
            })?,
            asset_type,
            scope,
            owner_id: row.owner_id,
            key: row.key,
            display_name: row.display_name,
            description: row.description,
            version: row.version,
            source,
            source_ref: row.source_ref,
            payload_digest: row.payload_digest,
            deprecated: row.deprecated,
            payload,
            created_at: super::parse_pg_timestamp_checked(
                &row.created_at,
                "library_assets.created_at",
            )?,
            updated_at: super::parse_pg_timestamp_checked(
                &row.updated_at,
                "library_assets.updated_at",
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    async fn new_repo() -> Option<PostgresSharedLibraryRepository> {
        let pool = test_pg_pool("shared_library_repository").await?;
        let repo = PostgresSharedLibraryRepository::new(pool);
        repo.initialize()
            .await
            .expect("应能初始化 library_assets schema");
        Some(repo)
    }

    fn sample_asset(key: &str, version: &str) -> LibraryAsset {
        LibraryAsset::new(
            LibraryAssetType::McpServerTemplate,
            LibraryAssetScope::Builtin,
            None,
            key,
            "Fetch",
            Some("Fetch MCP".to_string()),
            version,
            LibraryAssetSource::Builtin,
            Some(key.to_string()),
            format!("digest-{version}"),
            json!({
                "transport": { "type": "http", "url": "https://example.com/mcp" },
                "route_policy": "direct"
            }),
        )
        .expect("valid asset")
    }

    #[tokio::test]
    async fn create_and_get_library_asset_roundtrip() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let asset = sample_asset(&format!("fetch-{}", uuid::Uuid::new_v4()), "1.0.0");

        repo.create(&asset).await.expect("create");

        let loaded = repo.get(asset.id).await.expect("get").expect("exists");
        assert_eq!(loaded.key, asset.key);
        assert_eq!(loaded.asset_type, LibraryAssetType::McpServerTemplate);
        assert_eq!(loaded.scope, LibraryAssetScope::Builtin);
        assert_eq!(loaded.source, LibraryAssetSource::Builtin);
        assert!(matches!(
            loaded.typed_payload().expect("typed"),
            agentdash_domain::shared_library::LibraryAssetPayload::McpServerTemplate(_)
        ));
    }

    #[tokio::test]
    async fn upsert_keeps_identity_stable() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let key = format!("fetch-{}", uuid::Uuid::new_v4());
        let first = sample_asset(&key, "1.0.0");
        let mut second = sample_asset(&key, "1.1.0");
        second.display_name = "Fetch Updated".to_string();

        let inserted = repo.upsert(&first).await.expect("first");
        let updated = repo.upsert(&second).await.expect("second");

        assert_eq!(inserted.id, updated.id);
        assert_eq!(updated.version, "1.1.0");
        assert_eq!(updated.display_name, "Fetch Updated");
    }

    #[tokio::test]
    async fn list_filters_by_type_and_hides_deprecated_by_default() {
        let Some(repo) = new_repo().await else {
            return;
        };
        let key = format!("fetch-{}", uuid::Uuid::new_v4());
        let mut asset = sample_asset(&key, "1.0.0");
        asset.deprecated = true;
        repo.create(&asset).await.expect("create");

        let visible = repo
            .list(LibraryAssetListFilter {
                asset_type: Some(LibraryAssetType::McpServerTemplate),
                scope: Some(LibraryAssetScope::Builtin),
                owner_id: None,
                include_deprecated: false,
            })
            .await
            .expect("list visible");
        assert!(!visible.iter().any(|item| item.id == asset.id));

        let with_deprecated = repo
            .list(LibraryAssetListFilter {
                include_deprecated: true,
                ..Default::default()
            })
            .await
            .expect("list all");
        assert!(with_deprecated.iter().any(|item| item.id == asset.id));
    }
}
