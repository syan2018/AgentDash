use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};

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
                content         TEXT NOT NULL,
                updated_at      TEXT NOT NULL,
                UNIQUE(owner_kind, owner_id, container_id, path)
            );

            CREATE INDEX IF NOT EXISTS idx_inline_fs_files_owner
                ON inline_fs_files(owner_kind, owner_id, container_id);
            "#,
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
    content: String,
    updated_at: String,
}

impl TryFrom<InlineFileRow> for InlineFile {
    type Error = DomainError;

    fn try_from(row: InlineFileRow) -> Result<Self, Self::Error> {
        let owner_kind = InlineFileOwnerKind::from_str(&row.owner_kind).ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "inline_fs_files.owner_kind 值无效: {}",
                row.owner_kind
            ))
        })?;

        Ok(InlineFile {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| DomainError::InvalidConfig(format!("inline_fs_files.id: {e}")))?,
            owner_kind,
            owner_id: Uuid::parse_str(&row.owner_id).map_err(|e| {
                DomainError::InvalidConfig(format!("inline_fs_files.owner_id: {e}"))
            })?,
            container_id: row.container_id,
            path: row.path,
            content: row.content,
            updated_at: super::parse_pg_timestamp_checked(
                &row.updated_at,
                "inline_fs_files.updated_at",
            )?,
        })
    }
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
        let row: Option<InlineFileRow> = sqlx::query_as(
            r#"
            SELECT id, owner_kind, owner_id, container_id, path, content, updated_at
            FROM inline_fs_files
            WHERE owner_kind = $1 AND owner_id = $2 AND container_id = $3 AND path = $4
            "#,
        )
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
        let rows: Vec<InlineFileRow> = sqlx::query_as(
            r#"
            SELECT id, owner_kind, owner_id, container_id, path, content, updated_at
            FROM inline_fs_files
            WHERE owner_kind = $1 AND owner_id = $2 AND container_id = $3
            ORDER BY path
            "#,
        )
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
        let rows: Vec<InlineFileRow> = sqlx::query_as(
            r#"
            SELECT id, owner_kind, owner_id, container_id, path, content, updated_at
            FROM inline_fs_files
            WHERE owner_kind = $1 AND owner_id = $2
            ORDER BY container_id, path
            "#,
        )
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
            INSERT INTO inline_fs_files (id, owner_kind, owner_id, container_id, path, content, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (owner_kind, owner_id, container_id, path)
            DO UPDATE SET content = EXCLUDED.content, updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(file.id.to_string())
        .bind(file.owner_kind.as_str())
        .bind(file.owner_id.to_string())
        .bind(&file.container_id)
        .bind(&file.path)
        .bind(&file.content)
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

        // 逐条 UPSERT — 对 SQLx 兼容性最好
        for file in files {
            sqlx::query(
                r#"
                INSERT INTO inline_fs_files (id, owner_kind, owner_id, container_id, path, content, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (owner_kind, owner_id, container_id, path)
                DO UPDATE SET content = EXCLUDED.content, updated_at = EXCLUDED.updated_at
                "#,
            )
            .bind(file.id.to_string())
            .bind(file.owner_kind.as_str())
            .bind(file.owner_id.to_string())
            .bind(&file.container_id)
            .bind(&file.path)
            .bind(&file.content)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("批量写入 inline_fs_files 失败: {e}"))
            })?;
        }

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
