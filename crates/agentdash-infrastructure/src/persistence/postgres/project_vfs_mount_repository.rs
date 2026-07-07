use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::project_vfs_mount::{ProjectVfsMount, ProjectVfsMountRepository};

use super::json_document::{from_jsonb, from_optional_jsonb, to_jsonb, to_optional_jsonb};

pub struct PostgresProjectVfsMountRepository {
    pool: PgPool,
}

impl PostgresProjectVfsMountRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["project_vfs_mounts"]).await
    }
}

#[derive(sqlx::FromRow)]
struct ProjectVfsMountRow {
    id: String,
    project_id: String,
    mount_id: String,
    display_name: String,
    description: Option<String>,
    capabilities: serde_json::Value,
    installed_source: Option<serde_json::Value>,
    content: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<ProjectVfsMountRow> for ProjectVfsMount {
    type Error = DomainError;

    fn try_from(row: ProjectVfsMountRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(&row.id, "project_vfs_mounts.id")?,
            project_id: parse_uuid(&row.project_id, "project_vfs_mounts.project_id")?,
            mount_id: row.mount_id,
            display_name: row.display_name,
            description: row.description,
            capabilities: from_jsonb(row.capabilities, "project_vfs_mounts.capabilities")?,
            installed_source: from_optional_jsonb(
                row.installed_source,
                "project_vfs_mounts.installed_source",
            )?,
            content: from_jsonb(row.content, "project_vfs_mounts.content")?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

const MOUNT_COLUMNS: &str = "id, project_id, mount_id, display_name, description, capabilities, installed_source, content, created_at, updated_at";

#[async_trait::async_trait]
impl ProjectVfsMountRepository for PostgresProjectVfsMountRepository {
    async fn create(&self, mount: &ProjectVfsMount) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO project_vfs_mounts (id, project_id, mount_id, display_name, description, capabilities, installed_source, content, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(mount.id.to_string())
        .bind(mount.project_id.to_string())
        .bind(&mount.mount_id)
        .bind(&mount.display_name)
        .bind(&mount.description)
        .bind(to_jsonb(&mount.capabilities, "project_vfs_mounts.capabilities")?)
        .bind(to_optional_jsonb(
            mount.installed_source.as_ref(),
            "project_vfs_mounts.installed_source",
        )?)
        .bind(to_jsonb(&mount.content, "project_vfs_mounts.content")?)
        .bind(mount.created_at)
        .bind(mount.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("写入 project_vfs_mounts 失败: {e}")))?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectVfsMount>, DomainError> {
        let sql = format!("SELECT {MOUNT_COLUMNS} FROM project_vfs_mounts WHERE id = $1");
        let row: Option<ProjectVfsMountRow> = sqlx::query_as(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("查询 project_vfs_mounts 失败: {e}"))
            })?;
        row.map(ProjectVfsMount::try_from).transpose()
    }

    async fn get_by_project_and_mount_id(
        &self,
        project_id: Uuid,
        mount_id: &str,
    ) -> Result<Option<ProjectVfsMount>, DomainError> {
        let sql = format!(
            "SELECT {MOUNT_COLUMNS} FROM project_vfs_mounts WHERE project_id = $1 AND mount_id = $2"
        );
        let row: Option<ProjectVfsMountRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .bind(mount_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("查询 project_vfs_mounts 失败: {e}"))
            })?;
        row.map(ProjectVfsMount::try_from).transpose()
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ProjectVfsMount>, DomainError> {
        let sql = format!(
            "SELECT {MOUNT_COLUMNS} FROM project_vfs_mounts WHERE project_id = $1 ORDER BY created_at"
        );
        let rows: Vec<ProjectVfsMountRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("查询 project_vfs_mounts 失败: {e}"))
            })?;
        rows.into_iter().map(ProjectVfsMount::try_from).collect()
    }

    async fn update(&self, mount: &ProjectVfsMount) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE project_vfs_mounts
             SET mount_id = $1, display_name = $2, description = $3, capabilities = $4, installed_source = $5, content = $6, updated_at = $7
             WHERE id = $8 AND project_id = $9",
        )
        .bind(&mount.mount_id)
        .bind(&mount.display_name)
        .bind(&mount.description)
        .bind(to_jsonb(&mount.capabilities, "project_vfs_mounts.capabilities")?)
        .bind(to_optional_jsonb(
            mount.installed_source.as_ref(),
            "project_vfs_mounts.installed_source",
        )?)
        .bind(to_jsonb(&mount.content, "project_vfs_mounts.content")?)
        .bind(mount.updated_at)
        .bind(mount.id.to_string())
        .bind(mount.project_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("更新 project_vfs_mounts 失败: {e}")))?;
        Ok(())
    }

    async fn delete(&self, project_id: Uuid, mount_id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM project_vfs_mounts WHERE project_id = $1 AND mount_id = $2")
            .bind(project_id.to_string())
            .bind(mount_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("删除 project_vfs_mounts 失败: {e}"))
            })?;
        Ok(())
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(raw).map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}
