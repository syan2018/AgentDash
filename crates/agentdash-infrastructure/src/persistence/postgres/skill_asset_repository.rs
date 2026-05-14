use sqlx::PgPool;

use agentdash_domain::DomainError;
use agentdash_domain::skill_asset::{
    SkillAsset, SkillAssetFile, SkillAssetFileKind, SkillAssetRepository, SkillAssetSource,
};

pub struct PostgresSkillAssetRepository {
    pool: PgPool,
}

impl PostgresSkillAssetRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS skill_assets (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                key TEXT NOT NULL,
                display_name TEXT NOT NULL,
                description TEXT NOT NULL,
                source TEXT NOT NULL,
                builtin_key TEXT,
                remote_source_url TEXT,
                remote_imported_at TEXT,
                remote_digest TEXT,
                disable_model_invocation BOOLEAN NOT NULL DEFAULT FALSE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                CONSTRAINT skill_assets_source_check CHECK (source IN ('builtin_seed', 'user', 'github')),
                CONSTRAINT skill_assets_builtin_key_consistency CHECK (
                    (source = 'builtin_seed' AND builtin_key IS NOT NULL)
                    OR (source <> 'builtin_seed' AND builtin_key IS NULL)
                )
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS skill_asset_files (
                id TEXT PRIMARY KEY,
                skill_asset_id TEXT NOT NULL REFERENCES skill_assets(id) ON DELETE CASCADE,
                path TEXT NOT NULL,
                content TEXT NOT NULL,
                kind TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                CONSTRAINT skill_asset_files_kind_check CHECK (kind IN ('skill', 'reference', 'script', 'asset'))
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_skill_assets_project_key ON skill_assets(project_id, key)")
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_skill_assets_project_builtin_key ON skill_assets(project_id, builtin_key) WHERE builtin_key IS NOT NULL")
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_skill_assets_project_id ON skill_assets(project_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_skill_asset_files_asset_path ON skill_asset_files(skill_asset_id, path)")
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }
}

const ASSET_COLS: &str = "id,project_id,key,display_name,description,source,builtin_key,remote_source_url,remote_imported_at,remote_digest,disable_model_invocation,created_at,updated_at";
const FILE_COLS: &str = "id,skill_asset_id,path,content,kind,created_at,updated_at";

#[async_trait::async_trait]
impl SkillAssetRepository for PostgresSkillAssetRepository {
    async fn create(&self, asset: &SkillAsset) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        insert_asset(&mut tx, asset).await?;
        replace_files(&mut tx, asset).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, id: uuid::Uuid) -> Result<Option<SkillAsset>, DomainError> {
        let asset = sqlx::query_as::<_, SkillAssetRow>(&format!(
            "SELECT {ASSET_COLS} FROM skill_assets WHERE id = $1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        let Some(row) = asset else {
            return Ok(None);
        };
        hydrate_asset(&self.pool, row).await.map(Some)
    }

    async fn get_by_project_and_key(
        &self,
        project_id: uuid::Uuid,
        key: &str,
    ) -> Result<Option<SkillAsset>, DomainError> {
        let asset = sqlx::query_as::<_, SkillAssetRow>(&format!(
            "SELECT {ASSET_COLS} FROM skill_assets WHERE project_id = $1 AND key = $2"
        ))
        .bind(project_id.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        match asset {
            Some(row) => hydrate_asset(&self.pool, row).await.map(Some),
            None => Ok(None),
        }
    }

    async fn get_by_project_and_builtin_key(
        &self,
        project_id: uuid::Uuid,
        builtin_key: &str,
    ) -> Result<Option<SkillAsset>, DomainError> {
        let asset = sqlx::query_as::<_, SkillAssetRow>(&format!(
            "SELECT {ASSET_COLS} FROM skill_assets WHERE project_id = $1 AND builtin_key = $2"
        ))
        .bind(project_id.to_string())
        .bind(builtin_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        match asset {
            Some(row) => hydrate_asset(&self.pool, row).await.map(Some),
            None => Ok(None),
        }
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<SkillAsset>, DomainError> {
        let rows = sqlx::query_as::<_, SkillAssetRow>(&format!(
            "SELECT {ASSET_COLS} FROM skill_assets WHERE project_id = $1 ORDER BY key ASC"
        ))
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        let mut assets = Vec::with_capacity(rows.len());
        for row in rows {
            assets.push(hydrate_asset(&self.pool, row).await?);
        }
        Ok(assets)
    }

    async fn update(&self, asset: &SkillAsset) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let result = sqlx::query(
            "UPDATE skill_assets SET key=$1, display_name=$2, description=$3, source=$4, builtin_key=$5, remote_source_url=$6, remote_imported_at=$7, remote_digest=$8, disable_model_invocation=$9, updated_at=$10 WHERE id=$11",
        )
        .bind(&asset.key)
        .bind(&asset.display_name)
        .bind(&asset.description)
        .bind(asset.source.tag())
        .bind(asset.source.builtin_key())
        .bind(remote_source_url(&asset.source))
        .bind(remote_imported_at(&asset.source))
        .bind(remote_digest(&asset.source))
        .bind(asset.disable_model_invocation)
        .bind(asset.updated_at.to_rfc3339())
        .bind(asset.id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "skill_asset",
                id: asset.id.to_string(),
            });
        }
        replace_files(&mut tx, asset).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM skill_assets WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "skill_asset",
                id: id.to_string(),
            });
        }
        Ok(())
    }
}

