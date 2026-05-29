use sqlx::{PgPool, Postgres, QueryBuilder};

use agentdash_domain::DomainError;
use agentdash_domain::common::{StoredFileContent, StoredFileContentKind};
use agentdash_domain::shared_library::InstalledAssetSource;
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
        crate::migration::assert_postgres_tables_ready(&self.pool, &["skill_assets"]).await
    }
}

const ASSET_COLS: &str = "id,project_id,key,display_name,description,source,builtin_key,remote_source_url,remote_imported_at,remote_digest,library_asset_id,source_ref,source_version,source_digest,installed_at,disable_model_invocation,created_at,updated_at";
const SKILL_FILE_CONTAINER_ID: &str = "files";
const SKILL_INLINE_FILE_COLS: &str =
    "id,owner_id,path,content_kind,mime_type,text_content,binary_content,size_bytes,updated_at";

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
        let mut assets = rows
            .into_iter()
            .map(SkillAssetRow::try_into_asset)
            .collect::<Result<Vec<_>, DomainError>>()?;
        attach_files_to_assets(&self.pool, &mut assets).await?;
        Ok(assets)
    }

    async fn update(&self, asset: &SkillAsset) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let result = sqlx::query(
            "UPDATE skill_assets SET key=$1, display_name=$2, description=$3, source=$4, builtin_key=$5, remote_source_url=$6, remote_imported_at=$7, remote_digest=$8, library_asset_id=$9, source_ref=$10, source_version=$11, source_digest=$12, installed_at=$13, disable_model_invocation=$14, updated_at=$15 WHERE id=$16",
        )
        .bind(&asset.key)
        .bind(&asset.display_name)
        .bind(&asset.description)
        .bind(asset.source.tag())
        .bind(asset.source.builtin_key())
        .bind(remote_source_url(&asset.source))
        .bind(remote_imported_at(&asset.source))
        .bind(remote_digest(&asset.source))
        .bind(installed_library_asset_id(&asset.installed_source))
        .bind(installed_source_ref(&asset.installed_source))
        .bind(installed_source_version(&asset.installed_source))
        .bind(installed_source_digest(&asset.installed_source))
        .bind(installed_at(&asset.installed_source))
        .bind(asset.disable_model_invocation)
        .bind(asset.updated_at)
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
        "INSERT INTO skill_assets ({ASSET_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)"
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
    .bind(installed_library_asset_id(&asset.installed_source))
    .bind(installed_source_ref(&asset.installed_source))
    .bind(installed_source_version(&asset.installed_source))
    .bind(installed_source_digest(&asset.installed_source))
    .bind(installed_at(&asset.installed_source))
    .bind(asset.disable_model_invocation)
    .bind(asset.created_at)
    .bind(asset.updated_at)
    .execute(&mut **tx)
    .await
    .map_err(db_err)?;
    Ok(())
}

async fn replace_files(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    asset: &SkillAsset,
) -> Result<(), DomainError> {
    sqlx::query(
        "DELETE FROM inline_fs_files WHERE owner_kind = 'skill_asset' AND owner_id = $1 AND container_id = $2",
    )
        .bind(asset.id.to_string())
        .bind(SKILL_FILE_CONTAINER_ID)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    if asset.files.is_empty() {
        return Ok(());
    }
    let asset_id = asset.id.to_string();
    let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO inline_fs_files (id, owner_kind, owner_id, container_id, path, content_kind, mime_type, text_content, binary_content, size_bytes, updated_at) ",
    );
    let sizes = asset
        .files
        .iter()
        .map(skill_file_size_bytes_i64)
        .collect::<Result<Vec<_>, _>>()?;
    builder.push_values(
        asset.files.iter().zip(sizes.iter()),
        |mut row, (file, size_bytes)| {
            row.push_bind(file.id.to_string())
                .push_bind("skill_asset")
                .push_bind(&asset_id)
                .push_bind(SKILL_FILE_CONTAINER_ID)
                .push_bind(&file.path)
                .push_bind(file.content_kind_str())
                .push_bind(file.mime_type())
                .push_bind(file.text_content())
                .push_bind(file.binary_content().map(|bytes| bytes.to_vec()))
                .push_bind(*size_bytes)
                .push_bind(file.updated_at);
        },
    );
    builder.build().execute(&mut **tx).await.map_err(db_err)?;
    Ok(())
}

