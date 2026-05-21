use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::inline_file::{
    InlineFile, InlineFileContent, InlineFileContentKind, InlineFileOwnerKind, InlineFileRepository,
};

pub struct PostgresInlineFileRepository {
    pool: PgPool,
}

impl PostgresInlineFileRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS inline_fs_files (
                id              TEXT PRIMARY KEY,
                owner_kind      TEXT NOT NULL,
                owner_id        TEXT NOT NULL,
                container_id    TEXT NOT NULL,
                path            TEXT NOT NULL,
                content_kind    TEXT NOT NULL,
                mime_type       TEXT,
                text_content    TEXT,
                binary_content  BYTEA,
                size_bytes      BIGINT NOT NULL,
                updated_at      TEXT NOT NULL,
                UNIQUE(owner_kind, owner_id, container_id, path),
                CONSTRAINT chk_inline_fs_files_content_kind
                    CHECK (content_kind IN ('text', 'binary')),
                CONSTRAINT chk_inline_fs_files_content_payload
                    CHECK (
                        (content_kind = 'text' AND text_content IS NOT NULL AND binary_content IS NULL)
                        OR
                        (content_kind = 'binary' AND binary_content IS NOT NULL AND text_content IS NULL AND mime_type IS NOT NULL)
                    )
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_inline_fs_files_owner ON inline_fs_files(owner_kind, owner_id, container_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct InlineFileRow {
    id: String,
    owner_kind: String,
    owner_id: String,
    container_id: String,
    path: String,
    content_kind: String,
    mime_type: Option<String>,
    text_content: Option<String>,
    binary_content: Option<Vec<u8>>,
    size_bytes: i64,
    updated_at: String,
}

impl TryFrom<InlineFileRow> for InlineFile {
    type Error = DomainError;

    fn try_from(row: InlineFileRow) -> Result<Self, Self::Error> {
        let owner_kind = row.owner_kind.parse::<InlineFileOwnerKind>().map_err(|_| {
            DomainError::InvalidConfig(format!(
                "inline_fs_files.owner_kind 值无效: {}",
                row.owner_kind
            ))
        })?;
        let content_kind = row
            .content_kind
            .parse::<InlineFileContentKind>()
            .map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "inline_fs_files.content_kind 值无效: {}",
                    row.content_kind
                ))
            })?;
        let content = match content_kind {
            InlineFileContentKind::Text => InlineFileContent::Text {
                content: row.text_content.ok_or_else(|| {
                    DomainError::InvalidConfig("inline_fs_files.text_content 不能为空".to_string())
                })?,
            },
            InlineFileContentKind::Binary => InlineFileContent::Binary {
                bytes: row.binary_content.ok_or_else(|| {
                    DomainError::InvalidConfig(
                        "inline_fs_files.binary_content 不能为空".to_string(),
                    )
                })?,
                mime_type: row.mime_type.ok_or_else(|| {
                    DomainError::InvalidConfig("inline_fs_files.mime_type 不能为空".to_string())
                })?,
            },
        };

        Ok(InlineFile {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| DomainError::InvalidConfig(format!("inline_fs_files.id: {e}")))?,
            owner_kind,
            owner_id: Uuid::parse_str(&row.owner_id).map_err(|e| {
                DomainError::InvalidConfig(format!("inline_fs_files.owner_id: {e}"))
            })?,
            container_id: row.container_id,
            path: row.path,
            content,
            size_bytes: u64::try_from(row.size_bytes).map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "inline_fs_files.size_bytes 值无效: {}",
                    row.size_bytes
                ))
            })?,
            updated_at: super::parse_pg_timestamp_checked(
                &row.updated_at,
                "inline_fs_files.updated_at",
            )?,
        })
    }
}

const INLINE_FILE_SELECT: &str = r#"
    SELECT id, owner_kind, owner_id, container_id, path,
           content_kind, mime_type, text_content, binary_content, size_bytes, updated_at
    FROM inline_fs_files
"#;

fn size_bytes_i64(file: &InlineFile) -> Result<i64, DomainError> {
    i64::try_from(file.size_bytes)
        .map_err(|_| DomainError::InvalidConfig(format!("inline 文件过大: {}", file.size_bytes)))
}

#[async_trait::async_trait]
impl InlineFileRepository for PostgresInlineFileRepository {
    async fn get_file(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<Option<InlineFile>, DomainError> {
        let query = format!(
            "{INLINE_FILE_SELECT} WHERE owner_kind = $1 AND owner_id = $2 AND container_id = $3 AND path = $4"
        );
        let row: Option<InlineFileRow> = sqlx::query_as(&query)
            .bind(owner_kind.as_str())
            .bind(owner_id.to_string())
            .bind(container_id)
            .bind(path)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(format!("查询 inline_fs_files 失败: {e}")))?;

        row.map(InlineFile::try_from).transpose()
    }

