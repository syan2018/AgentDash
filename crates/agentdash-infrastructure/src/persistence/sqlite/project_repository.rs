use sqlx::SqlitePool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::project::{Project, ProjectConfig, ProjectRepository};

pub struct SqliteProjectRepository {
    pool: SqlitePool,
}

impl SqliteProjectRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                config TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        // 向后兼容：如果旧表还有 backend_id 列，忽略错误（SQLite 不支持 DROP COLUMN IF EXISTS，忽略即可）
        let _ = sqlx::query("ALTER TABLE projects DROP COLUMN backend_id")
            .execute(&self.pool)
            .await;

        Ok(())
    }
}

#[async_trait::async_trait]
impl ProjectRepository for SqliteProjectRepository {
    async fn create(&self, project: &Project) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO projects (id, name, description, config, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(project.id.to_string())
        .bind(&project.name)
        .bind(&project.description)
        .bind(serde_json::to_string(&project.config)?)
        .bind(project.created_at.to_rfc3339())
        .bind(project.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Project>, DomainError> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, name, description, config, created_at, updated_at
             FROM projects WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(|r| r.try_into()).transpose()
    }

    async fn list_all(&self) -> Result<Vec<Project>, DomainError> {
        let rows = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, name, description, config, created_at, updated_at
             FROM projects ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn update(&self, project: &Project) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE projects SET name = ?, description = ?, config = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&project.name)
        .bind(&project.description)
        .bind(serde_json::to_string(&project.config)?)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(project.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "project",
                id: project.id.to_string(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "project",
                id: id.to_string(),
            });
        }
        Ok(())
    }
}

// --- SQLx 行映射辅助结构 ---

#[derive(sqlx::FromRow)]
struct ProjectRow {
    id: String,
    name: String,
    description: String,
    config: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ProjectRow> for Project {
    type Error = DomainError;

    fn try_from(row: ProjectRow) -> Result<Self, Self::Error> {
        Ok(Project {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "project",
                id: row.id.clone(),
            })?,
            name: row.name,
            description: row.description,
            config: serde_json::from_str::<ProjectConfig>(&row.config).unwrap_or_default(),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}