fn skill_file_size_bytes_i64(file: &SkillAssetFile) -> Result<i64, DomainError> {
    i64::try_from(file.size_bytes).map_err(|_| {
        DomainError::InvalidConfig(format!("SkillAsset 文件过大: {}", file.size_bytes))
    })
}

async fn hydrate_asset(pool: &PgPool, row: SkillAssetRow) -> Result<SkillAsset, DomainError> {
    let mut asset = row.try_into_asset()?;
    let files = sqlx::query_as::<_, SkillAssetFileRow>(&format!(
        "SELECT {SKILL_INLINE_FILE_COLS} FROM inline_fs_files WHERE owner_kind = 'skill_asset' AND owner_id = $1 AND container_id = $2 ORDER BY path ASC"
    ))
    .bind(asset.id.to_string())
    .bind(SKILL_FILE_CONTAINER_ID)
    .fetch_all(pool)
    .await
    .map_err(db_err)?
    .into_iter()
    .map(TryInto::try_into)
    .collect::<Result<Vec<_>, DomainError>>()?;
    asset.files = files;
    Ok(asset)
}

/// 为一组已加载的 SkillAsset 批量挂载 files —— 用单次 `ANY($1)` 查询替代 N+1。
async fn attach_files_to_assets(
    pool: &PgPool,
    assets: &mut [SkillAsset],
) -> Result<(), DomainError> {
    if assets.is_empty() {
        return Ok(());
    }
    let asset_ids: Vec<String> = assets.iter().map(|asset| asset.id.to_string()).collect();
    let file_rows = sqlx::query_as::<_, SkillAssetFileRow>(&format!(
        "SELECT {SKILL_INLINE_FILE_COLS} FROM inline_fs_files WHERE owner_kind = 'skill_asset' AND owner_id = ANY($1) AND container_id = $2 ORDER BY path ASC"
    ))
    .bind(&asset_ids)
    .bind(SKILL_FILE_CONTAINER_ID)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    let mut files_by_asset: std::collections::HashMap<uuid::Uuid, Vec<SkillAssetFile>> =
        std::collections::HashMap::with_capacity(assets.len());
    for row in file_rows {
        let file: SkillAssetFile = row.try_into()?;
        files_by_asset
            .entry(file.skill_asset_id)
            .or_default()
            .push(file);
    }
    for asset in assets.iter_mut() {
        asset.files = files_by_asset.remove(&asset.id).unwrap_or_default();
    }
    Ok(())
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
    remote_imported_at: Option<chrono::DateTime<chrono::Utc>>,
    remote_digest: Option<String>,
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<chrono::DateTime<chrono::Utc>>,
    disable_model_invocation: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
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
                imported_at: self.remote_imported_at.ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=github 但 remote_imported_at 为空".to_string(),
                    )
                })?,
                digest: self.remote_digest.clone().ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=github 但 remote_digest 为空".to_string(),
                    )
                })?,
            },
            "clawhub" => SkillAssetSource::Clawhub {
                url: self.remote_source_url.clone().ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=clawhub 但 remote_source_url 为空".to_string(),
                    )
                })?,
                imported_at: self.remote_imported_at.ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=clawhub 但 remote_imported_at 为空".to_string(),
                    )
                })?,
                digest: self.remote_digest.clone().ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=clawhub 但 remote_digest 为空".to_string(),
                    )
                })?,
            },
            "skills_sh" => SkillAssetSource::SkillsSh {
                url: self.remote_source_url.clone().ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=skills_sh 但 remote_source_url 为空".to_string(),
                    )
                })?,
                imported_at: self.remote_imported_at.ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=skills_sh 但 remote_imported_at 为空".to_string(),
                    )
                })?,
                digest: self.remote_digest.clone().ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "skill_assets.source=skills_sh 但 remote_digest 为空".to_string(),
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
            installed_source: parse_installed_source(
                self.library_asset_id,
                self.source_ref,
                self.source_version,
                self.source_digest,
                self.installed_at,
            )?,
            disable_model_invocation: self.disable_model_invocation,
            files: Vec::new(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct SkillAssetFileRow {
    id: String,
    owner_id: String,
    path: String,
    content_kind: String,
    mime_type: Option<String>,
    text_content: Option<String>,
    binary_content: Option<Vec<u8>>,
    size_bytes: i64,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<SkillAssetFileRow> for SkillAssetFile {
    type Error = DomainError;

    fn try_from(row: SkillAssetFileRow) -> Result<Self, Self::Error> {
        let content_kind = row
            .content_kind
            .parse::<StoredFileContentKind>()
            .map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "inline_fs_files.content_kind 值无效: {}",
                    row.content_kind
                ))
            })?;
        let content = match content_kind {
            StoredFileContentKind::Text => StoredFileContent::Text {
                content: row.text_content.ok_or_else(|| {
                    DomainError::InvalidConfig(String::from(
                        "inline_fs_files.text_content 不能为空",
                    ))
                })?,
            },
            StoredFileContentKind::Binary => StoredFileContent::Binary {
                bytes: row.binary_content.ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "inline_fs_files.binary_content 不能为空".to_string(),
                    )
                })?,
                mime_type: row.mime_type.ok_or_else(|| {
                    DomainError::InvalidConfig(String::from("inline_fs_files.mime_type 不能为空"))
                })?,
            },
        };
        let updated_at = row.updated_at;
        let size_bytes = u64::try_from(row.size_bytes).map_err(|_| {
            DomainError::InvalidConfig(format!(
                "inline_fs_files.size_bytes 值无效: {}",
                row.size_bytes
            ))
        })?;
        Ok(Self {
            id: parse_uuid(&row.id, "skill_asset_file")?,
            skill_asset_id: parse_uuid(&row.owner_id, "inline_fs_files.owner_id")?,
            kind: SkillAssetFileKind::from_path(&row.path),
            path: row.path,
            content,
            size_bytes,
            created_at: updated_at,
            updated_at,
        })
    }
}

