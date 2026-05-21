use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::project_filespace::{
    ProjectFilespace, ProjectFilespaceRepository, ProjectVfsMountBinding,
    ProjectVfsMountBindingRepository, ProjectVfsMountSource,
};

pub struct PostgresProjectFilespaceRepository {
    pool: PgPool,
}

impl PostgresProjectFilespaceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS project_filespaces (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                key TEXT NOT NULL,
                display_name TEXT NOT NULL,
                description TEXT,
                installed_source TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(project_id, key)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_project_filespaces_project ON project_filespaces(project_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS project_vfs_mount_bindings (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                mount_id TEXT NOT NULL,
                display_name TEXT NOT NULL,
                source TEXT NOT NULL,
                capabilities TEXT NOT NULL,
                default_write BOOLEAN NOT NULL DEFAULT FALSE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(project_id, mount_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_project_vfs_mount_bindings_project ON project_vfs_mount_bindings(project_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct ProjectFilespaceRow {
    id: String,
    project_id: String,
    key: String,
    display_name: String,
    description: Option<String>,
    installed_source: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ProjectFilespaceRow> for ProjectFilespace {
    type Error = DomainError;

    fn try_from(row: ProjectFilespaceRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(&row.id, "project_filespaces.id")?,
            project_id: parse_uuid(&row.project_id, "project_filespaces.project_id")?,
            key: row.key,
            display_name: row.display_name,
            description: row.description,
            installed_source: parse_json_optional(
                row.installed_source,
                "project_filespaces.installed_source",
            )?,
            created_at: super::parse_pg_timestamp_checked(
                &row.created_at,
                "project_filespaces.created_at",
            )?,
            updated_at: super::parse_pg_timestamp_checked(
                &row.updated_at,
                "project_filespaces.updated_at",
            )?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ProjectVfsMountBindingRow {
    id: String,
    project_id: String,
    mount_id: String,
    display_name: String,
    source: String,
    capabilities: String,
    default_write: bool,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ProjectVfsMountBindingRow> for ProjectVfsMountBinding {
    type Error = DomainError;

    fn try_from(row: ProjectVfsMountBindingRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(&row.id, "project_vfs_mount_bindings.id")?,
            project_id: parse_uuid(&row.project_id, "project_vfs_mount_bindings.project_id")?,
            mount_id: row.mount_id,
            display_name: row.display_name,
            source: parse_json_column(&row.source, "project_vfs_mount_bindings.source")?,
            capabilities: parse_json_column(
                &row.capabilities,
                "project_vfs_mount_bindings.capabilities",
            )?,
            default_write: row.default_write,
            created_at: super::parse_pg_timestamp_checked(
                &row.created_at,
                "project_vfs_mount_bindings.created_at",
            )?,
            updated_at: super::parse_pg_timestamp_checked(
                &row.updated_at,
                "project_vfs_mount_bindings.updated_at",
            )?,
        })
    }
}

const FILESPACE_COLUMNS: &str =
    "id, project_id, key, display_name, description, installed_source, created_at, updated_at";
const BINDING_COLUMNS: &str = "id, project_id, mount_id, display_name, source, capabilities, default_write, created_at, updated_at";

#[async_trait::async_trait]
impl ProjectFilespaceRepository for PostgresProjectFilespaceRepository {
    async fn create(&self, filespace: &ProjectFilespace) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO project_filespaces (id, project_id, key, display_name, description, installed_source, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(filespace.id.to_string())
        .bind(filespace.project_id.to_string())
        .bind(&filespace.key)
        .bind(&filespace.display_name)
        .bind(&filespace.description)
        .bind(serialize_json_optional(
            &filespace.installed_source,
            "project_filespaces.installed_source",
        )?)
        .bind(filespace.created_at.to_rfc3339())
        .bind(filespace.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("写入 project_filespaces 失败: {e}")))?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectFilespace>, DomainError> {
        let sql = format!("SELECT {FILESPACE_COLUMNS} FROM project_filespaces WHERE id = $1");
        let row: Option<ProjectFilespaceRow> = sqlx::query_as(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("查询 project_filespaces 失败: {e}"))
            })?;
        row.map(ProjectFilespace::try_from).transpose()
    }

    async fn get_by_project_and_key(
        &self,
        project_id: Uuid,
        key: &str,
    ) -> Result<Option<ProjectFilespace>, DomainError> {
        let sql = format!(
            "SELECT {FILESPACE_COLUMNS} FROM project_filespaces WHERE project_id = $1 AND key = $2"
        );
        let row: Option<ProjectFilespaceRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("查询 project_filespaces 失败: {e}"))
            })?;
        row.map(ProjectFilespace::try_from).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectFilespace>, DomainError> {
        let sql = format!(
            "SELECT {FILESPACE_COLUMNS} FROM project_filespaces WHERE project_id = $1 ORDER BY created_at"
        );
        let rows: Vec<ProjectFilespaceRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("查询 project_filespaces 失败: {e}"))
            })?;
        rows.into_iter().map(ProjectFilespace::try_from).collect()
    }

    async fn update(&self, filespace: &ProjectFilespace) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE project_filespaces
             SET key = $1, display_name = $2, description = $3, installed_source = $4, updated_at = $5
             WHERE id = $6 AND project_id = $7",
        )
        .bind(&filespace.key)
        .bind(&filespace.display_name)
        .bind(&filespace.description)
        .bind(serialize_json_optional(
            &filespace.installed_source,
            "project_filespaces.installed_source",
        )?)
        .bind(filespace.updated_at.to_rfc3339())
        .bind(filespace.id.to_string())
        .bind(filespace.project_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("更新 project_filespaces 失败: {e}")))?;
        Ok(())
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM project_filespaces WHERE project_id = $1 AND id = $2")
            .bind(project_id.to_string())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("删除 project_filespaces 失败: {e}"))
            })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl ProjectVfsMountBindingRepository for PostgresProjectFilespaceRepository {
    async fn create(&self, binding: &ProjectVfsMountBinding) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO project_vfs_mount_bindings (id, project_id, mount_id, display_name, source, capabilities, default_write, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(binding.id.to_string())
        .bind(binding.project_id.to_string())
        .bind(&binding.mount_id)
        .bind(&binding.display_name)
        .bind(serialize_json_column(
            &binding.source,
            "project_vfs_mount_bindings.source",
        )?)
        .bind(serialize_json_column(
            &binding.capabilities,
            "project_vfs_mount_bindings.capabilities",
        )?)
        .bind(binding.default_write)
        .bind(binding.created_at.to_rfc3339())
        .bind(binding.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| {
            DomainError::InvalidConfig(format!("写入 project_vfs_mount_bindings 失败: {e}"))
        })?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectVfsMountBinding>, DomainError> {
        let sql = format!("SELECT {BINDING_COLUMNS} FROM project_vfs_mount_bindings WHERE id = $1");
        let row: Option<ProjectVfsMountBindingRow> = sqlx::query_as(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("查询 project_vfs_mount_bindings 失败: {e}"))
            })?;
        row.map(ProjectVfsMountBinding::try_from).transpose()
    }

    async fn get_by_project_and_mount_id(
        &self,
        project_id: Uuid,
        mount_id: &str,
    ) -> Result<Option<ProjectVfsMountBinding>, DomainError> {
        let sql = format!(
            "SELECT {BINDING_COLUMNS} FROM project_vfs_mount_bindings WHERE project_id = $1 AND mount_id = $2"
        );
        let row: Option<ProjectVfsMountBindingRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .bind(mount_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("查询 project_vfs_mount_bindings 失败: {e}"))
            })?;
        row.map(ProjectVfsMountBinding::try_from).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectVfsMountBinding>, DomainError> {
        let sql = format!(
            "SELECT {BINDING_COLUMNS} FROM project_vfs_mount_bindings WHERE project_id = $1 ORDER BY created_at"
        );
        let rows: Vec<ProjectVfsMountBindingRow> = sqlx::query_as(&sql)
            .bind(project_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("查询 project_vfs_mount_bindings 失败: {e}"))
            })?;
        rows.into_iter()
            .map(ProjectVfsMountBinding::try_from)
            .collect()
    }

    async fn update(&self, binding: &ProjectVfsMountBinding) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE project_vfs_mount_bindings
             SET mount_id = $1, display_name = $2, source = $3, capabilities = $4, default_write = $5, updated_at = $6
             WHERE id = $7 AND project_id = $8",
        )
        .bind(&binding.mount_id)
        .bind(&binding.display_name)
        .bind(serialize_json_column(
            &binding.source,
            "project_vfs_mount_bindings.source",
        )?)
        .bind(serialize_json_column(
            &binding.capabilities,
            "project_vfs_mount_bindings.capabilities",
        )?)
        .bind(binding.default_write)
        .bind(binding.updated_at.to_rfc3339())
        .bind(binding.id.to_string())
        .bind(binding.project_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| {
            DomainError::InvalidConfig(format!("更新 project_vfs_mount_bindings 失败: {e}"))
        })?;
        Ok(())
    }

    async fn delete(&self, project_id: Uuid, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM project_vfs_mount_bindings WHERE project_id = $1 AND id = $2")
            .bind(project_id.to_string())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("删除 project_vfs_mount_bindings 失败: {e}"))
            })?;
        Ok(())
    }
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(raw).map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn parse_json_optional<T: serde::de::DeserializeOwned>(
    raw: Option<String>,
    field: &str,
) -> Result<Option<T>, DomainError> {
    raw.map(|value| parse_json_column(&value, field))
        .transpose()
}

fn serialize_json_column<T: serde::Serialize>(
    value: &T,
    field: &str,
) -> Result<String, DomainError> {
    serde_json::to_string(value)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn serialize_json_optional<T: serde::Serialize>(
    value: &Option<T>,
    field: &str,
) -> Result<Option<String>, DomainError> {
    value
        .as_ref()
        .map(|source| serialize_json_column(source, field))
        .transpose()
}

#[allow(dead_code)]
fn _source_type(source: &ProjectVfsMountSource) -> &'static str {
    match source {
        ProjectVfsMountSource::Filespace { .. } => "filespace",
        ProjectVfsMountSource::ExternalService { .. } => "external_service",
    }
}