    async fn list_files(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<Vec<InlineFile>, DomainError> {
        let query = format!(
            "{INLINE_FILE_SELECT} WHERE owner_kind = $1 AND owner_id = $2 AND container_id = $3 ORDER BY path"
        );
        let rows: Vec<InlineFileRow> = sqlx::query_as(&query)
            .bind(owner_kind.as_str())
            .bind(owner_id.to_string())
            .bind(container_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(format!("查询 inline_fs_files 失败: {e}")))?;

        rows.into_iter().map(InlineFile::try_from).collect()
    }

    async fn list_files_by_owner(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
    ) -> Result<Vec<InlineFile>, DomainError> {
        let query = format!(
            "{INLINE_FILE_SELECT} WHERE owner_kind = $1 AND owner_id = $2 ORDER BY container_id, path"
        );
        let rows: Vec<InlineFileRow> = sqlx::query_as(&query)
            .bind(owner_kind.as_str())
            .bind(owner_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(format!("查询 inline_fs_files 失败: {e}")))?;

        rows.into_iter().map(InlineFile::try_from).collect()
    }

    async fn upsert_file(&self, file: &InlineFile) -> Result<(), DomainError> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO inline_fs_files (
                id, owner_kind, owner_id, container_id, path,
                content_kind, mime_type, text_content, binary_content, size_bytes, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (owner_kind, owner_id, container_id, path)
            DO UPDATE SET
                content_kind = EXCLUDED.content_kind,
                mime_type = EXCLUDED.mime_type,
                text_content = EXCLUDED.text_content,
                binary_content = EXCLUDED.binary_content,
                size_bytes = EXCLUDED.size_bytes,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(file.id.to_string())
        .bind(file.owner_kind.as_str())
        .bind(file.owner_id.to_string())
        .bind(&file.container_id)
        .bind(&file.path)
        .bind(file.content_kind_str())
        .bind(file.mime_type())
        .bind(file.text_content())
        .bind(file.binary_content().map(|bytes| bytes.to_vec()))
        .bind(size_bytes_i64(file)?)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("写入 inline_fs_files 失败: {e}")))?;

        Ok(())
    }

    async fn upsert_files(&self, files: &[InlineFile]) -> Result<(), DomainError> {
        if files.is_empty() {
            return Ok(());
        }

        let now = chrono::Utc::now().to_rfc3339();
        let sizes = files
            .iter()
            .map(size_bytes_i64)
            .collect::<Result<Vec<_>, _>>()?;
        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO inline_fs_files (id, owner_kind, owner_id, container_id, path, content_kind, mime_type, text_content, binary_content, size_bytes, updated_at) ",
        );
        builder.push_values(
            files.iter().zip(sizes.iter()),
            |mut row, (file, size_bytes)| {
                row.push_bind(file.id.to_string())
                    .push_bind(file.owner_kind.as_str())
                    .push_bind(file.owner_id.to_string())
                    .push_bind(&file.container_id)
                    .push_bind(&file.path)
                    .push_bind(file.content_kind_str())
                    .push_bind(file.mime_type())
                    .push_bind(file.text_content())
                    .push_bind(file.binary_content().map(|bytes| bytes.to_vec()))
                    .push_bind(*size_bytes)
                    .push_bind(&now);
            },
        );
        builder.push(
            " ON CONFLICT (owner_kind, owner_id, container_id, path) DO UPDATE SET content_kind = EXCLUDED.content_kind, mime_type = EXCLUDED.mime_type, text_content = EXCLUDED.text_content, binary_content = EXCLUDED.binary_content, size_bytes = EXCLUDED.size_bytes, updated_at = EXCLUDED.updated_at",
        );
        builder.build().execute(&self.pool).await.map_err(|e| {
            DomainError::InvalidConfig(format!("批量写入 inline_fs_files 失败: {e}"))
        })?;

        Ok(())
    }

    async fn delete_file(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            DELETE FROM inline_fs_files
            WHERE owner_kind = $1 AND owner_id = $2 AND container_id = $3 AND path = $4
            "#,
        )
        .bind(owner_kind.as_str())
        .bind(owner_id.to_string())
        .bind(container_id)
        .bind(path)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("删除 inline_fs_files 失败: {e}")))?;

        Ok(())
    }

    async fn delete_by_container(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            DELETE FROM inline_fs_files
            WHERE owner_kind = $1 AND owner_id = $2 AND container_id = $3
            "#,
        )
        .bind(owner_kind.as_str())
        .bind(owner_id.to_string())
        .bind(container_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("删除 inline_fs_files 失败: {e}")))?;

        Ok(())
    }

    async fn delete_by_owner(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            DELETE FROM inline_fs_files
            WHERE owner_kind = $1 AND owner_id = $2
            "#,
        )
        .bind(owner_kind.as_str())
        .bind(owner_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("删除 inline_fs_files 失败: {e}")))?;

        Ok(())
    }

    async fn count_files(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM inline_fs_files
            WHERE owner_kind = $1 AND owner_id = $2 AND container_id = $3
            "#,
        )
        .bind(owner_kind.as_str())
        .bind(owner_id.to_string())
        .bind(container_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("统计 inline_fs_files 失败: {e}")))?;

        Ok(count.0)
    }
}