fn parse_uuid(raw: &str, entity: &'static str) -> Result<uuid::Uuid, DomainError> {
    raw.parse().map_err(|_| DomainError::NotFound {
        entity,
        id: raw.to_string(),
    })
}

fn remote_source_url(source: &SkillAssetSource) -> Option<&str> {
    match source {
        SkillAssetSource::Github { url, .. }
        | SkillAssetSource::Clawhub { url, .. }
        | SkillAssetSource::SkillsSh { url, .. } => Some(url.as_str()),
        SkillAssetSource::BuiltinSeed { .. } | SkillAssetSource::User => None,
    }
}

fn remote_imported_at(source: &SkillAssetSource) -> Option<chrono::DateTime<chrono::Utc>> {
    match source {
        SkillAssetSource::Github { imported_at, .. }
        | SkillAssetSource::Clawhub { imported_at, .. }
        | SkillAssetSource::SkillsSh { imported_at, .. } => Some(*imported_at),
        SkillAssetSource::BuiltinSeed { .. } | SkillAssetSource::User => None,
    }
}

fn remote_digest(source: &SkillAssetSource) -> Option<&str> {
    match source {
        SkillAssetSource::Github { digest, .. }
        | SkillAssetSource::Clawhub { digest, .. }
        | SkillAssetSource::SkillsSh { digest, .. } => Some(digest.as_str()),
        SkillAssetSource::BuiltinSeed { .. } | SkillAssetSource::User => None,
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