async fn insert_asset(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    asset: &SkillAsset,
) -> Result<(), DomainError> {
    sqlx::query(&format!(
        "INSERT INTO skill_assets ({ASSET_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"
    ))
    .bind(asset.id.to_string())
    .bind(asset.project_id.to_string())
    .bind(&asset.key)
    .bind(&asset.display_name)
    .bind(&asset.description)
    .bind(asset.source.tag())
    .bind(asset.source.builtin_key())
    .bind(remote_source_url(&asset.source))
    .bind(remote_imported_at(&asset.source))
    .bind(remote_digest(&asset.source))
    .bind(asset.disable_model_invocation)
    .bind(asset.created_at.to_rfc3339())
    .bind(asset.updated_at.to_rfc3339())
    .execute(&mut **tx)
    .await
    .map_err(db_err)?;
    Ok(())
}

async fn replace_files(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    asset: &SkillAsset,
) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM skill_asset_files WHERE skill_asset_id = $1")
        .bind(asset.id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    for file in &asset.files {
        sqlx::query(&format!(
            "INSERT INTO skill_asset_files ({FILE_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7)"
        ))
        .bind(file.id.to_string())
        .bind(asset.id.to_string())
        .bind(&file.path)
        .bind(&file.content)
        .bind(file.kind.tag())
        .bind(file.created_at.to_rfc3339())
        .bind(file.updated_at.to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(())
}

async fn hydrate_asset(pool: &PgPool, row: SkillAssetRow) -> Result<SkillAsset, DomainError> {
    let mut asset = row.try_into_asset()?;
    let files = sqlx::query_as::<_, SkillAssetFileRow>(&format!(
        "SELECT {FILE_COLS} FROM skill_asset_files WHERE skill_asset_id = $1 ORDER BY path ASC"
    ))
    .bind(asset.id.to_string())
    .fetch_all(pool)
    .await
    .map_err(db_err)?
    .into_iter()
    .map(TryInto::try_into)
    .collect::<Result<Vec<_>, DomainError>>()?;
    asset.files = files;
    Ok(asset)
}

#[derive(sqlx::FromRow)]
struct SkillAssetRow {
    id: String,
    project_id: String,
    key: String,
    display_name: String,
    description: String,
    source: String,
    builtin_key: Option<String>,
    remote_source_url: Option<String>,
    remote_imported_at: Option<String>,
    remote_digest: Option<String>,
    disable_model_invocation: bool,
    created_at: String,
    updated_at: String,
}

impl SkillAssetRow {
    fn try_into_asset(self) -> Result<SkillAsset, DomainError> {
        let source = match self.source.as_str() {
            "builtin_seed" => SkillAssetSource::BuiltinSeed {
                key: self.builtin_key.clone().ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=builtin_seed 但 builtin_key 为空".to_string(),
                    )
                })?,
            },
            "github" => SkillAssetSource::Github {
                url: self.remote_source_url.clone().ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=github 但 remote_source_url 为空".to_string(),
                    )
                })?,
                imported_at: super::parse_pg_timestamp_checked(
                    self.remote_imported_at.as_deref().ok_or_else(|| {
                        DomainError::InvalidConfig(
                            "skill_assets.source=github 但 remote_imported_at 为空".to_string(),
                        )
                    })?,
                    "skill_assets.remote_imported_at",
                )?,
                digest: self.remote_digest.clone().ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=github 但 remote_digest 为空".to_string(),
                    )
                })?,
            },
            "user" => SkillAssetSource::User,
            other => {
                return Err(DomainError::InvalidConfig(format!(
                    "skill_assets.source 非法: {other}"
                )));
            }
        };
        Ok(SkillAsset {
            id: parse_uuid(&self.id, "skill_asset")?,
            project_id: parse_uuid(&self.project_id, "skill_assets.project_id")?,
            key: self.key,
            display_name: self.display_name,
            description: self.description,
            source,
            disable_model_invocation: self.disable_model_invocation,
            files: Vec::new(),
            created_at: super::parse_pg_timestamp_checked(
                &self.created_at,
                "skill_assets.created_at",
            )?,
            updated_at: super::parse_pg_timestamp_checked(
                &self.updated_at,
                "skill_assets.updated_at",
            )?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct SkillAssetFileRow {
    id: String,
    skill_asset_id: String,
    path: String,
    content: String,
    kind: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<SkillAssetFileRow> for SkillAssetFile {
    type Error = DomainError;

    fn try_from(row: SkillAssetFileRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(&row.id, "skill_asset_file")?,
            skill_asset_id: parse_uuid(&row.skill_asset_id, "skill_asset_files.skill_asset_id")?,
            path: row.path,
            content: row.content,
            kind: parse_file_kind(&row.kind)?,
            created_at: super::parse_pg_timestamp_checked(
                &row.created_at,
                "skill_asset_files.created_at",
            )?,
            updated_at: super::parse_pg_timestamp_checked(
                &row.updated_at,
                "skill_asset_files.updated_at",
            )?,
        })
    }
}

fn parse_uuid(raw: &str, entity: &'static str) -> Result<uuid::Uuid, DomainError> {
    raw.parse().map_err(|_| DomainError::NotFound {
        entity,
        id: raw.to_string(),
    })
}

fn parse_file_kind(raw: &str) -> Result<SkillAssetFileKind, DomainError> {
    match raw {
        "skill" => Ok(SkillAssetFileKind::Skill),
        "reference" => Ok(SkillAssetFileKind::Reference),
        "script" => Ok(SkillAssetFileKind::Script),
        "asset" => Ok(SkillAssetFileKind::Asset),
        other => Err(DomainError::InvalidConfig(format!(
            "skill_asset_files.kind 非法: {other}"
        ))),
    }
}

fn remote_source_url(source: &SkillAssetSource) -> Option<&str> {
    match source {
        SkillAssetSource::Github { url, .. } => Some(url.as_str()),
        SkillAssetSource::BuiltinSeed { .. } | SkillAssetSource::User => None,
    }
}

fn remote_imported_at(source: &SkillAssetSource) -> Option<String> {
    match source {
        SkillAssetSource::Github { imported_at, .. } => Some(imported_at.to_rfc3339()),
        SkillAssetSource::BuiltinSeed { .. } | SkillAssetSource::User => None,
    }
}

fn remote_digest(source: &SkillAssetSource) -> Option<&str> {
    match source {
        SkillAssetSource::Github { digest, .. } => Some(digest.as_str()),
        SkillAssetSource::BuiltinSeed { .. } | SkillAssetSource::User => None,
    }
}

fn db_err(error: sqlx::Error) -> DomainError {
    DomainError::InvalidConfig(error.to_string())
}
